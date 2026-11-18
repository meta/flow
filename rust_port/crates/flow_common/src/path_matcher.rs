/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use globset::GlobBuilder;
use globset::GlobMatcher;
use globset::escape;
use regex::Regex;

use crate::sys_utils::normalize_filename_dir_sep;

#[derive(Debug, Clone)]
pub struct PathMatcher {
    /// Stems extracted from paths
    stems: Vec<PathBuf>,
    /// Map from stems to list of (original path, regexified path)
    stem_map: BTreeMap<PathBuf, Vec<(String, Regex)>>,
    /// Glob patterns are matched against paths relative to their Flow root.
    glob_patterns: Vec<RootedGlob>,
}

#[derive(Debug, Clone)]
pub struct RootedGlob {
    matcher: GlobMatcher,
}

pub fn validate_glob(pattern: &str) -> Result<(), globset::Error> {
    GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .map(|_| ())
}

impl RootedGlob {
    pub fn new(root: &Path, pattern: &str) -> Result<Self, globset::Error> {
        let mut root = fixup_path(root);
        let mut pattern = pattern;
        while let Some(relative_pattern) = pattern.strip_prefix("../") {
            root.pop();
            pattern = relative_pattern;
        }
        let root_string = root.to_string_lossy();
        let root = escape(normalize_filename_dir_sep(&root_string).as_ref());
        let pattern = format!("{}/{pattern}", root.trim_end_matches('/'));
        let matcher = GlobBuilder::new(&pattern)
            .literal_separator(true)
            .build()?
            .compile_matcher();
        Ok(Self { matcher })
    }

    pub fn is_match(&self, path: &str) -> bool {
        self.matcher
            .is_match(normalize_filename_dir_sep(path).as_ref())
    }

    pub fn supports_directory_pruning(&self) -> bool {
        let pattern = self.matcher.glob().glob();
        pattern.ends_with('/')
            || pattern == "**"
            || pattern.ends_with("/**")
            || pattern == "**/*"
            || pattern.ends_with("/**/*")
    }
}

/// Given a path, return the max prefix not containing a wildcard or a terminal filename
fn path_stem(path: &Path) -> PathBuf {
    let wc = Regex::new(r"^[^*?]*[*?]").unwrap();
    // Strip filename
    let path = if path.exists() && !path.is_dir() {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    let path_str = path.to_string_lossy();
    // Strip back to non-wildcard prefix
    let stem = if let Some(m) = wc.find(&path_str) {
        let matched = m.as_str();
        Path::new(matched)
            .parent()
            .unwrap_or(Path::new(""))
            .to_string_lossy()
            .into_owned()
    } else {
        path_str.into_owned()
    };

    PathBuf::from(stem)
}

fn has_glob_metacharacter(component: &str) -> bool {
    let mut escaped = false;
    for character in component.chars() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if matches!(character, '*' | '?' | '[' | '{') {
            return true;
        }
    }
    false
}

fn glob_stem(root: &Path, pattern: &str) -> PathBuf {
    let mut stem = root.to_path_buf();
    let mut found_metacharacter = false;
    for component in Path::new(pattern).components() {
        if has_glob_metacharacter(&component.as_os_str().to_string_lossy()) {
            found_metacharacter = true;
            break;
        }
        stem.push(component.as_os_str());
    }
    let stem = fixup_path(&stem);
    if found_metacharacter {
        stem
    } else {
        path_stem(&stem)
    }
}

/// Translate a path with wildcards into a regex
fn path_patt(path: &Path) -> Regex {
    let path_str = path.to_string_lossy();
    let str = normalize_filename_dir_sep(&path_str);
    let mut regex_string = String::new();
    // Because we accept both * and **, convert in 2 steps
    for (i, s) in str.split("**").enumerate() {
        if i > 0 {
            // For subsequent parts, we had a "**" delimiter before this text
            regex_string.push_str(".*");
        }
        // note: unix directory separators specifiers only. Windows directory
        // separators will already have been normalized to unix directory
        // separators
        let s = s.replace('*', "[^/]*");
        // Replace ? with . (matches any single character)
        let s = s.replace('?', ".");
        regex_string.push_str(&s);
    }
    Regex::new(&regex_string).unwrap()
}

/// Helper - eliminate noncanonical entries where possible.
/// No other normalization is done
fn fixup_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    match crate::files::cached_canonicalize(path) {
        Ok(canonical) => {
            if canonical.to_string_lossy() == s {
                return path.to_path_buf();
            }
        }
        Err(_) => {}
    }
    let (root, entries): (String, Vec<&str>) = {
        let entries: Vec<&str> = if cfg!(windows) {
            s.split(['/', '\\']).collect()
        } else {
            s.split(std::path::MAIN_SEPARATOR).collect()
        };
        match entries.as_slice() {
            ["", rest @ ..] if !cfg!(windows) => {
                // "/foo" -> ("/", ["foo"])
                (std::path::MAIN_SEPARATOR.to_string(), rest.to_vec())
            }
            [root, rest @ ..] if cfg!(windows) && root.len() == 2 && root.as_bytes()[1] == b':' => {
                // "C:\\foo" -> ("C:\\", ["foo"])
                (
                    format!("{}{}", root, std::path::MAIN_SEPARATOR),
                    rest.to_vec(),
                )
            }
            // Relative path
            _ => ("".to_string(), entries),
        }
    };
    let mut processed_entries = Vec::new();
    for entry in entries {
        if entry == "." {
            continue;
        }
        if entry == ".." {
            if processed_entries.pop().is_none() {
                processed_entries.push(entry);
            }
        } else {
            processed_entries.push(entry);
        }
    }

    let mut result = Path::new(&root).to_path_buf();
    for entry in processed_entries {
        result = result.join(entry);
    }
    result
}

