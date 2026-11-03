/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::RwLock;

use dupe::Dupe;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::file_key::DCTS_EXT;
use flow_parser::file_key::DMTS_EXT;
use flow_parser::file_key::DTS_EXT;
use flow_parser::file_key::FileKey;
use flow_parser::file_key::has_dts_ext;
use regex::Regex;

use crate::path_matcher::PathMatcher;
use crate::path_matcher::RootedGlob;
use crate::sys_utils::normalize_filename_dir_sep;

// Sharded cache for std::fs::canonicalize() to avoid kernel dcache lock contention
// when many threads call realpath concurrently.
const CANONICALIZE_CACHE_SHARDS: usize = 128;

struct CanonicalizeCache {
    shards: [Mutex<HashMap<PathBuf, Option<PathBuf>>>; CANONICALIZE_CACHE_SHARDS],
}

impl CanonicalizeCache {
    fn new() -> Self {
        Self {
            shards: std::array::from_fn(|_| Mutex::new(HashMap::new())),
        }
    }

    fn get_or_insert(&self, path: &Path) -> std::io::Result<PathBuf> {
        let mut hasher = std::hash::DefaultHasher::new();
        path.hash(&mut hasher);
        let shard_idx = (hasher.finish() as usize) % CANONICALIZE_CACHE_SHARDS;

        let mut shard = self.shards[shard_idx].lock().unwrap();
        if let Some(cached) = shard.get(path) {
            match cached {
                Some(p) => Ok(p.clone()),
                None => Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "cached canonicalize failure",
                )),
            }
        } else {
            // Use `dunce::canonicalize` instead of `std::fs::canonicalize`: on
            // Windows the latter returns a `\\?\` verbatim path, whose `?` is an
            // illegal filename character once the root is escaped into derived
            // paths (e.g. the server lock file), causing spurious "server already
            // running" failures. `dunce` strips the prefix when it is safe to do
            // so and is a passthrough to std on other platforms.
            let result = dunce::canonicalize(path);
            let cached_value = result.as_ref().ok().cloned();
            shard.insert(path.to_path_buf(), cached_value);
            result
        }
    }
}

static CANONICALIZE_CACHE: OnceLock<CanonicalizeCache> = OnceLock::new();

/// Cached version of std::fs::canonicalize(). Thread-safe, uses sharded locking.
pub fn cached_canonicalize(path: &Path) -> std::io::Result<PathBuf> {
    CANONICALIZE_CACHE
        .get_or_init(CanonicalizeCache::new)
        .get_or_insert(path)
}

fn resolved_path<'a>(original: &'a str, resolved: &Path) -> Cow<'a, str> {
    let Some(resolved) = resolved.to_str() else {
        return Cow::Borrowed(original);
    };
    if resolved == original {
        Cow::Borrowed(original)
    } else {
        Cow::Owned(resolved.to_string())
    }
}

/// Canonicalizes a path, returning the original string when no UTF-8 result is available.
pub fn canonicalize_path(path: &str) -> Cow<'_, str> {
    match cached_canonicalize(Path::new(path)) {
        Ok(resolved) => resolved_path(path, &resolved),
        Err(_) => Cow::Borrowed(path),
    }
}

// utilities for supported filenames

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibDir {
    Prelude(PathBuf),
    Flowlib(PathBuf),
    FlowlibWithLibDomDts(PathBuf),
    Tslib(PathBuf),
}
#[derive(Debug, Clone)]
pub struct IgnorePattern {
    pub negated: bool,
    pub backup: Option<String>,
    pub matcher: RootedGlob,
}

#[derive(Debug, Clone)]
pub struct UntypedPattern {
    pub negated: bool,
    pub matcher: RootedGlob,
}

#[derive(Debug, Clone)]
pub struct FileOptions {
    pub default_lib_dir: Option<LibDir>,
    pub ignores: Vec<IgnorePattern>,
    pub untyped: Vec<UntypedPattern>,
    pub declarations: Vec<(String, Regex)>,
    pub implicitly_include_root: bool,
    pub includes: PathMatcher,
    pub haste_paths_excludes: Vec<Regex>,
    pub haste_paths_includes: Vec<Regex>,
    pub lib_paths: Vec<PathBuf>,
    pub module_declaration_dirnames: Vec<String>,
    pub module_file_exts: Vec<FlowSmolStr>,
    pub module_resource_exts: HashSet<FlowSmolStr>,
    pub multi_platform: bool,
    pub multi_platform_extensions: Vec<FlowSmolStr>,
    pub multi_platform_extension_group_mapping: Vec<(FlowSmolStr, Vec<FlowSmolStr>)>,
    pub node_resolver_dirnames: Vec<String>,
}

impl Default for FileOptions {
    fn default() -> Self {
        Self {
            default_lib_dir: None,
            ignores: Vec::new(),
            untyped: Vec::new(),
            declarations: Vec::new(),
            implicitly_include_root: true,
            includes: PathMatcher::empty(),
            haste_paths_excludes: Vec::new(),
            haste_paths_includes: Vec::new(),
            lib_paths: Vec::new(),
            module_declaration_dirnames: Vec::new(),
            module_file_exts: Vec::new(),
            module_resource_exts: HashSet::new(),
            multi_platform: false,
            multi_platform_extensions: Vec::new(),
            multi_platform_extension_group_mapping: Vec::new(),
            node_resolver_dirnames: vec!["node_modules".to_string()],
        }
    }
}

// During node module resolution, we need to look for node_modules/ directories
// as we walk up the path. But checking each directory for them would be expensive.
// So instead we memorize which directories contain node_modules/ directories.
//
// This is complicated by the node_resolver_dirnames option, which means
// node_modules/ directories might go by other names. So we need to keep track
// of which directories contain node_modules/ directories and which aliases we've
// seen
#[allow(non_upper_case_globals)]
pub static node_modules_containers: LazyLock<RwLock<BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>>> =
    LazyLock::new(|| RwLock::new(BTreeMap::new()));

#[derive(Debug)]
pub struct SymlinkMap {
    building_paths: Mutex<Option<HashMap<PathBuf, PathBuf>>>,
    complete_paths: OnceLock<HashMap<PathBuf, PathBuf>>,
}

impl Default for SymlinkMap {
    fn default() -> Self {
        Self {
            building_paths: Mutex::new(Some(HashMap::new())),
            complete_paths: OnceLock::new(),
        }
    }
}

impl SymlinkMap {
    fn insert(&self, path: PathBuf, real_path: PathBuf) {
        self.building_paths
            .lock()
            .expect("symlink map lock should not be poisoned")
            .as_mut()
            .expect("paths cannot be inserted after the map is complete")
            .insert(path, real_path);
    }

    pub fn mark_complete(&self) {
        let paths = self
            .building_paths
            .lock()
            .expect("symlink map lock should not be poisoned")
            .take()
            .expect("symlink map should only be completed once");
        self.complete_paths
            .set(paths)
            .expect("symlink map should only be completed once");
    }

    pub fn is_complete(&self) -> bool {
        self.complete_paths.get().is_some()
    }

