/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_common::files;
use flow_common::options::Options;
use flow_common_utils::graph::Graph;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_heap::heap_state::CommittedHeap;
use flow_heap::parsing_heaps::Transaction;
use flow_parser::file_key::FileKey;
use flow_parser::file_key::FileKeyInner;
use flow_server_env::dependency_info::DependencyInfo;
use flow_server_env::server_env::Env;
use flow_services_export::export_index::Export;
use flow_services_export::export_index::ExportIndex;
use flow_services_export::export_index::Kind;
use flow_services_export::export_index::Source;
use flow_typing_errors::flow_error::ErrorSet;
use rayon::prelude::*;

const SAVED_STATE_FILE_TABLE_MAGIC: u64 = 0x464C4F5753465431; // "FLOWSFT1"
const SAVED_STATE_RAW_BLOCK_MAGIC: u64 = 0x464C4F5752415731; // "FLOWRAW1"

/// The saved-state counterpart of `server_env::GlobalLibFiles`: the paths a `[libs]` stanza
/// named, the source files discovery classified as global TypeScript declaration libraries,
/// and the source modules containing `declare global` blocks.
///
/// Not quite the same contents, though, hence the name: unlike the env's `configured`, the
/// builtin flowlibs are left out, because the server which loads the saved state extracts
/// and typechecks its own copy of those.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct SavedGlobalLibFiles {
    pub non_flowlib_configured: BTreeSet<FlowSmolStr>,
    pub discovered: BTreeSet<FileKey>,
    pub global_augmentations: FlowOrdSet<FileKey>,
}

// The committed heap is dumped directly to disk via CommittedHeap.save_heap. This type holds only
// lightweight env-level metadata (file sets, dependency graph, etc.); the per-file data lives in
// the heap dump itself. On load, CommittedHeap.from_heap_dump builds a whole heap out of it, so no
// per-file restoration is needed.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct SavedStateEnvData {
    pub flowconfig_hash: FlowSmolStr,
    pub parsed_files: FlowOrdSet<FileKey>,
    pub unparsed_files: FlowOrdSet<FileKey>,
    pub package_json_files: FlowOrdSet<FileKey>,
    pub global_lib_files: SavedGlobalLibFiles,
    pub local_errors: BTreeMap<FileKey, ErrorSet>,
    pub node_modules_containers: BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>,
    pub dependency_info: DependencyInfo,
    pub duplicate_providers: BTreeMap<FlowSmolStr, (FileKey, Vec<FileKey>)>,
    pub export_index: Option<ExportIndex>,
}

