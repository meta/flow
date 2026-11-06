/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::io;
use std::io::Read;
use std::io::Write;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;

use dupe::Dupe;
use flow_common::flow_import_specifier::FlowImportSpecifier;
use flow_common_modulename::HasteModuleInfo;
use flow_common_modulename::Modulename;
use flow_parser::file_key::FileKey;
use flow_parser_utils::package_json::PackageJson;
use flow_utils_concurrency::lockfree_overlay_map::commit_map_with_capacity;
use parking_lot::RecursiveRwLock;
use rayon::prelude::*;

use crate::haste_module::HasteModule;
use crate::heap_state::CommittedHeap;
use crate::heap_state::CommittedHeapData;
use crate::heap_state::CommittedHeapReadGuard;
use crate::heap_state::HeapOverlay;
use crate::heap_state::HeapReader;
use crate::heap_state::HeapWriter;
use crate::parse::FileEntry;
use crate::parse::MergeHashes;
use crate::parse::PackageParse;
use crate::parse::Parse;
use crate::parse::TypedParse;
use crate::parse::UntypedParse;
use crate::resolved_requires::Dependency;
use crate::resolved_requires::DependencyTarget;
use crate::resolved_requires::ResolvedModule;
use crate::resolved_requires::ResolvedRequires;

pub type BeforeCompact<'a> = &'a mut dyn FnMut();

/// A shared handle to the heap snapshot owned by an [`ActiveTransaction`].
///
/// Cloning this handle does not extend the active lifetime. After the `ActiveTransaction` is
/// dropped, a read-only handle can reacquire the committed heap for an individual read, but it
/// cannot write.
pub struct Transaction {
    heap: Arc<CommittedHeap>,
    overlay: HeapOverlay,
    symlink_paths: flow_common::files::SymlinkMap,
    /// The committed-heap read guard, held for the active lifetime of a unit of work so the
    /// base cannot change underneath it, and `None` after its owner is dropped. Detached reads
    /// take a short-lived guard instead.
    ///
    /// Publishing a transaction needs the matching write guard, so a holder that keeps this
    /// while idle blocks every future commit. Cached IDE artifacts legitimately outlive the
    /// request that built them and keep holding the `Arc<Transaction>`, so the guard — not
    /// the transaction — is what has to be given back. `ActiveTransaction` makes that handoff
    /// structural by releasing the guard when the unit-of-work scope ends.
    committed: RecursiveRwLock<Option<CommittedHeapReadGuard>>,
}

/// Owns the active lifetime of a transaction.
///
/// Dropping this owner releases the committed-heap read guard even if cached artifacts keep
/// `Arc<Transaction>` handles alive. The owner is deliberately not cloneable: code may clone the
/// transaction handle for lazy artifacts, but only the lexical unit of work owns the long-lived
/// guard.
pub struct ActiveTransaction(Option<Arc<Transaction>>);

impl Deref for ActiveTransaction {
    type Target = Transaction;

    fn deref(&self) -> &Self::Target {
        self.0
            .as_deref()
            .expect("an active transaction cannot be used after commit")
    }
}

impl ActiveTransaction {
    pub fn new(heap: Arc<CommittedHeap>) -> Self {
        let guard = heap.read_arc_recursive();
        guard.active_transactions.fetch_add(1, Ordering::AcqRel);
        Self(Some(Arc::new(Transaction {
            heap,
            overlay: HeapOverlay::new(),
            symlink_paths: flow_common::files::SymlinkMap::default(),
            committed: RecursiveRwLock::new(Some(guard)),
        })))
    }

    pub fn handle(&self) -> Arc<Transaction> {
        self.0
            .as_ref()
            .expect("an active transaction cannot be used after commit")
            .dupe()
    }

    /// Publishes the overlay into the heap the transaction was opened on.
    pub fn commit(mut self) {
        let transaction = self
            .0
            .take()
            .expect("an active transaction may only be committed once");
        let destination = transaction.committed_heap();
        transaction.commit(&destination);
    }
}

impl Transaction {
    pub fn symlink_paths(&self) -> &flow_common::files::SymlinkMap {
        &self.symlink_paths
    }
}

impl Drop for ActiveTransaction {
    fn drop(&mut self) {
        if let Some(transaction) = self.0.as_ref() {
            transaction.release();
        }
    }
}

/// Borrows the committed heap for the duration of one read. Produced by
/// [`Transaction::latest_reader`] / [`Transaction::committed_reader`].
pub struct HeapAccess<'a> {
    state: CommittedStateAccess<'a>,
    overlay: Option<&'a HeapOverlay>,
}

impl HeapAccess<'_> {
    pub fn reader(&self) -> HeapReader<'_> {
        match self.overlay {
            Some(overlay) => HeapReader::transactional(&self.state.data, overlay),
            None => HeapReader::committed(&self.state.data),
        }
    }
}

enum CommittedStateAccess<'a> {
    Active(parking_lot::RecursiveRwLockReadGuard<'a, Option<CommittedHeapReadGuard>>),
    Detached(DetachedCommittedState),
}

struct DetachedCommittedState(CommittedHeapReadGuard);

impl DetachedCommittedState {
    fn new(state: CommittedHeapReadGuard) -> Self {
        state.active_transactions.fetch_add(1, Ordering::AcqRel);
        Self(state)
    }
}

impl std::ops::Deref for DetachedCommittedState {
    type Target = crate::heap_state::CommittedHeapState;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for DetachedCommittedState {
    fn drop(&mut self) {
        self.0.active_transactions.fetch_sub(1, Ordering::AcqRel);
    }
}

impl std::ops::Deref for CommittedStateAccess<'_> {
    type Target = crate::heap_state::CommittedHeapState;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Active(slot) => slot
                .as_ref()
                .expect("an active transaction should hold a committed heap guard"),
            Self::Detached(state) => state,
        }
    }
}

pub struct HashStats {
    pub nonempty_slots: i32,
    pub used_slots: i32,
    pub slots: i32,
}

impl CommittedHeap {
    pub fn hash_stats(&self) -> HashStats {
        let heap = self.state.load_full();
        let state = heap.read();
        let file_count = state.data.files.len() as i32;
        let haste_count = state.data.haste_modules.len() as i32;
        Self::hash_stats_from_counts(file_count, haste_count)
    }

    fn hash_stats_from_counts(file_count: i32, haste_count: i32) -> HashStats {
        let used_slots = file_count + haste_count;
        HashStats {
            nonempty_slots: used_slots,
            used_slots,
            slots: used_slots,
        }
    }

    pub fn heap_size(&self) -> i32 {
        let heap = self.state.load_full();
        let state = heap.read();
        (state.data.files.len() + state.data.haste_modules.len()) as i32
    }

    pub fn clear_reader_cache(&self) {
        self.state.load().read().reader_cache.clear();
    }

    pub fn remove_reader_cache_batch(&self, keys: &[FileKey]) {
        self.state.load().read().reader_cache.remove_batch(keys);
    }

    // Saving frees nothing, so unlike a compaction it leaves every heap address a cache may be
    // holding valid. Callers that want their caches emptied around a save do it themselves.
    pub fn save_heap(&self, writer: &mut impl Write) -> io::Result<()> {
        let heap = self.state.load_full();
        let state = heap.read();
        Transaction::write_heap(writer, &state.data)
    }