    /// Returns `None` when the map is not complete (e.g. not a full init run).
    pub fn try_resolve<'a>(&self, normalized_path: &'a str) -> Option<Cow<'a, str>> {
        let paths = self.complete_paths.get()?;
        let normalized_path_buf = Path::new(normalized_path);
        let resolved = paths
            .get(normalized_path_buf)
            .map_or(normalized_path_buf, PathBuf::as_path);
        Some(resolved_path(normalized_path, resolved))
    }
}

pub fn lexically_normalize_path(path: &str) -> Cow<'_, str> {
    if !path
        .split(&['/', '\\'][..])
        .any(|component| component == "." || component == "..")
    {
        return Cow::Borrowed(path);
    }

    let mut normalized = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component),
        }
    }
    resolved_path(path, &normalized)
}

pub const GLOBAL_FILE_NAME: &str = "(global)";
pub const FLOW_EXT: &str = ".flow";

pub fn has_flow_ext(file: &FileKey) -> bool {
    file.check_suffix(FLOW_EXT)
}

pub fn chop_flow_ext(file: &FileKey) -> FileKey {
    if has_flow_ext(file) {
        file.chop_suffix(FLOW_EXT)
    } else {
        file.dupe()
    }
}

pub fn has_ts_ext(file: &FileKey) -> bool {
    file.check_suffix(".ts")
        || file.check_suffix(".tsx")
        || file.check_suffix(".mts")
        || file.check_suffix(".cts")
}

pub fn js_to_dts(path: &str) -> Option<String> {
    if let Some(stem) = path.strip_suffix(".js") {
        Some(format!("{}{}", stem, DTS_EXT))
    } else if let Some(stem) = path.strip_suffix(".mjs") {
        Some(format!("{}{}", stem, DMTS_EXT))
    } else {
        path.strip_suffix(".cjs")
            .map(|stem| format!("{}{}", stem, DCTS_EXT))
    }
}

pub fn has_declaration_ext(file: &FileKey) -> bool {
    has_flow_ext(file)
}

pub fn chop_declaration_ext(file: &FileKey) -> FileKey {
    if has_flow_ext(file) {
        chop_flow_ext(file)
    } else {
        file.dupe()
    }
}

pub fn is_prefix(prefix: &str, path: &str) -> bool {
    if prefix.ends_with(std::path::MAIN_SEPARATOR) {
        return path.starts_with(prefix);
    }
    if path == prefix {
        return true;
    }
    if let Some(rest) = path.strip_prefix(prefix) {
        return rest.starts_with(std::path::MAIN_SEPARATOR);
    }
    false
}

/// Like `is_prefix` but always uses '/' as the separator. Use this when
/// comparing against File_key suffixes, which are always '/'-based for saved
/// state portability, even on Windows.
pub fn is_file_key_prefix(prefix: &str, suffix: &str) -> bool {
    if suffix == prefix {
        return true;
    }
    let prefix_with_sep = if prefix.ends_with('/') {
        prefix.to_string()
    } else {
        format!("{}/", prefix)
    };
    suffix.starts_with(&prefix_with_sep)
}

pub fn haste_name_opt(options: &FileOptions, file: &FileKey) -> Option<String> {
    fn is_valid_haste_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '$' || c == '_' || c == '.' || c == '-'
    }

    fn extract_haste_name(normalized_file_name: &str) -> String {
        use std::path::Path;

        // Get basename
        let basename = Path::new(normalized_file_name)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(normalized_file_name);

        // Chop .flow suffix if present
        let without_flow = basename.strip_suffix(".flow").unwrap_or(basename);

        // Chop .js suffix if present
        match without_flow.strip_suffix(".js") {
            None => normalized_file_name.to_string(),
            Some(name) => {
                // Validate all characters
                if !name.is_empty() && name.chars().all(is_valid_haste_char) {
                    name.to_string()
                } else {
                    normalized_file_name.to_string()
                }
            }
        }
    }

    // Standardize \ to / in path for Windows
    let abs_path = file.to_absolute();
    let normalized_file_name = normalize_filename_dir_sep(&abs_path);

    let matches_includes = options
        .haste_paths_includes
        .iter()
        .any(|r| r.is_match(&normalized_file_name));
    let matches_excludes = options
        .haste_paths_excludes
        .iter()
        .any(|r| r.is_match(&normalized_file_name));

    if matches_includes && !matches_excludes {
        Some(extract_haste_name(&normalized_file_name))
    } else {
        None
    }
}

pub fn grouped_platform_extension_opt<'a>(
    options: &'a FileOptions,
    filename: &str,
) -> Option<&'a (FlowSmolStr, Vec<FlowSmolStr>)> {
    options
        .multi_platform_extension_group_mapping
        .iter()
        .find(|(group_ext, _platforms)| {
            options
                .module_file_exts
                .iter()
                .any(|module_ext| filename.ends_with(&format!("{}{}", group_ext, module_ext)))
        })
}

pub fn relative_interface_mref_of_possibly_platform_specific_file(
    options: &FileOptions,
    file: &FileKey,
) -> Option<String> {
    if !options.multi_platform {
        return None;
    }

    options.module_file_exts.iter().find_map(|module_file_ext| {
        if !file.check_suffix(module_file_ext) {
            return None;
        }
        let file_without_module_file_ext = file.chop_suffix(module_file_ext);
        let platform_match = options
            .multi_platform_extensions
            .iter()
            .find_map(|platform_ext| {
                if file_without_module_file_ext.check_suffix(platform_ext) {
                    let base_file = file_without_module_file_ext.chop_suffix(platform_ext);
                    let base = base_file.as_str();
                    let basename = std::path::Path::new(base)
                        .file_name()
                        .and_then(|s| s.to_str())?;
                    Some(format!("./{}.js", basename))
                } else {
                    None
                }
            });

        if platform_match.is_some() {
            return platform_match;
        }
        if let Some((group_ext, _)) = grouped_platform_extension_opt(options, file.as_str()) {
            let base_file = file_without_module_file_ext.chop_suffix(group_ext);
            let base = base_file.as_str();
            let basename = std::path::Path::new(base)
                .file_name()
                .and_then(|s| s.to_str())?;
            Some(format!("./{}.js", basename))
        } else {
            None
        }
    })
}

pub fn platform_specific_extensions_and_indices_opt(
    options: &FileOptions,
    filename: &str,
) -> Option<Vec<(usize, FlowSmolStr)>> {
    let single_match =
        options
            .multi_platform_extensions
            .iter()
            .enumerate()
            .find_map(|(idx, platform_ext)| {
                if options.module_file_exts.iter().any(|module_ext| {
                    filename.ends_with(&format!("{}{}", platform_ext, module_ext))
                }) {
                    Some((idx, platform_ext.dupe()))
                } else {
                    None
                }
            });

    if let Some(result) = single_match {
        return Some(vec![result]);
    }

    if let Some((_group_ext, platforms)) = grouped_platform_extension_opt(options, filename) {
        Some(
            platforms
                .iter()
                .filter_map(|p| {
                    let ext = FlowSmolStr::from(format!(".{}", p));
                    options
                        .multi_platform_extensions
                        .iter()
                        .position(|e| e == &ext)
                        .map(|idx| (idx, ext))
                })
                .collect(),
        )
    } else {
        None
    }
}