/// Filter a list of prefixes to only those with which f starts
fn find_prefixes<'a>(stems: &'a [PathBuf], f: &str) -> Vec<&'a PathBuf> {
    stems
        .iter()
        .filter(|prefix| {
            let prefix_str = prefix.to_string_lossy();
            f.starts_with(prefix_str.as_ref())
        })
        .collect()
}

/// Find a match for f in a list of patterns
fn match_patt<'a>(f: &str, patterns: &'a [(String, Regex)]) -> Option<&'a str> {
    for (path, patt) in patterns {
        if patt.is_match(f) {
            return Some(path);
        }
    }
    None
}

impl PathMatcher {
    pub fn empty() -> Self {
        Self {
            stems: Vec::new(),
            stem_map: BTreeMap::new(),
            glob_patterns: Vec::new(),
        }
    }

    pub fn stems(&self) -> &[PathBuf] {
        &self.stems
    }

    /// Add a path to the matcher, calculating the appropriate stem and pattern
    pub fn add_path(&mut self, path: &Path) {
        let path = fixup_path(path);
        let stem = path_stem(&path);
        let patt = path_patt(&path);
        let pstr = path.to_string_lossy().into_owned();

        if let Some(entries) = self.stem_map.get_mut(&stem) {
            entries.push((pstr, patt));
        } else {
            self.stem_map.insert(stem.clone(), vec![(pstr, patt)]);
            self.stems.push(stem);
        }
    }

    pub fn add_glob(&mut self, root: &Path, pattern: &str) -> Result<(), globset::Error> {
        self.stems.push(glob_stem(root, pattern));
        self.glob_patterns.push(RootedGlob::new(root, pattern)?);
        Ok(())
    }

    /// Check if a file path matches any pattern in the matcher
    pub fn matches(&self, f: &str) -> bool {
        self.matches_with_globs(f, |_| true)
    }

    /// Check if every path under the directory `dir` matches: `dir` matches a path, or a glob
    /// whose match on a directory carries to everything under it.
    pub fn matches_subtree(&self, dir: &str) -> bool {
        self.matches_with_globs(dir, RootedGlob::supports_directory_pruning)
    }

