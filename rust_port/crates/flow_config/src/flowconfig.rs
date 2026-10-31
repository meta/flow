/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::DefaultHasher;
use std::path::Path;

use flow_common::options::AssertOperator;
use flow_common::options::LogSaving;
use flow_common::options::ModuleSystem;
use flow_common::options::ReactRule;
use flow_common::options::ReactRuntime;
use flow_common::options::SavedStateFetcher;
use flow_common::options::SupportedOs;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_lint_settings::lint_settings::LintSettings;
use flow_lint_settings::severity::Severity;
use flow_lint_settings::strict_mode_settings::StrictModeSettings;
use regex::Regex;
use starlark_map::small_map::SmallMap;

pub struct Warning(pub u32, pub String);
pub struct Error(pub u32, pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowconfigGlobPattern {
    source: String,
    pattern: String,
    negated: bool,
}

#[derive(Debug)]
pub enum FlowconfigGlobError {
    Empty,
    Absolute,
    ProjectRoot,
    Invalid(String),
}

impl FlowconfigGlobError {
    pub fn message(&self) -> String {
        match self {
            Self::Empty => "Glob pattern must not be empty".to_owned(),
            Self::Absolute => "Glob patterns must be relative to the project root".to_owned(),
            Self::ProjectRoot => "Glob patterns are already relative to the project root and must not use <PROJECT_ROOT>".to_owned(),
            Self::Invalid(error) => format!("Invalid glob pattern: {error}"),
        }
    }
}

impl FlowconfigGlobPattern {
    pub fn try_new(source: String, allow_negation: bool) -> Result<Self, FlowconfigGlobError> {
        let pattern = source.as_str();
        let (negated, pattern) = if allow_negation {
            match pattern.strip_prefix('!') {
                Some(pattern) => (true, pattern),
                None => (false, pattern),
            }
        } else {
            (false, pattern)
        };
        if pattern.is_empty() {
            return Err(FlowconfigGlobError::Empty);
        }
        if Path::new(pattern).is_absolute()
            || flow_common::files::ABSOLUTE_PATH_REGEXP.is_match(pattern)
        {
            return Err(FlowconfigGlobError::Absolute);
        }
        if pattern.contains(flow_common::files::PROJECT_ROOT_TOKEN) {
            return Err(FlowconfigGlobError::ProjectRoot);
        }
        flow_common::path_matcher::validate_glob(pattern)
            .map_err(|error| FlowconfigGlobError::Invalid(error.to_string()))?;
        let pattern = pattern.to_owned();
        Ok(Self {
            source,
            pattern,
            negated,
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    pub fn is_negated(&self) -> bool {
        self.negated
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileWatcher {
    NoFileWatcher,
    DFind,
    Watchman,
    EdenFS,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LazyMode {
    Lazy,
    NonLazy,
    /// lazy_mode=watchman is deprecated, but implies file_watcher=Watchman
    WatchmanDeprecated,
}

pub mod opts {
    use super::*;

    struct RawValue(u32, String);
    struct RawValues(Vec<RawValue>);
    struct RawOptions(SmallMap<String, RawValues>);

    enum ErrorKind {
        FailedToParseValue(String),
        FailedToSet(String),
        DuplicateOption,
    }

    struct OptError(u32, ErrorKind);

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BuiltinLib {
        Flowlib,
        FlowlibWithLibDomDts,
        Prelude,
        Tslib,
    }

    #[derive(Debug, Clone)]
    pub struct Opts {
        pub agent_helper_hints: bool,
        pub agent_progress_keepalive: Option<u32>,
        pub all: Option<bool>,
        pub autoimports: Option<bool>,
        pub autoimports_min_characters: Option<u32>,
        pub autoimports_ranked_by_usage: bool,
        pub autoimports_ranked_by_usage_boost_exact_match_min_length: u32,
        pub automatic_require_default: Option<bool>,
        pub babel_loose_array_spread: Option<bool>,
        pub ban_spread_key_props: Option<bool>,
        pub component_syntax: bool,
        pub async_component_syntax: bool,
        pub async_component_syntax_includes: Vec<String>,
        pub dev_only_refinement_info_as_errors: bool,
        pub emoji: Option<bool>,
        pub enable_const_params: Option<bool>,
        pub enums: bool,
        pub estimate_recheck_time: Option<bool>,
        pub export_as_namespace_support: bool,
        pub saved_state_restart_on_reinit: bool,
        pub facebook_fbs: Option<String>,
        pub facebook_fbt: Option<String>,
        pub facebook_module_interop: bool,
        pub file_watcher: Option<FileWatcher>,
        pub file_watcher_edenfs_throttle_time_ms: u32,
        pub file_watcher_edenfs_timeout: u32,
        pub file_watcher_edenfs_max_commit_distance: u32,
        pub file_watcher_edenfs_subprocess_mergebase: bool,
        pub file_watcher_mergebase_with: Option<String>,
        pub file_watcher_mergebase_with_git: Option<String>,
        pub file_watcher_mergebase_with_hg: Option<String>,
        pub file_watcher_timeout: Option<u32>,
        pub files_implicitly_include_root: bool,
        /// print spaces between brackets in object literals
        pub format_bracket_spacing: Option<bool>,
        /// prefer single-quoted strings
        pub format_single_quotes: Option<bool>,
        #[cfg(fbcode_build)]
        pub fox: bool,
        pub haste_disallow_dynamic_import: bool,
        pub haste_module_ref_prefix: Option<String>,
        pub haste_paths_excludes: Vec<String>,
        pub haste_paths_includes: Vec<String>,
        pub hook_compatibility: bool,
        pub hook_compatibility_includes: Vec<String>,
        pub hook_compatibility_excludes: Vec<String>,
        pub ignore_non_literal_requires: bool,
        pub include_warnings: bool,
        pub jest_integration: bool,
        pub lazy_mode: Option<LazyMode>,
        pub log_per_error_typing_telemetry: bool,
        pub log_saving: BTreeMap<String, LogSaving>,
        pub max_files_checked_per_worker: u32,
        pub max_files_checked_per_worker_rust_port: Option<u32>,
        pub max_header_tokens: u32,
        pub max_seconds_for_check_per_worker: f64,
        pub max_workers: Option<u32>,
        pub max_workers_full_check: Option<u32>,
        pub merge_timeout: Option<u32>,
        pub missing_module_generators: Vec<(Regex, String)>,
        pub module_declaration_dirnames: Vec<String>,
        pub module_file_exts: Vec<FlowSmolStr>,
        pub module_name_mappers: Vec<(Regex, String)>,
        pub module_resource_exts: HashSet<FlowSmolStr>,
        pub module_system: ModuleSystem,
        pub modules_are_use_strict: bool,
        pub multi_platform: Option<bool>,
        pub multi_platform_extensions: Vec<FlowSmolStr>,
        pub multi_platform_extension_group_mapping: Vec<(FlowSmolStr, Vec<FlowSmolStr>)>,
        pub multi_platform_ambient_supports_platform_project_overrides:
            Vec<(FlowSmolStr, Vec<FlowSmolStr>)>,
        pub munge_underscores: bool,
        pub builtin_lib: BuiltinLib,
        pub no_implicit_override: bool,
        pub no_unchecked_indexed_access: bool,
        pub node_modules_errors: bool,
        pub node_main_fields: Vec<String>,
        pub node_package_export_conditions: Vec<String>,
        pub node_resolver_allow_root_relative: bool,
        pub node_resolver_dirnames: Vec<String>,
        pub node_resolver_root_relative_dirnames: Vec<(Option<String>, String)>,
        pub normalize_intersections: bool,
        pub pattern_matching: Option<bool>,
        pub projects: Vec<FlowSmolStr>,
        pub projects_overlap_mapping: BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>,
        pub projects_path_mapping: Vec<(String, Vec<FlowSmolStr>)>,
        pub projects_strict_boundary: bool,
        pub react_custom_jsx_typing: bool,
        pub react_managed_attributes: bool,
        pub stylex_shorthand_prop: Option<String>,
        pub react_rules: Vec<ReactRule>,
        pub react_runtime: ReactRuntime,
        pub records: Option<bool>,
        pub records_includes: Vec<String>,
        pub recursion_limit: i32,
        pub relay_integration: bool,
        pub relay_integration_esmodules: bool,
        pub relay_integration_excludes: Vec<String>,
        pub relay_integration_module_prefix: Option<String>,
        pub relay_integration_module_prefix_includes: Vec<String>,
        pub root_name: Option<String>,
        pub saved_state_parallel_decompress: bool,
        pub saved_state_fetcher: SavedStateFetcher,
        pub saved_state_persist_export_index: bool,
        pub saved_state_skip_version_check: bool,
        pub supported_operating_systems: Vec<SupportedOs>,
        pub strict_es6_import_export: bool,
        pub strict_property_initialization: bool,
        pub export_star_excludes_default: Option<bool>,
        pub ts_syntax: Option<bool>,
        pub ts_utility_syntax: bool,
        pub tslib_syntax: Option<bool>,
        pub typescript_library_definition_support: bool,
        pub enable_custom_error: bool,
        pub assert_operator: AssertOperator,
        pub type_expansion_recursion_limit: u32,
        pub unsuppressable_error_codes: HashSet<String>,
        pub use_unknown_in_catch_variables: Option<bool>,
        pub vpn_less: bool,
        pub wait_for_recheck: bool,
        pub watchman_defer_states: Vec<String>,
        pub watchman_sync_timeout: Option<u32>,
    }

    // the order of this list determines precedence. ./foo resolves to foo.js before foo.json
    const MODULE_FILE_EXTS: &[&str] = &[".js", ".jsx", ".mjs", ".cjs", ".json"];

    const MODULE_RESOURCE_EXTS: &[&str] = &[
        ".css", ".jpg", ".png", ".gif", ".eot", ".svg", ".ttf", ".woff", ".woff2", ".mp4", ".webm",
        ".webp",
    ];

    pub fn default_options() -> Opts {
        let module_resource_exts = MODULE_RESOURCE_EXTS
            .iter()
            .map(|&s| FlowSmolStr::new(s))
            .collect();

        Opts {
            agent_helper_hints: false,
            agent_progress_keepalive: None,
            all: None,
            autoimports: None,
            autoimports_min_characters: None,
            autoimports_ranked_by_usage: true,
            autoimports_ranked_by_usage_boost_exact_match_min_length: 5,
            automatic_require_default: None,
            babel_loose_array_spread: None,
            ban_spread_key_props: None,
            component_syntax: true,
            async_component_syntax: false,
            async_component_syntax_includes: Vec::new(),
            dev_only_refinement_info_as_errors: false,
            emoji: None,
            enable_const_params: None,
            enums: true,
            estimate_recheck_time: None,
            export_as_namespace_support: false,
            saved_state_restart_on_reinit: true,
            facebook_fbs: None,
            facebook_fbt: None,
            facebook_module_interop: false,
            file_watcher: None,
            file_watcher_edenfs_throttle_time_ms: 50,
            file_watcher_edenfs_timeout: 60,
            file_watcher_edenfs_max_commit_distance: 0,
            file_watcher_edenfs_subprocess_mergebase: false,
            file_watcher_mergebase_with: None,
            file_watcher_mergebase_with_git: None,
            file_watcher_mergebase_with_hg: None,
            file_watcher_timeout: None,
            files_implicitly_include_root: true,
            format_bracket_spacing: None,
            format_single_quotes: None,
            #[cfg(fbcode_build)]
            fox: false,
            haste_disallow_dynamic_import: false,
            haste_module_ref_prefix: None,
            haste_paths_excludes: vec![
                ocaml_str_to_rust_regex("\\(.*\\)?/node_modules/.*"),
                ocaml_str_to_rust_regex("<PROJECT_ROOT>/@flowtyped/.*"),
            ],
            haste_paths_includes: vec![ocaml_str_to_rust_regex("<PROJECT_ROOT>/.*")],
            hook_compatibility: true,
            hook_compatibility_includes: Vec::new(),
            hook_compatibility_excludes: Vec::new(),
            ignore_non_literal_requires: false,
            include_warnings: false,
            jest_integration: false,
            lazy_mode: None,
            log_per_error_typing_telemetry: false,
            log_saving: BTreeMap::new(),
            max_files_checked_per_worker: 100,
            max_files_checked_per_worker_rust_port: None,
            max_header_tokens: 10,
            max_seconds_for_check_per_worker: 5.0,
            max_workers: None,
            max_workers_full_check: None,
            merge_timeout: Some(100),
            missing_module_generators: Vec::new(),
            module_declaration_dirnames: vec!["<PROJECT_ROOT>/@flowtyped".to_owned()],
            module_file_exts: MODULE_FILE_EXTS
                .iter()
                .map(|&s| FlowSmolStr::new(s))
                .collect(),
            module_name_mappers: Vec::new(),
            module_resource_exts,
            module_system: ModuleSystem::Node,
            modules_are_use_strict: false,
            multi_platform: None,
            multi_platform_extensions: Vec::new(),
            multi_platform_extension_group_mapping: Vec::new(),
            multi_platform_ambient_supports_platform_project_overrides: Vec::new(),
            munge_underscores: false,
            builtin_lib: BuiltinLib::FlowlibWithLibDomDts,
            no_implicit_override: false,
            no_unchecked_indexed_access: false,
            node_modules_errors: false,
            node_main_fields: vec!["main".to_owned()],
            node_package_export_conditions: vec![],
            node_resolver_allow_root_relative: false,
            node_resolver_dirnames: vec!["node_modules".to_owned()],
            node_resolver_root_relative_dirnames: vec![(None, String::new())],
            normalize_intersections: false,
            pattern_matching: None,
            projects: vec![FlowSmolStr::new_inline("default")],
            projects_overlap_mapping: BTreeMap::new(),
            projects_path_mapping: Vec::new(),
            projects_strict_boundary: false,
            react_custom_jsx_typing: false,
            react_managed_attributes: false,
            stylex_shorthand_prop: None,
            // Keep in sync with `component_syntax: true` above: when component
            // syntax is on (the default), the React rules are on too. Explicitly
            // setting `component_syntax=false` in a .flowconfig clears these again
            // (see `component_syntax_parser`).
            react_rules: vec![
                ReactRule::ValidateRefAccessDuringRender,
                ReactRule::DeepReadOnlyProps,
                ReactRule::DeepReadOnlyHookReturns,
                ReactRule::RulesOfHooks,
            ],
            react_runtime: ReactRuntime::Classic,
            records: None,
            records_includes: Vec::new(),
            recursion_limit: 10000,
            relay_integration: false,
            relay_integration_esmodules: false,
            relay_integration_excludes: Vec::new(),
            relay_integration_module_prefix: None,
            relay_integration_module_prefix_includes: vec![ocaml_str_to_rust_regex(
                "<PROJECT_ROOT>/.*",
            )],
            root_name: None,
            saved_state_parallel_decompress: false,
            saved_state_fetcher: SavedStateFetcher::DummyFetcher,
            saved_state_persist_export_index: true,
            saved_state_skip_version_check: false,
            supported_operating_systems: Vec::new(),
            strict_es6_import_export: false,
            strict_property_initialization: false,
            export_star_excludes_default: None,
            ts_syntax: None,
            ts_utility_syntax: true,
            tslib_syntax: None,
            typescript_library_definition_support: false,
            enable_custom_error: false,
            assert_operator: AssertOperator::Disabled,
            type_expansion_recursion_limit: 3,
            unsuppressable_error_codes: HashSet::new(),
            use_unknown_in_catch_variables: None,
            vpn_less: false,
            wait_for_recheck: false,
            watchman_defer_states: Vec::new(),
            watchman_sync_timeout: None,
        }
    }

    fn parse_lines(lines: &[(u32, String)]) -> Result<RawOptions, Error> {
        let mut result = SmallMap::new();
        for (line_num, line) in lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some((key, value)) = trimmed.split_once('=') {
                let key = key.trim().to_string();
                let value = value.trim().to_string();

                result
                    .entry(key)
                    .or_insert_with(|| RawValues(vec![]))
                    .0
                    .push(RawValue(*line_num, value));
            } else {
                return Err(Error(*line_num, "Unable to parse line.".to_string()));
            }
        }

        Ok(RawOptions(result))
    }

    /// `init` gets called on the options object immediately before
    /// parsing the *first* occurrence of the user-specified config option. This
    /// is useful in cases where the user's value should blow away the default
    /// value (rather than being aggregated to it).
    ///
    /// For example: We want the default value of 'module.file_ext' to be
    /// `[".js", ".jsx"]`, but if the user specifies any 'module.file_ext'
    /// settings, we want to start from a clean list.
    fn opt<T>(
        parser: impl Fn(&str) -> Result<T, String>,
        setter: fn(&mut Opts, T) -> Result<(), String>,
        init: Option<fn(&mut Opts)>,
        multiple: bool,
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        if let Some(init_fn) = init {
            init_fn(config);
        }

        if !multiple && values.0.len() > 1 {
            // Error when duplicate options were incorrectly given
            let dupe_line = values.0[0].0;
            return Err(OptError(dupe_line, ErrorKind::DuplicateOption));
        }
        for RawValue(line_num, value_str) in values.0 {
            let value = parser(&value_str)
                .map_err(|msg| OptError(line_num, ErrorKind::FailedToParseValue(msg)))?;
            setter(config, value).map_err(|msg| OptError(line_num, ErrorKind::FailedToSet(msg)))?;
        }
        Ok(())
    }

    fn optparse_string(s: &str) -> Result<String, String> {
        unescape_ocaml_string(s)
    }

    fn unescape_ocaml_string(s: &str) -> Result<String, String> {
        let mut result = String::with_capacity(s.len());
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '\\' {
                if i + 1 >= chars.len() {
                    return Err("Invalid ocaml string: Illegal backslash at end".to_string());
                }
                match chars[i + 1] {
                    '\\' => {
                        result.push('\\');
                        i += 2;
                    }
                    'n' => {
                        result.push('\n');
                        i += 2;
                    }
                    'r' => {
                        result.push('\r');
                        i += 2;
                    }
                    't' => {
                        result.push('\t');
                        i += 2;
                    }
                    'b' => {
                        result.push('\u{0008}');
                        i += 2;
                    }
                    '\'' => {
                        result.push('\'');
                        i += 2;
                    }
                    '"' => {
                        result.push('"');
                        i += 2;
                    }
                    _ => {
                        result.push(chars[i]);
                        result.push(chars[i + 1]);
                        i += 2;
                    }
                }
            } else {
                result.push(chars[i]);
                i += 1;
            }
        }
        Ok(result)
    }

    pub fn ocaml_str_to_rust_regex(pattern: &str) -> String {
        let mut result = String::with_capacity(pattern.len());
        let chars: Vec<char> = pattern.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '\\' && i + 1 < chars.len() {
                match chars[i + 1] {
                    '(' => {
                        result.push('(');
                        i += 2;
                    }
                    ')' => {
                        result.push(')');
                        i += 2;
                    }
                    '|' => {
                        result.push('|');
                        i += 2;
                    }
                    _ => {
                        result.push(chars[i]);
                        result.push(chars[i + 1]);
                        i += 2;
                    }
                }
            } else if chars[i] == '(' {
                result.push_str("\\(");
                i += 1;
            } else if chars[i] == ')' {
                result.push_str("\\)");
                i += 1;
            } else if chars[i] == '|' {
                result.push_str("\\|");
                i += 1;
            } else {
                result.push(chars[i]);
                i += 1;
            }
        }
        result
    }

    fn optparse_regexp(s: &str) -> Result<Regex, String> {
        let converted = ocaml_str_to_rust_regex(s);
        Regex::new(&converted).map_err(|e| {
            let reason = if e.to_string().contains("unclosed character class") {
                "[ class not closed by ]".to_string()
            } else {
                e.to_string()
            };
            format!("Invalid ocaml regular expression: {}", reason)
        })
    }

    fn ocaml_replacement_to_rust(template: &str) -> String {
        let mut result = String::with_capacity(template.len());
        let chars: Vec<char> = template.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '\\' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
                result.push_str("${");
                result.push(chars[i + 1]);
                result.push('}');
                i += 2;
            } else if chars[i] == '\\' && i + 1 < chars.len() && chars[i + 1] == '\\' {
                result.push('\\');
                i += 2;
            } else if chars[i] == '\\' && i + 1 < chars.len() {
                result.push('\\');
                result.push(chars[i + 1]);
                i += 2;
            } else {
                result.push(chars[i]);
                i += 1;
            }
        }
        result
    }

    fn enum_parser<T: Copy>(
        allowed_values: &[(&str, T)],
        setter: fn(&mut Opts, T) -> Result<(), String>,
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        let value_map: HashMap<_, _> = allowed_values.iter().map(|(k, v)| (*k, *v)).collect();

        let parser = |s: &str| -> Result<T, String> {
            value_map.get(s).cloned().ok_or_else(|| {
                let valid_values = allowed_values
                    .iter()
                    .map(|(k, _)| *k)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "Unsupported value: \"{}\". Supported values are: {}",
                    s, valid_values
                )
            })
        };

        opt(parser, setter, None, false, values, config)
    }

    const MAPPING_REGEX: &str = r"^'([^']*)'[ \t]*->[ \t]*'([^']*)'$";

    fn optparse_mapping(s: &str) -> Result<(String, String), String> {
        let re = Regex::new(MAPPING_REGEX).unwrap();
        if let Some(caps) = re.captures(s) {
            Ok((
                caps.get(1).unwrap().as_str().to_string(),
                caps.get(2).unwrap().as_str().to_string(),
            ))
        } else {
            Err(
                "Expected a mapping of form: 'single-quoted-string' -> 'single-quoted-string'"
                    .to_string(),
            )
        }
    }

    fn optparse_conditional_mapping(s: &str) -> Result<(String, Option<String>), String> {
        let re = Regex::new(MAPPING_REGEX).unwrap();
        if let Some(caps) = re.captures(s) {
            Ok((
                caps.get(1).unwrap().as_str().to_string(),
                Some(caps.get(2).unwrap().as_str().to_string()),
            ))
        } else {
            optparse_string(s).map(|s| (s, None))
        }
    }

    fn parse_boolean(
        setter: fn(&mut Opts, bool) -> Result<(), String>,
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        enum_parser(&[("true", true), ("false", false)], setter, values, config)
    }

    fn parse_string(
        setter: fn(&mut Opts, String) -> Result<(), String>,
        init: Option<fn(&mut Opts)>,
        multiple: bool,
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        opt(optparse_string, setter, init, multiple, values, config)
    }

    fn parse_uint(
        setter: fn(&mut Opts, u32) -> Result<(), String>,
        init: Option<fn(&mut Opts)>,
        multiple: bool,
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        let parser = |s: &str| {
            let v = s
                .parse::<i32>()
                .map_err(|e| format!("Failed to parse integer: {}", e))?;
            if v < 0 {
                Err("Number cannot be negative!".to_string())
            } else {
                Ok(v as u32)
            }
        };
        opt(parser, setter, init, multiple, values, config)
    }

    fn parse_mapping<T>(
        transformer: fn((String, String)) -> Result<T, String>,
        setter: fn(&mut Opts, T) -> Result<(), String>,
        init: Option<fn(&mut Opts)>,
        multiple: bool,
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        opt(
            |s| transformer(optparse_mapping(s)?),
            setter,
            init,
            multiple,
            values,
            config,
        )
    }

    fn parse_conditional_mapping<T>(
        transformer: fn((String, Option<String>)) -> Result<T, String>,
        setter: fn(&mut Opts, T) -> Result<(), String>,
        init: Option<fn(&mut Opts)>,
        multiple: bool,
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        opt(
            |s| transformer(optparse_conditional_mapping(s)?),
            setter,
            init,
            multiple,
            values,
            config,
        )
    }

    fn optparse_json(s: &str) -> Result<serde_json::Value, String> {
        serde_json::from_str(s).map_err(|e| format!("Failed to parse JSON: {}", e))
    }

    fn parse_json<T>(
        transformer: fn(serde_json::Value) -> Result<T, String>,
        setter: fn(&mut Opts, T) -> Result<(), String>,
        init: Option<fn(&mut Opts)>,
        multiple: bool,
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        opt(
            |s| transformer(optparse_json(s)?),
            setter,
            init,
            multiple,
            values,
            config,
        )
    }

    fn max_files_checked_per_worker_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_uint(
            |opts, v| {
                opts.max_files_checked_per_worker = v;
                Ok(())
            },
            None,
            false,
            values,
            config,
        )
    }

    fn max_files_checked_per_worker_rust_port_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_uint(
            |opts, v| {
                opts.max_files_checked_per_worker_rust_port = Some(v);
                Ok(())
            },
            None,
            false,
            values,
            config,
        )
    }

    fn max_seconds_for_check_per_worker_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_uint(
            |opts, v| {
                opts.max_seconds_for_check_per_worker = v as i32 as f64;
                Ok(())
            },
            None,
            false,
            values,
            config,
        )
    }

    fn file_ext_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        fn init_fn(opts: &mut Opts) {
            opts.module_file_exts = vec![];
        }
        parse_string(
            |opts, v| {
                let v = FlowSmolStr::new(v);
                if v.ends_with(".flow") {
                    return Err(format!(
                        "Cannot use file extension '{}' since it ends with the reserved extension '.flow'",
                        v
                    ));
                }
                if opts.multi_platform_extensions.contains(&v) {
                    return Err(format!(
                        "Cannot use file extension '{}' since it conflicts with the multiplatform extension '{}'",
                        v, v
                    ));
                }
                if !opts.module_file_exts.contains(&v) {
                    // ignore duplicates. doesn't seem super important to error.
                    opts.module_file_exts.push(v);
                }
                Ok(())
            },
            Some(init_fn),
            true,
            values,
            config,
        )
    }

    fn projects_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        fn init_fn(opts: &mut Opts) {
            opts.projects = vec![];
        }
        parse_string(
            |opts, v| {
                let v = FlowSmolStr::from(v);
                if !opts.projects.contains(&v) {
                    opts.projects.push(v);
                }
                Ok(())
            },
            Some(init_fn),
            true,
            values,
            config,
        )
    }

    fn projects_path_mapping_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        parse_mapping(
            |(path, projects_str)| {
                let projects: Vec<_> = projects_str
                    .split(',')
                    .map(|s| FlowSmolStr::new(s.trim()))
                    .collect();
                Ok((ocaml_str_to_rust_regex(&path), projects))
            },
            |opts, (path, projects)| {
                for project in &projects {
                    if !opts.projects.contains(project) {
                        return Err(format!(
                            "{} is not a known project name configured in `experimental.projects`.",
                            project
                        ));
                    }
                }

                // The validation and tracking below ensures that a project never appears in more than
                // one unique overlapping pattern.
                if projects.len() > 1 {
                    let ns_set: BTreeSet<FlowSmolStr> = projects.iter().cloned().collect();
                    for ns in &projects {
                        if let Some(existing_set) = opts.projects_overlap_mapping.get(ns) {
                            if existing_set != &ns_set {
                                return Err(format!(
                                    "Project {} appears in multiple overlapping project path mappings. This is unsupported.",
                                    ns
                                ));
                            }
                        } else {
                            opts.projects_overlap_mapping
                                .insert(ns.clone(), ns_set.clone());
                        }
                    }
                }

                opts.projects_path_mapping.push((path, projects));
                Ok(())
            },
            None,
            true,
            values,
            config,
        )
    }

    fn haste_paths_excludes_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        fn init_fn(opts: &mut Opts) {
            opts.haste_paths_excludes =
                vec![ocaml_str_to_rust_regex("<PROJECT_ROOT>/@flowtyped/.*")];
        }
        parse_string(
            |opts, v| {
                opts.haste_paths_excludes.push(ocaml_str_to_rust_regex(&v));
                Ok(())
            },
            Some(init_fn),
            true,
            values,
            config,
        )
    }

    fn log_saving_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        fn init_fn(opts: &mut Opts) {
            opts.log_saving = BTreeMap::new();
        }
        fn transformer(json: serde_json::Value) -> Result<(String, LogSaving), String> {
            let (method_name, threshold_time_ms, limit, rate) =
                if let serde_json::Value::Array(mut arr) = json
                    && arr.len() == 4
                {
                    let serde_json::Value::Number(d) = arr.pop().unwrap() else {
                        return Err("rate must be a number".to_owned());
                    };
                    let d = d.as_f64().unwrap();
                    let c = match arr.pop().unwrap() {
                        serde_json::Value::Number(c) => Some(c.as_u64().unwrap()),
                        serde_json::Value::Null => None,
                        _ => {
                            return Err("limit must be a number or null".to_owned());
                        }
                    };
                    let mut b = match arr.pop().unwrap() {
                        serde_json::Value::Number(b) => Some(b.as_u64().unwrap() as i64),
                        serde_json::Value::Null => None,
                        _ => {
                            return Err("threshold_time_ms must be a number or null".to_owned());
                        }
                    };
                    let serde_json::Value::String(a) = arr.pop().unwrap() else {
                        return Err("method name must be a string".to_owned());
                    };
                    if &a == "timeout" {
                        // timeout threshold is currently hardcoded as LspInteraction.max_age
                        b = None;
                    }
                    (a, b.unwrap_or(-1), c, d)
                } else {
                    return Err(
                        "must be of the form [\"method name\", threshold_time_ms, limit, rate]"
                            .to_owned(),
                    );
                };

            if !(0.0..=100.0).contains(&rate) {
                return Err(
                    "rate (fourth element) means a percentage, so must be within 0 to 100"
                        .to_string(),
                );
            }

            Ok((
                method_name,
                LogSaving {
                    threshold_time_ms,
                    limit,
                    rate,
                },
            ))
        }

        parse_json(
            transformer,
            |opts, (method_name, log_saving)| {
                opts.log_saving.insert(method_name, log_saving);
                Ok(())
            },
            Some(init_fn),
            true,
            values,
            config,
        )
    }

    fn haste_paths_includes_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        fn init_fn(opts: &mut Opts) {
            opts.haste_paths_includes = vec![];
        }
        parse_string(
            |opts, v| {
                opts.haste_paths_includes.push(ocaml_str_to_rust_regex(&v));
                Ok(())
            },
            Some(init_fn),
            true,
            values,
            config,
        )
    }

    fn component_syntax_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        parse_boolean(
            |opts, v| {
                if v {
                    opts.component_syntax = true;
                    opts.react_rules = vec![
                        ReactRule::ValidateRefAccessDuringRender,
                        ReactRule::DeepReadOnlyProps,
                        ReactRule::DeepReadOnlyHookReturns,
                        ReactRule::RulesOfHooks,
                    ];
                } else {
                    opts.component_syntax = false;
                    opts.react_rules = Vec::new();
                }
                Ok(())
            },
            values,
            config,
        )
    }

    fn hook_compatibility_includes_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        fn init_fn(opts: &mut Opts) {
            opts.hook_compatibility_includes = Vec::new();
        }
        parse_string(
            |opts, v| {
                opts.hook_compatibility_includes
                    .push(ocaml_str_to_rust_regex(&v));
                Ok(())
            },
            Some(init_fn),
            true,
            values,
            config,
        )
    }

    fn hook_compatibility_excludes_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        fn init_fn(opts: &mut Opts) {
            opts.hook_compatibility_excludes = Vec::new();
        }
        parse_string(
            |opts, v| {
                opts.hook_compatibility_excludes
                    .push(ocaml_str_to_rust_regex(&v));
                Ok(())
            },
            Some(init_fn),
            true,
            values,
            config,
        )
    }

    fn hook_compatibility_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        parse_boolean(
            |opts, v| {
                opts.hook_compatibility = v;
                Ok(())
            },
            values,
            config,
        )
    }

    fn async_component_syntax_includes_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        fn init_fn(opts: &mut Opts) {
            opts.async_component_syntax_includes = Vec::new();
        }
        parse_string(
            |opts, v| {
                opts.async_component_syntax_includes
                    .push(ocaml_str_to_rust_regex(&v));
                Ok(())
            },
            Some(init_fn),
            true,
            values,
            config,
        )
    }

    fn automatic_require_default_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_boolean(
            |opts, v| {
                opts.automatic_require_default = Some(v);
                Ok(())
            },
            values,
            config,
        )
    }

    fn babel_loose_array_spread_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_boolean(
            |opts, v| {
                opts.babel_loose_array_spread = Some(v);
                Ok(())
            },
            values,
            config,
        )
    }

    fn ban_spread_key_props_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        parse_boolean(
            |opts, v| {
                opts.ban_spread_key_props = Some(v);
                Ok(())
            },
            values,
            config,
        )
    }

    fn estimate_recheck_time_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        parse_boolean(
            |opts, v| {
                opts.estimate_recheck_time = Some(v);
                Ok(())
            },
            values,
            config,
        )
    }

    fn saved_state_restart_on_reinit_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_boolean(
            |opts, v| {
                opts.saved_state_restart_on_reinit = v;
                Ok(())
            },
            values,
            config,
        )
    }

    fn facebook_module_interop_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_boolean(
            |opts, v| {
                opts.facebook_module_interop = v;
                Ok(())
            },
            values,
            config,
        )
    }

    fn file_watcher_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        enum_parser(
            &[
                ("none", FileWatcher::NoFileWatcher),
                ("dfind", FileWatcher::DFind),
                ("watchman", FileWatcher::Watchman),
                ("edenfs", FileWatcher::EdenFS),
            ],
            |opts, v| {
                opts.file_watcher = Some(v);
                Ok(())
            },
            values,
            config,
        )
    }

    fn file_watcher_edenfs_throttle_time_ms_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_uint(
            |opts, v| {
                opts.file_watcher_edenfs_throttle_time_ms = v;
                Ok(())
            },
            None,
            false,
            values,
            config,
        )
    }

    fn file_watcher_edenfs_timeout_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_uint(
            |opts, v| {
                opts.file_watcher_edenfs_timeout = v;
                Ok(())
            },
            None,
            false,
            values,
            config,
        )
    }

    fn file_watcher_edenfs_max_commit_distance_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_uint(
            |opts, v| {
                opts.file_watcher_edenfs_max_commit_distance = v;
                Ok(())
            },
            None,
            false,
            values,
            config,
        )
    }

    fn file_watcher_edenfs_subprocess_mergebase_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_boolean(
            |opts, v| {
                opts.file_watcher_edenfs_subprocess_mergebase = v;
                Ok(())
            },
            values,
            config,
        )
    }

    fn file_watcher_mergebase_with_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_string(
            |opts, v| {
                opts.file_watcher_mergebase_with = Some(v);
                Ok(())
            },
            None,
            false,
            values,
            config,
        )
    }

    fn file_watcher_mergebase_with_git_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_string(
            |opts, v| {
                opts.file_watcher_mergebase_with_git = Some(v);
                Ok(())
            },
            None,
            false,
            values,
            config,
        )
    }

    fn file_watcher_mergebase_with_hg_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_string(
            |opts, v| {
                opts.file_watcher_mergebase_with_hg = Some(v);
                Ok(())
            },
            None,
            false,
            values,
            config,
        )
    }

    fn format_bracket_spacing_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        parse_boolean(
            |opts, v| {
                opts.format_bracket_spacing = Some(v);
                Ok(())
            },
            values,
            config,
        )
    }

    fn format_single_quotes_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        parse_boolean(
            |opts, v| {
                opts.format_single_quotes = Some(v);
                Ok(())
            },
            values,
            config,
        )
    }

    fn haste_module_ref_prefix_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_string(
            |opts, v| {
                opts.haste_module_ref_prefix = Some(v);
                Ok(())
            },
            None,
            false,
            values,
            config,
        )
    }

    fn ignore_non_literal_requires_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_boolean(
            |opts, v| {
                opts.ignore_non_literal_requires = v;
                Ok(())
            },
            values,
            config,
        )
    }

    fn lazy_mode_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        let allowed = vec![
            ("true", LazyMode::Lazy),
            ("false", LazyMode::NonLazy),
            // legacy, deprecated
            ("fs", LazyMode::Lazy),
            ("watchman", LazyMode::WatchmanDeprecated),
            ("none", LazyMode::NonLazy),
        ];
        enum_parser(
            &allowed,
            |opts, v| {
                opts.lazy_mode = Some(v);
                Ok(())
            },
            values,
            config,
        )
    }

    fn merge_timeout_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        parse_uint(
            |opts, v| {
                opts.merge_timeout = if v == 0 { None } else { Some(v) };
                Ok(())
            },
            None,
            false,
            values,
            config,
        )
    }

    fn missing_module_generators_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        fn init_fn(opts: &mut Opts) {
            opts.missing_module_generators = Vec::new();
        }
        parse_mapping(
            |(pattern, generator)| {
                let pattern = optparse_regexp(&pattern)?;
                let generator = ocaml_replacement_to_rust(&generator);
                Ok((pattern, generator))
            },
            |opts, v| {
                opts.missing_module_generators.push(v);
                Ok(())
            },
            Some(init_fn),
            true,
            values,
            config,
        )
    }

    fn module_system_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        enum_parser(
            &[("node", ModuleSystem::Node), ("haste", ModuleSystem::Haste)],
            |opts, v| {
                opts.module_system = v;
                Ok(())
            },
            values,
            config,
        )
    }

    fn multi_platform_extensions_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        fn init_fn(opts: &mut Opts) {
            opts.multi_platform_extensions = vec![];
        }
        parse_string(
            |opts, v| {
                let v = FlowSmolStr::from(v);
                if v.ends_with(".flow") {
                    return Err(format!(
                        "Cannot use file extension '{}' since it ends with the reserved extension '.flow'",
                        v
                    ));
                }
                if opts.module_file_exts.contains(&v) {
                    return Err(format!(
                        "Cannot use file extension '{}' since it conflicts with the module extension '{}'",
                        v, v
                    ));
                }
                if opts.multi_platform_extensions.contains(&v) {
                    return Ok(());
                }
                if opts.multi_platform_extensions.len() > 64 {
                    return Err(format!(
                        "Too many platforms extension are set. The max is {}",
                        64
                    ));
                }
                opts.multi_platform_extensions.push(v);
                Ok(())
            },
            Some(init_fn),
            true,
            values,
            config,
        )
    }

    fn multi_platform_extension_group_mapping_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_mapping(
            |(group_ext, platforms)| Ok((FlowSmolStr::from(group_ext), platforms)),
            |opts, (group_ext, platforms)| {
                if group_ext.ends_with(".flow") {
                    return Err(format!(
                        "Cannot use file extension '{}' since it ends with the reserved extension '.flow'",
                        group_ext
                    ));
                }
                if opts.module_file_exts.contains(&group_ext) {
                    return Err(format!(
                        "Cannot use file extension '{}' since it conflicts with the module extension '{}'",
                        group_ext, group_ext
                    ));
                }
                if opts.multi_platform_extensions.contains(&group_ext) {
                    return Ok(());
                }
                let platforms: Vec<FlowSmolStr> = platforms
                    .split(',')
                    .map(|s| FlowSmolStr::from(s.trim()))
                    .collect();
                for p in &platforms {
                    let ext = FlowSmolStr::from(format!(".{}", p));
                    if !opts.multi_platform_extensions.contains(&ext) {
                        return Err(format!("Unknown platform '{}'.", p));
                    }
                }
                opts.multi_platform_extension_group_mapping
                    .push((group_ext, platforms));
                Ok(())
            },
            None,
            true,
            values,
            config,
        )
    }

    fn multi_platform_ambient_supports_platform_project_overrides_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_mapping(
            |(project, platforms)| Ok((FlowSmolStr::from(project), platforms)),
            |opts, (project, platforms)| {
                let platforms: Vec<FlowSmolStr> = platforms
                    .split(',')
                    .map(|s| FlowSmolStr::from(s.trim()))
                    .collect();
                for p in &platforms {
                    let ext = FlowSmolStr::from(format!(".{}", p));
                    if !opts.multi_platform_extensions.contains(&ext) {
                        return Err(format!("Unknown platform '{}'.", p));
                    }
                }
                opts.multi_platform_ambient_supports_platform_project_overrides
                    .push((project, platforms));
                Ok(())
            },
            None,
            true,
            values,
            config,
        )
    }

    fn name_mapper_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        parse_mapping(
            |(pattern, template)| {
                let pattern = optparse_regexp(&pattern)?;
                let template = ocaml_replacement_to_rust(&template);
                Ok((pattern, template))
            },
            |opts, v| {
                opts.module_name_mappers.push(v);
                Ok(())
            },
            None,
            true,
            values,
            config,
        )
    }

    fn name_mapper_extension_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        parse_mapping(
            |(file_ext, template)| {
                let pattern_str = format!("^\\(.*\\)\\.{}$", regex::escape(&file_ext));
                let pattern = optparse_regexp(&pattern_str)?;
                let template = ocaml_replacement_to_rust(&template);
                Ok((pattern, template))
            },
            |opts, v| {
                opts.module_name_mappers.push(v);
                Ok(())
            },
            None,
            true,
            values,
            config,
        )
    }

    fn module_declaration_dirnames_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_string(
            |opts, v| {
                opts.module_declaration_dirnames.push(v);
                Ok(())
            },
            Some(|opts: &mut Opts| opts.module_declaration_dirnames.clear()),
            true,
            values,
            config,
        )
    }

    fn node_main_field_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        parse_string(
            |opts, v| {
                opts.node_main_fields.push(v);
                Ok(())
            },
            Some(|opts: &mut Opts| opts.node_main_fields.clear()),
            true,
            values,
            config,
        )
    }

    fn node_package_export_condition_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_string(
            |opts, v| {
                opts.node_package_export_conditions.push(v);
                Ok(())
            },
            Some(|opts: &mut Opts| opts.node_package_export_conditions.clear()),
            true,
            values,
            config,
        )
    }

    fn node_resolve_dirname_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        parse_string(
            |opts, v| {
                if &v == "." || &v == ".." {
                    return Err(format!(
                        "\"{v}\" is not a valid value for `module.system.node.resolve_dirname`. Each value must be a valid directory name. Maybe try `module.system.node.allow_root_relative=true`?"
                    ));
                }
                opts.node_resolver_dirnames.push(v);
                Ok(())
            },
            Some(|opts: &mut Opts| opts.node_resolver_dirnames.clear()),
            true,
            values,
            config,
        )
    }

    fn node_resolver_allow_root_relative_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_boolean(
            |opts, v| {
                opts.node_resolver_allow_root_relative = v;
                Ok(())
            },
            values,
            config,
        )
    }

    fn node_resolver_root_relative_dirnames_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        fn init_fn(opts: &mut Opts) {
            opts.node_resolver_root_relative_dirnames = vec![];
        }
        parse_conditional_mapping(
            |(applicable_directory, root_relative_dirname)| match root_relative_dirname {
                Some(root_relative_dirname) => {
                    Ok((Some(applicable_directory), root_relative_dirname))
                }
                None => Ok((None, applicable_directory)),
            },
            |opts, v| {
                opts.node_resolver_root_relative_dirnames.push(v);
                Ok(())
            },
            Some(init_fn),
            true,
            values,
            config,
        )
    }

    fn react_runtime_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        enum_parser(
            &[
                ("classic", ReactRuntime::Classic),
                ("automatic", ReactRuntime::Automatic),
            ],
            |opts, v| {
                opts.react_runtime = v;
                Ok(())
            },
            values,
            config,
        )
    }

    fn relay_integration_excludes_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        fn init_fn(opts: &mut Opts) {
            opts.relay_integration_excludes = vec![];
        }
        parse_string(
            |opts, v| {
                opts.relay_integration_excludes
                    .push(ocaml_str_to_rust_regex(&v));
                Ok(())
            },
            Some(init_fn),
            true,
            values,
            config,
        )
    }

    fn relay_integration_module_prefix_includes_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        fn init_fn(opts: &mut Opts) {
            opts.relay_integration_module_prefix_includes = vec![];
        }
        parse_string(
            |opts, v| {
                opts.relay_integration_module_prefix_includes
                    .push(ocaml_str_to_rust_regex(&v));
                Ok(())
            },
            Some(init_fn),
            true,
            values,
            config,
        )
    }

    fn root_name_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        parse_string(
            |opts, v| {
                opts.root_name = Some(v);
                Ok(())
            },
            None,
            false,
            values,
            config,
        )
    }

    fn saved_state_fetcher_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        enum_parser(
            &[
                ("none", SavedStateFetcher::DummyFetcher),
                ("local", SavedStateFetcher::LocalFetcher),
                ("scm", SavedStateFetcher::ScmFetcher),
                ("fb", SavedStateFetcher::FbFetcher),
            ],
            |opts, v| {
                opts.saved_state_fetcher = v;
                Ok(())
            },
            values,
            config,
        )
    }

    fn saved_state_parallel_decompress_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_boolean(
            |opts, v| {
                opts.saved_state_parallel_decompress = v;
                Ok(())
            },
            values,
            config,
        )
    }

    fn saved_state_persist_export_index_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_boolean(
            |opts, v| {
                opts.saved_state_persist_export_index = v;
                Ok(())
            },
            values,
            config,
        )
    }

    fn assert_operator_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        enum_parser(
            &[
                ("false", AssertOperator::Disabled),
                ("disabled", AssertOperator::Disabled),
                ("true", AssertOperator::Enabled),
                ("enabled", AssertOperator::Enabled),
                ("specialized", AssertOperator::Specialized),
            ],
            |opts, v| {
                opts.assert_operator = v;
                Ok(())
            },
            values,
            config,
        )
    }

    fn strict_es6_import_export_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_boolean(
            |opts, v| {
                opts.strict_es6_import_export = v;
                Ok(())
            },
            values,
            config,
        )
    }

    fn unsuppressable_error_codes_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        fn init_fn(opts: &mut Opts) {
            opts.unsuppressable_error_codes = HashSet::new();
        }
        parse_string(
            |opts, v| {
                opts.unsuppressable_error_codes.insert(v);
                Ok(())
            },
            Some(init_fn),
            true,
            values,
            config,
        )
    }

    fn use_unknown_in_catch_variables_parser(
        values: RawValues,
        config: &mut Opts,
    ) -> Result<(), OptError> {
        parse_boolean(
            |opts, v| {
                opts.use_unknown_in_catch_variables = Some(v);
                Ok(())
            },
            values,
            config,
        )
    }

    fn watchman_defer_states_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        parse_string(
            |opts, v| {
                opts.watchman_defer_states.push(v);
                Ok(())
            },
            None,
            true,
            values,
            config,
        )
    }

    fn watchman_sync_timeout_parser(values: RawValues, config: &mut Opts) -> Result<(), OptError> {
        parse_uint(
            |opts, v| {
                opts.watchman_sync_timeout = Some(v);
                Ok(())
            },
            None,
            false,
            values,
            config,
        )
    }

    pub fn parse(config: &mut Opts, lines: &[(u32, String)]) -> Result<Vec<Warning>, Error> {
        let mut raw_opts = parse_lines(lines)?;
        let mut warnings = Vec::new();

        let error_of_opt_error = |key: &str, OptError(line, kind): OptError| {
            let msg = match kind {
                ErrorKind::FailedToParseValue(m) => {
                    format!("Error parsing value for \"{}\". {}", key, m)
                }
                ErrorKind::FailedToSet(m) => {
                    format!("Error setting value for \"{}\". {}", key, m)
                }
                ErrorKind::DuplicateOption => format!("Duplicate option: \"{}\"", key),
            };
            Error(line, msg)
        };

        type Parser = fn(RawValues, &mut Opts) -> Result<(), OptError>;
        let parsers: &[(&str, Parser)] = &[
            ("agent_progress_keepalive", |values, config| {
                parse_uint(
                    |opts, v| {
                        opts.agent_progress_keepalive = Some(v);
                        Ok(())
                    },
                    None,
                    false,
                    values,
                    config,
                )
            }),
            ("all", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.all = Some(v);
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("autoimports", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.autoimports = Some(v);
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("autoimports.min_characters", |values, config| {
                parse_uint(
                    |opts, v| {
                        if opts.autoimports == Some(false) {
                            return Err(
                                "Cannot be configured unless autoimport is enabled.".to_string()
                            );
                        }
                        opts.autoimports_min_characters = Some(v);
                        Ok(())
                    },
                    None,
                    false,
                    values,
                    config,
                )
            }),
            ("autoimports_ranked_by_usage", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.autoimports_ranked_by_usage = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            (
                "autoimports_ranked_by_usage.experimental.boost_exact_match_min_length",
                |values, config| {
                    parse_uint(
                        |opts, v| {
                            if opts.autoimports == Some(false) {
                                return Err("Cannot be configured unless autoimport is enabled."
                                    .to_string());
                            }
                            opts.autoimports_ranked_by_usage_boost_exact_match_min_length = v;
                            Ok(())
                        },
                        None,
                        false,
                        values,
                        config,
                    )
                },
            ),
            ("babel_loose_array_spread", |values, config| {
                babel_loose_array_spread_parser(values, config)
            }),
            ("ban_spread_key_props", |values, config| {
                ban_spread_key_props_parser(values, config)
            }),
            // check_is_status is deprecated and ignored. `flow check` is always an alias for `flow status`.
            ("check_is_status", |values, config| {
                parse_boolean(|_opts, _v| Ok(()), values, config)
            }),
            ("component_syntax", |values, config| {
                component_syntax_parser(values, config)
            }),
            ("dev_only.refinement_info_as_errors", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.dev_only_refinement_info_as_errors = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("emoji", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.emoji = Some(v);
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("enums", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.enums = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("estimate_recheck_time", |values, config| {
                estimate_recheck_time_parser(values, config)
            }),
            ("saved_state_restart_on_reinit", |values, config| {
                saved_state_restart_on_reinit_parser(values, config)
            }),
            ("experimental.agent_helper_hints", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.agent_helper_hints = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("experimental.assert_operator", |values, config| {
                assert_operator_parser(values, config)
            }),
            ("experimental.async_component_syntax", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.async_component_syntax = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            (
                "experimental.async_component_syntax.includes",
                |values, config| async_component_syntax_includes_parser(values, config),
            ),
            (
                "experimental.component_syntax.hook_compatibility",
                |values, config| hook_compatibility_parser(values, config),
            ),
            (
                "experimental.component_syntax.hook_compatibility.excludes",
                |values, config| hook_compatibility_excludes_parser(values, config),
            ),
            (
                "experimental.component_syntax.hook_compatibility.includes",
                |values, config| hook_compatibility_includes_parser(values, config),
            ),
            ("experimental.const_params", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.enable_const_params = Some(v);
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("experimental.enable_custom_error", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.enable_custom_error = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            (
                "experimental.export_as_namespace_support",
                |values, config| {
                    parse_boolean(
                        |opts, v| {
                            opts.export_as_namespace_support = v;
                            Ok(())
                        },
                        values,
                        config,
                    )
                },
            ),
            ("experimental.facebook_module_interop", |values, config| {
                facebook_module_interop_parser(values, config)
            }),
            (
                "experimental.log_per_error_typing_telemetry",
                |values, config| {
                    parse_boolean(
                        |opts, v| {
                            opts.log_per_error_typing_telemetry = v;
                            Ok(())
                        },
                        values,
                        config,
                    )
                },
            ),
            (
                "experimental.module.automatic_require_default",
                |values, config| automatic_require_default_parser(values, config),
            ),
            ("experimental.multi_platform", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.multi_platform = Some(v);
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            (
                "experimental.multi_platform.extensions",
                |values, config| multi_platform_extensions_parser(values, config),
            ),
            (
                "experimental.multi_platform.ambient_supports_platform.project_overrides",
                |values, config| {
                    multi_platform_ambient_supports_platform_project_overrides_parser(
                        values, config,
                    )
                },
            ),
            (
                "experimental.multi_platform.extension_group_mapping",
                |values, config| multi_platform_extension_group_mapping_parser(values, config),
            ),
            ("experimental.normalize_intersections", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.normalize_intersections = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("experimental.projects", |values, config| {
                projects_parser(values, config)
            }),
            ("experimental.projects.strict_boundary", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.projects_strict_boundary = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("experimental.projects_path_mapping", |values, config| {
                projects_path_mapping_parser(values, config)
            }),
            ("experimental.react_managed_attributes", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.react_managed_attributes = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("experimental.records", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.records = Some(v);
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("experimental.records.includes", |values, config| {
                fn init_fn(opts: &mut Opts) {
                    opts.records_includes = vec![];
                }
                parse_string(
                    |opts, v| {
                        opts.records_includes.push(ocaml_str_to_rust_regex(&v));
                        Ok(())
                    },
                    Some(init_fn),
                    true,
                    values,
                    config,
                )
            }),
            ("experimental.strict_es6_import_export", |values, config| {
                strict_es6_import_export_parser(values, config)
            }),
            (
                "experimental.strict_property_initialization",
                |values, config| {
                    parse_boolean(
                        |opts, v| {
                            opts.strict_property_initialization = v;
                            Ok(())
                        },
                        values,
                        config,
                    )
                },
            ),
            (
                "experimental.export_star_excludes_default",
                |values, config| {
                    parse_boolean(
                        |opts, v| {
                            opts.export_star_excludes_default = Some(v);
                            Ok(())
                        },
                        values,
                        config,
                    )
                },
            ),
            ("experimental.ts_syntax", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.ts_syntax = Some(v);
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("experimental.ts_utility_syntax", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.ts_utility_syntax = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("experimental.tslib_syntax", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.tslib_syntax = Some(v);
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            (
                "experimental.typescript_library_definition_support",
                |values, config| {
                    parse_boolean(
                        |opts, v| {
                            opts.typescript_library_definition_support = v;
                            Ok(())
                        },
                        values,
                        config,
                    )
                },
            ),
            (
                "experimental.type_expansion_recursion_limit",
                |values, config| {
                    parse_uint(
                        |opts, v| {
                            opts.type_expansion_recursion_limit = v;
                            Ok(())
                        },
                        None,
                        false,
                        values,
                        config,
                    )
                },
            ),
            ("facebook.fbs", |values, config| {
                parse_string(
                    |opts, v| {
                        opts.facebook_fbs = Some(v);
                        Ok(())
                    },
                    None,
                    false,
                    values,
                    config,
                )
            }),
            ("facebook.fbt", |values, config| {
                parse_string(
                    |opts, v| {
                        opts.facebook_fbt = Some(v);
                        Ok(())
                    },
                    None,
                    false,
                    values,
                    config,
                )
            }),
            ("file_watcher", |values, config| {
                file_watcher_parser(values, config)
            }),
            ("file_watcher.edenfs.throttle_time_ms", |values, config| {
                file_watcher_edenfs_throttle_time_ms_parser(values, config)
            }),
            ("file_watcher.edenfs.timeout", |values, config| {
                file_watcher_edenfs_timeout_parser(values, config)
            }),
            (
                "file_watcher.edenfs.max_commit_distance",
                |values, config| file_watcher_edenfs_max_commit_distance_parser(values, config),
            ),
            (
                "file_watcher.edenfs.subprocess_mergebase",
                |values, config| file_watcher_edenfs_subprocess_mergebase_parser(values, config),
            ),
            ("file_watcher.mergebase_with", |values, config| {
                file_watcher_mergebase_with_parser(values, config)
            }),
            ("file_watcher.mergebase_with_git", |values, config| {
                file_watcher_mergebase_with_git_parser(values, config)
            }),
            ("file_watcher.mergebase_with_hg", |values, config| {
                file_watcher_mergebase_with_hg_parser(values, config)
            }),
            ("file_watcher.defer_state", |values, config| {
                watchman_defer_states_parser(values, config)
            }),
            ("file_watcher.watchman.defer_state", |values, config| {
                watchman_defer_states_parser(values, config)
            }),
            ("file_watcher.watchman.sync_timeout", |values, config| {
                watchman_sync_timeout_parser(values, config)
            }),
            ("file_watcher_timeout", |values, config| {
                parse_uint(
                    |opts, v| {
                        opts.file_watcher_timeout = Some(v);
                        Ok(())
                    },
                    None,
                    false,
                    values,
                    config,
                )
            }),
            ("files.implicitly_include_root", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.files_implicitly_include_root = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("format.bracket_spacing", |values, config| {
                format_bracket_spacing_parser(values, config)
            }),
            ("format.single_quotes", |values, config| {
                format_single_quotes_parser(values, config)
            }),
            ("include_warnings", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.include_warnings = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("jest_integration", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.jest_integration = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("lazy_mode", |values, config| {
                lazy_mode_parser(values, config)
            }),
            ("log_saving", |values, config| {
                log_saving_parser(values, config)
            }),
            ("max_header_tokens", |values, config| {
                parse_uint(
                    |opts, v| {
                        opts.max_header_tokens = v;
                        Ok(())
                    },
                    None,
                    false,
                    values,
                    config,
                )
            }),
            ("merge_timeout", |values, config| {
                merge_timeout_parser(values, config)
            }),
            ("module.declaration_dirnames", |values, config| {
                module_declaration_dirnames_parser(values, config)
            }),
            ("module.file_ext", |values, config| {
                file_ext_parser(values, config)
            }),
            ("module.ignore_non_literal_requires", |values, config| {
                ignore_non_literal_requires_parser(values, config)
            }),
            ("module.missing_module_generators", |values, config| {
                missing_module_generators_parser(values, config)
            }),
            ("module.name_mapper", |values, config| {
                name_mapper_parser(values, config)
            }),
            ("module.name_mapper.extension", |values, config| {
                name_mapper_extension_parser(values, config)
            }),
            ("module.system", |values, config| {
                module_system_parser(values, config)
            }),
            (
                "module.system.haste.disallow_dynamic_import",
                |values, config| {
                    parse_boolean(
                        |opts, v| {
                            if v && opts.module_system != ModuleSystem::Haste {
                                return Err("Cannot be configured unless `module.system=haste`."
                                    .to_string());
                            }
                            opts.haste_disallow_dynamic_import = v;
                            Ok(())
                        },
                        values,
                        config,
                    )
                },
            ),
            ("module.system.haste.module_ref_prefix", |values, config| {
                haste_module_ref_prefix_parser(values, config)
            }),
            ("module.system.haste.paths.excludes", |values, config| {
                haste_paths_excludes_parser(values, config)
            }),
            ("module.system.haste.paths.includes", |values, config| {
                haste_paths_includes_parser(values, config)
            }),
            (
                "module.system.node.allow_root_relative",
                |values, config| node_resolver_allow_root_relative_parser(values, config),
            ),
            ("module.system.node.main_field", |values, config| {
                node_main_field_parser(values, config)
            }),
            (
                "module.system.node.package_export_condition",
                |values, config| node_package_export_condition_parser(values, config),
            ),
            ("module.system.node.resolve_dirname", |values, config| {
                node_resolve_dirname_parser(values, config)
            }),
            (
                "module.system.node.root_relative_dirname",
                |values, config| node_resolver_root_relative_dirnames_parser(values, config),
            ),
            ("module.use_strict", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.modules_are_use_strict = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("munge_underscores", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.munge_underscores = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("name", |values, config| root_name_parser(values, config)),
            ("builtin_lib", |values, config| {
                enum_parser(
                    &[
                        ("default", BuiltinLib::FlowlibWithLibDomDts),
                        ("core-only", BuiltinLib::Flowlib),
                        ("prelude", BuiltinLib::Prelude),
                        ("experimental.tslib", BuiltinLib::Tslib),
                    ],
                    |opts, v| {
                        opts.builtin_lib = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("no_implicit_override", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.no_implicit_override = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("no_unchecked_indexed_access", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.no_unchecked_indexed_access = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("node_modules_errors", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.node_modules_errors = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("pattern_matching", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.pattern_matching = Some(v);
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("react.custom_jsx_typing", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.react_custom_jsx_typing = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("react.runtime", |values, config| {
                react_runtime_parser(values, config)
            }),
            ("recursion_limit", |values, config| {
                parse_uint(
                    |opts, v| {
                        opts.recursion_limit = v as i32;
                        Ok(())
                    },
                    None,
                    false,
                    values,
                    config,
                )
            }),
            ("relay_integration", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.relay_integration = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("relay_integration.esmodules", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.relay_integration_esmodules = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("relay_integration.excludes", |values, config| {
                relay_integration_excludes_parser(values, config)
            }),
            ("relay_integration.module_prefix", |values, config| {
                parse_string(
                    |opts, v| {
                        opts.relay_integration_module_prefix = Some(v);
                        Ok(())
                    },
                    None,
                    false,
                    values,
                    config,
                )
            }),
            (
                "relay_integration.module_prefix.includes",
                |values, config| relay_integration_module_prefix_includes_parser(values, config),
            ),
            ("saved_state.fetcher", |values, config| {
                saved_state_fetcher_parser(values, config)
            }),
            ("saved_state.parallel_decompress", |values, config| {
                saved_state_parallel_decompress_parser(values, config)
            }),
            ("saved_state.persist_export_index", |values, config| {
                saved_state_persist_export_index_parser(values, config)
            }),
            (
                "saved_state.skip_version_check_DO_NOT_USE_OR_YOU_WILL_BE_FIRED",
                |values, config| {
                    parse_boolean(
                        |opts, v| {
                            opts.saved_state_skip_version_check = v;
                            Ok(())
                        },
                        values,
                        config,
                    )
                },
            ),
            ("server.max_workers", |values, config| {
                parse_uint(
                    |opts, v| {
                        opts.max_workers = Some(v);
                        Ok(())
                    },
                    None,
                    false,
                    values,
                    config,
                )
            }),
            ("server.max_workers.full_check", |values, config| {
                parse_uint(
                    |opts, v| {
                        opts.max_workers_full_check = Some(v);
                        Ok(())
                    },
                    None,
                    false,
                    values,
                    config,
                )
            }),
            ("server.max_workers.windows", |values, config| {
                parse_uint(
                    |opts, v| {
                        if cfg!(windows) {
                            opts.max_workers = Some(v);
                        }
                        Ok(())
                    },
                    None,
                    false,
                    values,
                    config,
                )
            }),
            ("supported_operating_systems", |values, config| {
                enum_parser(
                    &[("CentOS", SupportedOs::CentOS)],
                    |opts, v| {
                        opts.supported_operating_systems.push(v);
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("stylex_shorthand_prop", |values, config| {
                parse_string(
                    |opts, v| {
                        opts.stylex_shorthand_prop = Some(v);
                        Ok(())
                    },
                    None,
                    false,
                    values,
                    config,
                )
            }),
            (
                "types_first.max_files_checked_per_worker",
                |values, config| max_files_checked_per_worker_parser(values, config),
            ),
            (
                "types_first.max_files_checked_per_worker.rust_port",
                |values, config| max_files_checked_per_worker_rust_port_parser(values, config),
            ),
            (
                "types_first.max_seconds_for_check_per_worker",
                |values, config| max_seconds_for_check_per_worker_parser(values, config),
            ),
            ("unsuppressable_error_codes", |values, config| {
                unsuppressable_error_codes_parser(values, config)
            }),
            ("use_unknown_in_catch_variables", |values, config| {
                use_unknown_in_catch_variables_parser(values, config)
            }),
            ("vpn_less", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.vpn_less = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
            ("wait_for_recheck", |values, config| {
                parse_boolean(
                    |opts, v| {
                        opts.wait_for_recheck = v;
                        Ok(())
                    },
                    values,
                    config,
                )
            }),
        ];

        for &(key, parser) in parsers {
            let Some(values) = raw_opts.0.shift_remove(key) else {
                continue;
            };
            parser(values, config).map_err(|err| error_of_opt_error(key, err))?;
        }

        // `experimental.fox` is Meta-only; parsed only under fbcode_build, unrecognized elsewhere.
        #[cfg(fbcode_build)]
        if let Some(values) = raw_opts.0.shift_remove("experimental.fox") {
            parse_boolean(
                |opts, v| {
                    opts.fox = v;
                    Ok(())
                },
                values,
                config,
            )
            .map_err(|err| error_of_opt_error("experimental.fox", err))?;
        }

        for (key, values) in raw_opts.0 {
            // If the user specified any options that aren't defined, issue a warning
            let msg = format!("Unsupported option specified! ({})", key);
            warnings.extend(
                values
                    .0
                    .into_iter()
                    .map(|RawValue(line_num, _)| Warning(line_num, msg.clone())),
            );
        }

        Ok(warnings)
    }
}

#[derive(Debug, Clone)]
pub struct Rollout {
    pub enabled_group: String,
    pub disabled_groups: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct FlowConfig {
    pub rollouts: BTreeMap<String, Rollout>,
    // completely ignored files (both module resolving and typing)
    // This type *should* just be a string list, but we have an undocumented feature that allows
    // you to specify a backup flowconfig to use if a file is ignored using our module-mapper syntax.
    // This should be reverted to string list after we properly support multiplatform flow roots.
    pub ignores: Vec<(FlowconfigGlobPattern, Option<String>)>,
    // files that should be treated as untyped
    pub untyped: Vec<FlowconfigGlobPattern>,
    // files that should be treated as declarations
    pub declarations: Vec<String>,
    // non-root include paths
    pub includes: Vec<FlowconfigGlobPattern>,
    // library paths. no wildcards
    pub libs: Vec<String>,
    // lint severities
    pub lint_severities: LintSettings<Severity>,
    // strict mode
    pub strict_mode: StrictModeSettings,
    // config options
    pub options: opts::Opts,
    // version constraint
    pub version: Option<String>,
}

struct Section((u32, String), Vec<(u32, String)>);

fn group_into_sections(lines: Vec<(u32, String)>) -> Result<Vec<Section>, Error> {
    let section_header_re = Regex::new(r"^\[(.*)\]$").unwrap();
    let mut seen = HashSet::new();
    let mut sections = Vec::new();
    let mut current_section = Section((0, String::new()), Vec::new());

    for (ln, line) in lines {
        if let Some(caps) = section_header_re.captures(&line) {
            sections.push(current_section);
            let section_name = caps.get(1).unwrap().as_str().to_string();
            if seen.contains(&section_name) {
                return Err(Error(
                    ln,
                    format!("contains duplicate section: \"{}\"", section_name),
                ));
            }
            seen.insert(section_name.clone());
            current_section = Section((ln, section_name), Vec::new());
        } else {
            current_section.1.push((ln, line));
        }
    }

    // finalize last section
    sections.push(current_section);
    Ok(sections)
}

fn trim_lines(lines: &[(u32, String)]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|(_, line)| {
            let line = line.trim();
            if line.is_empty() {
                None
            } else {
                Some(line.to_owned())
            }
        })
        .collect()
}

fn trim_numbered_lines(lines: &[(u32, String)]) -> Vec<(u32, String)> {
    lines
        .iter()
        .filter_map(|(number, line)| {
            let line = line.trim();
            if line.is_empty() {
                None
            } else {
                Some((*number, line.to_owned()))
            }
        })
        .collect()
}

// parse [include] lines
fn parse_flowconfig_glob(
    line: u32,
    pattern: String,
    allow_negation: bool,
) -> Result<FlowconfigGlobPattern, Error> {
    FlowconfigGlobPattern::try_new(pattern, allow_negation)
        .map_err(|error| Error(line, error.message()))
}

fn parse_includes(config: &mut FlowConfig, lines: &[(u32, String)]) -> Result<(), Error> {
    let includes = trim_numbered_lines(lines);
    config.includes = includes
        .into_iter()
        .map(|(line, include)| parse_flowconfig_glob(line, include, false))
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(())
}

fn parse_libs(config: &mut FlowConfig, lines: &[(u32, String)]) {
    config.libs = trim_lines(lines);
}

fn parse_ignores(config: &mut FlowConfig, lines: &[(u32, String)]) -> Result<(), Error> {
    let mapping_re = Regex::new(r"^'([^']*)'[ \t]*->[ \t]*'([^']*)'$").unwrap();
    let raw_ignores = trim_numbered_lines(lines);
    let ignores = raw_ignores
        .into_iter()
        .map(|(line, ignore)| {
            if let Some(caps) = mapping_re.captures(&ignore) {
                let pattern =
                    parse_flowconfig_glob(line, caps.get(1).unwrap().as_str().to_owned(), true)?;
                Ok((pattern, Some(caps.get(2).unwrap().as_str().to_owned())))
            } else {
                let pattern = parse_flowconfig_glob(line, ignore, true)?;
                Ok((pattern, None))
            }
        })
        .collect::<Result<Vec<_>, Error>>()?;
    config.ignores = ignores;
    Ok(())
}

fn parse_untyped(config: &mut FlowConfig, lines: &[(u32, String)]) -> Result<(), Error> {
    config.untyped = trim_numbered_lines(lines)
        .into_iter()
        .map(|(line, untyped)| parse_flowconfig_glob(line, untyped, true))
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(())
}

fn parse_declarations(config: &mut FlowConfig, lines: &[(u32, String)]) {
    config.declarations = trim_lines(lines)
        .into_iter()
        .map(|s| opts::ocaml_str_to_rust_regex(&s))
        .collect();
}

fn parse_options(config: &mut FlowConfig, lines: &[(u32, String)]) -> Result<Vec<Warning>, Error> {
    opts::parse(&mut config.options, lines)
}

fn parse_version(config: &mut FlowConfig, lines: &[(u32, String)]) -> Result<(), Error> {
    let potential_versions = trim_numbered_lines(lines);
    if let Some((ln, version_str)) = potential_versions.first() {
        if !flow_common_semver::semver::is_valid_range(version_str) {
            return Err(Error(
                *ln,
                format!(
                    "Expected version to match %d.%d.%d, with an optional leading ^, got {}",
                    version_str
                ),
            ));
        }
        config.version = Some(version_str.clone());
    }
    Ok(())
}

fn parse_lints(config: &mut FlowConfig, lines: &[(u32, String)]) -> Result<Vec<Warning>, Error> {
    let lines = trim_numbered_lines(lines);
    let mut starting_lint_settings = LintSettings::<Severity>::empty_severities();
    std::mem::swap(&mut starting_lint_settings, &mut config.lint_severities);
    let (lint_severities, lint_warnings) = LintSettings::of_lines(starting_lint_settings, lines)
        .map_err(
            |flow_lint_settings::lint_settings::Error { line, message }| Error(line, message),
        )?;
    config.lint_severities = lint_severities;
    let warnings = lint_warnings
        .into_iter()
        .map(|flow_lint_settings::lint_settings::Warning { line, message }| Warning(line, message))
        .collect();
    Ok(warnings)
}

fn parse_strict(config: &mut FlowConfig, lines: &[(u32, String)]) -> Result<(), Error> {
    let lines = trim_numbered_lines(lines);
    let strict_mode =
        StrictModeSettings::of_lines(&lines).map_err(|(line, msg)| Error(line, msg))?;
    config.strict_mode = strict_mode;
    Ok(())
}

// Rollouts are based on randomness, but we want it to be stable from run to run. So we seed our
// pseudo random number generator with
//
// 1. The hostname
// 2. The user
// 3. The name of the rollout
fn calculate_rollout_percentage(rollout_name: &str) -> u32 {
    use std::hash::Hash;
    use std::hash::Hasher;
    let host_name = hostname::get().expect("Cannot get hostname");
    let mut hasher = DefaultHasher::new();
    host_name.hash(&mut hasher);
    #[cfg(unix)]
    {
        unsafe { libc::getuid() }.hash(&mut hasher);
    }
    rollout_name.hash(&mut hasher);
    (hasher.finish() % 100) as u32
}

// The optional rollout section has 0 or more lines. Each line defines a single rollout. For example
//
// [rollouts]
//
// testA=40% on, 60% off
// testB=50% blue, 20% yellow, 30% pink
//
// The first line defines a rollout named "testA" with two groups.
// The second line defines a rollout named "testB" with three groups.
//
// Each rollout's groups must sum to 100.
fn parse_rollouts(config: &mut FlowConfig, lines: &[(u32, String)]) -> Result<(), Error> {
    let lines = trim_numbered_lines(lines);
    let mut rollouts = BTreeMap::new();

    let rollout_re = Regex::new(r"^([a-zA-Z0-9._]+)=(.*)$").unwrap();
    let group_re = Regex::new(r"^([0-9]+)% ([a-zA-Z0-9._]+)$").unwrap();

    for (line_num, line) in lines {
        // A rollout's name is can only contain [a-zA-Z0-9._]
        if let Some(caps) = rollout_re.captures(&line) {
            let rollout_name = caps.get(1).unwrap().as_str();
            let rollout_values_raw = caps.get(2).unwrap().as_str();
            let my_pct = calculate_rollout_percentage(rollout_name);

            let mut enabled_group: Option<String> = None;
            let mut disabled_groups = BTreeSet::new();
            let mut pct_total = 0u32;
            let mut seen_groups = HashSet::new();

            // Groups are delimited with commas
            for raw_group in rollout_values_raw.split(',') {
                let raw_group = raw_group.trim();

                // A rollout group has the for "X% label", where label can only contain
                // [a-zA-Z0-9._]
                if let Some(group_caps) = group_re.captures(raw_group) {
                    let group_pct: u32 = group_caps.get(1).unwrap().as_str().parse().unwrap();
                    let group_name = group_caps.get(2).unwrap().as_str();

                    if enabled_group.as_deref() == Some(group_name)
                        || seen_groups.contains(group_name)
                    {
                        return Err(Error(
                            line_num,
                            format!(
                                "Groups must have unique names. There is more than one {:?} group",
                                group_name
                            ),
                        ));
                    }

                    seen_groups.insert(group_name.to_string());

                    if enabled_group.is_none() && my_pct < group_pct + pct_total {
                        // This is the first group that passes my_pct, so we enable it
                        enabled_group = Some(group_name.to_string());
                    } else {
                        // Either we've already chosen the enabled group or we haven't passed my_pct
                        disabled_groups.insert(group_name.to_string());
                    }

                    pct_total += group_pct;
                } else {
                    return Err(Error(
                        line_num,
                        "Malformed rollout group. A group should be a percentage and an identifier, like `50% on`".to_string(),
                    ));
                }
            }

            if pct_total != 100 {
                return Err(Error(
                    line_num,
                    format!(
                        "Rollout groups must sum to 100%. {:?} sums to {}%",
                        rollout_name, pct_total
                    ),
                ));
            }

            if rollouts.contains_key(rollout_name) {
                return Err(Error(
                    line_num,
                    format!(
                        "Rollouts must have unique names. There already is a {:?} rollout",
                        rollout_name
                    ),
                ));
            }

            match enabled_group {
                None => {
                    return Err(Error(
                        line_num,
                        "Invariant violation: failed to choose a group".to_string(),
                    ));
                }
                Some(enabled_group) => {
                    rollouts.insert(
                        rollout_name.to_string(),
                        Rollout {
                            enabled_group,
                            disabled_groups,
                        },
                    );
                }
            }
        } else {
            return Err(Error(
                line_num,
                "Malformed rollout. A rollout should be an identifier followed by a list of groups, like `myRollout=10% on, 50% off`".to_string(),
            ));
        }
    }

    config.rollouts = rollouts;
    Ok(())
}

fn parse_section(
    config: &mut FlowConfig,
    section: Section,
    ignore_version: bool,
) -> Result<Vec<Warning>, Error> {
    let Section((section_ln, section_name), lines) = section;

    match (section_name.as_str(), lines.as_slice()) {
        ("", []) if section_ln == 0 => Ok(vec![]),
        ("", [(ln, message), ..]) if section_ln == 0 => Err(Error(
            *ln,
            format!("Unexpected config line not in any section: {message}"),
        )),
        ("include", lines) => {
            parse_includes(config, lines)?;
            Ok(vec![])
        }
        ("ignore", lines) => {
            parse_ignores(config, lines)?;
            Ok(vec![])
        }
        ("libs", lines) => {
            parse_libs(config, lines);
            Ok(vec![])
        }
        ("lints", lines) => parse_lints(config, lines),
        ("declarations", lines) => {
            parse_declarations(config, lines);
            Ok(vec![])
        }
        ("strict", lines) => {
            parse_strict(config, lines)?;
            Ok(vec![])
        }
        ("options", lines) => parse_options(config, lines),
        ("untyped", lines) => {
            parse_untyped(config, lines)?;
            Ok(vec![])
        }
        ("version", lines) => {
            if ignore_version {
                Ok(vec![])
            } else {
                parse_version(config, lines)?;
                Ok(vec![])
            }
        }
        _ => Ok(vec![Warning(
            section_ln,
            format!("Unsupported config section: \"{}\"", section_name),
        )]),
    }
}

/// Filter every section (except the rollouts section) for disabled rollouts. For example, if a
/// line starts with (my_rollout=on) and the "on" group is not enabled for the "my_rollout"
/// rollout, then drop the line completely.
///
/// Lines with enabled rollouts just have the prefix stripped
fn filter_sections_by_rollout(
    sections: Vec<Section>,
    config: &FlowConfig,
) -> Result<Vec<Section>, Error> {
    // The rollout prefix looks like `(rollout_name=group_name)`
    let rollout_regex = Regex::new(r"^\(([a-zA-Z0-9._]+)=([a-zA-Z0-9._]+)\)(.*)$").unwrap();

    let mut filtered_sections = Vec::new();

    for Section((section_ln, section_name), lines) in sections {
        let mut filtered_lines = Vec::new();

        for (line_num, line) in lines {
            if let Some(caps) = rollout_regex.captures(&line) {
                let rollout_name = caps.get(1).unwrap().as_str();
                let group_name = caps.get(2).unwrap().as_str();
                let line_content = caps.get(3).unwrap().as_str();

                match config.rollouts.get(rollout_name) {
                    None => {
                        return Err(Error(
                            line_num,
                            format!("Unknown rollout {:?}", rollout_name),
                        ));
                    }
                    Some(rollout) => {
                        if rollout.enabled_group == group_name {
                            filtered_lines.push((line_num, line_content.to_string()));
                        } else if rollout.disabled_groups.contains(group_name) {
                        } else {
                            return Err(Error(
                                line_num,
                                format!(
                                    "Unknown group {:?} in rollout {:?}",
                                    group_name, rollout_name
                                ),
                            ));
                        }
                    }
                }
            } else {
                filtered_lines.push((line_num, line));
            }
        }

        filtered_sections.push(Section((section_ln, section_name), filtered_lines));
    }

    Ok(filtered_sections)
}

fn process_rollouts_with_sections(
    config: &mut FlowConfig,
    sections: Vec<Section>,
) -> Result<Vec<Section>, Error> {
    let mut rollout_section_lines: Option<Vec<(u32, String)>> = None;
    let mut filtered_sections = Vec::new();
    for section in sections {
        let Section((section_ln, section_name), lines) = section;
        if section_name == "rollouts" {
            rollout_section_lines = Some(lines);
        } else {
            filtered_sections.push(Section((section_ln, section_name), lines));
        }
    }
    if let Some(lines) = rollout_section_lines {
        parse_rollouts(config, &lines)?;
    }
    let filtered_sections = filter_sections_by_rollout(filtered_sections, config)?;

    Ok(filtered_sections)
}

fn parse(
    config: &mut FlowConfig,
    lines: Vec<(u32, String)>,
    ignore_version: bool,
) -> Result<Vec<Warning>, Error> {
    let sections = process_rollouts_with_sections(config, group_into_sections(lines)?)?;

    let mut warnings = Vec::new();
    for section in sections {
        warnings.extend(parse_section(config, section, ignore_version)?);
    }
    Ok(warnings)
}

fn is_meaningful_line(line: &str) -> bool {
    if line.is_empty() {
        return false;
    }
    !line.starts_with('#') // Line starts with #
        && !line.starts_with(';') // Line starts with ;
        && !line.starts_with('\u{1F4A9}') // Line starts with poop emoji
}

fn read(filename: &str) -> Result<(Vec<(u32, String)>, u64), std::io::Error> {
    use std::hash::Hash;
    use std::hash::Hasher;

    let contents = std::fs::read_to_string(filename)?;

    let mut hasher = DefaultHasher::new();
    contents.hash(&mut hasher);
    let hash = hasher.finish();

    let lines: Vec<(u32, String)> = contents
        .lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let trimmed = line.trim();
            if is_meaningful_line(trimmed) {
                Some(((i + 1) as u32, trimmed.to_string()))
            } else {
                None
            }
        })
        .collect();

    Ok((lines, hash))
}

pub fn empty_config() -> FlowConfig {
    FlowConfig {
        rollouts: BTreeMap::new(),
        ignores: Vec::new(),
        untyped: Vec::new(),
        declarations: Vec::new(),
        includes: Vec::new(),
        libs: Vec::new(),
        lint_severities: LintSettings::<Severity>::default_severities(),
        strict_mode: StrictModeSettings::empty(),
        options: opts::default_options(),
        version: None,
    }
}

pub fn init(
    ignores: Vec<String>,
    untyped: Vec<String>,
    declarations: Vec<String>,
    includes: Vec<String>,
    libs: Vec<String>,
    options: Vec<String>,
    lints: Vec<String>,
) -> Result<(FlowConfig, Vec<Warning>), Error> {
    let mut warnings = Vec::new();
    let mut config = empty_config();

    parse_ignores(
        &mut config,
        &ignores.into_iter().map(|s| (1, s)).collect::<Vec<_>>(),
    )?;
    parse_untyped(
        &mut config,
        &untyped.into_iter().map(|s| (1, s)).collect::<Vec<_>>(),
    )?;
    parse_declarations(
        &mut config,
        &declarations.into_iter().map(|s| (1, s)).collect::<Vec<_>>(),
    );
    parse_includes(
        &mut config,
        &includes.into_iter().map(|s| (1, s)).collect::<Vec<_>>(),
    )?;
    warnings.extend(parse_options(
        &mut config,
        &options.into_iter().map(|s| (1, s)).collect::<Vec<_>>(),
    )?);
    parse_libs(
        &mut config,
        &libs.into_iter().map(|s| (1, s)).collect::<Vec<_>>(),
    );
    warnings.extend(parse_lints(
        &mut config,
        &lints.into_iter().map(|s| (1, s)).collect::<Vec<_>>(),
    )?);

    Ok((config, warnings))
}

pub fn write<W: std::io::Write>(out: &mut W, config: &FlowConfig) -> std::io::Result<()> {
    fn section_header(out: &mut impl std::io::Write, section: &str) -> std::io::Result<()> {
        writeln!(out, "[{}]", section)
    }

    fn section_if_nonempty(
        out: &mut impl std::io::Write,
        header: &str,
        is_empty: bool,
        f: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        if is_empty {
            Ok(())
        } else {
            section_header(out, header)?;
            f(out)?;
            writeln!(out)
        }
    }

    fn write_ignores(
        out: &mut dyn std::io::Write,
        ignores: &[(FlowconfigGlobPattern, Option<String>)],
    ) -> std::io::Result<()> {
        for (ignore, backup_opt) in ignores {
            match backup_opt {
                None => writeln!(out, "{}", ignore.source())?,
                Some(backup) => writeln!(out, "{} -> {}", ignore.source(), backup)?,
            }
        }
        Ok(())
    }

    fn write_lines(out: &mut dyn std::io::Write, values: &[String]) -> std::io::Result<()> {
        for value in values {
            writeln!(out, "{}", value)?;
        }
        Ok(())
    }

    fn write_options(out: &mut dyn std::io::Write, config: &FlowConfig) -> std::io::Result<()> {
        let options = &config.options;
        let default_options = opts::default_options();
        if options.module_system != default_options.module_system {
            let module_system = match options.module_system {
                ModuleSystem::Node => "node",
                ModuleSystem::Haste => "haste",
            };
            writeln!(out, "module.system={}", module_system)?;
        }
        if options.all != default_options.all {
            writeln!(out, "all={}", options.all.unwrap_or(false))?;
        }
        if options.include_warnings != default_options.include_warnings {
            writeln!(out, "include_warnings={}", options.include_warnings)?;
        }
        Ok(())
    }

    fn write_lints(out: &mut dyn std::io::Write, config: &FlowConfig) -> std::io::Result<()> {
        let lint_severities = &config.lint_severities;
        let lint_default = lint_severities.get_default();
        // Don't print an 'all' setting if it matches the default setting.
        if lint_default != LintSettings::<Severity>::empty_severities().get_default() {
            writeln!(out, "all={}", lint_default.as_str())?;
        }
        let mut result = Ok(());
        lint_severities.iter(|kind, (state, _)| {
            if result.is_ok() {
                result = writeln!(out, "{}={}", kind.as_str(), state.as_str());
            }
        });
        result
    }

    fn write_strict(
        out: &mut dyn std::io::Write,
        strict_mode: &StrictModeSettings,
    ) -> std::io::Result<()> {
        let mut result = Ok(());
        strict_mode.iter(|kind| {
            if result.is_ok() {
                result = writeln!(out, "{}", kind.as_str());
            }
        });
        result
    }

    section_header(out, "ignore")?;
    write_ignores(out, &config.ignores)?;
    writeln!(out)?;
    section_if_nonempty(out, "untyped", config.untyped.is_empty(), |out| {
        for untyped in &config.untyped {
            writeln!(out, "{}", untyped.source())?;
        }
        Ok(())
    })?;
    section_if_nonempty(out, "declarations", config.declarations.is_empty(), |out| {
        write_lines(out, &config.declarations)
    })?;
    section_header(out, "include")?;
    for include in &config.includes {
        writeln!(out, "{}", include.source())?;
    }
    writeln!(out)?;
    section_header(out, "libs")?;
    write_lines(out, &config.libs)?;
    writeln!(out)?;
    section_header(out, "lints")?;
    write_lints(out, config)?;
    writeln!(out)?;
    section_header(out, "options")?;
    write_options(out, config)?;
    writeln!(out)?;
    section_header(out, "strict")?;
    write_strict(out, &config.strict_mode)
}

pub fn get(path: &str) -> Result<(FlowConfig, Vec<Warning>, String), Error> {
    get_with_ignored_version(path, false)
}

pub fn get_with_ignored_version(
    path: &str,
    ignore_version: bool,
) -> Result<(FlowConfig, Vec<Warning>, String), Error> {
    let (lines, hash) =
        read(path).map_err(|e| Error(0, format!("Failed to read config: {}", e)))?;
    let mut config = empty_config();
    let warnings = parse(&mut config, lines, ignore_version)?;
    let hash_string = format!("{:016x}", hash);
    Ok((config, warnings, hash_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_operator_defaults_to_disabled() {
        // Omitting the option preserves parsing while disabling Flow assertion use.
        let mut config = empty_config();
        match parse(&mut config, vec![(1, "[options]".to_owned())], true) {
            Ok(warnings) => {
                assert!(warnings.is_empty()); // OK
                assert_eq!(config.options.assert_operator, AssertOperator::Disabled); // OK
                assert!(!config.options.assert_operator.usable()); // OK
                assert!(!config.options.assert_operator.specialized()); // OK
            }
            Err(Error(line, message)) => {
                panic!("default assertion config errored at line {line}: {message}")
            }
        }
    }

    #[test]
    fn assert_operator_supported_spellings() {
        // Boolean spellings and existing enum aliases retain their distinct modes.
        for (value, expected) in [
            ("false", AssertOperator::Disabled),
            ("disabled", AssertOperator::Disabled),
            ("true", AssertOperator::Enabled),
            ("enabled", AssertOperator::Enabled),
            ("specialized", AssertOperator::Specialized),
        ] {
            let mut config = empty_config();
            let result = parse(
                &mut config,
                vec![
                    (1, "[options]".to_owned()),
                    (2, format!("experimental.assert_operator={value}")),
                ],
                true,
            );
            match result {
                Ok(warnings) => {
                    assert!(warnings.is_empty(), "unexpected warning for {value}"); // OK
                    assert_eq!(config.options.assert_operator, expected, "{value}"); // OK
                    assert_eq!(
                        config.options.assert_operator.usable(),
                        value != "false" && value != "disabled"
                    ); // OK
                    assert_eq!(
                        config.options.assert_operator.specialized(),
                        value == "specialized"
                    ); // OK
                }
                Err(Error(line, message)) => {
                    panic!("assertion config {value} errored at line {line}: {message}")
                }
            }
        }
    }

    #[test]
    fn assert_operator_unparsed_is_invalid() {
        // The removed spelling must produce a config error, not a warning or alias.
        let mut config = empty_config();
        let result = parse(
            &mut config,
            vec![
                (1, "[options]".to_owned()),
                (2, "experimental.assert_operator=unparsed".to_owned()),
            ],
            true,
        );
        match result {
            Ok(_) => panic!("unparsed assertion config must be rejected"),
            Err(Error(line, message)) => {
                assert_eq!(line, 2); // OK
                assert_eq!(
                    message,
                    "Error parsing value for \"experimental.assert_operator\". Unsupported value: \"unparsed\". Supported values are: false, disabled, true, enabled, specialized"
                ); // OK
            }
        }
    }

    #[cfg(fbcode_build)]
    #[test]
    fn experimental_fox_is_supported() {
        let mut config = empty_config();
        let result = parse(
            &mut config,
            vec![
                (1, "[options]".to_owned()),
                (2, "experimental.fox=true".to_owned()),
            ],
            true,
        );

        match result {
            Ok(warnings) => {
                assert!(warnings.is_empty());
                assert!(config.options.fox);
            }
            Err(Error(line, message)) => {
                panic!("experimental.fox errored at line {}: {}", line, message)
            }
        }
    }

    #[cfg(not(fbcode_build))]
    #[test]
    fn experimental_fox_is_unsupported_in_open_source() {
        let mut config = empty_config();
        let result = parse(
            &mut config,
            vec![
                (1, "[options]".to_owned()),
                (2, "experimental.fox=true".to_owned()),
            ],
            true,
        );

        match result {
            Ok(warnings) => {
                assert_eq!(warnings.len(), 1);
                assert_eq!(
                    warnings[0].1.as_str(),
                    "Unsupported option specified! (experimental.fox)"
                );
            }
            Err(Error(line, message)) => {
                panic!("experimental.fox errored at line {}: {}", line, message)
            }
        }
    }
}