pub fn chop_platform_suffix_for_file(options: &FileOptions, file: &FileKey) -> FileKey {
    let platform_extensions: Vec<FlowSmolStr> = options
        .multi_platform_extensions
        .iter()
        .chain(
            options
                .multi_platform_extension_group_mapping
                .iter()
                .map(|(ext, _)| ext),
        )
        .cloned()
        .collect();

    platform_extensions
        .into_iter()
        .find_map(|platform_ext| {
            options.module_file_exts.iter().find_map(|module_ext| {
                let ext = format!("{}{}", platform_ext, module_ext);
                if file.check_suffix(&ext) {
                    let chopped = file.chop_suffix(&ext);
                    Some(chopped.with_suffix(module_ext))
                } else {
                    None
                }
            })
        })
        .unwrap_or_else(|| file.dupe())
}

pub fn chop_platform_suffix_for_haste_module(options: &FileOptions, module_name: &str) -> String {
    let platform_extensions: Vec<FlowSmolStr> = options
        .multi_platform_extensions
        .iter()
        .chain(
            options
                .multi_platform_extension_group_mapping
                .iter()
                .map(|(ext, _)| ext),
        )
        .cloned()
        .collect();

    platform_extensions
        .into_iter()
        .find_map(|platform_ext| module_name.strip_suffix(platform_ext.as_str()))
        .unwrap_or(module_name)
        .to_owned()
}

pub fn is_json_file(filename: &str) -> bool {
    if let Some(ext) = Path::new(filename).extension() {
        ext == "json"
    } else {
        false
    }
}

// This is the set of file extensions which we watch for changes
pub fn get_all_watched_extensions(options: &FileOptions) -> HashSet<String> {
    let mut result: HashSet<_> = options
        .module_file_exts
        .iter()
        .map(|s| s.as_str().to_owned())
        .collect();
    result.extend(
        options
            .module_resource_exts
            .iter()
            .map(|s| s.as_str().to_owned()),
    );
    result
}

pub fn is_valid_path(options: &FileOptions, path: &str) -> bool {
    fn get_extension(s: &str) -> Option<String> {
        let ext = Path::new(s).extension()?.to_str()?;
        Some(format!(".{}", ext))
    }

    // Given a file foo.bar.baz.bat, checks the extensions .bat, .baz.bat, and .bar.baz.bat
    fn check_ext(file_exts: &HashSet<String>, basename: &str) -> bool {
        fn helper(
            file_exts: &HashSet<String>,
            basename: &str,
            acc: &str,
            ext: Option<String>,
        ) -> bool {
            let Some(ext) = ext else { return false };

            let acc = format!("{}{}", ext, acc);
            if file_exts.contains(&acc) {
                return true;
            }

            if let Some(chopped) = basename.strip_suffix(&ext) {
                let ext = get_extension(chopped);
                helper(file_exts, chopped, &acc, ext)
            } else {
                false
            }
        }

        let acc = "";
        if let Some(basename) = basename.strip_suffix(FLOW_EXT) {
            helper(file_exts, basename, acc, get_extension(basename))
        } else {
            helper(file_exts, basename, acc, get_extension(basename))
        }
    }

    let Some(basename) = Path::new(path).file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    if basename.starts_with('.') {
        return false;
    }
    let file_exts = get_all_watched_extensions(options);
    check_ext(&file_exts, basename) || basename == "package.json"
}

pub fn is_node_module(options: &FileOptions, path: &str) -> bool {
    let basename = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    options.node_resolver_dirnames.contains(&basename)
}

pub fn is_flow_file(options: &FileOptions, path: &str) -> bool {
    is_valid_path(options, path) && !Path::new(path).is_dir()
}

pub fn make_path_absolute(root: &Path, path_str: &str) -> PathBuf {
    let path = Path::new(path_str);
    let joined = if path.is_relative() {
        root.join(path)
    } else {
        PathBuf::from(path)
    };
    cached_canonicalize(&joined).unwrap_or(joined)
}

pub static ABSOLUTE_PATH_REGEXP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(/|[A-Za-z]:[/\\])").unwrap());

pub const PROJECT_ROOT_TOKEN: &str = "<PROJECT_ROOT>";
pub const BUILTIN_ROOT_TOKEN: &str = "<BUILTINS>";
fn can_prune_dir(options: &FileOptions) -> bool {
    options
        .ignores
        .iter()
        .all(|pattern| !pattern.negated && pattern.matcher.supports_directory_pruning())
}

pub fn is_in_flowlib(options: &FileOptions, path: &str) -> bool {
    match &options.default_lib_dir {
        None => false,
        Some(libdir) => {
            let root_path = match libdir {
                LibDir::Prelude(path)
                | LibDir::Flowlib(path)
                | LibDir::FlowlibWithLibDomDts(path)
                | LibDir::Tslib(path) => path,
            };
            let path = Path::new(path);
            path.starts_with(root_path)
                || cached_canonicalize(root_path)
                    .is_ok_and(|canonical_root| path.starts_with(canonical_root))
        }
    }
}

pub fn ordered_and_unordered_lib_paths(options: &FileOptions) -> Vec<String> {
    let (libs, filter): (Vec<PathBuf>, Box<dyn Fn(&str) -> bool + '_>) =
        match &options.default_lib_dir {
            None => (
                options.lib_paths.clone(),
                Box::new(|path: &str| is_valid_path(options, path)),
            ),
            Some(libdir) => {
                let libs = &options.lib_paths;
                let root_path = match libdir {
                    LibDir::Prelude(path)
                    | LibDir::Flowlib(path)
                    | LibDir::FlowlibWithLibDomDts(path)
                    | LibDir::Tslib(path) => path,
                };
                let root = root_path.clone();
                let root_resolved = root.to_string_lossy().to_string();
                let filter: Box<dyn Fn(&str) -> bool + '_> = Box::new(move |path: &str| {
                    is_prefix(&root_resolved, path) || is_valid_path(options, path)
                });
                let mut new_libs = vec![root];
                new_libs.extend(libs.iter().cloned());
                (new_libs, filter)
            }
        };

    let libs: Vec<String> = if libs.is_empty() {
        Vec::new()
    } else {
        libs.iter()
            .flat_map(|lib| {
                let lib_str = lib.to_string_lossy().to_string();
                let filter_prime = |path: &str| -> bool {
                    (path == lib_str || filter(path))
                        && !is_json_file(path)
                        && !is_dynamically_discovered_builtin_ts_libdef(options, path)
                };

                let mut files: std::collections::BTreeSet<String> =
                    std::collections::BTreeSet::new();
                let mut symlink_ancestor_cache = HashMap::new();
                if lib.exists() {
                    for entry in jwalk::WalkDir::new(lib)
                        .skip_hidden(false)
                        .parallelism(jwalk::Parallelism::RayonNewPool(2))
                        .into_iter()
                        .filter_map(|e| e.ok())
                    {
                        let path = entry.path();
                        if entry.file_type().is_dir() {
                            continue;
                        }
                        if !entry.file_type().is_file() {
                            continue;
                        }
                        if should_canonicalize_discovered_entry(
                            &path,
                            entry.path_is_symlink(),
                            None,
                            &mut symlink_ancestor_cache,
                        ) {
                            if let Ok(real_path) = cached_canonicalize(&path) {
                                let path_str = real_path.to_string_lossy().to_string();
                                if filter_prime(&path_str) {
                                    files.insert(path_str);
                                }
                            }
                        } else {
                            let path_str = path.to_string_lossy().to_string();
                            if filter_prime(&path_str) {
                                files.insert(path_str);
                            }
                        }
                    }
                }
                files.into_iter().collect::<Vec<_>>()
            })
            .collect()
    };

    libs
}

