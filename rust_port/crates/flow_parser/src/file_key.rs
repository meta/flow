/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// File_key.t stores relative path suffixes internally, following Hack's
// Relative_path.t approach. The project root and flowlib root are set once
// at startup via global state. to_absolute/to_string returns the absolute
// path on demand.
//
// LibFile and SourceFile paths under the flowlib root use a marker prefix
// internally to distinguish them from project-root files. This marker is
// stripped in to_string/to_absolute and replaced with the flowlib root.

use std::cell::RefCell;
use std::hash::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::RwLock;

use dupe::Dupe;
use hashbrown::HashTable;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum FileKeyInner {
    LibFile(String),
    SourceFile(String),
    JsonFile(String),
    /// A resource that might get required, like .css, .jpg, etc.
    /// We don't parse these, just check that they exist.
    ResourceFile(String),
}

pub const DTS_EXT: &str = ".d.ts";
pub const DMTS_EXT: &str = ".d.mts";
pub const DCTS_EXT: &str = ".d.cts";
pub const DTS_EXTENSIONS: [&str; 3] = [DCTS_EXT, DMTS_EXT, DTS_EXT];

pub fn has_dts_ext(filename: &str) -> bool {
    DTS_EXTENSIONS.iter().any(|ext| filename.ends_with(ext))
}

pub fn has_ts_ext(filename: &str) -> bool {
    [".ts", ".tsx", ".mts", ".cts"]
        .iter()
        .any(|ext| filename.ends_with(ext))
}

// Marker for flowlib LibFile paths. This is the single source of truth —
// saved_state.ml uses File_key.flowlib_marker, not a separate constant.
pub const FLOWLIB_MARKER: &str = "<BUILTIN_FLOW_LIB>///";

// Global root refs — set once at startup, before any File_key construction
// or to_string calls. All roots have enforced trailing slashes so that
// strip_prefix always splits at a directory boundary.
//
// Using `Option<String>` (not a bare `String`) so that "not yet initialized"
// is distinguishable from "set to empty". Following Hack's Relative_path
// convention: accessing an unset root raises an immediate, descriptive error
// rather than silently producing a broken path.
//
// See set_project_root/set_flowlib_root for the setters.
static PROJECT_ROOT: RwLock<Option<String>> = RwLock::new(None);

static FLOWLIB_ROOT: RwLock<Option<String>> = RwLock::new(None);

fn enforce_trailing_sep(s: &str, dir_sep: char) -> String {
    let len = s.len();
    if len > 0 && !s.ends_with('/') && !s.ends_with('\\') {
        format!("{}{}", s, dir_sep)
    } else {
        s.to_string()
    }
}

/// Normalize a path's directory separators to '/'. The dir_sep_char
/// parameter specifies which character to treat as a directory separator
/// (in addition to '/' which is always recognized). On Unix this is '/'
/// (making this a no-op); on Windows it is '\\'.
fn normalize_dir_sep_with(dir_sep_char: char, s: &str) -> String {
    if dir_sep_char == '/' {
        s.to_string()
    } else if s.contains(dir_sep_char) {
        s.chars()
            .map(|c| if c == dir_sep_char { '/' } else { c })
            .collect()
    } else {
        s.to_string()
    }
}

fn dirname_slash(s: &str) -> &str {
    match s.rfind('/') {
        Some(0) => "/",
        Some(i) => &s[..i],
        None => s,
    }
}