    /// Builds the heap a dump describes.
    ///
    /// A dump is a whole heap, not an edit to one, so there is no way to read it into a heap that
    /// already exists: it becomes a heap, and whether it is the heap the server runs on is for the
    /// server orchestrator to decide — see [`CommittedHeap::swap_in`].
    pub fn from_heap_dump(reader: &mut impl Read) -> io::Result<Self> {
        Ok(Self::with_data(Transaction::read_heap(reader)?))
    }

    /// Same, for a dump whose file table is stored outside it. See [`CommittedHeap::from_heap_dump`].
    pub fn from_heap_dump_with_file_table(
        reader: &mut impl Read,
        files: Arc<Vec<FileKey>>,
    ) -> io::Result<Self> {
        Ok(Self::with_data(Transaction::read_heap_with_file_table(
            reader, files,
        )?))
    }

    fn should_collect(state: &crate::heap_state::CommittedHeapState, gc_state: &GcState) -> bool {
        if state.data.is_fully_empty() || state.active_transactions.load(Ordering::Acquire) != 0 {
            return false;
        }
        let estimated_garbage = gc_state.free_size.saturating_add(gc_state.new_alloc_size);
        let heap_size = state.data.files.len() + state.data.haste_modules.len();
        estimated_garbage.saturating_mul(5) >= heap_size
    }

    fn should_compact(state: &crate::heap_state::CommittedHeapState, gc_state: &GcState) -> bool {
        let heap_size = state.data.files.len() + state.data.haste_modules.len();
        let scanned_size = heap_size.saturating_sub(gc_state.new_alloc_size);
        gc_state.free_size.saturating_mul(5) >= scanned_size
    }

    fn start_cycle(self: &Arc<Self>, gc_state: &mut GcState) {
        let heap = self.dupe();
        let (sender, receiver) = mpsc::sync_channel(1);
        rayon::spawn(move || {
            let heap = heap.state.load_full();
            let state = heap.read();
            let files = state.data.files.keys().map(Dupe::dupe).collect::<Vec<_>>();
            let haste_modules = state
                .data
                .haste_modules
                .keys()
                .map(Dupe::dupe)
                .collect::<Vec<_>>();
            let _ignored = sender.send(GcMarkKeys {
                files,
                haste_modules,
            });
        });
        gc_state.mark_keys = Some(receiver);
        gc_state.files.clear();
        gc_state.haste_modules.clear();
        gc_state.free_files.clear();
        gc_state.free_haste_modules.clear();
        gc_state.mark_file_index = 0;
        gc_state.mark_haste_index = 0;
        gc_state.sweep_file_index = 0;
        gc_state.sweep_haste_index = 0;
        gc_state.new_alloc_size = 0;
        gc_state.free_size = 0;
        gc_state.phase = GcPhase::Mark;
    }

    fn mark_slice(&self, gc_state: &mut GcState, work: usize) -> usize {
        if let Some(receiver) = gc_state.mark_keys.as_ref() {
            let keys = if work == usize::MAX {
                match receiver.recv() {
                    Ok(keys) => keys,
                    Err(err) => {
                        eprintln!("failed to collect heap keys for GC: {err}");
                        GcMarkKeys {
                            files: Vec::new(),
                            haste_modules: Vec::new(),
                        }
                    }
                }
            } else {
                match receiver.try_recv() {
                    Ok(keys) => keys,
                    Err(mpsc::TryRecvError::Empty) => return 0,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        eprintln!("failed to collect heap keys for GC: channel disconnected");
                        GcMarkKeys {
                            files: Vec::new(),
                            haste_modules: Vec::new(),
                        }
                    }
                }
            };
            gc_state.files = keys.files;
            gc_state.haste_modules = keys.haste_modules;
            gc_state.mark_keys = None;
        }
        let mut work = work;
        if work > 0 && gc_state.mark_file_index < gc_state.files.len() {
            let used = work.min(gc_state.files.len() - gc_state.mark_file_index);
            gc_state.mark_file_index += used;
            work -= used;
        }
        if work > 0 && gc_state.mark_haste_index < gc_state.haste_modules.len() {
            let used = work.min(gc_state.haste_modules.len() - gc_state.mark_haste_index);
            gc_state.mark_haste_index += used;
            work -= used;
        }
        if work > 0
            && gc_state.mark_file_index == gc_state.files.len()
            && gc_state.mark_haste_index == gc_state.haste_modules.len()
        {
            gc_state.phase = GcPhase::Sweep;
            work = 0;
        }
        work
    }

    fn file_entry_is_free(data: &CommittedHeapData, file: &FileKey, entry: &FileEntry) -> bool {
        entry.parse_latest().is_none()
            && entry.get_haste_info().is_none()
            && data
                .file_dependents
                .get(file)
                .is_none_or(|deps| deps.is_empty())
            && entry.get_alternate_file().is_none()
    }

    fn haste_module_is_free(
        data: &CommittedHeapData,
        info: &HasteModuleInfo,
        module: &HasteModule,
    ) -> bool {
        module.get_provider().is_none()
            && data
                .haste_dependents
                .get(info)
                .is_none_or(|deps| deps.is_empty())
            && data
                .haste_provider_candidates
                .get(info)
                .is_none_or(|providers| providers.is_empty())
    }

    fn sweep_slice(data: &CommittedHeapData, gc_state: &mut GcState, work: usize) {
        let mut work = work;
        while work > 0 && gc_state.sweep_file_index < gc_state.files.len() {
            let file = gc_state.files[gc_state.sweep_file_index].dupe();
            if let Some(entry) = data.files.get(&file)
                && Self::file_entry_is_free(data, &file, entry)
            {
                gc_state.free_size = gc_state.free_size.saturating_add(1);
                gc_state.free_files.push(file);
            }
            gc_state.sweep_file_index += 1;
            work -= 1;
        }
        while work > 0 && gc_state.sweep_haste_index < gc_state.haste_modules.len() {
            let info = gc_state.haste_modules[gc_state.sweep_haste_index].dupe();
            if let Some(module) = data.haste_modules.get(&info)
                && Self::haste_module_is_free(data, &info, module)
            {
                gc_state.free_size = gc_state.free_size.saturating_add(1);
                gc_state.free_haste_modules.push(info);
            }
            gc_state.sweep_haste_index += 1;
            work -= 1;
        }
        if gc_state.sweep_file_index == gc_state.files.len()
            && gc_state.sweep_haste_index == gc_state.haste_modules.len()
        {
            gc_state.phase = GcPhase::Idle;
            gc_state.files.clear();
            gc_state.haste_modules.clear();
            gc_state.sweep_file_index = 0;
            gc_state.sweep_haste_index = 0;
        }
    }

    fn compact_helper(
        self: &Arc<Self>,
        free_files: Vec<FileKey>,
        free_haste_modules: Vec<HasteModuleInfo>,
        before_compact: BeforeCompact<'_>,
    ) {
        if free_files.is_empty() && free_haste_modules.is_empty() {
            return;
        }
        before_compact();
        let heap = self.state.load_full();
        let mut state = heap.write();
        let data = &mut state.data;
        for file in free_files {
            let should_remove = data
                .files
                .get(&file)
                .is_some_and(|entry| Self::file_entry_is_free(data, &file, entry));
            if should_remove {
                data.files.remove(&file);
                data.file_dependents.remove(&file);
            }
        }
        for info in free_haste_modules {
            let should_remove = data
                .haste_modules
                .get(&info)
                .is_some_and(|module| Self::haste_module_is_free(data, &info, module));
            if should_remove {
                data.haste_modules.remove(&info);
                data.haste_dependents.remove(&info);
                data.haste_provider_candidates.remove(&info);
            }
        }
        state.reader_cache.clear();
        let mut gc_state = state.gc_state.lock();
        gc_state.free_size = 0;
        gc_state.new_alloc_size = 0;
        gc_state.free_files.clear();
        gc_state.free_haste_modules.clear();
    }

    pub fn collect_slice(self: &Arc<Self>, work: usize, before_compact: BeforeCompact<'_>) -> bool {
        self.collect_slice_with_force(false, work, before_compact)
    }

    fn collect_slice_with_force(
        self: &Arc<Self>,
        force: bool,
        work: usize,
        before_compact: BeforeCompact<'_>,
    ) -> bool {
        let heap = self.state.load_full();
        if heap.read().active_transactions.load(Ordering::Acquire) != 0 {
            return true;
        }
        let mut work = work;
        while work > 0 {
            let state = heap.read();
            let mut gc_state = state.gc_state.lock();
            match gc_state.phase {
                GcPhase::Idle => {
                    if force || Self::should_collect(&state, &gc_state) {
                        self.start_cycle(&mut gc_state);
                    } else {
                        work = 0;
                    }
                }
                GcPhase::Mark => work = self.mark_slice(&mut gc_state, work),
                GcPhase::Sweep => {
                    Self::sweep_slice(&state.data, &mut gc_state, work);
                    work = 0;
                }
            }
        }
        let (is_idle, compact_files, compact_haste_modules) = {
            let state = heap.read();
            let mut gc_state = state.gc_state.lock();
            let is_idle = gc_state.phase == GcPhase::Idle;
            let should_compact = is_idle && Self::should_compact(&state, &gc_state);
            let files = if should_compact {
                std::mem::take(&mut gc_state.free_files)
            } else {
                Vec::new()
            };
            let haste_modules = if should_compact {
                std::mem::take(&mut gc_state.free_haste_modules)
            } else {
                Vec::new()
            };
            (is_idle, files, haste_modules)
        };
        if !compact_files.is_empty() || !compact_haste_modules.is_empty() {
            self.compact_helper(compact_files, compact_haste_modules, before_compact);
        }
        is_idle
    }

    pub fn collect_full(self: &Arc<Self>, before_compact: BeforeCompact<'_>) {
        while !self.collect_slice_with_force(true, usize::MAX, before_compact) {}
    }

    fn finish_cycle(self: &Arc<Self>) {
        let heap = self.state.load_full();
        loop {
            let state = heap.read();
            let mut gc_state = state.gc_state.lock();
            if gc_state.phase != GcPhase::Mark {
                break;
            }
            self.mark_slice(&mut gc_state, usize::MAX);
        }
        loop {
            let state = heap.read();
            let mut gc_state = state.gc_state.lock();
            if gc_state.phase != GcPhase::Sweep {
                break;
            }
            Self::sweep_slice(&state.data, &mut gc_state, usize::MAX);
        }
    }

    pub fn compact(self: &Arc<Self>, before_compact: BeforeCompact<'_>) {
        let heap = self.state.load_full();
        assert_eq!(
            heap.read().active_transactions.load(Ordering::Acquire),
            0,
            "cannot compact while a transaction is active"
        );
        self.finish_cycle();
        {
            let state = heap.read();
            let mut gc_state = state.gc_state.lock();
            self.start_cycle(&mut gc_state);
        }
        self.finish_cycle();
        let (files, haste_modules) = {
            let state = heap.read();
            let mut gc_state = state.gc_state.lock();
            (
                std::mem::take(&mut gc_state.free_files),
                std::mem::take(&mut gc_state.free_haste_modules),
            )
        };
        self.compact_helper(files, haste_modules, before_compact);
    }
}