/// The bundled TypeScript libraries that are modules. Their `.js` names keep their declarations
/// Flow-strict, but like the bundled `.d.ts` files they are TypeScript source files.
const BUILTIN_TS_MODULES: &[&str] = &["lib.esnext.iterator.js"];

pub fn has_builtin_ts_module_name(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .is_some_and(|name| BUILTIN_TS_MODULES.iter().any(|module| name == *module))
}

pub fn is_builtin_ts_module(options: &FileOptions, path: &str) -> bool {
    has_builtin_ts_module_name(path) && is_in_flowlib(options, path)
}

fn is_dynamically_discovered_builtin_ts_libdef(options: &FileOptions, path: &str) -> bool {
    (has_dts_ext(path) && is_in_flowlib(options, path)) || is_builtin_ts_module(options, path)
}

pub fn is_configured_lib_file(options: &FileOptions, path: &str) -> bool {
    if is_json_file(path) || is_dynamically_discovered_builtin_ts_libdef(options, path) {
        return false;
    }
    let valid_path = is_valid_path(options, path);
    let path = Path::new(path);
    let in_default_lib_dir = is_in_flowlib(options, path.to_string_lossy().as_ref());
    in_default_lib_dir
        || options
            .lib_paths
            .iter()
            .any(|lib_path| path == lib_path || (path.starts_with(lib_path) && valid_path))
}

fn is_matching_regex_path(path: &str, pattern: &str, regex: &Regex, current: bool) -> bool {
    if pattern.starts_with('!') {
        current && !regex.is_match(path)
    } else {
        current || regex.is_match(path)
    }
}

fn is_matching_glob_path(path: &str, negated: bool, matcher: &RootedGlob, current: bool) -> bool {
    if negated {
        current && !matcher.is_match(path)
    } else {
        current || matcher.is_match(path)
    }
}

fn is_matching(path: &str, pattern_list: &[(String, Regex)]) -> bool {
    pattern_list.iter().fold(false, |current, (pattern, rx)| {
        is_matching_regex_path(path, pattern, rx, current)
    })
}

fn is_matching_untyped(path: &str, pattern_list: &[UntypedPattern]) -> bool {
    pattern_list.iter().fold(false, |current, pattern| {
        is_matching_glob_path(path, pattern.negated, &pattern.matcher, current)
    })
}

fn is_matching_ignore(path: &str, pattern_list: &[IgnorePattern]) -> (bool, Option<String>) {
    pattern_list.iter().fold(
        (false, None),
        |(matched_already, current_backup), pattern| {
            let matches =
                is_matching_glob_path(path, pattern.negated, &pattern.matcher, matched_already);
            let new_backup = if matches && current_backup.is_none() {
                pattern.backup.clone()
            } else {
                current_backup
            };
            (matches, new_backup)
        },
    )
}

/// true if a file path matches an `[ignore]` entry in config
pub fn is_ignored(options: &FileOptions, path: &str) -> (bool, Option<String>) {
    // On Windows, the path may use \ instead of /, but let's standardize the
    // ignore regex to use /
    let path = normalize_filename_dir_sep(path);
    is_matching_ignore(&path, &options.ignores)
}

/// true if a file path matches an `[untyped]` entry in config
pub fn is_untyped(options: &FileOptions, path: &str) -> bool {
    // On Windows, the path may use \ instead of /, but let's standardize the
    // ignore glob to use /
    let path = normalize_filename_dir_sep(path);
    is_matching_untyped(&path, &options.untyped)
}

/// true if a file path matches a `[declarations]` entry in config
pub fn is_declaration(options: &FileOptions, path: &str) -> bool {
    // On Windows, the path may use \ instead of /, but let's standardize the
    // ignore regex to use /
    let path = normalize_filename_dir_sep(path);
    is_matching(&path, &options.declarations)
}

/// true if a file path matches an `[include]` path in config
pub fn is_included(options: &FileOptions, f: &str) -> bool {
    options.includes.matches(f)
}

pub fn wanted(options: &FileOptions, include_libdef: bool, path: &str) -> bool {
    let is_lib_file = is_configured_lib_file(options, path);
    if include_libdef {
        !is_ignored(options, path).0
            || is_lib_file
            || is_dynamically_discovered_builtin_ts_libdef(options, path)
    } else {
        !is_ignored(options, path).0 && !is_lib_file
    }
}

// Calls `next` repeatedly until it is resolved, returning a SSet of results
pub fn get_all(next: &mut dyn FnMut() -> Vec<String>) -> BTreeSet<String> {
    let mut accum = BTreeSet::new();
    loop {
        let result = next();
        if result.is_empty() {
            return accum;
        } else {
            for x in result {
                accum.insert(x);
            }
        }
    }
}

pub static DIR_SEP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[/\\]").unwrap());

pub static CURRENT_DIR_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.").unwrap());

pub static PARENT_DIR_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.\.").unwrap());

pub fn watched_paths(options: &FileOptions) -> Vec<PathBuf> {
    let mut stems = options.includes.stems().to_vec();
    stems.sort();
    // Drop a stem if a previously-kept stem is its ancestor. `dedup_by` calls
    // the closure with `(later, earlier)`; returning true removes the later
    // element.
    stems.dedup_by(|b, a| b.starts_with(&*a));
    stems
}

const MAX_FILES: usize = 1000;

fn has_symlink_ancestor(path: &Path, symlink_ancestor_cache: &mut HashMap<PathBuf, bool>) -> bool {
    fn dir_has_symlink_in_path(
        dir: &Path,
        symlink_ancestor_cache: &mut HashMap<PathBuf, bool>,
    ) -> bool {
        if let Some(result) = symlink_ancestor_cache.get(dir) {
            return *result;
        }

        let parent_has_symlink = dir
            .parent()
            .is_some_and(|parent| dir_has_symlink_in_path(parent, symlink_ancestor_cache));
        let is_symlink = std::fs::symlink_metadata(dir)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false);
        let result = parent_has_symlink || is_symlink;
        symlink_ancestor_cache.insert(dir.to_path_buf(), result);
        result
    }

    path.parent()
        .is_some_and(|parent| dir_has_symlink_in_path(parent, symlink_ancestor_cache))
}