// Resolve a relative suffix against a root directory, handling ".." and "."
// segments. Same approach as Files.normalize_path, which can't be used
// directly due to parser -> common circular dep.
// `dir_sep` controls which character is treated as a directory separator
// for normalization (defaults to the platform separator).
fn resolve_root_with<F: FnOnce() -> String>(
    is_relative: fn(&str) -> bool,
    dir_sep: char,
    get_root: F,
    suffix: &str,
) -> String {
    let len = suffix.len();
    if len == 0 || suffix == "-" {
        return suffix.to_string();
    }
    if !is_relative(suffix) {
        return suffix.to_string();
    }

    let normalized = normalize_dir_sep_with(dir_sep, suffix);
    let parts: Vec<&str> = normalized.split('/').collect();
    let has_dots = parts.iter().any(|s| *s == "." || *s == "..");
    if !has_dots {
        // No ".." or "." segments — simple concatenation preserves the
        // original separators, important for cross-platform round-trips.
        let root = get_root();
        return format!("{}{}", root, suffix);
    }

    // Normalize root separators so dirname_slash (which only splits on '/')
    // can traverse up correctly on Windows where roots use backslashes.
    let root = normalize_dir_sep_with(dir_sep, &get_root());
    // Strip trailing separator — get_root() returns roots with enforced
    // trailing slashes, but dirname_slash "/foo/" just strips the slash
    // instead of going up a directory.
    let rlen = root.len();
    let root_str = if rlen > 1 && root.ends_with('/') {
        &root[..rlen - 1]
    } else {
        &root
    };

    let mut dir = root_str.to_string();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "" => {
                i += 1;
            }
            "." => {
                i += 1;
            }
            ".." => {
                dir = dirname_slash(&dir).to_string();
                i += 1;
            }
            _ => {
                // remaining segments
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

fn enforce_trailing_slash(s: &str) -> String {
    enforce_trailing_sep(s, std::path::MAIN_SEPARATOR)
}

pub fn set_project_root(root: &str) {
    *PROJECT_ROOT.write().unwrap() = Some(enforce_trailing_slash(root));
}

pub fn set_flowlib_root(root: &str) {
    *FLOWLIB_ROOT.write().unwrap() = Some(enforce_trailing_slash(root));
}

// Retrieve a root, crashing immediately if not set. Following Hack's
// Relative_path convention: an unset root is a programmer error that
// should be caught at the call site, not masked downstream.
pub fn get_project_root() -> String {
    PROJECT_ROOT
        .read()
        .unwrap()
        .clone()
        .expect("File_key: project_root has not been set")
}

pub fn get_flowlib_root() -> String {
    FLOWLIB_ROOT
        .read()
        .unwrap()
        .clone()
        .expect("File_key: flowlib_root has not been set")
}

pub fn relative_flowlib_path(path: &str) -> Option<String> {
    let guard = FLOWLIB_ROOT.read().unwrap();
    let root = guard.as_deref().filter(|root| !root.is_empty())?;
    let root = normalize_dir_sep_with(std::path::MAIN_SEPARATOR, root);
    let path = normalize_dir_sep_with(std::path::MAIN_SEPARATOR, path);
    path.strip_prefix(&root).map(str::to_owned)
}

fn is_relative(path: &str) -> bool {
    std::path::Path::new(path).is_relative()
}

// libs, then source and json files at the same priority since JSON files are
// basically source files. We don't actually read resource files so they come
// last
fn order_of_inner(inner: &FileKeyInner) -> i32 {
    match inner {
        FileKeyInner::LibFile(_) => 1,
        FileKeyInner::SourceFile(_) => 2,
        FileKeyInner::JsonFile(_) => 2,
        FileKeyInner::ResourceFile(_) => 3,
    }
}

fn inner_as_str(inner: &FileKeyInner) -> &str {
    match inner {
        FileKeyInner::LibFile(x) => x.as_str(),
        FileKeyInner::SourceFile(x) => x.as_str(),
        FileKeyInner::JsonFile(x) => x.as_str(),
        FileKeyInner::ResourceFile(x) => x.as_str(),
    }
}

impl PartialEq for FileKeyInner {
    fn eq(&self, other: &Self) -> bool {
        order_of_inner(self) == order_of_inner(other) && inner_as_str(self) == inner_as_str(other)
    }
}

impl Eq for FileKeyInner {}

impl Hash for FileKeyInner {
    fn hash<H: Hasher>(&self, state: &mut H) {
        order_of_inner(self).hash(state);
        inner_as_str(self).hash(state);
    }
}

/// A file key is interned for the lifetime of the process, so it is cloned without touching a
/// reference count. Reference counts shared by every worker that holds a location in a popular
/// file are a source of contention between workers.
#[derive(Debug, Clone, Dupe, serde::Serialize, serde::Deserialize)]
pub struct FileKey {
    #[serde(deserialize_with = "deserialize_interned")]
    inner: &'static FileKeyInner,
}

const INTERNER_SHARDS: usize = 64;

/// Each interned key is stored with its hash, so neither a lookup nor a resize of a shard hashes
/// a path again.
static INTERNER: [Mutex<HashTable<(u64, &'static FileKeyInner)>>; INTERNER_SHARDS] =
    [const { Mutex::new(HashTable::new()) }; INTERNER_SHARDS];

const LOCAL_CACHE_SIZE: usize = 4096;

thread_local! {
    /// A direct-mapped cache in front of the interner, so that most constructions find their
    /// interned key without taking a lock, while the memory held per thread stays bounded.
    static LOCAL_CACHE: RefCell<Vec<Option<&'static FileKeyInner>>> =
        RefCell::new(vec![None; LOCAL_CACHE_SIZE]);
}

fn intern(inner: FileKeyInner) -> &'static FileKeyInner {
    let hash = {
        let mut hasher = DefaultHasher::new();
        inner.hash(&mut hasher);
        hasher.finish()
    };
    let slot = (hash as usize) % LOCAL_CACHE_SIZE;
    if let Some(cached) = LOCAL_CACHE.with(|cache| cache.borrow()[slot])
        && *cached == inner
    {
        return cached;
    }
    let shard = ((hash >> 32) as usize) % INTERNER_SHARDS;
    let interned = {
        let mut interned_table = INTERNER[shard]
            .lock()
            .expect("file key interner lock should not be poisoned");
        match interned_table.find(hash, |&(_, interned)| *interned == inner) {
            Some(&(_, interned)) => interned,
            None => {
                let interned: &'static FileKeyInner = Box::leak(Box::new(inner));
                interned_table.insert_unique(hash, (hash, interned), |&(hash, _)| hash);
                interned
            }
        }
    };
    LOCAL_CACHE.with(|cache| cache.borrow_mut()[slot] = Some(interned));
    interned
}

fn deserialize_interned<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<&'static FileKeyInner, D::Error> {
    <FileKeyInner as serde::Deserialize>::deserialize(deserializer).map(intern)
}

impl PartialEq for FileKey {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.inner, other.inner) || self.inner == other.inner
    }
}