pub(crate) const GC_MAP_SHARDS: usize = 256;
const HEAP_MAGIC_RUST_SHARDED_INDEXED_LZ4_EXTERNAL_FILES: u64 = 0x464C4F57525A5337; // "FLOWRZS7"
const HEAP_MAGIC_RUST_SHARDED_LOCAL_INDEXED_LZ4: u64 = 0x464C4F57525A5338; // "FLOWRZS8"

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GcPhase {
    Idle,
    Mark,
    Sweep,
}

#[derive(Debug)]
struct GcMarkKeys {
    files: Vec<FileKey>,
    haste_modules: Vec<HasteModuleInfo>,
}

#[derive(Debug)]
pub(crate) struct GcState {
    pub(crate) phase: GcPhase,
    pub(crate) files: Vec<FileKey>,
    pub(crate) haste_modules: Vec<HasteModuleInfo>,
    pub(crate) free_files: Vec<FileKey>,
    pub(crate) free_haste_modules: Vec<HasteModuleInfo>,
    mark_keys: Option<mpsc::Receiver<GcMarkKeys>>,
    pub(crate) mark_file_index: usize,
    pub(crate) mark_haste_index: usize,
    pub(crate) sweep_file_index: usize,
    pub(crate) sweep_haste_index: usize,
    pub(crate) new_alloc_size: usize,
    pub(crate) free_size: usize,
}