fn has_symlink_ancestor_under_root(
    root: &Path,
    path: &Path,
    symlink_ancestor_cache: &mut HashMap<PathBuf, bool>,
) -> bool {
    fn dir_has_symlink_in_path(
        root: &Path,
        dir: &Path,
        symlink_ancestor_cache: &mut HashMap<PathBuf, bool>,
    ) -> bool {
        if !dir.starts_with(root) {
            return false;
        }
        if let Some(result) = symlink_ancestor_cache.get(dir) {
            return *result;
        }

        let parent_has_symlink = dir != root
            && dir.parent().is_some_and(|parent| {
                dir_has_symlink_in_path(root, parent, symlink_ancestor_cache)
            });
        let is_symlink = dir != root
            && std::fs::symlink_metadata(dir)
                .map(|metadata| metadata.file_type().is_symlink())
                .unwrap_or(false);
        let result = parent_has_symlink || is_symlink;
        symlink_ancestor_cache.insert(dir.to_path_buf(), result);
        result
    }

    path.parent()
        .is_some_and(|parent| dir_has_symlink_in_path(root, parent, symlink_ancestor_cache))
}

fn should_canonicalize_discovered_entry(
    path: &Path,
    path_is_symlink: bool,
    symlink_ancestor_root: Option<&Path>,
    symlink_ancestor_cache: &mut HashMap<PathBuf, bool>,
) -> bool {
    path_is_symlink
        || path
            .components()
            .any(|component| component.as_os_str() == OsStr::new("node_modules"))
        || match symlink_ancestor_root {
            None => has_symlink_ancestor(path, symlink_ancestor_cache),
            Some(root) => has_symlink_ancestor_under_root(root, path, symlink_ancestor_cache),
        }
}

fn wanted_filter(options: &FileOptions, path: &Path, all: bool, include_libdef: bool) -> bool {
    let path_str = path.to_string_lossy();
    all || wanted(options, include_libdef, &path_str)
}

fn realpath_filter(options: &FileOptions, path: &Path, all: bool, include_libdef: bool) -> bool {
    let path_str = path.to_string_lossy();
    (is_valid_path(options, &path_str)
        || (include_libdef && is_dynamically_discovered_builtin_ts_libdef(options, &path_str))
        || (include_libdef && is_configured_lib_file(options, &path_str)))
        && wanted_filter(options, path, all, include_libdef)
}

fn path_is_in_crawl_scope(
    options: &FileOptions,
    root: &Path,
    path: &Path,
    include_libdef: bool,
) -> bool {
    (options.implicitly_include_root && path.starts_with(root)) || {
        let path_str = path.to_string_lossy();
        is_included(options, &path_str)
            || (include_libdef && is_dynamically_discovered_builtin_ts_libdef(options, &path_str))
            || (include_libdef && is_configured_lib_file(options, &path_str))
    }
}

fn no_subdir_path_filter(
    options: &FileOptions,
    root: &Path,
    path: &Path,
    all: bool,
    include_libdef: bool,
) -> bool {
    path_is_in_crawl_scope(options, root, path, include_libdef)
        && realpath_filter(options, path, all, include_libdef)
}

/// Whether the initial full crawl covers a path, making an absent alias mapping conclusive.
pub fn initial_crawl_covers_path(options: &FileOptions, root: &Path, path: &Path) -> bool {
    path_is_in_crawl_scope(options, root, path, true)
        && (wanted_filter(options, path, false, true)
            || has_symlink_ancestor_under_root(root, path, &mut HashMap::new()))
}

/// Creates a "next" function for finding the files in a given FlowConfig root.
/// This means all the files under the root (if the implicit behavior is enabled)
/// and all the included files, minus the ignored files and the libs.
///
/// If `all` is true, ignored files and libs are also returned.
/// If `include_libdef` is true, libdef files are also included.
/// If subdir is set, then we return the subset of files under subdir.
pub fn make_next_files(
    root: &Path,
    subdir: Option<&Path>,
    all: bool,
    sort: bool,
    options: Arc<FileOptions>,
    include_libdef: bool,
    symlink_map: Option<&SymlinkMap>,
    mut send_chunked: impl FnMut(Vec<PathBuf>),
) {
    fn subdir_path_filter(
        options: &FileOptions,
        root: &Path,
        subdir: &Path,
        path: &Path,
        all: bool,
        include_libdef: bool,
    ) -> bool {
        path.starts_with(subdir)
            && ((options.implicitly_include_root && path.starts_with(root)) || {
                let path_str = path.to_string_lossy();
                is_included(options, &path_str)
                    || (include_libdef
                        && is_dynamically_discovered_builtin_ts_libdef(options, &path_str))
                    || (include_libdef && is_configured_lib_file(options, &path_str))
            })
            && realpath_filter(options, path, all, include_libdef)
    }

    let node_module_filter = |file: &str| is_node_module(&options, file);

    let starting_point_paths = {
        let mut starting_point_paths: Vec<PathBuf> = if include_libdef {
            let mut paths: Vec<PathBuf> = options
                .default_lib_dir
                .as_ref()
                .map(|libdir| match libdir {
                    LibDir::Prelude(path)
                    | LibDir::Flowlib(path)
                    | LibDir::FlowlibWithLibDomDts(path)
                    | LibDir::Tslib(path) => path.clone(),
                })
                .into_iter()
                .collect();
            paths.extend(options.lib_paths.iter().cloned());
            paths
        } else {
            Vec::new()
        };
        if let Some(subdir) = subdir {
            starting_point_paths.push(subdir.to_path_buf());
        } else {
            starting_point_paths.extend(watched_paths(&options));
        }
        starting_point_paths.sort();
        starting_point_paths.dedup_by(|b, a| b.starts_with(&*a));
        starting_point_paths
    };

    let can_prune = can_prune_dir(&options);
    let mut chunk = Vec::new();
    let mut symlink_ancestor_cache = HashMap::new();
    for starting_point_path in starting_point_paths {
        let duped_options = options.dupe();
        for entry in jwalk::WalkDir::new(starting_point_path)
            .skip_hidden(false)
            .sort(sort)
            .follow_links(true)
            .parallelism(jwalk::Parallelism::RayonNewPool(2))
            .process_read_dir(move |_depth, path, _read_dir_state, children| {
                if can_prune && !wanted_filter(&duped_options, path, all, include_libdef) {
                    children.clear();
                }
            })
        {
            let Ok(entry) = entry else {
                continue;
            };
            if entry.file_type().is_dir() {
                let path = entry.path();
                let path_str = path.to_string_lossy();
                if node_module_filter(path_str.as_ref()) {
                    let dirname = Path::new(&*path_str)
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let basename = Path::new(&*path_str)
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let mut containers = node_modules_containers.write().unwrap();
                    containers
                        .entry(FlowSmolStr::new(&dirname))
                        .or_default()
                        .insert(FlowSmolStr::new(&basename));
                }
                continue;
            }
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();

            if let Some(symlink_map) = symlink_map
                && (entry.path_is_symlink()
                    || has_symlink_ancestor_under_root(root, &path, &mut symlink_ancestor_cache))
                && let Ok(real_path) = cached_canonicalize(&path)
                && real_path != path
            {
                symlink_map.insert(path.clone(), real_path);
            }

            if !match subdir {
                Some(subdir) => {
                    subdir_path_filter(&options, root, subdir, &path, all, include_libdef)
                }
                None => no_subdir_path_filter(&options, root, &path, all, include_libdef),
            } {
                continue;
            }
            if should_canonicalize_discovered_entry(
                &path,
                entry.path_is_symlink(),
                Some(root),
                &mut symlink_ancestor_cache,
            ) {
                // Note: try_exists() check removed because canonicalize() already
                // verifies the path exists (returns Err for non-existent paths).
                if let Ok(real_path) = cached_canonicalize(&path) {
                    if path == real_path
                        || realpath_filter(&options, &real_path, all, include_libdef)
                    {
                        chunk.push(real_path);
                        if chunk.len() == MAX_FILES {
                            chunk.reverse();
                            send_chunked(chunk);
                            chunk = Vec::new();
                        }
                    }
                }
            } else {
                chunk.push(path);
                if chunk.len() == MAX_FILES {
                    chunk.reverse();
                    send_chunked(chunk);
                    chunk = Vec::new();
                }
            }
        }
    }
    if !chunk.is_empty() {
        chunk.reverse();
        send_chunked(chunk);
    }
}