struct SavedStateEnvDataForSerialization {
    flowconfig_hash: FlowSmolStr,
    parsed_files: FlowOrdSet<FileKey>,
    unparsed_files: FlowOrdSet<FileKey>,
    package_json_files: FlowOrdSet<FileKey>,
    global_lib_files: SavedGlobalLibFiles,
    local_errors: BTreeMap<FileKey, ErrorSet>,
    node_modules_containers: BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>,
    duplicate_providers: BTreeMap<FlowSmolStr, (FileKey, Vec<FileKey>)>,
    export_index: Option<ExportIndex>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SavedStateEnvFileTable {
    files: Vec<FileKey>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SavedStateEnvBaseData {
    flowconfig_hash: FlowSmolStr,
    parsed_files: Vec<u32>,
    unparsed_files: Vec<u32>,
    package_json_files: Vec<u32>,
    non_flowlib_configured: BTreeSet<FlowSmolStr>,
    discovered: Vec<u32>,
    global_augmentations: Vec<u32>,
    local_errors: Vec<(u32, ErrorSet)>,
    node_modules_containers: BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>,
    duplicate_providers: BTreeMap<FlowSmolStr, (u32, Vec<u32>)>,
    export_index: Option<SerializedExportIndex>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SerializedIndexedGraph {
    keys: Vec<u32>,
    forward: Vec<Vec<u32>>,
    backward: Vec<Vec<u32>>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SerializedExportIndex {
    entries: Vec<(FlowSmolStr, Vec<(SerializedExport, i32)>)>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SerializedExport {
    source: SerializedSource,
    kind: Kind,
}

#[derive(serde::Serialize, serde::Deserialize)]
enum SerializedSource {
    Global,
    Builtin(flow_common::flow_import_specifier::Userland),
    FileKey(u32),
}

struct DeserializedSavedStateEnvBaseData {
    flowconfig_hash: FlowSmolStr,
    parsed_files: FlowOrdSet<FileKey>,
    unparsed_files: FlowOrdSet<FileKey>,
    package_json_files: FlowOrdSet<FileKey>,
    global_lib_files: SavedGlobalLibFiles,
    local_errors: BTreeMap<FileKey, ErrorSet>,
    node_modules_containers: BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>,
    duplicate_providers: BTreeMap<FlowSmolStr, (FileKey, Vec<FileKey>)>,
    export_index: Option<ExportIndex>,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone)]
pub enum InvalidReason {
    Bad_header,
    Build_mismatch { expected: String, actual: String },
    Changed_files,
    Failed_to_marshal(String),
    Failed_to_decompress(String),
    Failed_to_load_heap(String),
    File_does_not_exist,
    Flowconfig_mismatch,
}

#[derive(Debug, Clone)]
pub struct InvalidSavedState(pub InvalidReason);

impl fmt::Display for InvalidReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&invalid_reason_to_string(self))
    }
}

impl std::error::Error for InvalidSavedState {}

impl fmt::Display for InvalidSavedState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub fn invalid_reason_to_string(reason: &InvalidReason) -> String {
    match reason {
        InvalidReason::Bad_header => "Invalid saved state header".to_string(),
        InvalidReason::Build_mismatch { .. } => {
            "Build ID of saved state does not match this binary".to_string()
        }
        InvalidReason::Changed_files => "A file change invalidated the saved state".to_string(),
        InvalidReason::Failed_to_marshal(_) => {
            "Failed to unmarshal data from saved state".to_string()
        }
        InvalidReason::Failed_to_decompress(_) => {
            "Failed to decompress saved state data".to_string()
        }
        InvalidReason::Failed_to_load_heap(msg) => format!("Failed to load heap: {msg}"),
        InvalidReason::File_does_not_exist => "Saved state file does not exist".to_string(),
        InvalidReason::Flowconfig_mismatch => {
            ".flowconfig has changed since saved state was generated".to_string()
        }
    }
}

pub fn backtrace_of_invalid_reason(reason: &InvalidReason) -> Option<String> {
    match reason {
        InvalidReason::Failed_to_marshal(msg)
        | InvalidReason::Failed_to_decompress(msg)
        | InvalidReason::Failed_to_load_heap(msg) => Some(msg.clone()),
        InvalidReason::Build_mismatch { expected, actual } => {
            Some(format!("Expected {expected:?}, got {actual}"))
        }
        InvalidReason::Bad_header
        | InvalidReason::Changed_files
        | InvalidReason::File_does_not_exist
        | InvalidReason::Flowconfig_mismatch => None,
    }
}

fn saved_state_version() -> String {
    if cfg!(debug_assertions) {
        flow_common_build_id::get_build_id()
    } else {
        flow_common::flow_version::version().to_string()
    }
}

// We write the Flow version at the beginning of each saved state file.
fn write_version(file: &mut impl Write) -> Result<(), InvalidReason> {
    file.write_all(saved_state_version().as_bytes())
        .map_err(|err| InvalidReason::Failed_to_marshal(err.to_string()))
}

fn verify_version(options: &Options, file: &mut impl Read) -> Result<(), InvalidReason> {
    let expected = saved_state_version();
    let mut buf = vec![0u8; expected.len()];
    file.read_exact(&mut buf)
        .map_err(|_| InvalidReason::Bad_header)?;
    if options.saved_state_skip_version_check {
        // This is really unsafe! Saved state is marshal'd OCaml data and it's
        // easy to introduce serialization differences that would lead to
        // segfaults. This is only for debugging.
        //
        // We still have to read the version because it consumes from the fd.
        return Ok(());
    }
    let actual = String::from_utf8_lossy(&buf).to_string();
    if actual == expected {
        Ok(())
    } else {
        flow_hh_logger::error!(
            "Saved-state file failed version check. Expected version {:?} but got {:?}",
            expected,
            actual
        );
        Err(InvalidReason::Build_mismatch { expected, actual })
    }
}

fn verify_flowconfig_hash(
    options: &Options,
    flowconfig_hash: &flow_data_structure_wrapper::smol_str::FlowSmolStr,
) -> Result<(), InvalidReason> {
    if options.saved_state_skip_version_check {
        return Ok(());
    }
    let current_flowconfig_hash = {
        let path = flow_server_files::server_files_js::config_file(
            &options.flowconfig_name,
            &options.root,
        );
        match flow_config::get_with_ignored_version(&path, true) {
            Ok((_, _, hash)) => FlowSmolStr::new(hash),
            Err(_) => return Err(InvalidReason::Flowconfig_mismatch),
        }
    };
    if current_flowconfig_hash != *flowconfig_hash {
        flow_hh_logger::error!(
            "Invalid saved state: .flowconfig has changed since this saved state was generated."
        );
        Err(InvalidReason::Flowconfig_mismatch)
    } else {
        Ok(())
    }
}

/// Configured libs are left out of the saved state because the loading server extracts and
/// parses its own copy of them. That reasoning does not extend to discovered globals, which
/// are ordinary source files in the repo: those are saved like any other file, so a restored
/// heap can answer what they hashed and classified as when the state was written.
fn is_configured_lib(env: &Env, file: &FileKey) -> bool {
    files::is_lib_file(&env.configured_libs(), file)
}

fn collect_global_lib_files(env: &Env, options: &Options) -> SavedGlobalLibFiles {
    SavedGlobalLibFiles {
        non_flowlib_configured: collect_non_flowlib_configured_libs(env, options),
        discovered: env.discovered_global_libdefs().clone(),
        global_augmentations: env.global_augmentation_files().dupe(),
    }
}

fn collect_non_flowlib_configured_libs(env: &Env, options: &Options) -> BTreeSet<FlowSmolStr> {
    let flowlib_root = options
        .file_options
        .default_lib_dir
        .as_ref()
        .map(|dir| match dir {
            flow_common::files::LibDir::Prelude(path)
            | flow_common::files::LibDir::Flowlib(path)
            | flow_common::files::LibDir::FlowlibWithLibDomDts(path)
            | flow_common::files::LibDir::Tslib(path) => path.clone(),
        });
    env.configured_libs()
        .iter()
        .filter(|lib| {
            flowlib_root
                .as_ref()
                .is_none_or(|root| !std::path::Path::new(lib.as_str()).starts_with(root))
        })
        .map(|lib| FlowSmolStr::new(files::relative_path(options.root.as_path(), lib.as_str())))
        .collect()
}

fn collect_node_modules_containers(root: &Path) -> BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>> {
    let node_modules_containers = files::node_modules_containers.read().unwrap();
    node_modules_containers
        .iter()
        .map(|(key, value)| {
            (
                FlowSmolStr::new(files::relative_path(root, key.as_str())),
                value.clone(),
            )
        })
        .collect()
}

fn collect_saved_state_env_data(env: &Env, options: &Options) -> SavedStateEnvDataForSerialization {
    flow_hh_logger::info!("Collecting env data for saved state");
    let root = options.root.as_path();
    let errors = env.errors();
    let duplicate_providers = errors
        .duplicate_providers
        .iter()
        .map(|(k, (leader, others))| {
            (
                k.dupe(),
                (
                    leader.dupe(),
                    others.iter().map(|f| f.dupe()).collect::<Vec<_>>(),
                ),
            )
        })
        .collect();
    let node_modules_containers = collect_node_modules_containers(root);
    SavedStateEnvDataForSerialization {
        flowconfig_hash: options.flowconfig_hash.dupe(),
        parsed_files: env
            .files
            .iter()
            .filter(|f| !is_configured_lib(env, f))
            .cloned()
            .collect(),
        unparsed_files: env
            .unparsed
            .iter()
            .filter(|f| !is_configured_lib(env, f))
            .cloned()
            .collect(),
        package_json_files: env.package_json_files.iter().cloned().collect(),
        global_lib_files: collect_global_lib_files(env, options),
        local_errors: errors.local_errors.clone(),
        node_modules_containers,
        duplicate_providers,
        export_index: if options.saved_state_persist_export_index {
            env.exports
                .as_ref()
                .map(|exports| flow_services_export::export_search::get_index(exports).clone())
        } else {
            None
        },
    }
}

fn file_index(file_to_index: &BTreeMap<FileKey, u32>, file: &FileKey) -> u32 {
    *file_to_index
        .get(file)
        .expect("file should have been collected in saved-state file table")
}

fn file_from_index(files: &[FileKey], index: u32) -> Result<FileKey, InvalidReason> {
    files
        .get(index as usize)
        .map(Dupe::dupe)
        .ok_or_else(|| InvalidReason::Failed_to_marshal("Invalid file index".to_string()))
}

fn file_key_tag(file: &FileKey) -> u8 {
    match file.inner() {
        FileKeyInner::LibFile(_) => 0,
        FileKeyInner::SourceFile(_) => 1,
        FileKeyInner::JsonFile(_) => 2,
        FileKeyInner::ResourceFile(_) => 3,
    }
}

fn file_key_from_tag(tag: u8, suffix: String) -> Result<FileKey, InvalidReason> {
    let inner = match tag {
        0 => FileKeyInner::LibFile(suffix),
        1 => FileKeyInner::SourceFile(suffix),
        2 => FileKeyInner::JsonFile(suffix),
        3 => FileKeyInner::ResourceFile(suffix),
        _ => {
            return Err(InvalidReason::Failed_to_marshal(
                "Invalid file key tag".to_string(),
            ));
        }
    };
    Ok(FileKey::new(inner))
}

fn common_prefix_len(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b.iter()).take_while(|(a, b)| a == b).count()
}