impl Eq for FileKey {}

impl Hash for FileKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}

fn order_of_filename(file_key: &FileKey) -> i32 {
    order_of_inner(file_key.inner)
}

impl PartialOrd for FileKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FileKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if std::ptr::eq(self.inner, other.inner) {
            return std::cmp::Ordering::Equal;
        }
        let k = order_of_filename(self).cmp(&order_of_filename(other));
        if k != std::cmp::Ordering::Equal {
            k
        } else {
            self.suffix().cmp(other.suffix())
        }
    }
}

pub fn compare_opt(a: Option<&FileKey>, b: Option<&FileKey>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
        (Some(a), Some(b)) => a.cmp(b),
    }
}

fn strip_prefix<'a>(prefix: &str, path: &'a str) -> &'a str {
    let plen = prefix.len();
    if plen > 0 && path.starts_with(prefix) {
        &path[plen..]
    } else {
        path
    }
}

fn relative_path_from(root: &str, path: &str) -> String {
    let dir_sep = std::path::MAIN_SEPARATOR;

    let split = |s: &str| -> Vec<String> {
        normalize_dir_sep_with(dir_sep, s)
            .split('/')
            .filter(|x| !x.is_empty())
            .map(|x| x.to_string())
            .collect()
    };

    let root_parts = split(root);
    let path_parts = split(path);

    let common_len = root_parts
        .iter()
        .zip(path_parts.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let remaining_root = &root_parts[common_len..];
    let remaining_path = &path_parts[common_len..];

    let ups: Vec<&str> = remaining_root.iter().map(|_| "..").collect();
    let remaining: Vec<&str> = remaining_path.iter().map(|s| s.as_str()).collect();

    let mut result: Vec<&str> = Vec::new();
    result.extend_from_slice(&ups);
    result.extend_from_slice(&remaining);
    result.join("/")
}

// The result always uses '/' separators for cross-platform consistency
// (saved state generated on Linux must work on Windows and vice versa).
pub fn strip_project_root(path: &str) -> String {
    let guard = PROJECT_ROOT.read().unwrap();
    match guard.as_deref() {
        Some(root) => {
            if path.starts_with(root) {
                normalize_dir_sep_with(std::path::MAIN_SEPARATOR, strip_prefix(root, path))
            } else if !is_relative(path) {
                relative_path_from(root, path)
            } else {
                path.to_string()
            }
        }
        None => path.to_string(),
    }
}

fn strip_flowlib_or_project_root(path: &str) -> String {
    let guard = FLOWLIB_ROOT.read().unwrap();
    match guard.as_deref() {
        Some(fl) if !fl.is_empty() && path.starts_with(fl) => {
            let stripped =
                normalize_dir_sep_with(std::path::MAIN_SEPARATOR, strip_prefix(fl, path));
            format!("{}{}", FLOWLIB_MARKER, stripped)
        }
        _ => strip_project_root(path),
    }
}

impl FileKey {
    pub fn new(inner: FileKeyInner) -> Self {
        Self {
            inner: intern(inner),
        }
    }

    pub fn suffix(&self) -> &str {
        inner_as_str(self.inner)
    }

    pub fn as_str(&self) -> &str {
        self.suffix()
    }

    pub fn inner(&self) -> &FileKeyInner {
        self.inner
    }

    // Returns the full absolute path — resolves the root on demand.
    // Allocates a new string via concatenation. Use suffix for hot paths
    // that don't need the absolute path (compare, Transaction hashing).
    pub fn to_absolute(&self) -> String {
        match self.inner {
            FileKeyInner::LibFile(x) | FileKeyInner::SourceFile(x) => {
                if let Some(rest) = x.strip_prefix(FLOWLIB_MARKER) {
                    resolve_root_with(
                        is_relative,
                        std::path::MAIN_SEPARATOR,
                        get_flowlib_root,
                        rest,
                    )
                } else {
                    resolve_root_with(is_relative, std::path::MAIN_SEPARATOR, get_project_root, x)
                }
            }
            FileKeyInner::JsonFile(x) | FileKeyInner::ResourceFile(x) => {
                resolve_root_with(is_relative, std::path::MAIN_SEPARATOR, get_project_root, x)
            }
        }
    }

    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(self.to_absolute())
    }

    pub fn is_lib_file(&self) -> bool {
        match self.inner {
            FileKeyInner::LibFile(_) => true,
            FileKeyInner::SourceFile(_) => false,
            FileKeyInner::JsonFile(_) => false,
            FileKeyInner::ResourceFile(_) => false,
        }
    }

    pub fn map<F: FnOnce(&str) -> String>(&self, f: F) -> FileKey {
        let new_inner = match self.inner {
            FileKeyInner::LibFile(filename) => FileKeyInner::LibFile(f(filename)),
            FileKeyInner::SourceFile(filename) => FileKeyInner::SourceFile(f(filename)),
            FileKeyInner::JsonFile(filename) => FileKeyInner::JsonFile(f(filename)),
            FileKeyInner::ResourceFile(filename) => FileKeyInner::ResourceFile(f(filename)),
        };
        FileKey::new(new_inner)
    }

    pub fn exists<F: FnOnce(&str) -> bool>(&self, f: F) -> bool {
        match self.inner {
            FileKeyInner::LibFile(intern)
            | FileKeyInner::SourceFile(intern)
            | FileKeyInner::JsonFile(intern)
            | FileKeyInner::ResourceFile(intern) => f(intern.as_str()),
        }
    }

    pub fn check_suffix(&self, sfx: &str) -> bool {
        self.exists(|filename| filename.ends_with(sfx))
    }

    pub fn chop_suffix(&self, sfx: &str) -> FileKey {
        self.map(|filename| filename.strip_suffix(sfx).expect(filename).to_string())
    }

    pub fn with_suffix(&self, sfx: &str) -> FileKey {
        self.map(|filename| format!("{}{}", filename, sfx))
    }

    pub fn source_file_of_absolute(path: &str) -> FileKey {
        FileKey::new(FileKeyInner::SourceFile(strip_flowlib_or_project_root(
            path,
        )))
    }

    pub fn json_file_of_absolute(path: &str) -> FileKey {
        FileKey::new(FileKeyInner::JsonFile(strip_project_root(path)))
    }

    pub fn resource_file_of_absolute(path: &str) -> FileKey {
        FileKey::new(FileKeyInner::ResourceFile(strip_project_root(path)))
    }

    pub fn lib_file_of_absolute(path: &str) -> FileKey {
        FileKey::new(FileKeyInner::LibFile(strip_flowlib_or_project_root(path)))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    static ROOTS_LOCK: Mutex<()> = Mutex::new(());

    // Exception-safe bracket: sets roots before the test body and restores
    // defaults afterwards, even if an assertion fails.
    fn with_roots<F: FnOnce()>(project: &str, flowlib: &str, f: F) {
        let _guard = ROOTS_LOCK.lock().unwrap();
        set_project_root(project);
        set_flowlib_root(flowlib);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        set_project_root("/project/");
        set_flowlib_root("/flowlib/");
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    fn default_roots<F: FnOnce()>(f: F) {
        with_roots("/data/project/", "/usr/lib/flowlib/", f);
    }

    #[test]
    fn source_file_in_root_suffix() {
        default_roots(|| {
            let fk = FileKey::source_file_of_absolute("/data/project/src/a.js");
            assert_eq!(fk.suffix(), "src/a.js");
        });
    }

    #[test]
    fn source_file_in_root_round_trip() {
        default_roots(|| {
            let fk = FileKey::source_file_of_absolute("/data/project/src/a.js");
            assert_eq!(fk.to_absolute(), "/data/project/src/a.js");
        });
    }

    #[test]
    fn source_file_out_of_root_suffix_is_relative() {
        default_roots(|| {
            let fk = FileKey::source_file_of_absolute("/other/place/foo.js");
            assert_eq!(fk.suffix(), "../../other/place/foo.js");
        });
    }

    #[test]
    fn source_file_out_of_root_round_trip() {
        default_roots(|| {
            let fk = FileKey::source_file_of_absolute("/other/place/foo.js");
            assert_eq!(fk.to_absolute(), "/other/place/foo.js");
        });
    }

    #[test]
    fn prefix_not_directory_boundary() {
        with_roots("/data/foo/", "/flowlib/", || {
            // /data/foo/ must NOT strip from /data/foobar/x.js
            let fk = FileKey::source_file_of_absolute("/data/foobar/x.js");
            assert_eq!(fk.suffix(), "../foobar/x.js");
            assert_eq!(fk.to_absolute(), "/data/foobar/x.js");
        });
    }

    #[test]
    fn root_string_at_non_prefix_position() {
        default_roots(|| {
            let fk = FileKey::source_file_of_absolute("/other/data/project/file.js");
            assert_eq!(fk.suffix(), "../../other/data/project/file.js");
            assert_eq!(fk.to_absolute(), "/other/data/project/file.js");
        });
    }

    #[test]
    fn json_file_out_of_root() {
        default_roots(|| {
            let fk = FileKey::json_file_of_absolute("/other/package.json");
            assert_eq!(fk.suffix(), "../../other/package.json");
            assert_eq!(fk.to_absolute(), "/other/package.json");
        });
    }

    #[test]
    fn resource_file_out_of_root() {
        default_roots(|| {
            let fk = FileKey::resource_file_of_absolute("/other/image.png");
            assert_eq!(fk.suffix(), "../../other/image.png");
            assert_eq!(fk.to_absolute(), "/other/image.png");
        });
    }

    #[test]
    fn lib_under_flowlib_root() {
        default_roots(|| {
            let fk = FileKey::lib_file_of_absolute("/usr/lib/flowlib/core.js");
            assert_eq!(fk.suffix(), format!("{}core.js", FLOWLIB_MARKER));
            assert_eq!(fk.to_absolute(), "/usr/lib/flowlib/core.js");
        });
    }

    #[test]
    fn source_file_under_flowlib_root() {
        default_roots(|| {
            let fk = FileKey::source_file_of_absolute("/usr/lib/flowlib/lib.dom.d.ts");
            assert_eq!(fk.suffix(), format!("{}lib.dom.d.ts", FLOWLIB_MARKER));
            assert_eq!(fk.to_absolute(), "/usr/lib/flowlib/lib.dom.d.ts");
        });
        with_roots("/data/moved/project/", "/tmp/other/flowlib/", || {
            let fk = FileKey::new(FileKeyInner::SourceFile(format!(
                "{}lib.dom.d.ts",
                FLOWLIB_MARKER
            )));
            assert_eq!(
                fk.to_absolute(),
                "/tmp/other/flowlib/lib.dom.d.ts",
                "a saved key should resolve against the loading server's flowlib root"
            );
        });
    }

    #[test]
    fn lib_under_project_root() {
        default_roots(|| {
            let fk = FileKey::lib_file_of_absolute("/data/project/lib/builtins.js");
            assert_eq!(fk.suffix(), "lib/builtins.js");
            assert_eq!(fk.to_absolute(), "/data/project/lib/builtins.js");
        });
    }

    #[test]
    fn sentinels_unchanged() {
        default_roots(|| {
            let fk = FileKey::new(FileKeyInner::SourceFile(String::new()));
            assert_eq!(fk.to_absolute(), "");

            let fk = FileKey::new(FileKeyInner::SourceFile("-".to_string()));
            assert_eq!(fk.to_absolute(), "-");
        });
    }

    #[test]
    fn absolute_suffix_preserved() {
        default_roots(|| {
            // If suffix is already absolute (e.g., created before root was set),
            // to_absolute returns it unchanged.
            let fk = FileKey::new(FileKeyInner::SourceFile("/abs/path/file.js".to_string()));
            assert_eq!(fk.to_absolute(), "/abs/path/file.js");
        });
    }

    #[test]
    fn enforce_trailing_sep_tests() {
        assert_eq!(enforce_trailing_sep("/data/project", '/'), "/data/project/");
        assert_eq!(
            enforce_trailing_sep("/data/project/", '/'),
            "/data/project/"
        );
        assert_eq!(enforce_trailing_sep("", '/'), "");
    }

    #[test]
    fn relative_path_from_tests() {
        assert_eq!(
            relative_path_from("/data/project/", "/other/place/foo.js"),
            "../../other/place/foo.js"
        );
        assert_eq!(
            relative_path_from("/data/foo/", "/data/foobar/x.js"),
            "../foobar/x.js"
        );
        assert_eq!(
            relative_path_from("/data/project/", "/data/project/src/a.js"),
            "src/a.js"
        );
    }

    #[test]
    fn compare_opt_tests() {
        let a = FileKey::new(FileKeyInner::SourceFile("a.js".to_string()));
        let b = FileKey::new(FileKeyInner::SourceFile("b.js".to_string()));
        assert_eq!(compare_opt(Some(&a), Some(&b)), std::cmp::Ordering::Less);
        assert_eq!(compare_opt(None, None), std::cmp::Ordering::Equal);
        assert_eq!(compare_opt(Some(&a), None), std::cmp::Ordering::Less);
        assert_eq!(compare_opt(None, Some(&b)), std::cmp::Ordering::Greater);
    }
}