pub fn expand_project_root_token(root: &Path, s: &str) -> String {
    let root_str = root.to_string_lossy();
    let root = normalize_filename_dir_sep(&root_str);
    s.replace(PROJECT_ROOT_TOKEN, &root)
}

pub fn expand_project_root_token_as_relative(s: &str) -> String {
    let s = s.replace(PROJECT_ROOT_TOKEN, "");
    let s = if s.starts_with('/') || s.starts_with('\\') {
        s[1..].to_string()
    } else {
        s
    };
    normalize_filename_dir_sep(&s).into_owned()
}

pub fn expand_builtin_root_token(flowlib_dir: &Path, s: &str) -> String {
    let flowlib_str = flowlib_dir.to_string_lossy();
    let flowlib_dir_str = normalize_filename_dir_sep(&flowlib_str);
    s.replace(BUILTIN_ROOT_TOKEN, &flowlib_dir_str)
}

pub fn normalized_concat(dir: &str, file: &str) -> String {
    let path = Path::new(dir).join(file);
    normalize_filename_dir_sep(&path.to_string_lossy()).into_owned()
}

pub fn normalized_root_prefix(root: &Path) -> String {
    let root = format!("{}{}", root.to_string_lossy(), std::path::MAIN_SEPARATOR);
    normalize_filename_dir_sep(&root).into_owned()
}

fn is_windows_root(root: &str) -> bool {
    cfg!(windows)
        && root.len() == 2
        && root.as_bytes()[1] == b':'
        && root.as_bytes()[0].is_ascii_alphabetic()
}

#[cfg(windows)]
fn msys_absolute_path(file: &str) -> Option<String> {
    fn drive_path(rest: &str) -> Option<String> {
        let bytes = rest.as_bytes();
        if bytes.len() == 1 && bytes[0].is_ascii_alphabetic() {
            return Some(format!("{}:/", char::from(bytes[0]).to_ascii_uppercase()));
        }
        if bytes.len() >= 2
            && bytes[0].is_ascii_alphabetic()
            && (bytes[1] == b'/' || bytes[1] == b'\\')
        {
            let drive = char::from(bytes[0]).to_ascii_uppercase();
            return Some(format!("{}:/{}", drive, &rest[2..]));
        }
        None
    }

    let rest = file.strip_prefix('/')?;
    drive_path(rest).or_else(|| rest.strip_prefix("cygdrive/").and_then(drive_path))
}

// Split on both '/' and '\\' to match OCaml behavior
pub fn normalize_path(dir: &str, file: &str) -> String {
    #[cfg(windows)]
    let normalized_file;
    #[cfg(windows)]
    let file = match msys_absolute_path(file) {
        Some(path) => {
            normalized_file = path;
            normalized_file.as_str()
        }
        None => file,
    };

    let parts: Vec<&str> = file.split(&['/', '\\'][..]).collect();
    match parts.as_slice() {
        ["", rest @ ..] if !rest.is_empty() => {
            // /<names> => /<names>
            construct_path(std::path::MAIN_SEPARATOR_STR, rest)
        }
        [root, rest @ ..] if is_windows_root(root) => {
            // c:\<names> => C:\<names>
            let root = root.to_uppercase();
            construct_path(&format!("{}{}", root, std::path::MAIN_SEPARATOR), rest)
        }
        _ => normalize_path_parts(dir, &parts),
    }
}

fn normalize_path_parts(dir: &str, parts: &[&str]) -> String {
    match parts {
        [".", rest @ ..] => {
            // ./<names> => dir/names
            normalize_path_parts(dir, rest)
        }
        ["..", rest @ ..] => {
            // ../<names> => parent(dir)/<names>
            let parent = Path::new(dir)
                .parent()
                .unwrap_or(Path::new(""))
                .to_string_lossy();
            normalize_path_parts(&parent, rest)
        }
        ["", rest @ ..] if !rest.is_empty() => {
            // .//<names> => dir/<names>
            normalize_path_parts(dir, rest)
        }
        _ => {
            // <names> => dir/<names>
            construct_path(dir, parts)
        }
    }
}

pub fn construct_path(dir: &str, parts: &[&str]) -> String {
    parts.iter().fold(dir.to_string(), |acc, part| {
        Path::new(&acc).join(part).to_string_lossy().into_owned()
    })
}

pub fn split_path(path: &str) -> Vec<String> {
    fn split_path_inner(path: &str, acc: &mut Vec<String>) {
        let dir = Path::new(path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());

        if path == dir {
            // Reached root
            // For relative paths like ".", stop
            if !Path::new(&dir).is_relative() {
                // For absolute paths, add the root
                match acc.first() {
                    // "/" becomes ["/"]
                    None => acc.insert(0, dir),
                    Some(_) => {
                        // "/path/to/foo.js" becomes ["/path", "to", "foo.js"]
                        let new_first =
                            Path::new(&dir).join(&acc[0]).to_string_lossy().into_owned();
                        acc[0] = new_first;
                    }
                }
            }
        } else {
            let basename = Path::new(path)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            acc.insert(0, basename);
            split_path_inner(&dir, acc);
        }
    }

    let mut result = Vec::new();
    split_path_inner(path, &mut result);
    result
}

fn make_relative(root: &[String], file: &[String]) -> Vec<String> {
    match (root, file) {
        ([dir1, root_rest @ ..], [dir2, file_rest @ ..]) if dir1 == dir2 => {
            make_relative(root_rest, file_rest)
        }
        (root, file) => {
            let mut result = vec!["..".to_string(); root.len()];
            result.extend_from_slice(file);
            result
        }
    }
}