impl Default for GcState {
    fn default() -> Self {
        Self {
            phase: GcPhase::Idle,
            files: Vec::new(),
            haste_modules: Vec::new(),
            free_files: Vec::new(),
            free_haste_modules: Vec::new(),
            mark_keys: None,
            mark_file_index: 0,
            mark_haste_index: 0,
            sweep_file_index: 0,
            sweep_haste_index: 0,
            new_alloc_size: 0,
            free_size: 0,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SerializedHeapHeader {
    magic: u64,
    file_count: u64,
    haste_module_count: u64,
    file_shard_count: u64,
    haste_module_shard_count: u64,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SerializedFileEntry {
    parse: Option<SerializedParse>,
    haste_info: Option<HasteModuleInfo>,
    dependents: Option<Vec<u32>>,
    alternate_file: Option<u32>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SerializedFileHeapShard {
    files: Vec<FileKey>,
    entries: Vec<(u32, SerializedFileEntry)>,
}

#[derive(serde::Serialize, serde::Deserialize)]
enum SerializedParse {
    Typed(SerializedTypedParse),
    Untyped {
        file_hash: u64,
    },
    Package {
        file_hash: u64,
        package_info: PackageJson,
    },
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SerializedTypedParse {
    file_hash: u64,
    dts_file_kind: Option<flow_parser::dts_file_kind::DtsFileKind>,
    has_ts_global_augmentation: bool,
    ast: Option<Vec<u8>>,
    docblock: Option<Vec<u8>>,
    aloc_table: Option<Vec<u8>>,
    type_sig: Option<Vec<u8>>,
    file_sig: Option<Vec<u8>>,
    exports: Vec<u8>,
    requires: Vec<FlowImportSpecifier>,
    resolved_requires: SerializedResolvedRequires,
    imports: Vec<u8>,
    leader: Option<u32>,
    sig_hash: Option<u64>,
    merge_hashes: Option<MergeHashes>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SerializedResolvedRequires {
    resolved_modules: Vec<SerializedResolvedModule>,
    phantom_dependencies: Vec<SerializedDependency>,
}

#[derive(serde::Serialize, serde::Deserialize)]
enum SerializedResolvedModule {
    HasteModule(Modulename),
    File(u32),
    String(FlowImportSpecifier),
    Null,
}

#[derive(serde::Serialize, serde::Deserialize)]
enum SerializedDependency {
    HasteModule(Modulename),
    File(u32),
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SerializedHasteModule {
    provider: Option<u32>,
    dependents: Vec<u32>,
    all_providers: Vec<u32>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SerializedHasteModuleHeapShard {
    files: Vec<FileKey>,
    entries: Vec<(HasteModuleInfo, SerializedHasteModule)>,
}

struct LoadedFileHeapShard {
    files: HashMap<FileKey, FileEntry>,
    dependents: HashMap<FileKey, Vec<FileKey>>,
}

struct LoadedHasteModuleHeapShard {
    modules: HashMap<HasteModuleInfo, HasteModule>,
    dependents: HashMap<HasteModuleInfo, Vec<FileKey>>,
    provider_candidates: HashMap<HasteModuleInfo, Vec<FileKey>>,
}

fn encode_into_writer<T: serde::Serialize>(writer: &mut impl Write, value: &T) -> io::Result<()> {
    bincode::serde::encode_into_std_write(value, writer, bincode::config::legacy())
        .map(|_| ())
        .map_err(io::Error::other)
}

fn decode_from_reader<T: serde::de::DeserializeOwned>(reader: &mut impl Read) -> io::Result<T> {
    bincode::serde::decode_from_std_read(reader, bincode::config::legacy())
        .map_err(io::Error::other)
}

fn encode_to_vec<T: serde::Serialize>(value: &T) -> io::Result<Vec<u8>> {
    bincode::serde::encode_to_vec(value, bincode::config::legacy()).map_err(io::Error::other)
}

fn decode_from_slice<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> io::Result<T> {
    bincode::serde::decode_from_slice(bytes, bincode::config::legacy())
        .map(|(value, _)| value)
        .map_err(io::Error::other)
}

fn write_u64(writer: &mut impl Write, value: u64) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

fn read_u64(reader: &mut impl Read) -> io::Result<u64> {
    let mut bytes = [0; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn write_compressed_block(writer: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    let compressed = lz4_flex::compress_prepend_size(bytes);
    write_u64(writer, compressed.len() as u64)?;
    writer.write_all(&compressed)
}

fn read_compressed_block(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let compressed_len = read_u64(reader)? as usize;
    let mut compressed = vec![0; compressed_len];
    reader.read_exact(&mut compressed)?;
    Ok(compressed)
}

fn decompress_block(compressed: &[u8]) -> io::Result<Vec<u8>> {
    lz4_flex::decompress_size_prepended(compressed).map_err(io::Error::other)
}

impl Transaction {
    /// Give the long-lived committed-heap read guard back so a commit can proceed.
    fn release(&self) {
        if let Some(state) = self.committed.write().take() {
            state.active_transactions.fetch_sub(1, Ordering::AcqRel);
        }
    }

    fn committed_state(&self) -> CommittedStateAccess<'_> {
        let active = self.committed.read();
        if active.is_some() {
            return CommittedStateAccess::Active(active);
        }
        drop(active);

        assert!(
            self.overlay.is_empty(),
            "a released transaction with an overlay cannot be read"
        );
        CommittedStateAccess::Detached(DetachedCommittedState::new(self.heap.read_arc_recursive()))
    }

    pub(crate) fn with_committed_state<R>(
        &self,
        f: impl FnOnce(&crate::heap_state::CommittedHeapState) -> R,
    ) -> R {
        let state = self.committed_state();
        f(&state)
    }

    pub(crate) fn with_reader_cache<R>(
        &self,
        f: impl FnOnce(&flow_heap_serialization::ReaderCache) -> R,
    ) -> R {
        let state = self.committed_state();
        f(&state.reader_cache)
    }

    pub(crate) fn latest_reader(&self) -> HeapAccess<'_> {
        HeapAccess {
            state: self.committed_state(),
            overlay: Some(&self.overlay),
        }
    }

    pub(crate) fn committed_reader(&self) -> HeapAccess<'_> {
        HeapAccess {
            state: self.committed_state(),
            overlay: None,
        }
    }

    /// Returns the committed base without publishing the active transaction overlay.
    pub fn committed_heap(&self) -> Arc<CommittedHeap> {
        self.heap.dupe()
    }

    pub(crate) fn heap_writer(&self) -> HeapWrite<'_> {
        HeapWrite {
            slot: self.committed.read(),
            overlay: &self.overlay,
        }
    }

    pub fn hash_stats(&self) -> HashStats {
        let (file_count, haste_count) = self.latest_counts();
        let used_slots = file_count + haste_count;
        HashStats {
            nonempty_slots: used_slots,
            used_slots,
            slots: used_slots,
        }
    }

    pub fn heap_size(&self) -> i32 {
        let (file_count, haste_count) = self.latest_counts();
        file_count + haste_count
    }

    fn latest_counts(&self) -> (i32, i32) {
        self.with_committed_state(|state| {
            (
                self.overlay.file_count_over(&state.data.files) as i32,
                self.overlay
                    .haste_module_count_over(&state.data.haste_modules) as i32,
            )
        })
    }

    pub fn snapshot_latest_heap(&self) -> CommittedHeap {
        let heap = self.clone_base_heap();
        heap.apply_overlay(&self.overlay);
        heap
    }

    fn clone_base_heap(&self) -> CommittedHeap {
        self.with_committed_state(|state| self.clone_base_heap_from(&state.data))
    }

    fn clone_base_heap_from(&self, base: &CommittedHeapData) -> CommittedHeap {
        let files_len = base.files.len();
        let haste_modules_len = base.haste_modules.len();
        let heap = CommittedHeap::with_capacity(files_len, haste_modules_len);
        {
            let state = heap.state.load_full();
            let mut state = state.write();
            let data = &mut state.data;
            for (file, entry) in base.files.iter() {
                data.files.insert(file.dupe(), entry.dupe());
            }
            for (info, module) in base.haste_modules.iter() {
                data.haste_modules.insert(info.dupe(), module.dupe());
            }
            data.file_dependents = base.file_dependents.clone();
            data.haste_dependents = base.haste_dependents.clone();
            data.haste_provider_candidates = base.haste_provider_candidates.clone();
        }
        heap
    }

    pub(crate) fn note_alloc(&self) {
        self.note_alloc_many(1);
    }

    pub(crate) fn note_alloc_many(&self, count: usize) {
        if count == 0 {
            return;
        }

        self.with_committed_state(|state| {
            let mut gc_state = state.gc_state.lock();
            gc_state.new_alloc_size = gc_state.new_alloc_size.saturating_add(count);
        });
    }

    fn file_index(file_to_index: &BTreeMap<FileKey, u32>, file: &FileKey) -> u32 {
        *file_to_index
            .get(file)
            .expect("file should have been collected in heap file table")
    }

    fn file_from_index(files: &[FileKey], index: u32) -> io::Result<FileKey> {
        files
            .get(index as usize)
            .map(Dupe::dupe)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid file index"))
    }

    fn collect_dependency_file_keys(dependency: &Dependency, file_keys: &mut BTreeSet<FileKey>) {
        match dependency.target() {
            DependencyTarget::HasteModule(_) => {}
            DependencyTarget::File(file) => {
                file_keys.insert(file.dupe());
            }
        }
    }

    fn collect_resolved_module_file_keys(
        module: &crate::resolved_requires::ResolvedModule,
        file_keys: &mut BTreeSet<FileKey>,
    ) {
        if let Some(dependency) = module.as_dependency() {
            Self::collect_dependency_file_keys(&dependency, file_keys);
        }
    }

    fn collect_resolved_requires_file_keys(
        resolved_requires: Option<ResolvedRequires>,
        file_keys: &mut BTreeSet<FileKey>,
    ) {
        if let Some(resolved_requires) = resolved_requires {
            for module in resolved_requires.get_resolved_modules() {
                Self::collect_resolved_module_file_keys(module, file_keys);
            }
            for dependency in resolved_requires.get_phantom_dependencies() {
                Self::collect_dependency_file_keys(dependency, file_keys);
            }
        }
    }

    fn collect_parse_file_keys(parse: &Parse, file_keys: &mut BTreeSet<FileKey>) {
        match parse {
            Parse::Typed(typed) => {
                Self::collect_resolved_requires_file_keys(
                    typed.resolved_requires.dupe(),
                    file_keys,
                );
                if let Some(leader) = typed.leader.dupe() {
                    file_keys.insert(leader);
                }
            }
            Parse::Untyped(_) | Parse::Package(_) => {}
        }
    }

    fn collect_file_entry_file_keys(
        file_entry: Option<&FileEntry>,
        dependents: Option<&[FileKey]>,
        file_keys: &mut BTreeSet<FileKey>,
    ) {
        if let Some(dependents) = dependents {
            file_keys.extend(dependents.iter().map(Dupe::dupe));
        }
        if let Some(file_entry) = file_entry {
            if let Some(parse) = file_entry.parse_latest() {
                Self::collect_parse_file_keys(&parse, file_keys);
            }
            if let Some(alternate_file) = file_entry.get_alternate_file() {
                file_keys.insert(alternate_file);
            }
        }
    }

    fn collect_haste_module_file_keys(
        haste_module: &HasteModule,
        dependents: Option<&[FileKey]>,
        provider_candidates: Option<&[FileKey]>,
        file_keys: &mut BTreeSet<FileKey>,
    ) {
        if let Some(provider) = haste_module.get_provider() {
            file_keys.insert(provider);
        }
        if let Some(dependents) = dependents {
            file_keys.extend(dependents.iter().map(Dupe::dupe));
        }
        if let Some(provider_candidates) = provider_candidates {
            file_keys.extend(provider_candidates.iter().map(Dupe::dupe));
        }
    }

    /// The heap this transaction reads, as a file table for a saved-state dump.
    ///
    /// Serializing goes through the transaction rather than the heap handle so that the dump and
    /// the `Env` being written alongside it describe the same heap. Reading the handle instead
    /// would pick up whatever it holds at that moment, which a saved-state load may since have
    /// swapped. It is a read of the state the transaction already holds, so it copies nothing.
    pub fn collect_heap_file_table(&self) -> Vec<FileKey> {
        self.with_committed_state(|state| Self::heap_file_table(&state.data))
    }

    /// Writes the heap this transaction reads. See [`Transaction::collect_heap_file_table`].
    pub fn save_heap_with_file_table(
        &self,
        writer: &mut impl Write,
        files: &[FileKey],
    ) -> io::Result<()> {
        self.with_committed_state(|state| {
            Self::write_heap_with_file_table(writer, &state.data, files)
        })
    }

    pub(crate) fn heap_file_table(data: &CommittedHeapData) -> Vec<FileKey> {
        let mut file_keys = BTreeSet::new();
        for (file, entry) in data.files.iter() {
            file_keys.insert(file.dupe());
            Self::collect_file_entry_file_keys(
                Some(entry),
                data.file_dependents
                    .get(file)
                    .map(|values| values.as_slice()),
                &mut file_keys,
            );
        }
        for (owner, dependents) in data.file_dependents.iter() {
            file_keys.insert(owner.dupe());
            file_keys.extend(dependents.iter().map(Dupe::dupe));
        }
        for (info, module) in data.haste_modules.iter() {
            Self::collect_haste_module_file_keys(
                module,
                data.haste_dependents
                    .get(info)
                    .map(|values| values.as_slice()),
                data.haste_provider_candidates
                    .get(info)
                    .map(|values| values.as_slice()),
                &mut file_keys,
            );
        }
        file_keys.into_iter().collect()
    }

    fn file_to_index(files: &[FileKey]) -> BTreeMap<FileKey, u32> {
        files
            .iter()
            .enumerate()
            .map(|(index, file)| {
                (
                    file.dupe(),
                    u32::try_from(index).expect("heap file table index should fit in u32"),
                )
            })
            .collect()
    }

    fn shard_index<K: Hash>(key: &K) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish() as usize % GC_MAP_SHARDS
    }

    fn sharded_file_entries(data: &CommittedHeapData) -> Vec<Vec<(FileKey, Option<FileEntry>)>> {
        let mut shards = (0..GC_MAP_SHARDS).map(|_| Vec::new()).collect::<Vec<_>>();
        let mut entries = BTreeMap::new();
        for (file, entry) in data.files.iter() {
            entries.insert(file.dupe(), Some(entry.dupe()));
        }
        for owner in data.file_dependents.keys() {
            entries.entry(owner.dupe()).or_insert(None);
        }
        for (file, entry) in entries {
            shards[Self::shard_index(&file)].push((file, entry));
        }
        shards
    }

    fn sharded_haste_modules(data: &CommittedHeapData) -> Vec<Vec<(HasteModuleInfo, HasteModule)>> {
        let mut shards = (0..GC_MAP_SHARDS).map(|_| Vec::new()).collect::<Vec<_>>();
        for (info, module) in data.haste_modules.iter() {
            shards[Self::shard_index(info)].push((info.dupe(), module.dupe()));
        }
        for shard in &mut shards {
            shard.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
        }
        shards
    }

    pub(crate) fn write_heap(
        writer: &mut impl Write,
        data: &CommittedHeapData,
    ) -> std::io::Result<()> {
        let file_shards = Self::sharded_file_entries(data);
        let header = SerializedHeapHeader {
            magic: HEAP_MAGIC_RUST_SHARDED_LOCAL_INDEXED_LZ4,
            file_count: file_shards.iter().map(Vec::len).sum::<usize>() as u64,
            haste_module_count: data.haste_modules.len() as u64,
            file_shard_count: GC_MAP_SHARDS as u64,
            haste_module_shard_count: GC_MAP_SHARDS as u64,
        };
        encode_into_writer(writer, &header)?;
        for shard in file_shards {
            let mut files = BTreeSet::new();
            for (file, entry) in shard.iter() {
                files.insert(file.dupe());
                Self::collect_file_entry_file_keys(
                    entry.as_ref(),
                    data.file_dependents
                        .get(file)
                        .map(|values| values.as_slice()),
                    &mut files,
                );
            }
            let files = files.into_iter().collect::<Vec<_>>();
            let file_to_index = Self::file_to_index(&files);
            let entries = shard
                .iter()
                .map(|(file, entry)| {
                    (
                        Self::file_index(&file_to_index, file),
                        Self::serialized_file_entry(
                            entry.as_ref(),
                            data.file_dependents
                                .get(file)
                                .map(|values| values.as_slice()),
                            &file_to_index,
                        ),
                    )
                })
                .collect();
            let shard = SerializedFileHeapShard { files, entries };
            let bytes = encode_to_vec(&shard)?;
            write_compressed_block(writer, &bytes)?;
        }
        for shard in Self::sharded_haste_modules(data) {
            let mut files = BTreeSet::new();
            for (info, module) in shard.iter() {
                Self::collect_haste_module_file_keys(
                    module,
                    data.haste_dependents
                        .get(info)
                        .map(|values| values.as_slice()),
                    data.haste_provider_candidates
                        .get(info)
                        .map(|values| values.as_slice()),
                    &mut files,
                );
            }
            let files = files.into_iter().collect::<Vec<_>>();
            let file_to_index = Self::file_to_index(&files);
            let entries = shard
                .iter()
                .map(|(info, module)| {
                    (
                        info.dupe(),
                        Self::serialized_haste_module(
                            module,
                            data.haste_dependents
                                .get(info)
                                .map(|values| values.as_slice()),
                            data.haste_provider_candidates
                                .get(info)
                                .map(|values| values.as_slice()),
                            &file_to_index,
                        ),
                    )
                })
                .collect();
            let shard = SerializedHasteModuleHeapShard { files, entries };
            let bytes = encode_to_vec(&shard)?;
            write_compressed_block(writer, &bytes)?;
        }
        Ok(())
    }

    pub(crate) fn write_heap_with_file_table(
        writer: &mut impl Write,
        data: &CommittedHeapData,
        files: &[FileKey],
    ) -> std::io::Result<()> {
        let file_shards = Self::sharded_file_entries(data);
        let header = SerializedHeapHeader {
            magic: HEAP_MAGIC_RUST_SHARDED_INDEXED_LZ4_EXTERNAL_FILES,
            file_count: file_shards.iter().map(Vec::len).sum::<usize>() as u64,
            haste_module_count: data.haste_modules.len() as u64,
            file_shard_count: GC_MAP_SHARDS as u64,
            haste_module_shard_count: GC_MAP_SHARDS as u64,
        };
        let file_to_index = Self::file_to_index(files);
        encode_into_writer(writer, &header)?;
        for shard in file_shards {
            let entries = shard
                .iter()
                .map(|(file, entry)| {
                    (
                        Self::file_index(&file_to_index, file),
                        Self::serialized_file_entry(
                            entry.as_ref(),
                            data.file_dependents
                                .get(file)
                                .map(|values| values.as_slice()),
                            &file_to_index,
                        ),
                    )
                })
                .collect();
            let shard = SerializedFileHeapShard {
                files: Vec::new(),
                entries,
            };
            let bytes = encode_to_vec(&shard)?;
            write_compressed_block(writer, &bytes)?;
        }
        for shard in Self::sharded_haste_modules(data) {
            let entries = shard
                .iter()
                .map(|(info, module)| {
                    (
                        info.dupe(),
                        Self::serialized_haste_module(
                            module,
                            data.haste_dependents
                                .get(info)
                                .map(|values| values.as_slice()),
                            data.haste_provider_candidates
                                .get(info)
                                .map(|values| values.as_slice()),
                            &file_to_index,
                        ),
                    )
                })
                .collect();
            let shard = SerializedHasteModuleHeapShard {
                files: Vec::new(),
                entries,
            };
            let bytes = encode_to_vec(&shard)?;
            write_compressed_block(writer, &bytes)?;
        }
        Ok(())
    }

    pub(crate) fn read_heap(reader: &mut impl Read) -> std::io::Result<CommittedHeapData> {
        let header: SerializedHeapHeader = decode_from_reader(reader)?;
        Self::validate_serialized_heap_header(&header, HEAP_MAGIC_RUST_SHARDED_LOCAL_INDEXED_LZ4)?;
        Self::read_heap_shards(reader, None)
    }

    pub(crate) fn read_heap_with_file_table(
        reader: &mut impl Read,
        files: Arc<Vec<FileKey>>,
    ) -> std::io::Result<CommittedHeapData> {
        let header: SerializedHeapHeader = decode_from_reader(reader)?;
        if header.magic != HEAP_MAGIC_RUST_SHARDED_LOCAL_INDEXED_LZ4
            && header.magic != HEAP_MAGIC_RUST_SHARDED_INDEXED_LZ4_EXTERNAL_FILES
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "hh_load_heap: invalid magic number",
            ));
        }
        if header.file_shard_count != GC_MAP_SHARDS as u64
            || header.haste_module_shard_count != GC_MAP_SHARDS as u64
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "hh_load_heap: invalid shard count",
            ));
        }
        let files = if header.magic == HEAP_MAGIC_RUST_SHARDED_INDEXED_LZ4_EXTERNAL_FILES {
            Some(files)
        } else {
            None
        };
        Self::read_heap_shards(reader, files)
    }