    fn matches_with_globs(&self, f: &str, glob_filter: impl Fn(&RootedGlob) -> bool) -> bool {
        let matching_stems = find_prefixes(&self.stems, f);
        let normalized_f = normalize_filename_dir_sep(f);

        matching_stems.iter().any(|stem| {
            if let Some(patts) = self.stem_map.get(*stem) {
                match_patt(&normalized_f, patts).is_some()
            } else {
                false
            }
        }) || self
            .glob_patterns
            .iter()
            .any(|glob| glob_filter(glob) && glob.is_match(f))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_path(path: &str) -> String {
        fixup_path(&Path::new("/project").join(path))
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn rooted_glob_is_separator_aware() {
        let glob = RootedGlob::new(Path::new("/project"), "src/*.{js,jsx}")
            .expect("test glob should compile");

        assert!(glob.is_match("/project/src/foo.js"));
        assert!(glob.is_match("/project/src/foo.jsx"));
        assert!(!glob.is_match("/project/src/nested/foo.js"));
    }

    #[test]
    fn rooted_glob_can_match_outside_the_root() {
        let glob = RootedGlob::new(Path::new("/project/root"), "../shared/**/*.js")
            .expect("test glob should compile");

        assert!(glob.is_match("/project/shared/nested/foo.js"));
        assert!(!glob.is_match("/project/root/shared/nested/foo.js"));
    }

    #[test]
    fn rooted_glob_treats_root_metacharacters_literally() {
        let glob = RootedGlob::new(Path::new("/project/[root]"), "src/**/*.js")
            .expect("test glob should compile");

        assert!(glob.is_match("/project/[root]/src/foo.js"));
        assert!(!glob.is_match("/project/r/src/foo.js"));
    }

    #[test]
    fn glob_directory_pruning_is_checked_against_globset() {
        let safe_recursive = [
            (
                "src/**",
                "src/generated",
                [
                    "src/generated/file.js",
                    "src/generated/nested/file.js",
                    "src/generated/.hidden",
                ],
            ),
            (
                "**",
                "generated",
                [
                    "generated/file.js",
                    "generated/nested/file.js",
                    "generated/.hidden",
                ],
            ),
            (
                "src-?/**",
                "src-a/generated",
                [
                    "src-a/generated/file.js",
                    "src-a/generated/nested/file.js",
                    "src-a/generated/.hidden",
                ],
            ),
            (
                "src-*/**",
                "src-long/generated",
                [
                    "src-long/generated/file.js",
                    "src-long/generated/nested/file.js",
                    "src-long/generated/.hidden",
                ],
            ),
            (
                "src-[ab]/**",
                "src-a/generated",
                [
                    "src-a/generated/file.js",
                    "src-a/generated/nested/file.js",
                    "src-a/generated/.hidden",
                ],
            ),
            (
                "src-[!x]/**",
                "src-a/generated",
                [
                    "src-a/generated/file.js",
                    "src-a/generated/nested/file.js",
                    "src-a/generated/.hidden",
                ],
            ),
            (
                "{src,lib}/**",
                "lib/generated",
                [
                    "lib/generated/file.js",
                    "lib/generated/nested/file.js",
                    "lib/generated/.hidden",
                ],
            ),
            (
                "**/generated/**",
                "packages/app/generated/output",
                [
                    "packages/app/generated/output/file.js",
                    "packages/app/generated/output/nested/file.js",
                    "packages/app/generated/output/.hidden",
                ],
            ),
            (
                r"src/\*/**",
                "src/*/generated",
                [
                    "src/*/generated/file.js",
                    "src/*/generated/nested/file.js",
                    "src/*/generated/.hidden",
                ],
            ),
            (
                "../shared/**",
                "../shared/generated",
                [
                    "../shared/generated/file.js",
                    "../shared/generated/nested/file.js",
                    "../shared/generated/.hidden",
                ],
            ),
            (
                "src/**/*",
                "src/generated",
                [
                    "src/generated/file.js",
                    "src/generated/nested/file.js",
                    "src/generated/.hidden",
                ],
            ),
            (
                "**/*",
                "generated",
                [
                    "generated/file.js",
                    "generated/nested/file.js",
                    "generated/.hidden",
                ],
            ),
        ];

        for (pattern, directory, descendants) in safe_recursive {
            let glob =
                RootedGlob::new(Path::new("/project"), pattern).expect("test glob should compile");
            assert!(glob.is_match(&project_path(directory)), "{pattern}");
            for descendant in descendants {
                assert!(
                    glob.is_match(&project_path(descendant)),
                    "{pattern}: {descendant}"
                );
            }
            assert!(glob.supports_directory_pruning(), "{pattern}");
            assert!(glob.supports_directory_pruning(), "{pattern}");
        }

        let explicit_directories = [
            ("src/", "src/"),
            ("src-?/", "src-a/"),
            ("src-*/", "src-long/"),
            ("src-[ab]/", "src-a/"),
            ("src-[!x]/", "src-a/"),
            ("{src,lib}/", "lib/"),
            (r"src/\*/", "src/*/"),
            ("../shared/", "../shared/"),
        ];
        for (pattern, directory) in explicit_directories {
            let glob =
                RootedGlob::new(Path::new("/project"), pattern).expect("test glob should compile");
            assert!(glob.is_match(&project_path(directory)), "{pattern}");
            assert!(glob.supports_directory_pruning(), "{pattern}");
            assert!(glob.supports_directory_pruning(), "{pattern}");
        }
    }

    #[test]
    fn supports_directory_pruning_rejects_unsafe_patterns() {
        let unsafe_globs = [
            ("src", "src", "src/file.js"),
            ("src/*", "src/generated", "src/generated/file.js"),
            ("src/?", "src/a", "src/a/file.js"),
            ("src/[ab]", "src/a", "src/a/file.js"),
            ("src/[!x]", "src/a", "src/a/file.js"),
            ("src/{a,b}", "src/a", "src/a/file.js"),
            ("**/generated", "src/generated", "src/generated/file.js"),
            (
                "src/**/generated",
                "src/a/generated",
                "src/a/generated/file.js",
            ),
            (r"src/\*", "src/*", "src/*/file.js"),
            ("src/*.js", "src/generated.js", "src/generated.js/file.js"),
        ];
        for (pattern, directory, descendant) in unsafe_globs {
            let glob =
                RootedGlob::new(Path::new("/project"), pattern).expect("test glob should compile");
            assert!(glob.is_match(&project_path(directory)), "{pattern}");
            assert!(
                !glob.is_match(&project_path(descendant)),
                "{pattern}: {descendant}"
            );
            assert!(!glob.supports_directory_pruning(), "{pattern}");
            assert!(!glob.supports_directory_pruning(), "{pattern}");
        }
    }

    #[test]
    fn path_matcher_combines_legacy_paths_and_globs() {
        let root = Path::new("/project");
        let mut matcher = PathMatcher::empty();
        matcher.add_path(Path::new("/legacy/included"));
        matcher
            .add_glob(root, "src/*.js")
            .expect("test glob should compile");

        assert!(matcher.matches("/legacy/included/file.js"));
        assert!(matcher.matches("/project/src/file.js"));
        assert!(!matcher.matches("/project/src/nested/file.js"));
        assert_eq!(
            matcher.stems(),
            &[
                PathBuf::from("/legacy/included"),
                PathBuf::from("/project/src"),
            ]
        );
    }
}