pub fn relative_path(root: &Path, file: &str) -> String {
    let file_path = Path::new(file);
    match file_path.strip_prefix(root) {
        Ok(relative) => normalize_filename_dir_sep(&relative.to_string_lossy()).into_owned(),
        Err(_) => {
            // Fallback to component-based relative path computation
            let root = root.to_string_lossy();
            let root = normalize_filename_dir_sep(&root);
            let file = normalize_filename_dir_sep(file);
            let root_components = split_path(&root);
            let file_components = split_path(&file);
            make_relative(&root_components, &file_components).join("/")
        }
    }
}

fn path_with_trailing_sep(path: &Path) -> String {
    let path = path.to_string_lossy();
    if path.ends_with('/') || path.ends_with('\\') {
        path.into_owned()
    } else {
        format!("{}{}", path, std::path::MAIN_SEPARATOR)
    }
}

fn dirname_slash(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) => "/",
        Some(i) => &path[..i],
        None => path,
    }
}

pub fn absolute_path(root: &Path, file: &str) -> String {
    if file == "." || file.is_empty() {
        return root.to_string_lossy().into_owned();
    }

    if Path::new(file).is_absolute() {
        return file.to_string();
    }

    let file = normalize_filename_dir_sep(file);
    let parts: Vec<&str> = file.split('/').collect();
    let has_dots = parts.iter().any(|part| *part == "." || *part == "..");
    if !has_dots {
        return format!("{}{}", path_with_trailing_sep(root), file);
    }

    let root = root.to_string_lossy();
    let root = normalize_filename_dir_sep(&root);
    let root_len = root.len();
    let root = if root_len > 1 && root.ends_with('/') {
        &root[..root_len - 1]
    } else {
        &root
    };

    let mut dir = root.to_string();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "" | "." => i += 1,
            ".." => {
                dir = dirname_slash(&dir).to_string();
                i += 1;
            }
            _ => {
                let remaining = parts[i..].join("/");
                let sep = if !dir.is_empty() && dir.ends_with('/') {
                    ""
                } else {
                    "/"
                };
                return format!("{}{}{}", dir, sep, remaining);
            }
        }
    }

    dir
}

// helper to get the full path to the "flow-typed" library dir
pub fn get_flowtyped_path(root: &Path) -> PathBuf {
    make_path_absolute(root, "flow-typed")
}

pub fn filename_from_string(options: &FileOptions, path: &str) -> flow_parser::file_key::FileKey {
    use flow_parser::file_key::FileKey;

    let ext = Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{}", s));

    match ext.as_deref() {
        Some(".json") => FileKey::json_file_of_absolute(path),
        Some(ext) if options.module_resource_exts.contains(ext) => {
            FileKey::resource_file_of_absolute(path)
        }
        _ if is_configured_lib_file(options, path) => FileKey::lib_file_of_absolute(path),
        _ => FileKey::source_file_of_absolute(path),
    }
}

/// Returns the key used to store a configured library file.
pub fn lib_file_key(path: &str) -> flow_parser::file_key::FileKey {
    use flow_parser::file_key::FileKey;

    FileKey::lib_file_of_absolute(path)
}

/// Returns whether a file is one of the discovered global library files.
pub fn is_lib_file(
    all_unordered_libs: &std::collections::BTreeSet<FlowSmolStr>,
    file: &flow_parser::file_key::FileKey,
) -> bool {
    if file.is_lib_file() {
        return true;
    }
    let absolute = file.to_absolute();
    all_unordered_libs.contains(absolute.as_str())
        || (std::path::MAIN_SEPARATOR != '/'
            && all_unordered_libs.contains(
                absolute
                    .replace('/', std::path::MAIN_SEPARATOR_STR)
                    .as_str(),
            ))
}

pub fn mkdirp(path_str: &str, _perm: u32) {
    // On Windows, paths may use the verbatim (`\\?\C:\...`) or device
    // (`\\.\C:\...`) prefix. The `?` and `.` are illegal filename characters,
    // and our splitting logic below would otherwise turn them into a path
    // component, causing `mkdir ?` / `mkdir .` to fail with "filename,
    // directory name, or volume label syntax is incorrect" (os error 123).
    // Strip the prefix so we operate on a normal absolute path.
    #[cfg(windows)]
    let path_str = path_str
        .strip_prefix(r"\\?\")
        .or_else(|| path_str.strip_prefix(r"\\.\"))
        .unwrap_or(path_str);

    let parts: Vec<&str> = path_str
        .split(&['/', '\\'][..])
        .filter(|s| !s.is_empty())
        .collect();
    // If path_str is absolute, then path_prefix will be something like C:\ on
    // Windows and / on Linux
    let path_prefix = ABSOLUTE_PATH_REGEXP
        .find(path_str)
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    // On Windows, the Str.split above will mean the first part of an absolute
    // path will be something like C:, so let's remove that
    let parts: &[&str] = match parts.as_slice() {
        [first_part, rest @ ..]
            if format!("{}{}", first_part, std::path::MAIN_SEPARATOR) == path_prefix =>
        {
            rest
        }
        _ => &parts,
    };
    parts.iter().fold(path_prefix, |path_str, part| {
        let new_path_str = Path::new(&path_str)
            .join(part)
            .to_string_lossy()
            .into_owned();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            match std::fs::DirBuilder::new().mode(_perm).create(&new_path_str) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => panic!("mkdirp: mkdir {} failed: {}", new_path_str, e),
            }
        }
        #[cfg(not(unix))]
        {
            match std::fs::create_dir(&new_path_str) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => panic!("mkdirp: mkdir {} failed: {}", new_path_str, e),
            }
        }
        new_path_str
    });
}

pub fn generate_is_within_node_modules_fn(
    root: &Path,
    options: &FileOptions,
) -> impl Fn(&str) -> bool {
    let root_components = split_path(&root.to_string_lossy());
    let node_resolver_dirnames: HashSet<String> =
        options.node_resolver_dirnames.iter().cloned().collect();

    move |path: &str| -> bool {
        let file_components = split_path(path);
        let relative_parts = make_relative(&root_components, &file_components);
        relative_parts
            .iter()
            .any(|part| node_resolver_dirnames.contains(part))
    }
}

pub fn is_within_node_modules(root: &Path, options: &FileOptions, path: &str) -> bool {
    let checker = generate_is_within_node_modules_fn(root, options);
    checker(path)
}