    fn read_heap_shards(
        reader: &mut impl Read,
        external_files: Option<Arc<Vec<FileKey>>>,
    ) -> std::io::Result<CommittedHeapData> {
        let mut file_shards = Vec::with_capacity(GC_MAP_SHARDS);
        for _ in 0..GC_MAP_SHARDS {
            file_shards.push(read_compressed_block(reader)?);
        }
        let mut haste_module_shards = Vec::with_capacity(GC_MAP_SHARDS);
        for _ in 0..GC_MAP_SHARDS {
            haste_module_shards.push(read_compressed_block(reader)?);
        }
        let external_files = external_files.as_deref().map(|files| files.as_slice());
        let (file_result, haste_result) = rayon::join(
            || {
                file_shards
                    .into_par_iter()
                    .map(|bytes| Self::load_file_heap_shard(bytes, external_files))
                    .collect::<io::Result<Vec<_>>>()
            },
            || {
                haste_module_shards
                    .into_par_iter()
                    .map(|bytes| Self::load_haste_heap_shard(bytes, external_files))
                    .collect::<io::Result<Vec<_>>>()
            },
        );
        let file_result = file_result?;
        let haste_result = haste_result?;

        // The committed maps are one map per kind, so the per-shard results have to be
        // merged into them one entry at a time. The file side and the haste side are
        // disjoint, so at least run those two merges against each other.
        let ((files, file_dependents), (haste_modules, haste_dependents, provider_candidates)) =
            rayon::join(
                || {
                    let mut files =
                        commit_map_with_capacity(file_result.iter().map(|s| s.files.len()).sum());
                    let mut file_dependents = HashMap::with_capacity(
                        file_result.iter().map(|s| s.dependents.len()).sum(),
                    );
                    for shard in file_result {
                        files.extend(shard.files);
                        file_dependents.extend(
                            shard
                                .dependents
                                .into_iter()
                                .map(|(owner, dependents)| (owner, Arc::new(dependents))),
                        );
                    }
                    (files, file_dependents)
                },
                || {
                    let mut haste_modules = commit_map_with_capacity(
                        haste_result.iter().map(|s| s.modules.len()).sum(),
                    );
                    let mut haste_dependents = HashMap::with_capacity(
                        haste_result.iter().map(|s| s.dependents.len()).sum(),
                    );
                    let mut provider_candidates = HashMap::with_capacity(
                        haste_result
                            .iter()
                            .map(|s| s.provider_candidates.len())
                            .sum(),
                    );
                    for shard in haste_result {
                        haste_modules.extend(shard.modules);
                        haste_dependents.extend(
                            shard
                                .dependents
                                .into_iter()
                                .map(|(owner, dependents)| (owner, Arc::new(dependents))),
                        );
                        provider_candidates.extend(
                            shard
                                .provider_candidates
                                .into_iter()
                                .map(|(owner, providers)| (owner, Arc::new(providers))),
                        );
                    }
                    (haste_modules, haste_dependents, provider_candidates)
                },
            );

        Ok(CommittedHeapData {
            files,
            haste_modules,
            file_dependents,
            haste_dependents,
            haste_provider_candidates: provider_candidates,
        })
    }