fn write_usize(buf: &mut Vec<u8>, mut value: usize) {
    while value >= 0x80 {
        buf.push((value as u8) | 0x80);
        value >>= 7;
    }
    buf.push(value as u8);
}

fn read_usize(bytes: &[u8], offset: &mut usize) -> Result<usize, InvalidReason> {
    let mut value = 0usize;
    let mut shift = 0;
    loop {
        let byte = bytes.get(*offset).copied().ok_or_else(|| {
            InvalidReason::Failed_to_marshal("Unexpected end of file table".to_string())
        })?;
        *offset += 1;
        value |= ((byte & 0x7f) as usize) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
        if shift >= usize::BITS as usize {
            return Err(InvalidReason::Failed_to_marshal(
                "Invalid file table integer".to_string(),
            ));
        }
    }
}

fn serialize_file_table(file_table: &SavedStateEnvFileTable) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&SAVED_STATE_FILE_TABLE_MAGIC.to_le_bytes());
    write_usize(&mut bytes, file_table.files.len());
    let mut previous = Vec::new();
    for file in &file_table.files {
        let suffix = file.suffix().as_bytes();
        let prefix_len = common_prefix_len(&previous, suffix);
        let tail = &suffix[prefix_len..];
        bytes.push(file_key_tag(file));
        write_usize(&mut bytes, prefix_len);
        write_usize(&mut bytes, tail.len());
        bytes.extend_from_slice(tail);
        previous.clear();
        previous.extend_from_slice(suffix);
    }
    bytes
}