pub fn imaginary_realpath(path: &str) -> String {
    // Realpath fails on non-existent paths. So let's find the longest prefix which exists. We
    // recurse using Path::exists, which is just a single stat, as opposed to canonicalize which
    // stats /foo, then /foo/bar, then /foo/bar/baz, etc
    fn find_real_prefix(path: &str, rev_suffix: &mut Vec<String>) -> String {
        let basename = Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        rev_suffix.insert(0, basename);
        let prefix = Path::new(path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".to_string());
        // Sys.file_exists should always return true for / and for . so we should never get into
        // infinite recursion. Let's assert that
        assert!(prefix != path);
        if Path::new(&prefix).exists() {
            prefix
        } else {
            find_real_prefix(&prefix, rev_suffix)
        }
    }

    let mut rev_suffix = Vec::new();
    let real_prefix = find_real_prefix(path, &mut rev_suffix);
    let abs = cached_canonicalize(Path::new(&real_prefix))
        .unwrap_or_else(|_| panic!("Realpath failed for existent path {}", real_prefix));
    let abs_str = abs.to_string_lossy().into_owned();
    rev_suffix.iter().fold(abs_str, |acc, part| {
        Path::new(&acc).join(part).to_string_lossy().into_owned()
    })
}

pub fn module_file_exts(options: &FileOptions) -> Vec<&str> {
    options
        .module_file_exts
        .iter()
        .map(|s| s.as_str())
        .collect()
}

pub fn node_resolver_dirnames(options: &FileOptions) -> &[String] {
    &options.node_resolver_dirnames
}

pub fn module_declaration_dirnames(options: &FileOptions) -> &[String] {
    &options.module_declaration_dirnames
}

pub fn canonicalize_filenames(
    cwd: &str,
    handle_imaginary: &dyn Fn(&str) -> String,
    filenames: &[String],
) -> Vec<String> {
    filenames
        .iter()
        .map(|filename| {
            let filename = crate::sys_utils::expanduser(filename);
            let filename = normalize_path(cwd, &filename);
            match cached_canonicalize(Path::new(&filename)) {
                Ok(abs) => abs.to_string_lossy().to_string(),
                Err(_) => handle_imaginary(&filename),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::FileOptions;
    use super::IgnorePattern;
    use crate::path_matcher::RootedGlob;

    #[test]
    fn is_valid_path_rejects_path_without_filename() {
        assert!(!super::is_valid_path(&FileOptions::default(), ""));
    }

    #[cfg(windows)]
    #[test]
    fn is_valid_path_rejects_windows_drive_root() {
        assert!(!super::is_valid_path(&FileOptions::default(), r"C:\"));
    }

    #[test]
    fn ignore_globs_preserve_ordered_negation() {
        let root = Path::new("/project");
        let patterns = vec![
            IgnorePattern {
                negated: false,
                backup: None,
                matcher: RootedGlob::new(root, "legacy/**").expect("test glob should compile"),
            },
            IgnorePattern {
                negated: false,
                backup: None,
                matcher: RootedGlob::new(root, "generated/**").expect("test glob should compile"),
            },
            IgnorePattern {
                negated: true,
                backup: None,
                matcher: RootedGlob::new(root, "generated/keep/**")
                    .expect("test glob should compile"),
            },
        ];

        assert_eq!(
            super::is_matching_ignore("/project/legacy/file.js", &patterns),
            (true, None)
        );
        assert_eq!(
            super::is_matching_ignore("/project/generated/file.js", &patterns),
            (true, None)
        );
        assert_eq!(
            super::is_matching_ignore("/project/generated/keep/file.js", &patterns),
            (false, None)
        );
    }

    #[cfg(windows)]
    #[test]
    fn cached_canonicalize_strips_windows_verbatim_prefix() {
        let tmp = std::env::temp_dir().join(format!(
            "flow_cached_canonicalize_verbatim_prefix_test_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&tmp).unwrap();

        let canonical = super::cached_canonicalize(&tmp).unwrap();

        let canonical = canonical.to_string_lossy();
        assert!(
            !canonical.starts_with(r"\\?\"),
            "expected cached_canonicalize to strip Windows verbatim prefix, got {}",
            canonical
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[cfg(windows)]
    #[test]
    fn mkdirp_handles_windows_verbatim_prefix() {
        // Repro for the panic
        //   mkdirp: mkdir ? failed: The filename, directory name, or
        //   volume label syntax is incorrect. (os error 123)
        // which used to fire when a path with the `\\?\` (verbatim) prefix
        // was passed in: the splitting logic produced a `?` directory
        // component that Windows refuses to create.
        let tmp = std::env::temp_dir();
        let tmp_str = tmp.to_string_lossy();
        // Make sure we exercise a `\\?\C:\...` style path even when the
        // platform's temp dir is plain.
        let extended = if tmp_str.starts_with(r"\\?\") {
            tmp_str.to_string()
        } else {
            format!(r"\\?\{}", tmp_str)
        };
        let nested = format!(
            "{}\\flow_mkdirp_verbatim_prefix_test\\nested\\dir",
            extended.trim_end_matches(['\\', '/']),
        );
        // Clean up first (ignore errors), then create.
        let _ = std::fs::remove_dir_all(format!(
            "{}\\flow_mkdirp_verbatim_prefix_test",
            tmp.to_string_lossy().trim_end_matches(['\\', '/']),
        ));
        super::mkdirp(&nested, 0o755);
        // The created directory should be reachable via the plain path
        // form, since the `\\?\` prefix is just an access-mode hint.
        let plain = format!(
            "{}\\flow_mkdirp_verbatim_prefix_test\\nested\\dir",
            tmp.to_string_lossy().trim_end_matches(['\\', '/']),
        );
        assert!(
            std::path::Path::new(&plain).is_dir(),
            "expected mkdirp to create {} when called with the \\\\?\\ prefix",
            plain
        );
        // Cleanup.
        let _ = std::fs::remove_dir_all(format!(
            "{}\\flow_mkdirp_verbatim_prefix_test",
            tmp.to_string_lossy().trim_end_matches(['\\', '/']),
        ));
    }

    #[test]
    fn absolute_path_preserves_relative_suffix_separators() {
        let root = if cfg!(windows) {
            std::path::Path::new(r"C:\project")
        } else {
            std::path::Path::new("/project")
        };
        let expected = if cfg!(windows) {
            r"C:\project\packages/app"
        } else {
            "/project/packages/app"
        };

        assert_eq!(super::absolute_path(root, "packages/app"), expected);
    }

    #[test]
    fn absolute_path_resolves_parent_segments_like_file_keys() {
        let root = if cfg!(windows) {
            std::path::Path::new(r"C:\tmp\project")
        } else {
            std::path::Path::new("/tmp/project")
        };
        let expected = if cfg!(windows) {
            "C:/tmp/outside/helper.js"
        } else {
            "/tmp/outside/helper.js"
        };

        assert_eq!(super::absolute_path(root, "../outside/helper.js"), expected);
    }

    #[cfg(not(windows))]
    #[test]
    fn normalize_path_ignores_empty_names_after_the_first() {
        assert_eq!(
            super::normalize_path("/project/dist", ".//index.js"),
            "/project/dist/index.js"
        );
        assert_eq!(
            super::normalize_path("/project/dist", "..//lib/index.js"),
            "/project/lib/index.js"
        );
        assert_eq!(
            super::normalize_path("/project/dist", "/lib/index.js"),
            "/lib/index.js"
        );
    }

    #[cfg(windows)]
    #[test]
    fn normalize_path_accepts_msys_drive_paths() {
        assert_eq!(
            super::normalize_path(r"C:\cwd", "/c/project/bar.js"),
            r"C:\project\bar.js"
        );
        assert_eq!(
            super::normalize_path(r"C:\cwd", "/cygdrive/c/project/bar.js"),
            r"C:\project\bar.js"
        );
    }
}