    fn serialized_resolved_module(
        module: &ResolvedModule,
        file_to_index: &BTreeMap<FileKey, u32>,
    ) -> SerializedResolvedModule {
        match module.to_result() {
            Ok(dependency) => match dependency.target() {
                DependencyTarget::HasteModule(info) => {
                    SerializedResolvedModule::HasteModule(Modulename::Haste(info.dupe()))
                }
                DependencyTarget::File(file) => {
                    SerializedResolvedModule::File(Self::file_index(file_to_index, file))
                }
            },
            Err(Some(specifier)) => SerializedResolvedModule::String(specifier),
            Err(None) => SerializedResolvedModule::Null,
        }
    }

    fn serialized_dependency(
        dependency: &Dependency,
        file_to_index: &BTreeMap<FileKey, u32>,
    ) -> SerializedDependency {
        match dependency.target() {
            DependencyTarget::HasteModule(info) => {
                SerializedDependency::HasteModule(Modulename::Haste(info.dupe()))
            }
            DependencyTarget::File(file) => {
                SerializedDependency::File(Self::file_index(file_to_index, file))
            }
        }
    }

    fn serialized_resolved_requires(
        resolved_requires: Option<ResolvedRequires>,
        file_to_index: &BTreeMap<FileKey, u32>,
    ) -> SerializedResolvedRequires {
        match resolved_requires {
            Some(resolved_requires) => SerializedResolvedRequires {
                resolved_modules: resolved_requires
                    .get_resolved_modules()
                    .iter()
                    .map(|module| Self::serialized_resolved_module(module, file_to_index))
                    .collect(),
                phantom_dependencies: resolved_requires
                    .get_phantom_dependencies()
                    .iter()
                    .map(|dependency| Self::serialized_dependency(dependency, file_to_index))
                    .collect(),
            },
            None => SerializedResolvedRequires {
                resolved_modules: Vec::new(),
                phantom_dependencies: Vec::new(),
            },
        }
    }