fn deserialize_file_table(bytes: &[u8]) -> Result<SavedStateEnvFileTable, InvalidReason> {
    if bytes.len() < 8 {
        return Err(InvalidReason::Failed_to_marshal(
            "Invalid file table header".to_string(),
        ));
    }
    let mut magic = [0; 8];
    magic.copy_from_slice(&bytes[..8]);
    if u64::from_le_bytes(magic) != SAVED_STATE_FILE_TABLE_MAGIC {
        return bincode::serde::decode_from_slice(bytes, bincode::config::legacy())
            .map(|(value, _)| value)
            .map_err(|err| InvalidReason::Failed_to_marshal(err.to_string()));
    }
    let mut offset = 8;
    let len = read_usize(bytes, &mut offset)?;
    let mut paths = Vec::with_capacity(len);
    let mut previous = Vec::new();
    for _ in 0..len {
        let tag = bytes.get(offset).copied().ok_or_else(|| {
            InvalidReason::Failed_to_marshal("Unexpected end of file table".to_string())
        })?;
        offset += 1;
        let prefix_len = read_usize(bytes, &mut offset)?;
        let tail_len = read_usize(bytes, &mut offset)?;
        let tail_end = offset.checked_add(tail_len).ok_or_else(|| {
            InvalidReason::Failed_to_marshal("Invalid file table tail length".to_string())
        })?;
        let tail = bytes.get(offset..tail_end).ok_or_else(|| {
            InvalidReason::Failed_to_marshal("Unexpected end of file table".to_string())
        })?;
        if prefix_len > previous.len() {
            return Err(InvalidReason::Failed_to_marshal(
                "Invalid file table prefix length".to_string(),
            ));
        }
        previous.truncate(prefix_len);
        previous.extend_from_slice(tail);
        paths.push((
            tag,
            std::str::from_utf8(&previous)
                .map_err(|err| InvalidReason::Failed_to_marshal(err.to_string()))?
                .to_owned(),
        ));
        offset = tail_end;
    }
    let files = paths
        .into_par_iter()
        .map(|(tag, path)| file_key_from_tag(tag, path))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SavedStateEnvFileTable { files })
}

fn collect_graph_file_keys(graph: &Graph<FileKey>, file_keys: &mut BTreeSet<FileKey>) {
    graph.fold(
        |file, dependencies, ()| {
            file_keys.insert(file.dupe());
            file_keys.extend(dependencies.iter().duped());
            file_keys.extend(graph.find_backward(file).iter().duped());
        },
        (),
    );
}

fn collect_export_index_file_keys(
    export_index: Option<&ExportIndex>,
    file_keys: &mut BTreeSet<FileKey>,
) {
    if let Some(export_index) = export_index {
        for (_name, exports) in export_index {
            for export in exports.as_ref().keys() {
                let Export(source, _kind) = export;
                if let Source::FileKey(file) = source {
                    file_keys.insert(file.dupe());
                }
            }
        }
    }
}

fn file_set_to_indices(
    files: FlowOrdSet<FileKey>,
    file_to_index: &BTreeMap<FileKey, u32>,
) -> Vec<u32> {
    files
        .into_iter()
        .map(|file| file_index(file_to_index, &file))
        .collect()
}

fn indices_to_file_set(
    indices: Vec<u32>,
    files: &[FileKey],
) -> Result<FlowOrdSet<FileKey>, InvalidReason> {
    indices
        .into_iter()
        .map(|index| file_from_index(files, index))
        .collect()
}

fn file_key_set_to_indices(
    files: &BTreeSet<FileKey>,
    file_to_index: &BTreeMap<FileKey, u32>,
) -> Vec<u32> {
    files
        .iter()
        .map(|file| file_index(file_to_index, file))
        .collect()
}

fn indexed_graph_from_graph(
    graph: &Graph<FileKey>,
    file_to_index: &BTreeMap<FileKey, u32>,
) -> SerializedIndexedGraph {
    let keys = graph.fold(
        |file, _dependencies, mut keys| {
            keys.push(file.dupe());
            keys
        },
        Vec::new(),
    );
    let forward = keys
        .iter()
        .map(|file| {
            graph
                .find_opt(file)
                .map(|files| file_key_set_to_indices(files, file_to_index))
                .unwrap_or_default()
        })
        .collect();
    let backward = keys
        .iter()
        .map(|file| {
            graph
                .find_backward_opt(file)
                .map(|files| file_key_set_to_indices(files, file_to_index))
                .unwrap_or_default()
        })
        .collect();
    let keys = keys
        .iter()
        .map(|file| file_index(file_to_index, file))
        .collect();
    SerializedIndexedGraph {
        keys,
        forward,
        backward,
    }
}

fn graph_from_indexed_graph(files: &[FileKey], graph: SerializedIndexedGraph) -> Graph<FileKey> {
    Graph::from_indexed_edges(files, graph.keys, graph.forward, graph.backward)
}

fn source_to_serialized(
    source: &Source,
    file_to_index: &BTreeMap<FileKey, u32>,
) -> SerializedSource {
    match source {
        Source::Global => SerializedSource::Global,
        Source::Builtin(name) => SerializedSource::Builtin(name.clone()),
        Source::FileKey(file) => SerializedSource::FileKey(file_index(file_to_index, file)),
    }
}

fn source_from_serialized(
    source: SerializedSource,
    files: &[FileKey],
) -> Result<Source, InvalidReason> {
    match source {
        SerializedSource::Global => Ok(Source::Global),
        SerializedSource::Builtin(name) => Ok(Source::Builtin(name)),
        SerializedSource::FileKey(index) => Ok(Source::FileKey(file_from_index(files, index)?)),
    }
}

fn export_index_to_serialized(
    export_index: Option<ExportIndex>,
    file_to_index: &BTreeMap<FileKey, u32>,
) -> Option<SerializedExportIndex> {
    export_index.map(|export_index| SerializedExportIndex {
        entries: export_index
            .into_iter()
            .map(|(name, exports)| {
                let exports = exports
                    .as_ref()
                    .iter()
                    .map(|(export, count)| {
                        let Export(source, kind) = export;
                        (
                            SerializedExport {
                                source: source_to_serialized(source, file_to_index),
                                kind: kind.clone(),
                            },
                            *count,
                        )
                    })
                    .collect();
                (name, exports)
            })
            .collect(),
    })
}

fn export_index_from_serialized(
    export_index: Option<SerializedExportIndex>,
    files: &[FileKey],
) -> Result<Option<ExportIndex>, InvalidReason> {
    match export_index {
        None => Ok(None),
        Some(export_index) => export_index
            .entries
            .into_iter()
            .map(|(name, exports)| {
                let exports = exports
                    .into_iter()
                    .map(|(export, count)| {
                        let source = source_from_serialized(export.source, files)?;
                        Ok((Export(source, export.kind), count))
                    })
                    .collect::<Result<BTreeMap<_, _>, InvalidReason>>()?;
                Ok((name, Arc::new(exports)))
            })
            .collect::<Result<ExportIndex, InvalidReason>>()
            .map(Some),
    }
}

fn serialize_saved_state_env_data(
    data: SavedStateEnvDataForSerialization,
    dependency_info: &DependencyInfo,
    heap_files: Vec<FileKey>,
) -> (
    SavedStateEnvFileTable,
    SavedStateEnvBaseData,
    SerializedIndexedGraph,
    SerializedIndexedGraph,
) {
    let mut file_keys = BTreeSet::new();
    file_keys.extend(heap_files);
    file_keys.extend(data.parsed_files.iter().duped());
    file_keys.extend(data.unparsed_files.iter().duped());
    file_keys.extend(data.package_json_files.iter().duped());
    file_keys.extend(data.global_lib_files.discovered.iter().duped());
    file_keys.extend(data.global_lib_files.global_augmentations.iter().duped());
    file_keys.extend(data.local_errors.keys().duped());
    for (leader, others) in data.duplicate_providers.values() {
        file_keys.insert(leader.dupe());
        file_keys.extend(others.iter().duped());
    }
    collect_graph_file_keys(dependency_info.sig_dependency_graph(), &mut file_keys);
    collect_graph_file_keys(
        dependency_info.implementation_dependency_graph(),
        &mut file_keys,
    );
    collect_export_index_file_keys(data.export_index.as_ref(), &mut file_keys);
    let files = file_keys.into_iter().collect::<Vec<_>>();
    let file_to_index = files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            (
                file.dupe(),
                u32::try_from(index).expect("saved-state file table index should fit in u32"),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let sig_dependency_graph =
        indexed_graph_from_graph(dependency_info.sig_dependency_graph(), &file_to_index);
    let implementation_dependency_graph = indexed_graph_from_graph(
        dependency_info.implementation_dependency_graph(),
        &file_to_index,
    );
    let SavedStateEnvDataForSerialization {
        flowconfig_hash,
        parsed_files,
        unparsed_files,
        package_json_files,
        global_lib_files,
        local_errors,
        node_modules_containers,
        duplicate_providers,
        export_index,
    } = data;
    (
        SavedStateEnvFileTable { files },
        SavedStateEnvBaseData {
            flowconfig_hash,
            parsed_files: file_set_to_indices(parsed_files, &file_to_index),
            unparsed_files: file_set_to_indices(unparsed_files, &file_to_index),
            package_json_files: file_set_to_indices(package_json_files, &file_to_index),
            non_flowlib_configured: global_lib_files.non_flowlib_configured,
            discovered: global_lib_files
                .discovered
                .iter()
                .map(|file| file_index(&file_to_index, file))
                .collect(),
            global_augmentations: global_lib_files
                .global_augmentations
                .iter()
                .map(|file| file_index(&file_to_index, file))
                .collect(),
            local_errors: local_errors
                .into_iter()
                .map(|(file, errors)| (file_index(&file_to_index, &file), errors))
                .collect(),
            node_modules_containers,
            duplicate_providers: duplicate_providers
                .into_iter()
                .map(|(name, (leader, others))| {
                    (
                        name,
                        (
                            file_index(&file_to_index, &leader),
                            others
                                .into_iter()
                                .map(|file| file_index(&file_to_index, &file))
                                .collect(),
                        ),
                    )
                })
                .collect(),
            export_index: export_index_to_serialized(export_index, &file_to_index),
        },
        sig_dependency_graph,
        implementation_dependency_graph,
    )
}

fn deserialize_saved_state_base_data(
    files: &[FileKey],
    data: SavedStateEnvBaseData,
) -> Result<DeserializedSavedStateEnvBaseData, InvalidReason> {
    let SavedStateEnvBaseData {
        flowconfig_hash,
        parsed_files,
        unparsed_files,
        package_json_files,
        non_flowlib_configured,
        discovered,
        global_augmentations,
        local_errors,
        node_modules_containers,
        duplicate_providers,
        export_index,
    } = data;
    let result = DeserializedSavedStateEnvBaseData {
        flowconfig_hash,
        parsed_files: indices_to_file_set(parsed_files, files)?,
        unparsed_files: indices_to_file_set(unparsed_files, files)?,
        package_json_files: indices_to_file_set(package_json_files, files)?,
        global_lib_files: SavedGlobalLibFiles {
            non_flowlib_configured,
            discovered: discovered
                .into_iter()
                .map(|file| file_from_index(files, file))
                .collect::<Result<BTreeSet<_>, InvalidReason>>()?,
            global_augmentations: global_augmentations
                .into_iter()
                .map(|file| file_from_index(files, file))
                .collect::<Result<FlowOrdSet<_>, InvalidReason>>()?,
        },
        local_errors: local_errors
            .into_iter()
            .map(|(file, errors)| Ok((file_from_index(files, file)?, errors)))
            .collect::<Result<BTreeMap<_, _>, InvalidReason>>()?,
        node_modules_containers,
        duplicate_providers: duplicate_providers
            .into_iter()
            .map(|(name, (leader, others))| {
                Ok((
                    name,
                    (
                        file_from_index(files, leader)?,
                        others
                            .into_iter()
                            .map(|file| file_from_index(files, file))
                            .collect::<Result<Vec<_>, InvalidReason>>()?,
                    ),
                ))
            })
            .collect::<Result<BTreeMap<_, _>, InvalidReason>>()?,
        export_index: export_index_from_serialized(export_index, files)?,
    };
    Ok(result)
}

fn join_saved_state_env_data(
    data: DeserializedSavedStateEnvBaseData,
    sig_dependency_graph: Graph<FileKey>,
    implementation_dependency_graph: Graph<FileKey>,
) -> SavedStateEnvData {
    let DeserializedSavedStateEnvBaseData {
        flowconfig_hash,
        parsed_files,
        unparsed_files,
        package_json_files,
        global_lib_files,
        local_errors,
        node_modules_containers,
        duplicate_providers,
        export_index,
    } = data;
    SavedStateEnvData {
        flowconfig_hash,
        parsed_files,
        unparsed_files,
        package_json_files,
        global_lib_files,
        local_errors,
        node_modules_containers,
        dependency_info: DependencyInfo::from_graphs(
            sig_dependency_graph,
            implementation_dependency_graph,
        ),
        duplicate_providers,
        export_index,
    }
}

fn write_serialized_raw<T: serde::Serialize>(
    file: &mut impl Write,
    label: &str,
    data: &T,
) -> Result<(), InvalidReason> {
    let start = Instant::now();
    let serialized = bincode::serde::encode_to_vec(data, bincode::config::legacy())
        .map_err(|err| InvalidReason::Failed_to_marshal(err.to_string()))?;
    let mut raw = Vec::with_capacity(8 + serialized.len());
    raw.extend_from_slice(&SAVED_STATE_RAW_BLOCK_MAGIC.to_le_bytes());
    raw.extend_from_slice(&serialized);
    flow_hh_logger::info!(
        "Serialized {} env data to {} raw bytes in {:?}",
        label,
        raw.len(),
        start.elapsed()
    );
    write_u64(file, raw.len() as u64)?;
    file.write_all(&raw)
        .map_err(|err| InvalidReason::Failed_to_marshal(err.to_string()))
}

fn write_serialized_file_table(
    file: &mut impl Write,
    file_table: &SavedStateEnvFileTable,
) -> Result<(), InvalidReason> {
    let start = Instant::now();
    let serialized = serialize_file_table(file_table);
    let compressed = lz4_flex::compress_prepend_size(&serialized);
    flow_hh_logger::info!(
        "Serialized file table env data from {} bytes to {} bytes in {:?}",
        serialized.len(),
        compressed.len(),
        start.elapsed()
    );
    write_u64(file, compressed.len() as u64)?;
    file.write_all(&compressed)
        .map_err(|err| InvalidReason::Failed_to_marshal(err.to_string()))
}

fn read_serialized(file: &mut impl Read) -> Result<Vec<u8>, InvalidReason> {
    let len = read_u64(file)? as usize;
    let mut serialized = vec![0; len];
    file.read_exact(&mut serialized)
        .map_err(|err| InvalidReason::Failed_to_marshal(err.to_string()))?;
    Ok(serialized)
}

fn unmarshal_serialized<T: serde::de::DeserializeOwned>(
    label: &str,
    compressed: &[u8],
) -> Result<T, InvalidReason> {
    let decompressed;
    let serialized = if compressed.len() >= 8
        && u64::from_le_bytes(compressed[..8].try_into().unwrap()) == SAVED_STATE_RAW_BLOCK_MAGIC
    {
        let serialized = &compressed[8..];
        flow_hh_logger::info!("Read raw {} env data as {} bytes", label, serialized.len());
        serialized
    } else {
        let start = Instant::now();
        decompressed = lz4_flex::decompress_size_prepended(compressed)
            .map_err(|err| InvalidReason::Failed_to_decompress(err.to_string()))?;
        flow_hh_logger::info!(
            "Decompressed {} env data to {} bytes in {:?}",
            label,
            decompressed.len(),
            start.elapsed()
        );
        &decompressed
    };
    let start = Instant::now();
    let result = bincode::serde::decode_from_slice(serialized, bincode::config::legacy())
        .map(|(value, _)| value)
        .map_err(|err| InvalidReason::Failed_to_marshal(err.to_string()));
    flow_hh_logger::info!("Unmarshaled {} env data in {:?}", label, start.elapsed());
    result
}

fn unmarshal_file_table(compressed: &[u8]) -> Result<SavedStateEnvFileTable, InvalidReason> {
    let start = Instant::now();
    let serialized = lz4_flex::decompress_size_prepended(compressed)
        .map_err(|err| InvalidReason::Failed_to_decompress(err.to_string()))?;
    flow_hh_logger::info!(
        "Decompressed file table env data to {} bytes in {:?}",
        serialized.len(),
        start.elapsed()
    );
    let start = Instant::now();
    let result = deserialize_file_table(&serialized);
    flow_hh_logger::info!("Unmarshaled file table env data in {:?}", start.elapsed());
    result
}

fn write_serialized_env_data_with_heap_files(
    file: &mut impl Write,
    data: SavedStateEnvDataForSerialization,
    dependency_info: &DependencyInfo,
    heap_files: Vec<FileKey>,
) -> Result<Vec<FileKey>, InvalidReason> {
    let (file_table, data, sig_dependency_graph, implementation_dependency_graph) =
        serialize_saved_state_env_data(data, dependency_info, heap_files);
    let files = file_table.files.clone();
    write_serialized_file_table(file, &file_table)?;
    write_serialized_raw(file, "base", &data)?;
    write_serialized_raw(file, "sig graph", &sig_dependency_graph)?;
    write_serialized_raw(
        file,
        "implementation graph",
        &implementation_dependency_graph,
    )?;
    Ok(files)
}

fn read_serialized_env_data_with_files(
    file: &mut impl Read,
) -> Result<(SavedStateEnvData, Arc<Vec<FileKey>>), InvalidReason> {
    let file_table = unmarshal_file_table(&read_serialized(file)?)?;
    let files = Arc::new(file_table.files);
    let data: SavedStateEnvBaseData = unmarshal_serialized("base", &read_serialized(file)?)?;
    let sig_dependency_graph: SerializedIndexedGraph =
        unmarshal_serialized("sig graph", &read_serialized(file)?)?;
    let implementation_dependency_graph: SerializedIndexedGraph =
        unmarshal_serialized("implementation graph", &read_serialized(file)?)?;
    let data = deserialize_saved_state_base_data(&files, data)?;
    let sig_dependency_graph = graph_from_indexed_graph(&files, sig_dependency_graph);
    let implementation_dependency_graph =
        graph_from_indexed_graph(&files, implementation_dependency_graph);
    Ok((
        join_saved_state_env_data(data, sig_dependency_graph, implementation_dependency_graph),
        files,
    ))
}

fn write_u64(file: &mut impl Write, value: u64) -> Result<(), InvalidReason> {
    file.write_all(&value.to_le_bytes())
        .map_err(|err| InvalidReason::Failed_to_marshal(err.to_string()))
}

fn read_u64(file: &mut impl Read) -> Result<u64, InvalidReason> {
    let mut bytes = [0; 8];
    file.read_exact(&mut bytes)
        .map_err(|err| InvalidReason::Failed_to_marshal(err.to_string()))?;
    Ok(u64::from_le_bytes(bytes))
}

// Saving the saved state generally consists of 3 things:
//
// 1. Collecting the various bits of data
// 2. Normalizing the data so it can be used by other Flow servers. This generally means turning
//    absolute paths into relative paths
// 3. Writing the data to the saved state file
//
// We care a little bit about the perf of generating saved state, so that any script which
// generates saved states has an easier time keeping up. But the perf of saving isn't as important
// as the perf of loading
pub fn save(
    path: &Path,
    transaction: &Arc<Transaction>,
    env: &Env,
    options: &Options,
) -> Result<(), InvalidReason> {
    let tmp_path = {
        let mut tmp_path = path.to_path_buf();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(".flow.saved_state");
        tmp_path.set_file_name(format!("{file_name}.tmp.{}", std::process::id()));
        tmp_path
    };
    let mut file =
        File::create(&tmp_path).map_err(|err| InvalidReason::Failed_to_marshal(err.to_string()))?;
    write_version(&mut file)?;
    let env_data = collect_saved_state_env_data(env, options);
    flow_hh_logger::info!("Serializing env metadata");
    let heap_files = transaction.collect_heap_file_table();
    let files = {
        let dependency_info = env.dependency_info();
        write_serialized_env_data_with_heap_files(
            &mut file,
            env_data,
            &dependency_info,
            heap_files,
        )?
    };
    flow_hh_logger::info!("Saving heap to saved-state file");
    let result = transaction
        .save_heap_with_file_table(&mut file, &files)
        .map_err(|err| InvalidReason::Failed_to_load_heap(err.to_string()));
    drop(file);
    match result {
        Ok(()) => {
            std::fs::rename(&tmp_path, path)
                .map_err(|err| InvalidReason::Failed_to_marshal(err.to_string()))?;
            flow_hh_logger::info!("Finished writing saved-state file at {:?}", path);
            Ok(())
        }
        Err(err) => {
            let _ = std::fs::remove_file(&tmp_path);
            Err(err)
        }
    }
}

fn denormalize_paths(
    options: &Options,
    non_flowlib_configured: &mut BTreeSet<FlowSmolStr>,
    node_modules_containers: &mut BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>,
) {
    // Raw string paths still need the root prepended
    let root = options.root.as_path();
    let prepend_root = |path: &str| files::absolute_path(root, path);
    *non_flowlib_configured = non_flowlib_configured
        .iter()
        .map(|path| FlowSmolStr::new(prepend_root(path.as_str())))
        .collect();
    *node_modules_containers = node_modules_containers
        .iter()
        .map(|(key, value)| (FlowSmolStr::new(prepend_root(key.as_str())), value.clone()))
        .collect();
}

// Loading the saved state generally consists of 2 things:
//
// 1. Loading the saved state from a file
// 2. Denormalizing the data. This generally means turning relative paths into absolute paths
//
// This is on the critical path for starting up a server with saved state. We really care about
// the perf
fn denormalize_direct_data(
    options: &Options,
    mut data: SavedStateEnvData,
) -> Result<SavedStateEnvData, InvalidReason> {
    flow_server_env::monitor_rpc::status_update(
        flow_server_env::server_status::Event::LoadSavedStateProgress(
            flow_server_env::server_status::Progress {
                total: None,
                finished: 0,
            },
        ),
    );
    verify_flowconfig_hash(options, &data.flowconfig_hash)?;
    denormalize_paths(
        options,
        &mut data.global_lib_files.non_flowlib_configured,
        &mut data.node_modules_containers,
    );
    Ok(data)
}

// A dump is a whole heap rather than an edit to one, so it comes back as a heap of its own; making
// it the heap the server runs on is the caller's call, not this function's.
pub fn load(
    path: &Path,
    options: &Options,
) -> Result<(SavedStateEnvData, Arc<CommittedHeap>), InvalidReason> {
    flow_hh_logger::info!("Reading saved-state file at {:?}", path);
    flow_server_env::monitor_rpc::status_update(
        flow_server_env::server_status::Event::ReadSavedState,
    );
    let mut file = File::open(path).map_err(|_| InvalidReason::File_does_not_exist)?;
    verify_version(options, &mut file)?;
    let (data, heap) = if options.saved_state_parallel_decompress {
        flow_hh_logger::info!("Reading env metadata from saved-state file");
        let file_table = read_serialized(&mut file)?;
        let file_table_thread = std::thread::spawn(move || unmarshal_file_table(&file_table));
        let env_start = Instant::now();
        let env_data = read_serialized(&mut file)?;
        let env_data_unmarshal_thread = std::thread::spawn(move || {
            unmarshal_serialized::<SavedStateEnvBaseData>("base", &env_data)
        });
        let sig_dependency_graph = read_serialized(&mut file)?;
        let sig_dependency_graph_unmarshal_thread = std::thread::spawn(move || {
            unmarshal_serialized::<SerializedIndexedGraph>("sig graph", &sig_dependency_graph)
        });
        let implementation_dependency_graph = read_serialized(&mut file)?;
        let implementation_dependency_graph_unmarshal_thread = std::thread::spawn(move || {
            unmarshal_serialized::<SerializedIndexedGraph>(
                "implementation graph",
                &implementation_dependency_graph,
            )
        });
        flow_hh_logger::info!("Deserializing env metadata + loading heap in parallel");
        let thread_panicked = || InvalidReason::Failed_to_decompress("thread panicked".to_string());
        let file_table = file_table_thread.join().map_err(|_| thread_panicked())??;
        let files = Arc::new(file_table.files);
        let env_data_files = files.dupe();
        let env_data_thread = std::thread::spawn(move || {
            let env_data = env_data_unmarshal_thread.join().map_err(|_| {
                InvalidReason::Failed_to_decompress("thread panicked".to_string())
            })??;
            deserialize_saved_state_base_data(&env_data_files, env_data)
        });
        let sig_dependency_graph_files = files.dupe();
        let sig_dependency_graph_thread = std::thread::spawn(move || {
            let sig_dependency_graph: SerializedIndexedGraph =
                sig_dependency_graph_unmarshal_thread.join().map_err(|_| {
                    InvalidReason::Failed_to_decompress("thread panicked".to_string())
                })??;
            Ok::<_, InvalidReason>(graph_from_indexed_graph(
                &sig_dependency_graph_files,
                sig_dependency_graph,
            ))
        });
        let implementation_dependency_graph_files = files.dupe();
        let implementation_dependency_graph_thread = std::thread::spawn(move || {
            let implementation_dependency_graph: SerializedIndexedGraph =
                implementation_dependency_graph_unmarshal_thread
                    .join()
                    .map_err(|_| {
                        InvalidReason::Failed_to_decompress("thread panicked".to_string())
                    })??;
            Ok::<_, InvalidReason>(graph_from_indexed_graph(
                &implementation_dependency_graph_files,
                implementation_dependency_graph,
            ))
        });
        let heap_start = Instant::now();
        let heap = CommittedHeap::from_heap_dump_with_file_table(&mut file, files)
            .map_err(|err| InvalidReason::Failed_to_load_heap(err.to_string()))?;
        flow_hh_logger::info!("Loaded heap in {:?}", heap_start.elapsed());
        let data = env_data_thread.join().map_err(|_| thread_panicked())??;
        let sig_dependency_graph = sig_dependency_graph_thread
            .join()
            .map_err(|_| thread_panicked())??;
        let implementation_dependency_graph = implementation_dependency_graph_thread
            .join()
            .map_err(|_| thread_panicked())??;
        let data =
            join_saved_state_env_data(data, sig_dependency_graph, implementation_dependency_graph);
        flow_hh_logger::info!("Loaded env metadata in {:?}", env_start.elapsed());
        (data, heap)
    } else {
        flow_hh_logger::info!("Reading env metadata from saved-state file");
        flow_hh_logger::info!("Deserializing env metadata");
        let (data, files) = read_serialized_env_data_with_files(&mut file)?;
        flow_hh_logger::info!("Loading heap from saved-state file");
        let heap = CommittedHeap::from_heap_dump_with_file_table(&mut file, files)
            .map_err(|err| InvalidReason::Failed_to_load_heap(err.to_string()))?;
        (data, heap)
    };
    let data = denormalize_direct_data(options, data)?;
    flow_hh_logger::info!("Finished loading saved-state");
    Ok((data, Arc::new(heap)))
}