    fn serialized_parse(parse: &Parse, file_to_index: &BTreeMap<FileKey, u32>) -> SerializedParse {
        match parse {
            Parse::Typed(typed) => SerializedParse::Typed(SerializedTypedParse {
                file_hash: typed.file_hash,
                dts_file_kind: typed.dts_file_kind,
                has_ts_global_augmentation: typed.has_ts_global_augmentation,
                ast: None,
                docblock: None,
                aloc_table: None,
                type_sig: None,
                file_sig: None,
                exports: typed.exports.as_ref().to_vec(),
                requires: typed.requires.as_ref().to_vec(),
                resolved_requires: Self::serialized_resolved_requires(
                    typed.resolved_requires.dupe(),
                    file_to_index,
                ),
                imports: typed.imports.as_ref().to_vec(),
                leader: None,
                sig_hash: None,
                merge_hashes: typed
                    .merge_hashes
                    .as_ref()
                    .map(|hashes| hashes.as_ref().clone()),
            }),
            Parse::Untyped(untyped) => SerializedParse::Untyped {
                file_hash: untyped.file_hash,
            },
            Parse::Package(package) => SerializedParse::Package {
                file_hash: package.file_hash,
                package_info: package.package_info.as_ref().clone(),
            },
        }
    }

    fn serialized_file_entry(
        file_entry: Option<&FileEntry>,
        dependents: Option<&[FileKey]>,
        file_to_index: &BTreeMap<FileKey, u32>,
    ) -> SerializedFileEntry {
        let parse = file_entry.and_then(FileEntry::parse_latest);
        SerializedFileEntry {
            parse: parse
                .as_ref()
                .map(|parse| Self::serialized_parse(parse, file_to_index)),
            haste_info: file_entry.and_then(FileEntry::get_haste_info),
            dependents: dependents.map(|files| Self::sorted_file_indexes(files, file_to_index)),
            alternate_file: file_entry
                .and_then(FileEntry::get_alternate_file)
                .map(|file| Self::file_index(file_to_index, &file)),
        }
    }

    fn serialized_haste_module(
        haste_module: &HasteModule,
        dependents: Option<&[FileKey]>,
        provider_candidates: Option<&[FileKey]>,
        file_to_index: &BTreeMap<FileKey, u32>,
    ) -> SerializedHasteModule {
        SerializedHasteModule {
            provider: haste_module
                .get_provider()
                .map(|file| Self::file_index(file_to_index, &file)),
            dependents: dependents
                .map(|files| Self::sorted_file_indexes(files, file_to_index))
                .unwrap_or_default(),
            all_providers: provider_candidates
                .map(|files| Self::sorted_file_indexes(files, file_to_index))
                .unwrap_or_default(),
        }
    }

    fn resolved_module_from_serialized(
        serialized: SerializedResolvedModule,
        files: &[FileKey],
    ) -> io::Result<ResolvedModule> {
        match serialized {
            SerializedResolvedModule::HasteModule(module) => Ok(ResolvedModule::dependency(
                Dependency::from_modulename(module),
            )),
            SerializedResolvedModule::File(file) => Ok(ResolvedModule::dependency(
                Dependency::file(Self::file_from_index(files, file)?),
            )),
            SerializedResolvedModule::String(specifier) => Ok(ResolvedModule::string(specifier)),
            SerializedResolvedModule::Null => Ok(ResolvedModule::null()),
        }
    }

    fn dependency_from_serialized(
        serialized: SerializedDependency,
        files: &[FileKey],
    ) -> io::Result<Dependency> {
        match serialized {
            SerializedDependency::HasteModule(module) => Ok(Dependency::from_modulename(module)),
            SerializedDependency::File(file) => {
                Ok(Dependency::file(Self::file_from_index(files, file)?))
            }
        }
    }

    fn resolved_requires_from_serialized(
        serialized: SerializedResolvedRequires,
        files: &[FileKey],
    ) -> io::Result<ResolvedRequires> {
        Ok(ResolvedRequires::new(
            serialized
                .resolved_modules
                .into_iter()
                .map(|module| Self::resolved_module_from_serialized(module, files))
                .collect::<io::Result<Vec<_>>>()?,
            serialized
                .phantom_dependencies
                .into_iter()
                .map(|dependency| Self::dependency_from_serialized(dependency, files))
                .collect::<io::Result<Vec<_>>>()?,
        ))
    }

    fn parse_from_serialized(serialized: SerializedParse, files: &[FileKey]) -> io::Result<Parse> {
        match serialized {
            SerializedParse::Typed(typed) => Ok(Parse::Typed(TypedParse {
                file_hash: typed.file_hash,
                dts_file_kind: typed.dts_file_kind,
                has_ts_global_augmentation: typed.has_ts_global_augmentation,
                ast: typed.ast.map(|bytes| Arc::from(bytes.into_boxed_slice())),
                docblock: typed
                    .docblock
                    .map(|bytes| Arc::from(bytes.into_boxed_slice())),
                aloc_table: typed
                    .aloc_table
                    .map(|bytes| Arc::from(bytes.into_boxed_slice())),
                type_sig: typed
                    .type_sig
                    .map(|bytes| Arc::from(bytes.into_boxed_slice())),
                type_sig_options: None,
                file_sig: typed
                    .file_sig
                    .map(|bytes| Arc::from(bytes.into_boxed_slice())),
                exports: Arc::from(typed.exports.into_boxed_slice()),
                requires: Arc::from(typed.requires.into_boxed_slice()),
                resolved_requires: Some(Self::resolved_requires_from_serialized(
                    typed.resolved_requires,
                    files,
                )?),
                imports: Arc::from(typed.imports.into_boxed_slice()),
                leader: typed
                    .leader
                    .map(|file| Self::file_from_index(files, file))
                    .transpose()?,
                sig_hash: typed.sig_hash,
                merge_hashes: typed.merge_hashes.map(Arc::new),
            })),
            SerializedParse::Untyped { file_hash } => {
                Ok(Parse::Untyped(UntypedParse::new(file_hash)))
            }
            SerializedParse::Package {
                file_hash,
                package_info,
            } => Ok(Parse::Package(PackageParse::new(
                file_hash,
                Arc::new(package_info),
            ))),
        }
    }

    fn sorted_file_indexes(files: &[FileKey], file_to_index: &BTreeMap<FileKey, u32>) -> Vec<u32> {
        let mut indexes = files
            .iter()
            .map(|file| Self::file_index(file_to_index, file))
            .collect::<Vec<_>>();
        indexes.sort_unstable();
        indexes.dedup();
        indexes
    }

    fn file_entry_from_serialized(
        file_key: FileKey,
        serialized: SerializedFileEntry,
        files: &[FileKey],
    ) -> io::Result<(Option<FileEntry>, Option<Vec<FileKey>>)> {
        let parse = serialized
            .parse
            .map(|parse| Self::parse_from_serialized(parse, files))
            .transpose()?;
        let dependents = serialized
            .dependents
            .map(|dependents| {
                dependents
                    .into_iter()
                    .map(|file| Self::file_from_index(files, file))
                    .collect::<io::Result<Vec<_>>>()
            })
            .transpose()?;
        let alternate_file = serialized
            .alternate_file
            .map(|file| Self::file_from_index(files, file))
            .transpose()?;
        let haste_info = serialized.haste_info;
        let entry = if parse.is_some() || haste_info.is_some() || alternate_file.is_some() {
            Some(FileEntry::new_committed(
                file_key,
                parse,
                haste_info,
                alternate_file,
            ))
        } else {
            None
        };
        Ok((entry, dependents))
    }

    fn haste_module_from_serialized(
        info: HasteModuleInfo,
        serialized: SerializedHasteModule,
        files: &[FileKey],
    ) -> io::Result<(HasteModule, Vec<FileKey>, Vec<FileKey>)> {
        let provider = serialized
            .provider
            .map(|file| Self::file_from_index(files, file))
            .transpose()?;
        let dependents = serialized
            .dependents
            .into_iter()
            .map(|file| Self::file_from_index(files, file))
            .collect::<io::Result<Vec<_>>>()?;
        let provider_candidates = serialized
            .all_providers
            .into_iter()
            .map(|file| Self::file_from_index(files, file))
            .collect::<io::Result<Vec<_>>>()?;
        Ok((
            HasteModule::new_committed(info, provider),
            dependents,
            provider_candidates,
        ))
    }

    fn validate_serialized_heap_header(
        header: &SerializedHeapHeader,
        expected_magic: u64,
    ) -> io::Result<()> {
        if header.magic != expected_magic {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "hh_load_heap: invalid magic number",
            ));
        }
        if header.file_shard_count != GC_MAP_SHARDS as u64
            || header.haste_module_shard_count != GC_MAP_SHARDS as u64
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "hh_load_heap: invalid shard count",
            ));
        }
        Ok(())
    }

    fn load_file_heap_shard(
        compressed: Vec<u8>,
        external_files: Option<&[FileKey]>,
    ) -> io::Result<LoadedFileHeapShard> {
        let bytes = decompress_block(&compressed)?;
        let shard: SerializedFileHeapShard = decode_from_slice(&bytes)?;
        let SerializedFileHeapShard {
            files: local_files,
            entries,
        } = shard;
        let files = external_files.unwrap_or(&local_files);
        let mut map = HashMap::with_capacity(entries.len());
        let mut dependents = HashMap::new();
        for (file, entry) in entries {
            let file = Self::file_from_index(files, file)?;
            let (entry, entry_dependents) =
                Self::file_entry_from_serialized(file.dupe(), entry, files)?;
            if let Some(entry_dependents) = entry_dependents
                && !entry_dependents.is_empty()
            {
                dependents.insert(file.dupe(), entry_dependents);
            }
            if let Some(entry) = entry {
                map.insert(file, entry);
            }
        }
        Ok(LoadedFileHeapShard {
            files: map,
            dependents,
        })
    }

    fn load_haste_heap_shard(
        compressed: Vec<u8>,
        external_files: Option<&[FileKey]>,
    ) -> io::Result<LoadedHasteModuleHeapShard> {
        let bytes = decompress_block(&compressed)?;
        let shard: SerializedHasteModuleHeapShard = decode_from_slice(&bytes)?;
        let SerializedHasteModuleHeapShard {
            files: local_files,
            entries,
        } = shard;
        let files = external_files.unwrap_or(&local_files);
        let count = entries.len() as u64;
        let mut map = HashMap::with_capacity(entries.len());
        let mut dependents = HashMap::new();
        let mut provider_candidates = HashMap::new();
        for (info, module) in entries {
            let (module, module_dependents, module_provider_candidates) =
                Self::haste_module_from_serialized(info.dupe(), module, files)?;
            if !module_dependents.is_empty() {
                dependents.insert(info.dupe(), module_dependents);
            }
            if !module_provider_candidates.is_empty() {
                provider_candidates.insert(info.dupe(), module_provider_candidates);
            }
            map.insert(info, module);
        }
        assert_eq!(
            map.len() as u64,
            count,
            "haste heap shard size should match saved shard header"
        );
        Ok(LoadedHasteModuleHeapShard {
            modules: map,
            dependents,
            provider_candidates,
        })
    }

    pub fn commit(self: Arc<Self>, destination: &Arc<CommittedHeap>) {
        let mut transaction = match Arc::try_unwrap(self) {
            Ok(transaction) => transaction,
            Err(_) => panic!("all transaction handles must be dropped before commit"),
        };
        // Publishing needs the write guard, so give up the read guard first.
        transaction.release();
        let source = transaction.heap.dupe();
        let overlay = &mut transaction.overlay;
        if Arc::ptr_eq(&source, destination) {
            source.apply_overlay_draining(overlay);
        } else {
            source.apply_commit_deltas_to_both(destination, overlay);
        }
    }
}

/// Borrows the committed heap for the duration of one write into the overlay.
pub struct HeapWrite<'a> {
    slot: parking_lot::RecursiveRwLockReadGuard<'a, Option<CommittedHeapReadGuard>>,
    overlay: &'a HeapOverlay,
}

impl HeapWrite<'_> {
    pub(crate) fn writer(&self) -> HeapWriter<'_> {
        let state = self.slot.as_ref().expect(
            "transaction was written after its guard was released; a transaction may only \
             be used within the unit of work its dispatcher created it for",
        );
        HeapWriter::new(&state.data, self.overlay)
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        // The cache to empty belongs to the state this transaction read, which is not necessarily
        // the state the heap holds now: a saved-state load swaps a whole new one in, and emptying
        // that one instead would throw away a cache this transaction never touched. Use the guard
        // while it is still here; a transaction that has already given it back — one that
        // committed — has published into whatever the heap holds now, so that is the cache to
        // empty for it.
        if !self.overlay.is_empty() {
            match self.committed.read().as_ref() {
                Some(state) => state.reader_cache.clear(),
                None => self.heap.read_arc_recursive().reader_cache.clear(),
            }
        }
        self.release();
        self.overlay.clear_latest_entries_parallel();
    }
}
