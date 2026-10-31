/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::io::BufRead;
use std::io::IsTerminal;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;
use std::process::Command as ProcessCommand;
use std::process::Stdio;
use std::sync::Arc;

use flow_command_spec::Command;
use flow_command_spec::arg_spec;
use flow_commands_connect::command_connect::ProgressMode;
use flow_common::options::Format;
use flow_common::options::LogSaving;
use flow_common::options::Options;
use flow_common::options::SavedStateFetcher;
use flow_common::path_matcher::PathMatcher;
use flow_common::path_matcher::RootedGlob;
use flow_common::verbose::Verbose;
use flow_common_errors::error_utils::cli_output;
use flow_common_exit::FlowExitStatus;
use flow_common_vcs::vcs::Vcs;
use flow_config::FlowConfig;
use flow_config::FlowconfigGlobPattern;
use flow_config::LazyMode;
use flow_config::opts::BuiltinLib as ConfigBuiltinLib;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_monitor_rpc::monitor_prot::MonitorToClientMessage;
use flow_server_env::file_watcher_status;
use flow_server_env::server_prot;
use flow_server_env::server_status;
use flow_server_monitor::FlowServerMonitorOptions;
use flow_server_utils::file_input::FileInput;
use flow_utils_concurrency::thread_pool::physical_parallelism;
use regex::Regex;

#[derive(Clone, Debug)]
pub enum UnicodeMode {
    Never,
    Always,
    Auto,
}

/// Report malformed command-line args and exit with `CommandlineUsageError`.
///
/// Telemetry is initialized first when it isn't already, so the failure is
/// recorded even on paths that run before the normal init. Mirrors Hack's
/// `Die.bad_args`.
pub fn die_bad_args(msg: &str) -> ! {
    flow_event_logger::init_flow_command_if_needed();
    flow_common_exit::exit(FlowExitStatus::CommandlineUsageError, Some(msg));
}

pub fn run_command(command: &Command, argv: &[String]) {
    match flow_command_spec::parse_or_show_help(command, argv) {
        Ok(Err(_)) => {
            println!("{}", command.string_of_usage());
            flow_common_exit::exit(FlowExitStatus::NoError, None);
        }
        Ok(Ok(args)) => {
            apply_from_flag(command, &args);
            if command.flags().contains_key("--no-cgroup") {
                maybe_run_in_cgroup(&args);
            }
            command.run(&args)
        }
        Err(error) => {
            let is_pretty_or_json_arg =
                |s: &str| s.starts_with("--pretty") || s.starts_with("--json");
            if let Some(json_arg) = argv.iter().find(|s| is_pretty_or_json_arg(s)) {
                let pretty = json_arg.starts_with("--pretty");
                flow_common_exit::set_json_mode(pretty);
            }
            let msg = flow_command_spec::error_message(&error);
            die_bad_args(&msg);
        }
    }
}

pub fn expand_file_list(
    filenames: &[String],
    options: Option<&flow_common::files::FileOptions>,
) -> BTreeSet<String> {
    let paths: Vec<String> = filenames
        .iter()
        .map(|f| {
            flow_common::files::cached_canonicalize(std::path::Path::new(f))
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| f.to_string())
        })
        .collect();
    let mut next_files: Box<dyn FnMut() -> Vec<String>> = match paths.as_slice() {
        [] => Box::new(Vec::new),
        _ => {
            let filter: Box<dyn Fn(&str) -> bool> = match options {
                Some(opts) => {
                    let opts = opts.clone();
                    Box::new(move |path: &str| flow_common::files::is_valid_path(&opts, path))
                }
                None => Box::new(|path: &str| path.ends_with(".js")),
            };
            let root = paths[0].clone();
            let others = paths[1..].to_vec();
            Box::new(flow_utils_find::make_next_files(filter, others, root))
        }
    };
    flow_common::files::get_all(&mut *next_files)
}

pub fn get_filenames_from_input(
    allow_imaginary: bool,
    input_file: Option<&str>,
    filenames: Option<&[String]>,
) -> Vec<String> {
    let cwd = std::env::current_dir()
        .expect("failed to get current directory")
        .to_string_lossy()
        .to_string();
    let handle_imaginary = |filename: &str| -> String {
        if allow_imaginary {
            flow_common::files::imaginary_realpath(filename)
        } else {
            let msg = format!("File not found: {:?}", filename);
            flow_common_exit::exit(FlowExitStatus::NoInput, Some(&msg));
        }
    };
    let input_file_filenames = match input_file {
        Some("-") => {
            let stdin = std::io::stdin();
            let lines: Vec<String> = stdin
                .lock()
                .lines()
                .map(|l| l.expect("failed to read stdin"))
                .collect();
            flow_common::files::canonicalize_filenames(&cwd, &handle_imaginary, &lines)
        }
        Some(input_file) => {
            let content = std::fs::read_to_string(input_file).expect("failed to read input file");
            let lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
            let file_dir = Path::new(input_file)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| cwd.clone());
            flow_common::files::canonicalize_filenames(&file_dir, &handle_imaginary, &lines)
        }
        None => vec![],
    };
    let cli_filenames = match filenames {
        Some(filenames) => {
            let names: Vec<String> = filenames.to_vec();
            flow_common::files::canonicalize_filenames(&cwd, &handle_imaginary, &names)
        }
        None => vec![],
    };
    let mut result = cli_filenames;
    result.extend(input_file_filenames);
    result
}

pub fn print_version() {
    println!(
        "Flow, a static type checker for JavaScript, version {}",
        flow_common::flow_version::version()
    );
}

pub fn expand_path(path: &str) -> String {
    let path = Path::new(path);
    if path.exists() {
        flow_common::files::cached_canonicalize(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .to_string()
    } else {
        let file = std::env::current_dir()
            .expect("failed to get current directory")
            .join(path);
        if file.exists() {
            flow_common::files::cached_canonicalize(&file)
                .unwrap_or(file)
                .to_string_lossy()
                .to_string()
        } else {
            let msg = format!("File not found: {}", file.to_string_lossy());
            flow_common_exit::exit(FlowExitStatus::InputError, Some(&msg));
        }
    }
}

pub fn collect_error_flags(
    rendering_mode: cli_output::RenderingMode,
    include_warnings: bool,
    max_warnings: Option<i32>,
    one_line: bool,
    list_files: bool,
    show_all_errors: bool,
    show_all_branches: bool,
    unicode: UnicodeMode,
    message_width: Option<i32>,
) -> cli_output::ErrorFlags {
    let include_warnings = match max_warnings {
        Some(_) => true,
        None => include_warnings,
    };
    let unicode = match unicode {
        UnicodeMode::Never => false,
        UnicodeMode::Always => true,
        UnicodeMode::Auto => flow_utils_tty::supports_emoji(),
    };
    let message_width = match message_width {
        Some(message_width) => message_width,
        None => flow_utils_tty::get_term_cols()
            .map(|cols| std::cmp::min(120, cols))
            .unwrap_or(120),
    };
    cli_output::ErrorFlags {
        rendering_mode,
        include_warnings,
        max_warnings,
        one_line,
        list_files,
        show_all_errors,
        show_all_branches,
        unicode,
        message_width,
    }
}

pub(crate) fn add_warning_flags(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--include-warnings",
        &arg_spec::truthy(),
        "Include warnings in the error output (warnings are excluded by default)",
        None,
    )
    .flag(
        "--max-warnings",
        &arg_spec::optional(arg_spec::int()),
        "Warnings above this number will cause a nonzero exit code (implies --include-warnings)",
        None,
    )
}

pub(crate) fn add_profile_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--profile",
        &arg_spec::truthy(),
        "Output profiling information",
        Some("FLOW_PROFILE"),
    )
}

#[derive(Clone, Debug)]
pub struct ErrorFlagsArgs {
    pub color: cli_output::RenderingMode,
    pub include_warnings: bool,
    pub max_warnings: Option<i32>,
    pub one_line: bool,
    pub list_files: bool,
    pub show_all_errors: bool,
    pub show_all_branches: bool,
    pub unicode: UnicodeMode,
    pub message_width: Option<i32>,
}

pub fn add_error_flags(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    add_warning_flags(
        spec.flag(
            "--color",
            &arg_spec::required(
                Some(cli_output::RenderingMode::CliColorAuto),
                arg_spec::enum_flag(vec![
                    ("never", cli_output::RenderingMode::CliColorNever),
                    ("always", cli_output::RenderingMode::CliColorAlways),
                    ("auto", cli_output::RenderingMode::CliColorAuto),
                    (
                        "unstable_ide_mode_EXPOSED_FOR_TESTING",
                        cli_output::RenderingMode::IdeDetailedError,
                    ),
                ]),
            ),
            "Display terminal output in color. never, always, auto (default: auto)",
            None,
        ),
    )
    .flag(
        "--one-line",
        &arg_spec::truthy(),
        "Escapes newlines so that each error prints on one line",
        None,
    )
    .flag(
        "--list-files",
        &arg_spec::truthy(),
        "List files with errors",
        None,
    )
    .flag(
        "--show-all-errors",
        &arg_spec::truthy(),
        "Print all errors (the default is to truncate after 50 errors)",
        None,
    )
    .flag(
        "--show-all-branches",
        &arg_spec::truthy(),
        "Print all branch errors (the default is to print the most relevant branches)",
        None,
    )
    .flag(
        "--unicode",
        &arg_spec::required(
            Some(UnicodeMode::Auto),
            arg_spec::enum_flag(vec![
                ("never", UnicodeMode::Never),
                ("always", UnicodeMode::Always),
                ("auto", UnicodeMode::Auto),
            ]),
        ),
        "Display terminal output with unicode decoration. never, always, auto (default: auto)",
        None,
    )
    .flag(
        "--message-width",
        &arg_spec::optional(arg_spec::int()),
        "Sets the width of messages but not code snippets (defaults to the smaller of 120 or the terminal width)",
        None,
    )
}

pub fn get_error_flags_args(args: &arg_spec::Values) -> ErrorFlagsArgs {
    ErrorFlagsArgs {
        color: flow_command_spec::get(
            args,
            "--color",
            &arg_spec::required(
                Some(cli_output::RenderingMode::CliColorAuto),
                arg_spec::enum_flag(vec![
                    ("never", cli_output::RenderingMode::CliColorNever),
                    ("always", cli_output::RenderingMode::CliColorAlways),
                    ("auto", cli_output::RenderingMode::CliColorAuto),
                    (
                        "unstable_ide_mode_EXPOSED_FOR_TESTING",
                        cli_output::RenderingMode::IdeDetailedError,
                    ),
                ]),
            ),
        )
        .unwrap(),
        include_warnings: flow_command_spec::get(args, "--include-warnings", &arg_spec::truthy())
            .unwrap(),
        max_warnings: flow_command_spec::get(
            args,
            "--max-warnings",
            &arg_spec::optional(arg_spec::int()),
        )
        .unwrap(),
        one_line: flow_command_spec::get(args, "--one-line", &arg_spec::truthy()).unwrap(),
        list_files: flow_command_spec::get(args, "--list-files", &arg_spec::truthy()).unwrap(),
        show_all_errors: flow_command_spec::get(args, "--show-all-errors", &arg_spec::truthy())
            .unwrap(),
        show_all_branches: flow_command_spec::get(args, "--show-all-branches", &arg_spec::truthy())
            .unwrap(),
        unicode: flow_command_spec::get(
            args,
            "--unicode",
            &arg_spec::required(
                Some(UnicodeMode::Auto),
                arg_spec::enum_flag(vec![
                    ("never", UnicodeMode::Never),
                    ("always", UnicodeMode::Always),
                    ("auto", UnicodeMode::Auto),
                ]),
            ),
        )
        .unwrap(),
        message_width: flow_command_spec::get(
            args,
            "--message-width",
            &arg_spec::optional(arg_spec::int()),
        )
        .unwrap(),
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct JsonFlags {
    pub json: bool,
    pub pretty: bool,
}

pub fn add_json_flags(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--json",
        &arg_spec::truthy(),
        "Output results in JSON format",
        None,
    )
    .flag(
        "--pretty",
        &arg_spec::truthy(),
        "Pretty-print JSON output",
        None,
    )
}

pub fn get_json_flags(args: &arg_spec::Values) -> JsonFlags {
    let json = flow_command_spec::get(args, "--json", &arg_spec::truthy()).unwrap();
    let pretty = flow_command_spec::get(args, "--pretty", &arg_spec::truthy()).unwrap();
    if json || pretty {
        flow_common_exit::set_json_mode(pretty);
    }
    JsonFlags { json, pretty }
}

pub fn add_temp_dir_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--temp-dir",
        &arg_spec::optional(arg_spec::string()),
        "Directory in which to store temp files (default: FLOW_TEMP_DIR, or /tmp/flow/)",
        Some("FLOW_TEMP_DIR"),
    )
}

pub fn add_lazy_flags(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--lazy",
        &arg_spec::truthy(),
        "Only check changed files. Shorthand for `--lazy-mode true`",
        None,
    )
    .flag(
        "--lazy-mode",
        &arg_spec::optional(lazy_mode_flag()),
        "If true, only check changed files",
        Some("FLOW_LAZY_MODE"),
    )
}

pub(crate) fn lazy_mode_flag() -> arg_spec::FlagType<Option<LazyMode>> {
    arg_spec::enum_flag(vec![
        ("true", LazyMode::Lazy),
        ("false", LazyMode::NonLazy),
        // legacy, deprecated options
        ("fs", LazyMode::Lazy),
        ("watchman", LazyMode::WatchmanDeprecated),
        ("none", LazyMode::NonLazy),
    ])
}

pub fn get_lazy_flags(args: &arg_spec::Values) -> Option<LazyMode> {
    let lazy_ = flow_command_spec::get(args, "--lazy", &arg_spec::truthy()).unwrap();
    let lazy_mode =
        flow_command_spec::get(args, "--lazy-mode", &arg_spec::optional(lazy_mode_flag())).unwrap();
    if lazy_ && lazy_mode.is_none() {
        // --lazy === --lazy-mode true
        Some(LazyMode::Lazy)
    } else {
        lazy_mode
    }
}

pub fn lazy_mode_arg(lazy_mode: LazyMode) -> &'static str {
    match lazy_mode {
        LazyMode::Lazy => "fs",
        LazyMode::NonLazy => "none",
        LazyMode::WatchmanDeprecated => "watchman",
    }
}

pub fn add_input_file_flag(spec: flow_command_spec::Spec, verb: &str) -> flow_command_spec::Spec {
    spec.flag(
        "--input-file",
        &arg_spec::optional(arg_spec::string()),
        &format!(
            "File containing list of files to {}, one per line. If -, list of files is read from the standard input.",
            verb
        ),
        None,
    )
}

pub fn add_verbose_focus_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--verbose-focus",
        &arg_spec::truthy(),
        "Print verbose output about target file only (implies --verbose)",
        None,
    )
}

pub fn add_from_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--from",
        &arg_spec::optional(arg_spec::string()),
        "Specify who is calling this CLI command (used by logging)",
        None,
    )
}

fn apply_from_flag(command: &Command, args: &arg_spec::Values) {
    if !command.flags().contains_key("--from") {
        return;
    }
    let from = args
        .get("--from")
        .and_then(|values| values.first().cloned());
    let from = match from {
        Some(from) => Some(from),
        None => default_from_flag(),
    };
    flow_event_logger::set_from(from);
    flow_event_logger::set_agent_id(flow_event_logger::agent_invocation_id());
}

fn default_from_flag() -> Option<String> {
    #[cfg(unix)]
    {
        let ppid = std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| {
                status.lines().find_map(|line| {
                    let mut words = line.split_whitespace();
                    match (words.next(), words.next()) {
                        (Some("PPid:"), Some(ppid)) => ppid.parse::<u32>().ok(),
                        _ => None,
                    }
                })
            })?;
        if ppid == 0 {
            return None;
        }
        let path = format!("/proc/{}/cmdline", ppid);
        let mut bytes = std::fs::read(path).ok()?;
        while bytes.last() == Some(&0) {
            bytes.pop();
        }
        let cmdline = String::from_utf8(bytes).ok()?;
        let cmdline = cmdline.replace('\0', " ");
        let cmdline = cmdline.trim();
        if cmdline.is_empty() {
            None
        } else {
            Some(format!("parent cmdline: {}", cmdline))
        }
    }
    #[cfg(not(unix))]
    {
        None
    }
}

pub fn add_strip_root_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--strip-root",
        &arg_spec::truthy(),
        "Print paths without the root",
        None,
    )
}

pub fn add_wait_for_recheck_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--wait-for-recheck",
        &arg_spec::optional(arg_spec::bool_flag()),
        "If the server is rechecking, wait for it to complete rather than run sooner using outdated data",
        None,
    )
}

pub(crate) fn add_vpn_less_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--vpn-less",
        &arg_spec::optional(arg_spec::bool_flag()),
        "True if enable the VPN-Less mode to query and download saved state",
        None,
    )
}

pub fn add_path_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--path",
        &arg_spec::optional(arg_spec::string()),
        "Specify (fake) path to file when reading data from stdin",
        None,
    )
}

pub fn add_autostop_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    // empty to omit it from --help
    spec.flag("--autostop", &arg_spec::truthy(), "", None)
}

pub fn add_verbose_flags(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--verbose",
        &arg_spec::truthy(),
        "Print verbose info during typecheck",
        None,
    )
    .flag(
        "--verbose-indent",
        &arg_spec::truthy(),
        "Indent verbose info during typecheck (implies --verbose)",
        None,
    )
    .flag(
        "--verbose-depth",
        &arg_spec::optional(arg_spec::string()),
        "Recursively print types up to specified depth (default 1, implies --verbose)",
        None,
    )
    .flag(
        "--verbose-flowlib",
        &arg_spec::truthy(),
        "Print verbose info while initializing the flowlib",
        None,
    )
}

pub fn verbose_flags(args: &arg_spec::Values) -> Option<Verbose> {
    let verbose = flow_command_spec::get(args, "--verbose", &arg_spec::truthy()).unwrap();
    let indent = flow_command_spec::get(args, "--verbose-indent", &arg_spec::truthy()).unwrap();
    let depth = flow_command_spec::get(
        args,
        "--verbose-depth",
        &arg_spec::optional(arg_spec::string()),
    )
    .unwrap();
    let enabled_during_flowlib =
        flow_command_spec::get(args, "--verbose-flowlib", &arg_spec::truthy()).unwrap();
    if !verbose && !indent && depth.is_none() {
        None
    } else {
        let depth = match depth {
            Some(depth) => depth.parse::<i32>().unwrap_or(1),
            None => 1,
        };
        Some(Verbose {
            indent: if indent { 2 } else { 0 },
            depth: if depth >= 0 { depth as u32 } else { 1 },
            enabled_during_flowlib,
            focused_files: None,
        })
    }
}

pub(crate) fn add_slow_to_check_logging_flags(
    spec: flow_command_spec::Spec,
) -> flow_command_spec::Spec {
    spec.flag(
        "--log-slow-files-interval",
        &arg_spec::optional(arg_spec::int()),
        "Specify the interval in seconds to log slow to check files. (default: 5 seconds)",
        None,
    )
    .flag(
        "--log-slow-components-threshold",
        &arg_spec::optional(arg_spec::int()),
        "Specify the threshold in seconds to log slow to check component. (default to not logging anything)",
        None,
    )
    .flag(
        "--log-slow-expressions-threshold",
        &arg_spec::optional(arg_spec::int()),
        "Specify the threshold in seconds to log slow to check expressions. (default to not logging anything)",
        None,
    )
}

pub(crate) fn add_quiet_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--quiet",
        &arg_spec::truthy(),
        "Suppress output about server startup",
        None,
    )
}

#[derive(Clone, Copy, Debug, Default)]
pub enum OnMismatchBehavior {
    #[default]
    ChooseNewest,
    StopServer,
    RestartClient,
    ErrorClient,
}

fn add_on_mismatch_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--on-mismatch",
        &arg_spec::required(
            Some(OnMismatchBehavior::ChooseNewest),
            arg_spec::enum_flag(vec![
                ("choose-newest", OnMismatchBehavior::ChooseNewest),
                ("stop-server", OnMismatchBehavior::StopServer),
                ("restart-client", OnMismatchBehavior::RestartClient),
                ("error-client", OnMismatchBehavior::ErrorClient),
            ]),
        ),
        "What to do when the client and server are different versions (choose-newest, stop-server, restart-client, error-client) (default: choose-newest)",
        None,
    )
}

pub fn add_root_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--root",
        &arg_spec::optional(arg_spec::string()),
        "Project root directory containing the .flowconfig",
        None,
    )
}

pub fn add_ignore_version_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--ignore-version",
        &arg_spec::truthy(),
        "Ignore the version constraint in .flowconfig",
        Some("FLOW_IGNORE_VERSION"),
    )
}

pub fn add_log_file_flags(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--log-file",
        &arg_spec::optional(arg_spec::string()),
        "Path to log file (default: /tmp/flow/<escaped root path>.log)",
        Some("FLOW_LOG_FILE"),
    )
    .flag(
        "--monitor-log-file",
        &arg_spec::optional(arg_spec::string()),
        "Path to log file (default: /tmp/flow/<escaped root path>.monitor_log)",
        Some("FLOW_MONITOR_LOG_FILE"),
    )
}

pub fn get_log_file_flags(args: &arg_spec::Values) -> (Option<String>, Option<String>) {
    fn normalize(log_file: String) -> String {
        let path = std::path::Path::new(&log_file);
        let dirname = path.parent().unwrap_or(std::path::Path::new("."));
        let basename = path.file_name().unwrap_or_default();
        dirname.join(basename).to_string_lossy().to_string()
    }
    let server_log_file =
        flow_command_spec::get(args, "--log-file", &arg_spec::optional(arg_spec::string()))
            .unwrap()
            .map(normalize);
    let monitor_log_file = flow_command_spec::get(
        args,
        "--monitor-log-file",
        &arg_spec::optional(arg_spec::string()),
    )
    .unwrap()
    .map(normalize);
    (server_log_file, monitor_log_file)
}

#[derive(Clone, Copy, Debug, Default)]
pub enum OffsetStyle {
    #[default]
    Utf8Bytes,
    JsIndices,
}

pub fn add_offset_style_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--offset-style",
        &arg_spec::optional(arg_spec::enum_flag(vec![
            ("utf8-bytes", OffsetStyle::Utf8Bytes),
            ("js-indices", OffsetStyle::JsIndices),
        ])),
        "How to compute offsets in JSON output (utf8-bytes, js-indices) (default: utf8-bytes)",
        None,
    )
}

pub fn get_offset_style(args: &arg_spec::Values) -> Option<OffsetStyle> {
    flow_command_spec::get(
        args,
        "--offset-style",
        &arg_spec::optional(arg_spec::enum_flag(vec![
            ("utf8-bytes", OffsetStyle::Utf8Bytes),
            ("js-indices", OffsetStyle::JsIndices),
        ])),
    )
    .unwrap()
}

pub fn offset_kind_of_offset_style(
    offset_style: Option<OffsetStyle>,
) -> flow_parser::offset_utils::OffsetKind {
    match offset_style {
        None | Some(OffsetStyle::Utf8Bytes) => flow_parser::offset_utils::OffsetKind::Utf8,
        Some(OffsetStyle::JsIndices) => flow_parser::offset_utils::OffsetKind::JavaScript,
    }
}

pub fn flowconfig_multi_error(errors: &[(u32, String)]) -> ! {
    let msg = errors
        .iter()
        .map(|(line, msg)| format!(".flowconfig:{} {}", line, msg))
        .collect::<Vec<_>>()
        .join("\n");
    flow_common_exit::exit(FlowExitStatus::InvalidFlowconfig, Some(&msg));
}

pub fn flowconfig_multi_warn(errors: &[(u32, String)]) {
    for (line, msg) in errors {
        eprintln!(".flowconfig:{} {}", line, msg);
    }
}

pub fn read_config_or_exit(flowconfig_path: &str, enforce_warnings: bool) -> FlowConfig {
    let (flowconfig, warnings, _flowconfig_hash) =
        match flow_config::get_with_ignored_version(flowconfig_path, false) {
            Ok(r) => r,
            Err(flow_config::Error(line, message)) => {
                flowconfig_multi_error(&[(line, message)]);
            }
        };
    flow_event_logger::set_root_name(flowconfig.options.root_name.clone());
    if warnings.is_empty() {
        flowconfig
    } else {
        let warnings = warnings
            .into_iter()
            .map(|flow_config::Warning(line, message)| (line, message))
            .collect::<Vec<_>>();
        if enforce_warnings {
            flowconfig_multi_error(&warnings);
        } else {
            flowconfig_multi_warn(&warnings);
        }
        flowconfig
    }
}

pub fn read_config_and_hash_or_exit(
    flowconfig_path: &str,
    enforce_warnings: bool,
) -> (FlowConfig, String) {
    let (flowconfig, warnings, flowconfig_hash) =
        match flow_config::get_with_ignored_version(flowconfig_path, false) {
            Ok(r) => r,
            Err(flow_config::Error(line, message)) => {
                flowconfig_multi_error(&[(line, message)]);
            }
        };
    flow_event_logger::set_root_name(flowconfig.options.root_name.clone());
    if warnings.is_empty() {
        return (flowconfig, flowconfig_hash);
    }
    let warnings = warnings
        .into_iter()
        .map(|flow_config::Warning(line, message)| (line, message))
        .collect::<Vec<_>>();
    if enforce_warnings {
        flowconfig_multi_error(&warnings);
    } else {
        flowconfig_multi_warn(&warnings);
    }
    (flowconfig, flowconfig_hash)
}

pub fn check_version(required_version: &Option<String>) -> Result<(), String> {
    match required_version {
        None => Ok(()),
        Some(version_constraint) => {
            // For the purposes of checking whether the *currently-running* version of Flow is compatible
            // with the given project, we'll include pre-releases. For example, this means that 0.61.0-beta
            // is compatible with >0.60.0, because it presumably implements the minimum necessary features
            // of 0.60.0.
            //
            // This is subtly different than determining which version of Flow to run in the first place,
            // like when npm/yarn is solving the `flow-bin` constraint. In that case, we do not want
            // >0.60.0 to opt into pre-releases automatically. Those sorts of checks should not pass
            // `~includes_prereleases`.
            //
            // So, if you've explicitly run v0.61.0-beta, and the flowconfig says `>0.60.0`, we'll allow it;
            // but if we were looking at the flowconfig to decide which version to run, you should not
            // choose the beta.
            match flow_common_semver::semver::satisfies(
                Some(true),
                version_constraint,
                flow_common::flow_version::version(),
            ) {
                Ok(true) => Ok(()),
                _ => {
                    let msg = format!(
                        "Wrong version of Flow. The config specifies version {} but this is version {}",
                        version_constraint,
                        flow_common::flow_version::version()
                    );
                    Err(msg)
                }
            }
        }
    }
}

pub fn assert_version(flowconfig: &FlowConfig) {
    let required_version = &flowconfig.version;
    match check_version(required_version) {
        Ok(()) => {}
        Err(msg) => flow_common_exit::exit(FlowExitStatus::InvalidFlowconfig, Some(&msg)),
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct FlowconfigFlags {
    pub ignores: Vec<String>,
    pub includes: Vec<String>,
    pub libs: Vec<String>,
    // We store raw_lint_severities as a string list instead of as a LintSettings.t so we
    // can defer parsing of the lint settings until after the flowconfig lint settings
    // are known, to properly detect redundant settings (and avoid false positives)
    pub raw_lint_severities: Vec<String>,
    pub untyped: Vec<String>,
    pub declarations: Vec<String>,
}

pub fn list_of_string_arg(arg: Option<String>) -> Vec<String> {
    match arg {
        Some(arg) if arg.is_empty() => vec![],
        Some(arg) => arg.split(',').map(ToOwned::to_owned).collect(),
        None => vec![],
    }
}

pub(crate) fn remove_exclusion(pattern: &str) -> &str {
    pattern.strip_prefix('!').unwrap_or(pattern)
}

fn parse_flowconfig_glob_or_exit(pattern: String, allow_negation: bool) -> FlowconfigGlobPattern {
    FlowconfigGlobPattern::try_new(pattern, allow_negation).unwrap_or_else(|error| {
        flow_common_exit::exit(FlowExitStatus::InvalidFlowconfig, Some(&error.message()))
    })
}

fn compile_ignore_pattern(
    root: &Path,
    pattern: FlowconfigGlobPattern,
    backup: Option<String>,
) -> flow_common::files::IgnorePattern {
    let matcher = RootedGlob::new(root, pattern.pattern())
        .expect("flowconfig glob should have been validated while parsing");
    flow_common::files::IgnorePattern {
        negated: pattern.is_negated(),
        backup,
        matcher,
    }
}

fn compile_untyped_pattern(
    root: &Path,
    pattern: FlowconfigGlobPattern,
) -> flow_common::files::UntypedPattern {
    let matcher = RootedGlob::new(root, pattern.pattern())
        .expect("flowconfig glob should have been validated while parsing");
    flow_common::files::UntypedPattern {
        negated: pattern.is_negated(),
        matcher,
    }
}

fn add_include_pattern(matcher: &mut PathMatcher, root: &Path, include: &FlowconfigGlobPattern) {
    matcher
        .add_glob(root, include.pattern())
        .expect("flowconfig glob should have been validated while parsing");
}

fn flowlib_builtin_lib(builtin_lib: ConfigBuiltinLib) -> flow_flowlib::BuiltinLib {
    match builtin_lib {
        ConfigBuiltinLib::Flowlib => flow_flowlib::BuiltinLib::Flowlib,
        ConfigBuiltinLib::FlowlibWithLibDomDts => flow_flowlib::BuiltinLib::FlowlibWithLibDomDts,
        ConfigBuiltinLib::Prelude => flow_flowlib::BuiltinLib::Prelude,
        ConfigBuiltinLib::Tslib => flow_flowlib::BuiltinLib::Tslib,
    }
}

pub fn file_options(
    flowconfig: &FlowConfig,
    root: &Path,
    builtin_lib_override: Option<&str>,
    temp_dir: &Path,
    ignores: Vec<(String, Option<String>)>,
    includes: Vec<String>,
    libs: Vec<String>,
    untyped: Vec<String>,
    declarations: Vec<String>,
) -> Arc<flow_common::files::FileOptions> {
    use flow_common::files::FileOptions;

    let builtin_lib = flowlib_builtin_lib(
        builtin_lib_override
            .and_then(config_builtin_lib_of_arg)
            .unwrap_or(flowconfig.options.builtin_lib),
    );
    let flowlib_dir = flow_flowlib::libdir(builtin_lib, temp_dir);
    let default_lib_dir = Some(match flowlib_dir {
        flow_flowlib::LibDir::Prelude(path) => flow_common::files::LibDir::Prelude(path),
        flow_flowlib::LibDir::Flowlib(path) => flow_common::files::LibDir::Flowlib(path),
        flow_flowlib::LibDir::FlowlibWithLibDomDts(path) => {
            flow_common::files::LibDir::FlowlibWithLibDomDts(path)
        }
        flow_flowlib::LibDir::Tslib(path) => flow_common::files::LibDir::Tslib(path),
    });

    let implicitly_include_root = flowconfig.options.files_implicitly_include_root;

    let all_ignores: Vec<(FlowconfigGlobPattern, Option<String>)> = {
        let mut merged = ignores
            .into_iter()
            .map(|(pattern, backup)| (parse_flowconfig_glob_or_exit(pattern, true), backup))
            .collect::<Vec<_>>();
        merged.extend(flowconfig.ignores.iter().cloned());
        merged
    };
    let ignores = all_ignores
        .into_iter()
        .map(|(pattern, backup)| compile_ignore_pattern(root, pattern, backup))
        .collect();

    let all_untyped: Vec<FlowconfigGlobPattern> = {
        let mut merged = untyped
            .into_iter()
            .map(|pattern| parse_flowconfig_glob_or_exit(pattern, true))
            .collect::<Vec<_>>();
        merged.extend(flowconfig.untyped.iter().cloned());
        merged
    };
    let untyped = all_untyped
        .into_iter()
        .map(|pattern| compile_untyped_pattern(root, pattern))
        .collect();

    let all_declarations: Vec<String> = {
        let mut merged = declarations;
        merged.extend(flowconfig.declarations.iter().cloned());
        merged
    };
    let declarations = all_declarations
        .into_iter()
        .map(|pattern| {
            let expanded =
                flow_common::files::expand_project_root_token(root, remove_exclusion(&pattern));
            let regex = Regex::new(&expanded).unwrap();
            (pattern, regex)
        })
        .collect();

    let flowtyped_path = flow_common::files::make_path_absolute(root, "flow-typed");
    let mut has_explicit_flowtyped_lib = false;
    let mut lib_paths: Vec<std::path::PathBuf> = flowconfig
        .libs
        .iter()
        .map(|path| {
            let expanded = flow_common::files::expand_project_root_token(root, path);
            let path_buf = flow_common::files::make_path_absolute(root, &expanded);
            if path_buf == flowtyped_path {
                has_explicit_flowtyped_lib = true;
            }
            path_buf
        })
        .collect();
    // "flow-typed" is always included in the libs list for convenience,
    // but there's no guarantee that it exists on the filesystem.
    if !has_explicit_flowtyped_lib && flowtyped_path.exists() {
        lib_paths.insert(0, flowtyped_path);
    }
    for lib in libs {
        lib_paths.push(flow_common::files::make_path_absolute(root, &lib));
    }

    let includes = {
        let mut merged_includes = includes
            .into_iter()
            .map(|include| parse_flowconfig_glob_or_exit(include, false))
            .collect::<Vec<_>>();
        merged_includes.extend(flowconfig.includes.iter().cloned());
        let mut matcher = PathMatcher::empty();
        for include in merged_includes {
            add_include_pattern(&mut matcher, root, &include);
        }
        // Implicitly included paths are added only if they're not already being watched
        let mut implicit_paths = Vec::new();
        if implicitly_include_root {
            implicit_paths.push(root.to_path_buf());
        }
        implicit_paths.extend(lib_paths.iter().cloned());
        // Shortest path first
        implicit_paths.sort_by_key(|path| path.to_string_lossy().len());
        for path in implicit_paths {
            let path_str = path.to_string_lossy().to_string();
            // If this include is already covered by an explicit include or a shorter implicit include,
            // then skip it
            if matcher.matches(&path_str) {
                continue;
            }
            matcher.add_path(&path);
        }
        matcher
    };

    let haste_paths_excludes = flowconfig
        .options
        .haste_paths_excludes
        .iter()
        .map(|s| {
            let expanded = flow_common::files::expand_project_root_token(root, s);
            Regex::new(&expanded).unwrap()
        })
        .collect();

    let haste_paths_includes = flowconfig
        .options
        .haste_paths_includes
        .iter()
        .map(|s| {
            let expanded = flow_common::files::expand_project_root_token(root, s);
            Regex::new(&expanded).unwrap()
        })
        .collect();

    let module_declaration_dirnames = flowconfig
        .options
        .module_declaration_dirnames
        .iter()
        .map(|dir| {
            let expanded = flow_common::files::expand_project_root_token(root, dir);
            flow_common::files::cached_canonicalize(std::path::Path::new(&expanded))
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or(expanded)
        })
        .collect();

    let mut module_file_exts = flowconfig.options.module_file_exts.clone();
    if flowconfig.options.typescript_library_definition_support {
        for ext in flow_parser::file_key::DTS_EXTENSIONS {
            if !module_file_exts.contains(&ext.into()) {
                module_file_exts.insert(0, ext.into());
            }
        }
    }

    Arc::new(FileOptions {
        default_lib_dir,
        ignores,
        untyped,
        declarations,
        implicitly_include_root,
        haste_paths_excludes,
        haste_paths_includes,
        includes,
        lib_paths,
        module_declaration_dirnames,
        module_file_exts,
        module_resource_exts: flowconfig
            .options
            .module_resource_exts
            .iter()
            .cloned()
            .collect(),
        multi_platform: flowconfig.options.multi_platform.unwrap_or(false),
        multi_platform_extensions: flowconfig.options.multi_platform_extensions.to_vec(),
        multi_platform_extension_group_mapping: flowconfig
            .options
            .multi_platform_extension_group_mapping
            .to_vec(),
        node_resolver_dirnames: flowconfig.options.node_resolver_dirnames.clone(),
    })
}

pub fn file_options_of_flowconfig(
    root: &Path,
    flowconfig: &FlowConfig,
) -> Arc<flow_common::files::FileOptions> {
    let temp_dir = normalize_temp_dir(&None);
    file_options(
        flowconfig,
        root,
        Some("prelude"),
        temp_dir.as_path(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    )
}

pub fn add_ignore_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--ignore",
        &arg_spec::optional(arg_spec::string()),
        "Specify one or more ignore patterns, comma separated",
        None,
    )
}

pub fn add_untyped_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--untyped",
        &arg_spec::optional(arg_spec::string()),
        "Specify one or more patterns, comma separated, for files to treat as untyped",
        None,
    )
}

pub fn add_declaration_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--declaration",
        &arg_spec::optional(arg_spec::string()),
        "Specify one or more patterns, comma separated, for files to treat as declarations",
        None,
    )
}

pub fn add_include_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--include",
        &arg_spec::optional(arg_spec::string()),
        "Specify one or more include patterns, comma separated",
        None,
    )
}

pub(crate) fn add_lib_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--lib",
        &arg_spec::optional(arg_spec::string()),
        "Specify one or more lib files/directories, comma separated",
        None,
    )
}

pub(crate) fn add_lints_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--lints",
        &arg_spec::optional(arg_spec::string()),
        "Specify one or more lint rules, comma separated",
        None,
    )
}

pub fn add_no_restart_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--no-auto-restart",
        &arg_spec::truthy(),
        "If the server dies, do not try and restart it; just exit",
        None,
    )
}

pub fn add_flowconfig_flags(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    add_lints_flag(add_lib_flag(add_include_flag(add_declaration_flag(
        add_untyped_flag(add_ignore_flag(spec)),
    ))))
}

#[derive(Clone, Debug, Default)]
pub struct ConnectParams {
    pub retries: i32,
    pub timeout: Option<i32>,
    pub no_auto_start: bool,
    pub autostop: bool,
    pub lazy_mode: Option<LazyMode>,
    pub temp_dir: Option<String>,
    pub ignore_version: bool,
    pub quiet: bool,
    pub on_mismatch: OnMismatchBehavior,
}

// The Rust port uses TCP loopback sockets. Under heavily parallel startup, a
// ready monitor can take longer than three seconds to drain connection attempts.
const DEFAULT_CONNECT_RETRIES: i32 = 10;

// Seconds between "Please wait. ..." lines for AI-agent invocations when the
// flowconfig does not set `agent_progress_keepalive`.
const DEFAULT_AGENT_PROGRESS_KEEPALIVE_SECS: u64 = 30;

fn progress_mode(
    quiet: bool,
    agent_invocation: bool,
    agent_progress_keepalive: Option<u32>,
) -> ProgressMode {
    if quiet {
        ProgressMode::Silent
    } else if !agent_invocation {
        ProgressMode::Live
    } else {
        match agent_progress_keepalive {
            Some(0) => ProgressMode::Silent,
            Some(secs) => ProgressMode::Throttled {
                interval: std::time::Duration::from_secs(secs as u64),
            },
            None => ProgressMode::Throttled {
                interval: std::time::Duration::from_secs(DEFAULT_AGENT_PROGRESS_KEEPALIVE_SECS),
            },
        }
    }
}

fn should_emit_keepalive(
    now: std::time::Instant,
    last_print: Option<std::time::Instant>,
    interval: std::time::Duration,
) -> bool {
    last_print.is_none_or(|t| now.duration_since(t) >= interval)
}

pub fn get_connect_flags(args: &arg_spec::Values) -> ConnectParams {
    let lazy_ = flow_command_spec::get(args, "--lazy", &arg_spec::truthy()).unwrap();
    let lazy_mode =
        flow_command_spec::get(args, "--lazy-mode", &arg_spec::optional(lazy_mode_flag())).unwrap();
    let lazy_mode = if lazy_ && lazy_mode.is_none() {
        Some(LazyMode::Lazy)
    } else {
        lazy_mode
    };
    let timeout = flow_command_spec::get(args, "--timeout", &arg_spec::int()).unwrap();
    match timeout {
        Some(n) if n <= 0 => {
            let msg = format!("Timeout must be a positive integer. Got {}", n);
            flow_common_exit::exit(FlowExitStatus::CommandlineUsageError, Some(&msg));
        }
        _ => {}
    }
    ConnectParams {
        retries: flow_command_spec::get(args, "--retries", &arg_spec::int())
            .unwrap()
            .unwrap_or(DEFAULT_CONNECT_RETRIES),
        timeout,
        no_auto_start: flow_command_spec::get(args, "--no-auto-start", &arg_spec::truthy())
            .unwrap(),
        autostop: false,
        lazy_mode,
        temp_dir: flow_command_spec::get(
            args,
            "--temp-dir",
            &arg_spec::optional(arg_spec::string()),
        )
        .unwrap(),
        ignore_version: flow_command_spec::get(args, "--ignore-version", &arg_spec::truthy())
            .unwrap(),
        quiet: flow_command_spec::get(args, "--quiet", &arg_spec::truthy()).unwrap(),
        on_mismatch: flow_command_spec::get(
            args,
            "--on-mismatch",
            &arg_spec::required(
                Some(OnMismatchBehavior::ChooseNewest),
                arg_spec::enum_flag(vec![
                    ("choose-newest", OnMismatchBehavior::ChooseNewest),
                    ("stop-server", OnMismatchBehavior::StopServer),
                    ("restart-client", OnMismatchBehavior::RestartClient),
                    ("error-client", OnMismatchBehavior::ErrorClient),
                ]),
            ),
        )
        .unwrap(),
    }
}

fn add_connect_flags_with_lazy_collector(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    let spec = spec
        .flag(
            "--timeout",
            &arg_spec::int(),
            "Maximum time to wait, in seconds",
            None,
        )
        .flag(
            "--retries",
            &arg_spec::int(),
            "Set the number of retries. (default: 10)",
            None,
        )
        .flag(
            "--no-auto-start",
            &arg_spec::truthy(),
            "If the server is not running, do not start it; just exit",
            None,
        );
    let spec = add_temp_dir_flag(spec);
    let spec = add_from_flag(spec);
    let spec = add_ignore_version_flag(spec);
    let spec = add_quiet_flag(spec);
    add_on_mismatch_flag(spec)
}

pub fn add_connect_flags_no_lazy(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    add_connect_flags_with_lazy_collector(spec)
}

pub fn add_connect_flags(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    add_connect_flags_with_lazy_collector(add_lazy_flags(spec))
}

pub fn add_connect_and_json_flags(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    // For commands that take both --quiet and --json or --pretty, make the latter two imply --quiet
    add_json_flags(add_connect_flags(spec))
}

pub fn get_connect_and_json_flags(args: &arg_spec::Values) -> (ConnectParams, bool, bool) {
    let connect_flags = get_connect_flags(args);
    let JsonFlags { json, pretty } = get_json_flags(args);
    (
        ConnectParams {
            quiet: connect_flags.quiet || json || pretty,
            ..connect_flags
        },
        json,
        pretty,
    )
}

pub fn server_log_file(flowconfig_name: &str, tmp_dir: &str, root: &std::path::Path) -> String {
    flow_server_files::server_files_js::log_file(flowconfig_name, tmp_dir, root)
}

pub fn monitor_log_file(flowconfig_name: &str, tmp_dir: &str, root: &std::path::Path) -> String {
    flow_server_files::server_files_js::monitor_log_file(flowconfig_name, tmp_dir, root)
}

#[derive(Clone, Debug)]
pub struct OptionsFlags {
    pub all: bool,
    pub debug: bool,
    pub flowconfig_flags: FlowconfigFlags,
    pub include_warnings: bool,
    pub max_warnings: Option<i32>,
    pub max_workers: Option<i32>,
    pub merge_timeout: Option<i32>,
    pub munge_underscore_members: bool,
    pub builtin_lib: Option<String>,
    pub profile: bool,
    pub quiet: bool,
    pub slow_to_check_logging: flow_common::slow_to_check_logging::SlowToCheckLogging,
    pub strip_root: bool,
    pub temp_dir: Option<String>,
    pub verbose: Option<Verbose>,
    pub wait_for_recheck: Option<bool>,
    pub vpn_less: Option<bool>,
    pub include_suppressions: bool,
    pub estimate_recheck_time: Option<bool>,
    pub distributed: bool,
    pub no_autoimports: bool,
}

#[derive(Clone, Debug)]
pub struct SavedStateFlags {
    pub saved_state_fetcher: Option<SavedStateFetcher>,
    pub saved_state_force_recheck: bool,
    pub saved_state_no_fallback: bool,
    pub saved_state_skip_version_check: bool,
    pub saved_state_verify: bool,
}

#[derive(Clone, Debug)]
pub struct BaseFlags {
    pub flowconfig_name: String,
}

fn builtin_lib_flag() -> arg_spec::FlagType<Option<String>> {
    arg_spec::enum_flag(vec![
        ("default", "default".to_owned()),
        ("core-only", "core-only".to_owned()),
        ("prelude", "prelude".to_owned()),
        ("experimental.tslib", "experimental.tslib".to_owned()),
    ])
}

fn config_builtin_lib_of_arg(value: &str) -> Option<ConfigBuiltinLib> {
    match value {
        "core-only" => Some(ConfigBuiltinLib::Flowlib),
        "default" => Some(ConfigBuiltinLib::FlowlibWithLibDomDts),
        "prelude" => Some(ConfigBuiltinLib::Prelude),
        "experimental.tslib" => Some(ConfigBuiltinLib::Tslib),
        _ => None,
    }
}

pub fn builtin_lib_arg(builtin_lib: ConfigBuiltinLib) -> &'static str {
    match builtin_lib {
        ConfigBuiltinLib::Flowlib => "core-only",
        ConfigBuiltinLib::FlowlibWithLibDomDts => "default",
        ConfigBuiltinLib::Prelude => "prelude",
        ConfigBuiltinLib::Tslib => "experimental.tslib",
    }
}

pub(crate) fn parse_lints_flag(
    base_settings: flow_lint_settings::lint_settings::LintSettings<
        flow_lint_settings::severity::Severity,
    >,
    flag_settings: &[String],
) -> flow_lint_settings::lint_settings::LintSettings<flow_lint_settings::severity::Severity> {
    let lines: Vec<(u32, String)> = flag_settings
        .iter()
        .enumerate()
        .map(|(i, s)| ((i + 1) as u32, s.clone()))
        .collect();
    let settings =
        match flow_lint_settings::lint_settings::LintSettings::of_lines(base_settings, lines) {
            Ok((settings, warnings)) if warnings.is_empty() => Ok(settings),
            Ok((_, warnings)) => {
                // upgrade CLI warnings to errors
                let first = &warnings[0];
                Err((first.line, first.message.clone()))
            }
            Err(err) => Err((err.line, err.message)),
        };
    match settings {
        Ok(settings) => settings,
        Err((line, msg)) => {
            let msg = format!("Error parsing --lints (rule {}): {}", line, msg);
            flow_common_exit::exit(FlowExitStatus::CommandlineUsageError, Some(&msg));
        }
    }
}

pub fn add_options_flags(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    let spec = add_profile_flag(spec.flag(
        "--debug",
        &arg_spec::truthy(),
        "Print debug info during typecheck",
        Some("FLOW_DEBUG"),
    ))
    .flag(
        "--all",
        &arg_spec::truthy(),
        "Typecheck all files, not just @flow",
        None,
    )
    .flag(
        "--wait-for-recheck",
        &arg_spec::optional_value_with_default(true, arg_spec::bool_flag()),
        "If true, always wait for rechecks to finish before serving commands (default: false)",
        None,
    )
    .flag(
        "--builtin-lib",
        &builtin_lib_flag(),
        "Select the embedded library definitions",
        Some("FLOW_BUILTIN_LIB"),
    )
    .flag(
        "--munge-underscore-members",
        &arg_spec::truthy(),
        "Treat any class member name with a leading underscore as private",
        None,
    )
    .flag(
        "--max-workers",
        &arg_spec::optional(arg_spec::int()),
        "Maximum number of workers to create (capped by number of cores)",
        Some("FLOW_MAX_WORKERS"),
    );
    let spec = add_warning_flags(spec);
    let spec = add_flowconfig_flags(spec);
    let spec = add_verbose_flags(spec);
    let spec = add_slow_to_check_logging_flags(spec);
    let spec = add_strip_root_flag(spec);
    let spec = add_temp_dir_flag(spec);
    let spec = add_vpn_less_flag(spec);
    add_quiet_flag(spec)
    .flag(
        "--merge-timeout",
        &arg_spec::int(),
        "The maximum time in seconds to attempt to typecheck a file or cycle of files. 0 means no timeout (default: 100)",
        Some("FLOW_MERGE_TIMEOUT"),
    )
    .flag(
        "--include-suppressed",
        &arg_spec::truthy(),
        "Ignore any `suppress_comment` lines in .flowconfig",
        None,
    )
    // restarting to save time is a hack and should be removed. this should
    // not be part of our public API, so not included in the docs.
    .flag(
        "--estimate-recheck-time",
        &arg_spec::optional_value_with_default(true, arg_spec::bool_flag()),
        "",
        Some("FLOW_ESTIMATE_RECHECK_TIME"),
    )
    .flag("--distributed", &arg_spec::truthy(), "", None)
    .flag(
        "--no-autoimports",
        &arg_spec::truthy(),
        "Disable auto-imports (always disabled for foreground commands like full-check/focus-check)",
        None,
    )
}

pub fn get_options_flags(args: &arg_spec::Values) -> OptionsFlags {
    let merge_timeout = flow_command_spec::get(args, "--merge-timeout", &arg_spec::int()).unwrap();
    match merge_timeout {
        Some(timeout) if timeout < 0 => {
            flow_common_exit::exit(
                FlowExitStatus::CommandlineUsageError,
                Some("--merge-timeout must be non-negative"),
            );
        }
        _ => {}
    }

    let ignores_str =
        flow_command_spec::get(args, "--ignore", &arg_spec::optional(arg_spec::string())).unwrap();
    let untyped_str =
        flow_command_spec::get(args, "--untyped", &arg_spec::optional(arg_spec::string())).unwrap();
    let declarations_str = flow_command_spec::get(
        args,
        "--declaration",
        &arg_spec::optional(arg_spec::string()),
    )
    .unwrap();
    let includes_str =
        flow_command_spec::get(args, "--include", &arg_spec::optional(arg_spec::string())).unwrap();
    let lib_str =
        flow_command_spec::get(args, "--lib", &arg_spec::optional(arg_spec::string())).unwrap();
    let lints_str =
        flow_command_spec::get(args, "--lints", &arg_spec::optional(arg_spec::string())).unwrap();

    OptionsFlags {
        all: flow_command_spec::get(args, "--all", &arg_spec::truthy()).unwrap(),
        debug: flow_command_spec::get(args, "--debug", &arg_spec::truthy()).unwrap(),
        flowconfig_flags: FlowconfigFlags {
            ignores: list_of_string_arg(ignores_str),
            includes: list_of_string_arg(includes_str),
            libs: list_of_string_arg(lib_str),
            raw_lint_severities: list_of_string_arg(lints_str),
            untyped: list_of_string_arg(untyped_str),
            declarations: list_of_string_arg(declarations_str),
        },
        include_warnings: flow_command_spec::get(args, "--include-warnings", &arg_spec::truthy())
            .unwrap(),
        max_warnings: flow_command_spec::get(
            args,
            "--max-warnings",
            &arg_spec::optional(arg_spec::int()),
        )
        .unwrap(),
        max_workers: flow_command_spec::get(
            args,
            "--max-workers",
            &arg_spec::optional(arg_spec::int()),
        )
        .unwrap(),
        merge_timeout,
        munge_underscore_members: flow_command_spec::get(
            args,
            "--munge-underscore-members",
            &arg_spec::truthy(),
        )
        .unwrap(),
        builtin_lib: flow_command_spec::get(args, "--builtin-lib", &builtin_lib_flag()).unwrap(),
        profile: flow_command_spec::get(args, "--profile", &arg_spec::truthy()).unwrap(),
        quiet: flow_command_spec::get(args, "--quiet", &arg_spec::truthy()).unwrap(),
        slow_to_check_logging: flow_common::slow_to_check_logging::SlowToCheckLogging {
            slow_files_logging_internal: flow_command_spec::get(
                args,
                "--log-slow-files-interval",
                &arg_spec::optional(arg_spec::int()),
            )
            .unwrap()
            .map(|x| x as f64),
            slow_components_logging_threshold: flow_command_spec::get(
                args,
                "--log-slow-components-threshold",
                &arg_spec::optional(arg_spec::int()),
            )
            .unwrap()
            .map(|x| x as f64),
            slow_expressions_logging_threshold: flow_command_spec::get(
                args,
                "--log-slow-expressions-threshold",
                &arg_spec::optional(arg_spec::int()),
            )
            .unwrap()
            .map(|x| x as f64),
        },
        strip_root: flow_command_spec::get(args, "--strip-root", &arg_spec::truthy()).unwrap(),
        temp_dir: flow_command_spec::get(
            args,
            "--temp-dir",
            &arg_spec::optional(arg_spec::string()),
        )
        .unwrap(),
        verbose: verbose_flags(args),
        wait_for_recheck: flow_command_spec::get(
            args,
            "--wait-for-recheck",
            &arg_spec::optional_value_with_default(true, arg_spec::bool_flag()),
        )
        .unwrap(),
        vpn_less: flow_command_spec::get(
            args,
            "--vpn-less",
            &arg_spec::optional_value_with_default(true, arg_spec::bool_flag()),
        )
        .unwrap(),
        include_suppressions: flow_command_spec::get(
            args,
            "--include-suppressed",
            &arg_spec::truthy(),
        )
        .unwrap(),
        estimate_recheck_time: flow_command_spec::get(
            args,
            "--estimate-recheck-time",
            &arg_spec::optional_value_with_default(true, arg_spec::bool_flag()),
        )
        .unwrap(),
        distributed: flow_command_spec::get(args, "--distributed", &arg_spec::truthy()).unwrap(),
        no_autoimports: flow_command_spec::get(args, "--no-autoimports", &arg_spec::truthy())
            .unwrap(),
    }
}

pub(crate) fn saved_state_fetcher_flag() -> arg_spec::FlagType<Option<SavedStateFetcher>> {
    arg_spec::enum_flag(vec![
        ("none", SavedStateFetcher::DummyFetcher),
        ("local", SavedStateFetcher::LocalFetcher),
        ("scm", SavedStateFetcher::ScmFetcher),
        ("fb", SavedStateFetcher::FbFetcher),
    ])
}

pub fn add_saved_state_flags(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--saved-state-fetcher",
        &saved_state_fetcher_flag(),
        "Which saved state fetcher Flow should use (none, local) (default: none)",
        None,
    )
    .flag(
        "--saved-state-force-recheck",
        &arg_spec::truthy(),
        "Force a lazy server to recheck the changes since the saved state was generated",
        None,
    )
    .flag(
        "--saved-state-no-fallback",
        &arg_spec::truthy(),
        "If saved state fails to load, exit (normally fallback is to initialize from scratch)",
        None,
    )
    // This is really unsafe! Saved state is marshal'd data and it's
    // easy to introduce serialization differences that would lead to
    // segfaults. This is only for debugging.
    .flag(
        "--saved-state-skip-version-check-DO_NOT_USE_OR_YOU_WILL_BE_FIRED",
        &arg_spec::truthy(),
        "",
        Some("FLOW_SAVED_STATE_SKIP_VERSION_CHECK_DO_NOT_USE_OR_YOU_WILL_BE_FIRED"),
    )
    .flag(
        "--saved-state-verify",
        &arg_spec::truthy(),
        "Verifies that the saved state matches what is on disk (for debugging only)",
        None,
    )
}

pub fn get_saved_state_flags(args: &arg_spec::Values) -> SavedStateFlags {
    SavedStateFlags {
        saved_state_fetcher: flow_command_spec::get(
            args,
            "--saved-state-fetcher",
            &saved_state_fetcher_flag(),
        )
        .unwrap(),
        saved_state_force_recheck: flow_command_spec::get(
            args,
            "--saved-state-force-recheck",
            &arg_spec::truthy(),
        )
        .unwrap(),
        saved_state_no_fallback: flow_command_spec::get(
            args,
            "--saved-state-no-fallback",
            &arg_spec::truthy(),
        )
        .unwrap(),
        saved_state_skip_version_check: flow_command_spec::get(
            args,
            "--saved-state-skip-version-check-DO_NOT_USE_OR_YOU_WILL_BE_FIRED",
            &arg_spec::truthy(),
        )
        .unwrap(),
        saved_state_verify: flow_command_spec::get(
            args,
            "--saved-state-verify",
            &arg_spec::truthy(),
        )
        .unwrap(),
    }
}

pub fn add_flowconfig_name_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--flowconfig-name",
        &arg_spec::required(Some(".flowconfig".to_string()), arg_spec::string()),
        "Set the name of the flow configuration file. (default: .flowconfig)",
        Some("FLOW_CONFIG_NAME"),
    )
}

pub fn add_base_flags(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    add_flowconfig_name_flag(spec)
}

pub fn get_base_flags(args: &arg_spec::Values) -> BaseFlags {
    BaseFlags {
        flowconfig_name: flow_command_spec::get(
            args,
            "--flowconfig-name",
            &arg_spec::required(Some(".flowconfig".to_string()), arg_spec::string()),
        )
        .unwrap(),
    }
}

const DEFAULT_FILE_WATCHER_TIMEOUT: u32 = 120;

const DEFAULT_FILE_WATCHER_MERGEBASE_WITH: &str = "master";

pub(crate) fn file_watcher_flag() -> arg_spec::FlagType<Option<flow_config::FileWatcher>> {
    arg_spec::enum_flag(vec![
        ("none", flow_config::FileWatcher::NoFileWatcher),
        ("dfind", flow_config::FileWatcher::DFind),
        ("watchman", flow_config::FileWatcher::Watchman),
        ("edenfs", flow_config::FileWatcher::EdenFS),
    ])
}

pub fn add_file_watcher_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    let watcher_names = if flow_edenfs_watcher::is_available() {
        "none, dfind, watchman, edenfs"
    } else {
        "none, dfind, watchman"
    };
    spec.flag(
        "--file-watcher",
        &arg_spec::optional(file_watcher_flag()),
        &format!("Which file watcher Flow should use ({}). Flow will ignore file system events if this is set to none. (default: dfind)", watcher_names),
        None,
    )
    .flag(
        "--file-watcher-debug",
        &arg_spec::truthy(),
        "Enable debug logging for the file watcher. This is very noisy",
        Some("FLOW_FILE_WATCHER_DEBUG"),
    )
    .flag(
        "--file-watcher-timeout",
        &arg_spec::optional(arg_spec::uint()),
        &format!(
            "Maximum time to wait for the file watcher to initialize, in seconds. 0 means no timeout (default: {})",
            DEFAULT_FILE_WATCHER_TIMEOUT
        ),
        None,
    )
    .flag(
        "--file-watcher-mergebase-with",
        &arg_spec::optional(arg_spec::string()),
        &format!(
            "Symbolic commit against which to compute the mergebase used to find changed files. (default: {})",
            DEFAULT_FILE_WATCHER_MERGEBASE_WITH
        ),
        None,
    )
    .flag(
        "--file-watcher-sync-timeout",
        &arg_spec::optional(arg_spec::uint()),
        "Maximum time to wait for the file watcher to synchronize, in milliseconds. 0 means no timeout. Currently only used by Watchman.",
        None,
    )
}

pub struct FileWatcherFlags {
    pub file_watcher: Option<flow_config::FileWatcher>,
    pub file_watcher_debug: bool,
    pub file_watcher_timeout: Option<u32>,
    pub file_watcher_mergebase_with: Option<String>,
    pub file_watcher_sync_timeout: Option<u32>,
}

pub fn get_file_watcher_flags(args: &arg_spec::Values) -> FileWatcherFlags {
    FileWatcherFlags {
        file_watcher: flow_command_spec::get(
            args,
            "--file-watcher",
            &arg_spec::optional(file_watcher_flag()),
        )
        .unwrap(),
        file_watcher_debug: flow_command_spec::get(
            args,
            "--file-watcher-debug",
            &arg_spec::truthy(),
        )
        .unwrap(),
        file_watcher_timeout: flow_command_spec::get(
            args,
            "--file-watcher-timeout",
            &arg_spec::optional(arg_spec::uint()),
        )
        .unwrap()
        .map(|x| x as u32),
        file_watcher_mergebase_with: flow_command_spec::get(
            args,
            "--file-watcher-mergebase-with",
            &arg_spec::optional(arg_spec::string()),
        )
        .unwrap(),
        file_watcher_sync_timeout: flow_command_spec::get(
            args,
            "--file-watcher-sync-timeout",
            &arg_spec::optional(arg_spec::uint()),
        )
        .unwrap()
        .map(|x| x as u32),
    }
}

pub fn add_options_and_json_flags(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    // For commands that take both --quiet and --json or --pretty, make the latter two imply --quiet
    add_json_flags(add_options_flags(spec))
}

pub fn add_json_version_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--json-version",
        &arg_spec::optional(arg_spec::string()),
        "Version of the JSON output format (1 or 2)",
        None,
    )
}

pub fn get_json_version(
    args: &arg_spec::Values,
) -> Option<flow_common_errors::error_utils::json_output::JsonVersion> {
    flow_command_spec::get(
        args,
        "--json-version",
        &arg_spec::optional(arg_spec::string()),
    )
    .unwrap()
    .map(|version| match version.as_str() {
        "1" => flow_common_errors::error_utils::json_output::JsonVersion::JsonV1,
        "2" => flow_common_errors::error_utils::json_output::JsonVersion::JsonV2,
        _ => {
            let msg = format!("Invalid --json-version value: {}", version);
            flow_common_exit::exit(FlowExitStatus::CommandlineUsageError, Some(&msg));
        }
    })
}

pub fn add_no_cgroup_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    // If a command uses this flag, then it will automatically exec systemd-run (if systemd-run is
    // available). This can add a couple hundred ms to the start up time of the command. Only commands
    // that are resource-intensive (or spawn resource intensive processes) and probably should run in
    // cgroups should use this flag.
    spec.flag(
        "--no-cgroup",
        &arg_spec::truthy(),
        "Don't automatically run this command in a cgroup (if cgroups are available)",
        None,
    )
}

pub fn maybe_run_in_cgroup(args: &arg_spec::Values) {
    let no_cgroup = flow_command_spec::get(args, "--no-cgroup", &arg_spec::truthy()).unwrap();
    if no_cgroup {
        return;
    }
    #[cfg(unix)]
    exec_in_cgroup_if_systemd_available();
}

#[cfg(unix)]
fn exec_in_cgroup_if_systemd_available() {
    // We only trigger this behavior if we're on Unix and systemd-run is in the path
    fn can_run_systemd() -> bool {
        // Use `timeout` in case it hangs mysteriously. `--quiet` only suppresses stdout.
        ProcessCommand::new("timeout")
            .arg("1")
            .arg("systemd-run")
            .arg("--quiet")
            .arg("--user")
            .arg("--scope")
            .arg("--")
            .arg("true")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            // If all goes right, `systemd-run` will return immediately with exit code 0 and run `true`
            // asynchronously as a service. If it goes wrong, it will exit with a non-zero exit code
            .status()
            .is_ok_and(|status| status.success())
    }

    // Sometimes systemd-run is available but we can't use it. For example, the systemd might not have
    // a proper working user session, so we might not be able to run commands via systemd-run as a
    // user process. Notably, `--user --scope` is broken under cgroupv2 in systemd < 238, and exits
    // code 1 (https://github.com/meta/flow/issues/8012).
    if !can_run_systemd() {
        return;
    }

    let argv: Vec<_> = std::env::args_os().collect();
    if argv.len() < 2 {
        return;
    }

    let flow_exe = argv[0].clone();
    let command = argv[1].clone();
    let mut flow_args = vec![flow_exe];
    flow_args.push(command);
    flow_args.push("--no-cgroup".into());
    flow_args.extend(argv.into_iter().skip(2));

    // Basically re-exec ourselves with the --no-cgroup flag using systemd-run
    use std::os::unix::process::CommandExt;

    let err = ProcessCommand::new("systemd-run")
        .arg("--quiet")
        .arg("--user")
        .arg("--scope")
        .arg("--slice")
        .arg("flow.slice")
        .arg("--")
        .args(&flow_args)
        .exec();
    eprintln!("Failed to re-exec under systemd-run: {}", err);
}

pub fn get_temp_dir(cli_value: &Option<String>) -> String {
    // Commands that expose `--temp-dir` pick FLOW_TEMP_DIR up through the flag's
    // env fallback, but in-process callers (dev-tools, `ls`, file options) pass
    // `None` and must resolve it the same way, or they talk to a different server
    // than the one the caller started.
    if let Some(v) = cli_value {
        return v.clone();
    }
    match std::env::var("FLOW_TEMP_DIR") {
        Ok(v) if !v.is_empty() => v,
        _ => flow_server_files::server_files_js::default_temp_dir()
            .to_string_lossy()
            .to_string(),
    }
}

pub fn normalize_temp_dir(cli_value: &Option<String>) -> PathBuf {
    let temp_dir = get_temp_dir(cli_value);
    #[cfg(windows)]
    let temp_dir = temp_dir.replace('/', "\\");
    let path = absolute(&temp_dir).unwrap_or_else(|error| {
        let message = format!("Failed to resolve temporary directory {temp_dir}: {error}");
        flow_common_exit::exit(FlowExitStatus::InputError, Some(&message));
    });
    flow_common::files::cached_canonicalize(&path).unwrap_or_else(|_| {
        PathBuf::from(flow_common::files::imaginary_realpath(
            &path.to_string_lossy(),
        ))
    })
}

pub fn make_options(
    flowconfig_name: String,
    flowconfig_hash: String,
    flowconfig: FlowConfig,
    lazy_mode: Option<LazyMode>,
    root: std::path::PathBuf,
    options_flags: OptionsFlags,
    saved_state_options_flags: SavedStateFlags,
) -> Options {
    let lazy_mode_override = lazy_mode;

    let OptionsFlags {
        all: all_override,
        debug: debug_override,
        flowconfig_flags,
        include_warnings: include_warnings_override,
        max_warnings: max_warnings_override,
        max_workers: max_workers_override,
        merge_timeout: merge_timeout_override,
        munge_underscore_members: munge_underscore_members_override,
        builtin_lib: builtin_lib_override,
        profile: profile_override,
        quiet: quiet_override,
        slow_to_check_logging: slow_to_check_logging_override,
        strip_root: strip_root_override,
        temp_dir: temp_dir_override,
        verbose: verbose_override,
        wait_for_recheck: wait_for_recheck_override,
        vpn_less: vpn_less_override,
        include_suppressions: include_suppressions_override,
        estimate_recheck_time: estimate_recheck_time_override,
        distributed: distributed_override,
        no_autoimports: no_autoimports_override,
    } = options_flags;

    let SavedStateFlags {
        saved_state_fetcher: saved_state_fetcher_override,
        saved_state_force_recheck,
        saved_state_no_fallback,
        saved_state_skip_version_check: saved_state_skip_version_check_override,
        saved_state_verify,
    } = saved_state_options_flags;

    let FlowconfigFlags {
        ignores: ignores_override,
        includes: includes_override,
        libs: libs_override,
        raw_lint_severities,
        untyped: untyped_override,
        declarations: declarations_override,
    } = flowconfig_flags;

    let temp_dir = {
        normalize_temp_dir(&temp_dir_override)
            .to_string_lossy()
            .to_string()
    };

    let file_options = file_options(
        &flowconfig,
        &root,
        builtin_lib_override.as_deref(),
        std::path::Path::new(&temp_dir),
        ignores_override
            .into_iter()
            .map(|ignore| (ignore, None))
            .collect(),
        includes_override,
        libs_override,
        untyped_override,
        declarations_override,
    );

    let FlowConfig {
        rollouts,
        ignores: _,
        untyped: _,
        declarations: _,
        includes: _,
        libs: _,
        lint_severities,
        strict_mode,
        options:
            flow_config::opts::Opts {
                agent_helper_hints: _,
                agent_progress_keepalive: _,
                all,
                autoimports,
                autoimports_min_characters,
                autoimports_ranked_by_usage,
                autoimports_ranked_by_usage_boost_exact_match_min_length,
                automatic_require_default,
                babel_loose_array_spread,
                ban_spread_key_props,
                component_syntax,
                async_component_syntax,
                async_component_syntax_includes,
                dev_only_refinement_info_as_errors,
                emoji: _emoji,
                enable_const_params,
                enums,
                estimate_recheck_time,
                export_as_namespace_support,
                saved_state_restart_on_reinit,
                facebook_fbs,
                facebook_fbt,
                facebook_module_interop,
                file_watcher: _file_watcher,
                file_watcher_edenfs_throttle_time_ms: _file_watcher_edenfs_throttle_time_ms,
                file_watcher_edenfs_timeout: _file_watcher_edenfs_timeout,
                file_watcher_edenfs_max_commit_distance: _file_watcher_edenfs_max_commit_distance,
                file_watcher_edenfs_subprocess_mergebase: _file_watcher_edenfs_subprocess_mergebase,
                file_watcher_mergebase_with: _file_watcher_mergebase_with,
                file_watcher_mergebase_with_git: _file_watcher_mergebase_with_git,
                file_watcher_mergebase_with_hg: _file_watcher_mergebase_with_hg,
                file_watcher_timeout: _file_watcher_timeout,
                files_implicitly_include_root: _,
                format_bracket_spacing,
                format_single_quotes,
                #[cfg(fbcode_build)]
                    fox: _,
                haste_disallow_dynamic_import,
                haste_module_ref_prefix,
                haste_paths_excludes: _,
                haste_paths_includes: _,
                hook_compatibility,
                hook_compatibility_includes,
                hook_compatibility_excludes,
                ignore_non_literal_requires,
                include_warnings,
                jest_integration,
                lazy_mode,
                log_per_error_typing_telemetry,
                log_saving,
                max_files_checked_per_worker,
                max_files_checked_per_worker_rust_port,
                max_header_tokens,
                max_seconds_for_check_per_worker,
                max_workers,
                merge_timeout,
                missing_module_generators,
                module_declaration_dirnames: _,
                module_file_exts: _,
                module_name_mappers,
                module_resource_exts: _,
                module_system,
                modules_are_use_strict,
                multi_platform: _,
                multi_platform_extensions: _,
                multi_platform_extension_group_mapping: _,
                multi_platform_ambient_supports_platform_project_overrides,
                munge_underscores,
                builtin_lib,
                no_implicit_override,
                no_unchecked_indexed_access,
                node_modules_errors,
                node_main_fields,
                node_package_export_conditions,
                node_resolver_allow_root_relative,
                node_resolver_dirnames: _,
                node_resolver_root_relative_dirnames,
                normalize_intersections,
                pattern_matching,
                projects,
                projects_overlap_mapping,
                projects_path_mapping,
                projects_strict_boundary,
                react_custom_jsx_typing,
                react_managed_attributes,
                stylex_shorthand_prop,
                react_rules,
                react_runtime,
                records,
                records_includes,
                recursion_limit,
                relay_integration,
                relay_integration_esmodules,
                relay_integration_excludes,
                relay_integration_module_prefix,
                relay_integration_module_prefix_includes,
                root_name,
                saved_state_parallel_decompress,
                saved_state_fetcher,
                saved_state_persist_export_index,
                saved_state_skip_version_check,
                supported_operating_systems,
                strict_es6_import_export,
                strict_property_initialization,
                export_star_excludes_default,
                ts_syntax,
                ts_utility_syntax,
                tslib_syntax,
                typescript_library_definition_support,
                enable_custom_error,
                assert_operator,
                type_expansion_recursion_limit,
                unsuppressable_error_codes,
                use_unknown_in_catch_variables,
                vpn_less,
                wait_for_recheck,
                watchman_defer_states: _watchman_defer_states,
                watchman_sync_timeout: _watchman_sync_timeout,
                max_workers_full_check,
            },
        version: _version,
    } = flowconfig;

    let ts_syntax = ts_syntax.unwrap_or(false);
    let export_star_excludes_default = export_star_excludes_default.unwrap_or(false);
    let tslib_syntax = tslib_syntax.unwrap_or(ts_syntax);

    let all = all_override || all.unwrap_or(false);
    let autoimports = !no_autoimports_override && autoimports.unwrap_or(true);
    let autoimports_min_characters = autoimports_min_characters.unwrap_or(0) as i32;
    let autoimports_ranked_by_usage_boost_exact_match_min_length =
        autoimports_ranked_by_usage_boost_exact_match_min_length as i32;
    let automatic_require_default = automatic_require_default.unwrap_or(false);
    let babel_loose_array_spread = babel_loose_array_spread.unwrap_or(false);
    let ban_spread_key_props = ban_spread_key_props.unwrap_or(false);
    let enable_const_params = enable_const_params.unwrap_or(false);
    let enable_pattern_matching = pattern_matching.unwrap_or(true);
    let enable_records = records.unwrap_or(false);
    let estimate_recheck_time = estimate_recheck_time_override
        .or(estimate_recheck_time)
        .unwrap_or(true);
    let saved_state_restart_on_reinit = match std::env::var("FLOW_SAVED_STATE_RESTART_ON_REINIT")
        .ok()
        .as_deref()
    {
        Some("1" | "true") => true,
        Some("0" | "false") => false,
        _ => saved_state_restart_on_reinit,
    };
    let lazy_mode = matches!(
        lazy_mode_override.unwrap_or(lazy_mode.unwrap_or(LazyMode::NonLazy)),
        LazyMode::Lazy | LazyMode::WatchmanDeprecated
    );
    let max_files_checked_per_worker =
        max_files_checked_per_worker_rust_port.unwrap_or(max_files_checked_per_worker) as i32;
    let max_header_tokens = max_header_tokens as i32;
    let merge_timeout = match merge_timeout_override {
        Some(0) => None,
        Some(timeout) => Some(timeout as f64),
        None => merge_timeout.map(|t| t as f64),
    };
    let use_unknown_in_catch_variables = use_unknown_in_catch_variables.unwrap_or(false);
    let type_expansion_recursion_limit = type_expansion_recursion_limit as i32;
    let lint_severities = parse_lints_flag(lint_severities, &raw_lint_severities);

    let enable_jest_integration = jest_integration;
    let enable_relay_integration = relay_integration;
    let vpn_less = vpn_less_override.unwrap_or(vpn_less);

    let unsuppressable_error_codes: Arc<HashSet<FlowSmolStr>> = Arc::new(
        unsuppressable_error_codes
            .into_iter()
            .map(FlowSmolStr::new)
            .collect(),
    );

    let log_saving: Arc<BTreeMap<String, LogSaving>> = Arc::new(
        log_saving
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    LogSaving {
                        threshold_time_ms: v.threshold_time_ms,
                        limit: v.limit,
                        rate: v.rate,
                    },
                )
            })
            .collect(),
    );

    let builtin_lib = flowlib_builtin_lib(
        builtin_lib_override
            .as_deref()
            .and_then(config_builtin_lib_of_arg)
            .unwrap_or(builtin_lib),
    );
    let flowlib_dir = flow_flowlib::libdir(builtin_lib, &std::path::PathBuf::from(&temp_dir));
    flow_flowlib::extract(&flowlib_dir);
    let flowlib_path = flow_flowlib::path_of_libdir(&flowlib_dir).to_path_buf();

    // Initialize File_key root prefixes for relative path storage
    flow_parser::file_key::set_project_root(&root.to_string_lossy());
    flow_parser::file_key::set_flowlib_root(&flowlib_path.to_string_lossy());

    let hook_compatibility_excludes: Arc<[Regex]> = hook_compatibility_excludes
        .iter()
        .map(|s| {
            let expanded = flow_common::files::expand_project_root_token(&root, s);
            Regex::new(&expanded).unwrap()
        })
        .collect::<Vec<_>>()
        .into();

    let hook_compatibility_includes: Arc<[Regex]> = hook_compatibility_includes
        .iter()
        .map(|s| {
            let expanded = flow_common::files::expand_project_root_token(&root, s);
            Regex::new(&expanded).unwrap()
        })
        .collect::<Vec<_>>()
        .into();

    let async_component_syntax_includes: Arc<[Regex]> = async_component_syntax_includes
        .iter()
        .map(|s| {
            let expanded = flow_common::files::expand_project_root_token(&root, s);
            Regex::new(&expanded).unwrap()
        })
        .collect::<Vec<_>>()
        .into();

    let relay_integration_excludes: Arc<[Regex]> = relay_integration_excludes
        .iter()
        .map(|s| {
            let expanded = flow_common::files::expand_project_root_token(&root, s);
            Regex::new(&expanded).unwrap()
        })
        .collect::<Vec<_>>()
        .into();

    let relay_integration_module_prefix_includes: Arc<[Regex]> =
        relay_integration_module_prefix_includes
            .iter()
            .map(|s| {
                let expanded = flow_common::files::expand_project_root_token(&root, s);
                Regex::new(&expanded).unwrap()
            })
            .collect::<Vec<_>>()
            .into();

    let enabled_rollouts: Arc<BTreeMap<String, String>> = Arc::new(
        rollouts
            .into_iter()
            .map(|(rollout, config)| (rollout, config.enabled_group))
            .collect(),
    );
    let format = Format {
        bracket_spacing: format_bracket_spacing.unwrap_or(true),
        single_quotes: format_single_quotes.unwrap_or(false),
    };

    let physical_cores = physical_parallelism().get();
    // Priority: CLI --max-workers > server.max_workers.full_check (non-lazy only)
    // > server.max_workers > system default (physical cores).
    // The result is capped at physical cores.
    let config_max_workers = if !lazy_mode {
        max_workers_full_check.or(max_workers)
    } else {
        max_workers
    };
    let max_workers = max_workers_override
        .unwrap_or_else(|| {
            std::env::var("FLOW_MAX_WORKERS")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or_else(|| {
                    config_max_workers
                        .map(|w| w as usize)
                        .unwrap_or(physical_cores)
                }) as i32
        })
        .min(physical_cores as i32);

    let log_file = std::path::PathBuf::from(flow_server_files::server_files_js::log_file(
        &flowconfig_name,
        &temp_dir,
        &root,
    ));

    let node_package_export_conditions = if typescript_library_definition_support {
        // When typescript_library_definition_support is enabled, we add "types"
        // and "import" to the valid export conditions. "types" points to .d.ts
        // declaration files. "import" matches the ESM entry point, which is
        // appropriate when packages provide .d.ts or .d.mts types for their
        // ESM exports. We only add these when not already user-configured.
        let mut conditions = node_package_export_conditions;
        if !conditions.contains(&"types".to_string()) {
            conditions.insert(0, "types".to_string());
        }
        if !conditions.contains(&"import".to_string()) {
            conditions.insert(0, "import".to_string());
        }
        conditions
    } else {
        node_package_export_conditions
    };

    let projects_options = {
        use flow_common::flow_projects::*;

        let projects_vec = vec1::Vec1::try_from_vec(projects)
            .unwrap_or_else(|_| vec1::vec1![FlowSmolStr::new_inline("default")]);

        let projects_overlap_mapping_vec: BTreeMap<FlowSmolStr, Vec<FlowSmolStr>> =
            projects_overlap_mapping
                .into_iter()
                .map(|(k, v)| (k, v.into_iter().collect()))
                .collect();
        let map_path = move |path: String| {
            let expanded = flow_common::files::expand_project_root_token_as_relative(&path);
            Regex::new(&format!("^{}", expanded)).unwrap()
        };

        Arc::new(ProjectsOptions::mk(
            projects_vec,
            projects_overlap_mapping_vec,
            map_path,
            projects_path_mapping,
            projects_strict_boundary,
            multi_platform_ambient_supports_platform_project_overrides,
        ))
    };

    let saved_state_persist_export_index =
        match std::env::var("FLOW_SAVED_STATE_PERSIST_EXPORT_INDEX")
            .ok()
            .as_deref()
        {
            Some("1" | "true") => true,
            Some("0" | "false") => false,
            _ => saved_state_persist_export_index,
        };
    Options {
        all,
        assert_operator,
        autoimports,
        autoimports_min_characters,
        autoimports_ranked_by_usage,
        autoimports_ranked_by_usage_boost_exact_match_min_length,
        automatic_require_default,
        babel_loose_array_spread,
        ban_spread_key_props,
        component_syntax,
        async_component_syntax,
        async_component_syntax_includes,
        debug: debug_override,
        dev_only_refinement_info_as_errors,
        distributed: distributed_override,
        enable_const_params,
        enable_custom_error,
        enable_jest_integration,
        enable_pattern_matching,
        enable_records,
        enable_relay_integration,
        enabled_rollouts,
        enums,
        estimate_recheck_time,
        export_as_namespace_support,
        saved_state_restart_on_reinit,
        facebook_fbs: facebook_fbs.map(FlowSmolStr::new),
        facebook_fbt: facebook_fbt.map(FlowSmolStr::new),
        facebook_module_interop,
        file_options,
        flowconfig_hash: FlowSmolStr::new(flowconfig_hash),
        flowconfig_name: FlowSmolStr::new(flowconfig_name),
        format,
        haste_disallow_dynamic_import,
        haste_module_ref_prefix: haste_module_ref_prefix.map(FlowSmolStr::new),
        hook_compatibility,
        hook_compatibility_excludes,
        hook_compatibility_includes,
        ignore_non_literal_requires,
        include_suppressions: include_suppressions_override,
        include_warnings: include_warnings_override
            || max_warnings_override.is_some()
            || include_warnings,
        lazy_mode,
        log_per_error_typing_telemetry,
        lint_severities,
        log_file: Arc::new(log_file),
        log_saving,
        max_files_checked_per_worker,
        max_header_tokens,
        max_seconds_for_check_per_worker,
        max_workers,
        merge_timeout,
        missing_module_generators: Arc::from(missing_module_generators),
        module_system,
        module_name_mappers: Arc::from(module_name_mappers),
        modules_are_use_strict,
        munge_underscores: munge_underscore_members_override || munge_underscores,
        no_implicit_override,
        no_unchecked_indexed_access,
        normalize_intersections,
        node_modules_errors,
        node_main_fields: Arc::from(node_main_fields),
        node_package_export_conditions: Arc::from(node_package_export_conditions),
        node_resolver_allow_root_relative,
        node_resolver_root_relative_dirnames: Arc::from(
            node_resolver_root_relative_dirnames
                .into_iter()
                .map(|(applicable_dir_opt, dirname)| {
                    (
                        applicable_dir_opt
                            .map(|s| flow_common::files::expand_project_root_token_as_relative(&s)),
                        dirname,
                    )
                })
                .collect::<Vec<_>>(),
        ),
        profile: profile_override,
        projects_options,
        quiet: quiet_override,
        records_includes: Arc::from(
            records_includes
                .iter()
                .map(|s| flow_common::files::expand_project_root_token(&root, s))
                .collect::<Vec<_>>(),
        ),
        react_custom_jsx_typing,
        react_managed_attributes,
        react_rules: Arc::from(react_rules),
        react_runtime,
        recursion_limit,
        relay_integration_esmodules,
        relay_integration_excludes,
        relay_integration_module_prefix: relay_integration_module_prefix.map(FlowSmolStr::new),
        relay_integration_module_prefix_includes,
        root: Arc::new(root),
        root_name: root_name.map(FlowSmolStr::new),
        saved_state_parallel_decompress,
        // The CLI flag overrides the .flowconfig
        saved_state_fetcher: saved_state_fetcher_override.unwrap_or(saved_state_fetcher),
        saved_state_force_recheck,
        saved_state_no_fallback,
        saved_state_persist_export_index,
        saved_state_skip_version_check: saved_state_skip_version_check_override
            || saved_state_skip_version_check,
        saved_state_verify,
        slow_to_check_logging: slow_to_check_logging_override,
        strict_es6_import_export,
        strict_mode,
        strict_property_initialization,
        strip_root: strip_root_override,
        supported_operating_systems,
        stylex_shorthand_prop,
        temp_dir: FlowSmolStr::new(temp_dir),
        export_star_excludes_default,
        ts_syntax,
        tslib_syntax,
        typescript_library_definition_support,
        ts_utility_syntax,
        type_expansion_recursion_limit,
        unsuppressable_error_codes,
        use_unknown_in_catch_variables,
        verbose: verbose_override.map(Arc::new),
        vpn_less,
        wait_for_recheck: wait_for_recheck_override.unwrap_or(wait_for_recheck),
    }
}

fn make_env<'a>(
    flowconfig: &FlowConfig,
    flowconfig_name: &'a str,
    connect_flags: &'a ConnectParams,
    root: &'a std::path::Path,
    tmp_dir: &'a str,
) -> flow_commands_connect::command_connect::Env<'a> {
    let retries = connect_flags.retries;
    let expiry = connect_flags
        .timeout
        .map(|n| std::time::Instant::now() + std::time::Duration::from_secs(n as u64));
    let rerun_on_mismatch = match connect_flags.on_mismatch {
        OnMismatchBehavior::ChooseNewest | OnMismatchBehavior::RestartClient => true,
        OnMismatchBehavior::StopServer | OnMismatchBehavior::ErrorClient => false,
    };
    let lazy_mode: Option<&'static str> = connect_flags.lazy_mode.map(|lm| match lm {
        LazyMode::Lazy => "true",
        LazyMode::NonLazy => "false",
        LazyMode::WatchmanDeprecated => "watchman",
    });
    flow_commands_connect::command_connect::Env {
        root,
        autostart: !connect_flags.no_auto_start,
        lazy_mode,
        retries,
        expiry,
        autostop: connect_flags.autostop,
        tmp_dir,
        ignore_version: connect_flags.ignore_version,
        emoji: flowconfig.options.emoji.unwrap_or(false),
        quiet: connect_flags.quiet,
        progress: progress_mode(
            connect_flags.quiet,
            flow_event_logger::agent_invocation_id().is_some(),
            flowconfig.options.agent_progress_keepalive,
        ),
        flowconfig_name,
        rerun_on_mismatch,
    }
}

fn search_for_root(
    flowconfig_name: &str,
    start: std::path::PathBuf,
    recursion_limit: i32,
) -> Option<std::path::PathBuf> {
    let parent = start.parent().map(Path::to_path_buf);
    match parent {
        None => None,
        // Reach fs root, nothing to do.
        Some(parent) if start == parent => None,
        Some(parent) => {
            if start.join(flowconfig_name).exists() {
                Some(start)
            } else if recursion_limit <= 0 {
                None
            } else {
                search_for_root(flowconfig_name, parent, recursion_limit - 1)
            }
        }
    }
}

pub fn guess_root(flowconfig_name: &str, dir_or_file: Option<&str>) -> std::path::PathBuf {
    // Given a valid file or directory, find a valid Flow root directory
    // NOTE: exits on invalid file or .flowconfig not found!
    let dir_or_file = dir_or_file.unwrap_or(".");
    let path = Path::new(dir_or_file);
    if !path.exists() {
        let msg = format!(
            "Could not find file or directory {}; canceling search for {}.\nSee \"flow init --help\" for more info",
            dir_or_file, flowconfig_name
        );
        flow_common_exit::exit(FlowExitStatus::CouldNotFindFlowconfig, Some(&msg));
    }
    let dir = if path.is_dir() {
        dir_or_file.to_string()
    } else {
        path.parent()
            .map(|p| {
                if p.as_os_str().is_empty() {
                    ".".to_string()
                } else {
                    p.to_string_lossy().to_string()
                }
            })
            .unwrap_or_else(|| ".".to_string())
    };
    let dir_path = Path::new(&expand_path(&dir)).to_path_buf();
    match search_for_root(flowconfig_name, dir_path, 50) {
        Some(root) => {
            flow_event_logger::set_root(Some(root.to_string_lossy().to_string()));
            root
        }
        None => {
            let msg = format!(
                "Could not find a {} in {} or any of its parent directories.\nSee \"flow init --help\" for more info\n",
                flowconfig_name, dir
            );
            flow_common_exit::exit(FlowExitStatus::CouldNotFindFlowconfig, Some(&msg));
        }
    }
}

pub fn find_a_root(
    flowconfig_name: &str,
    root: Option<&str>,
    input: Option<&FileInput>,
) -> std::path::PathBuf {
    // Favor the root argument, over the input file, over the current directory
    // as the place to begin searching for the root.
    guess_root(
        flowconfig_name,
        match (root, input) {
            (Some(provided_root), _) => Some(provided_root),
            (None, Some(provided_input)) => provided_input.path_of_file_input(),
            (None, None) => None,
        },
    )
}

pub fn get_the_root(
    flowconfig_name: &str,
    root: Option<&str>,
    input: Option<&FileInput>,
) -> std::path::PathBuf {
    // If a root is given then validate it and use it. Otherwise, favor the input file
    // over the current directory as the place to begin searching for the root.
    match root {
        Some(provided_root) => {
            let root_dir = Path::new(provided_root);
            if root_dir.exists() && root_dir.is_dir() {
                let root_dir = expand_path(provided_root);
                let root_config = Path::new(&root_dir).join(flowconfig_name);
                if root_config.exists() {
                    Path::new(&root_dir).to_path_buf()
                } else {
                    let msg = format!("Failed to open {}", root_config.display());
                    flow_common_exit::exit(FlowExitStatus::CouldNotFindFlowconfig, Some(&msg));
                }
            } else {
                let msg = format!("Invalid root directory {}", provided_root);
                flow_common_exit::exit(FlowExitStatus::CouldNotFindFlowconfig, Some(&msg));
            }
        }
        None => guess_root(
            flowconfig_name,
            input.and_then(FileInput::path_of_file_input),
        ),
    }
}

pub fn convert_input_pos(line: i32, column: i32) -> (i32, i32) {
    // convert 1,1 based line/column to 1,0 for internal use
    let column = if column > 1 { column - 1 } else { 0 };
    (line, column)
}

pub fn get_path_of_file(file: &str) -> String {
    // copied (and adapted) from Hack's ClientCheck module
    if Path::new(file).exists() {
        expand_path(file)
    } else {
        // Filename.concat does not return a normalized path when the file does
        // not exist. Thus, we do it on our own...
        let cwd = std::env::current_dir()
            .expect("failed to get current directory")
            .to_string_lossy()
            .to_string();
        flow_common::files::normalize_path(&cwd, file)
    }
}

pub fn get_file_from_filename_or_stdin(
    cmd: &str,
    filename: Option<&str>,
    path: Option<&str>,
) -> FileInput {
    match filename {
        Some(filename) => {
            let file_path = Path::new(filename);
            if !file_path.exists() {
                let msg = format!(
                    "Could not find file {}; canceling.\nSee \"flow {} --help\" for more info",
                    filename, cmd
                );
                flow_common_exit::exit(FlowExitStatus::NoInput, Some(&msg));
            } else if file_path.is_dir() {
                let msg = format!(
                    "Provided argument {} is not a file; canceling.\nSee \"flow {} --help\" for more info",
                    filename, cmd
                );
                flow_common_exit::exit(FlowExitStatus::PathIsNotAFile, Some(&msg));
            } else {
                FileInput::FileName(expand_path(filename))
            }
        }
        None => {
            let mut content = String::new();
            std::io::stdin()
                .lock()
                .read_to_string(&mut content)
                .expect("failed to read stdin");
            let filename = match path {
                Some("") | None => None,
                Some(path) => Some(get_path_of_file(path)),
            };
            FileInput::FileContent(filename, content.into())
        }
    }
}

pub fn parse_location_with_optional_filename(
    spec: &Command,
    path: Option<&str>,
    args: &[String],
) -> (FileInput, i32, i32) {
    // Takes a list of strings. If there are 2 then they are both parsed as intengers
    // and stdin is read from. If there are 3 then the first is treated as a input file
    // and the following 2 are parsed as integers.
    let exit = || -> ! {
        let msg = spec.string_of_usage();
        flow_common_exit::exit(FlowExitStatus::CommandlineUsageError, Some(&msg));
    };
    let (file, line, column) = match args {
        [file, line, column] => (FileInput::FileName(expand_path(file)), line, column),
        [line, column] => (
            get_file_from_filename_or_stdin(spec.name(), None, path),
            line,
            column,
        ),
        _ => exit(),
    };
    let (line, column) = match (line.parse::<i32>(), column.parse::<i32>()) {
        (Ok(line), Ok(column)) => (line, column),
        _ => exit(),
    };
    let (line, column) = convert_input_pos(line, column);
    (file, line, column)
}

pub fn exe_name() -> String {
    std::env::args()
        .next()
        .and_then(|arg| {
            std::path::Path::new(&arg)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "flow".to_string())
}

// What should we do when we connect to the flow server monitor, but it dies before responding to
// us? Well, we should consume a retry and try to connect again, potentially even starting a new
// server
fn connect_and_make_request_inner(
    flowconfig_name: &str,
    connect_flags: &ConnectParams,
    root: &std::path::Path,
    request: &server_prot::request::Command,
    retries: i32,
) -> server_prot::response::Response {
    // Sends the command over the socket
    fn send_command(
        stream: &mut flow_common_socket::socket::SocketStream,
        cmd: &server_prot::request::Command,
    ) -> Result<(), bincode::error::EncodeError> {
        let command = flow_monitor_rpc::server_command_with_context::ServerCommandWithContext {
            client_logging_context: flow_event_logger::get_context(),
            command: cmd.clone(),
        };
        flow_parser::loc::with_full_source_serde(|| {
            bincode::serde::encode_into_std_write(
                &command,
                stream,
                bincode::config::legacy()
                    .with_limit::<{ flow_commands_connect::command_connect_simple::MAX_FRAME_BYTES }>(),
            )
        })?;
        Ok(())
    }

    fn print_status(msg: &str, enabled: bool, use_spinner: bool) {
        if !enabled {
            return;
        }

        let stderr = std::io::stderr();
        let mut stderr = stderr.lock();
        if use_spinner && stderr.is_terminal() {
            if flow_utils_tty::spinner_used() {
                flow_utils_tty::print_clear_line(&mut stderr)
                    .expect("failed to clear spinner line");
            }
            write!(stderr, "{}: {}", msg, flow_utils_tty::spinner(false))
                .expect("failed to write spinner status");
            stderr.flush().expect("failed to flush spinner status");
        } else {
            writeln!(stderr, "{}", msg).expect("failed to write spinner status");
            stderr.flush().expect("failed to flush spinner status");
        }
    }

    // Waits for a response over the socket. If the connection dies, this will throw an exception
    fn wait_for_response(
        stream: &mut flow_common_socket::socket::SocketStream,
        progress: ProgressMode,
        emoji: bool,
        _root: &std::path::Path,
    ) -> Result<server_prot::response::Response, ()> {
        let use_emoji = flow_utils_tty::supports_emoji() && emoji;
        let mut last_print: Option<std::time::Instant> = None;
        stream.set_read_timeout(None).map_err(|_| ())?;
        loop {
            let response: Result<MonitorToClientMessage, _> =
                flow_parser::loc::with_full_source_serde(|| {
                    bincode::serde::decode_from_std_read(
                        &mut *stream,
                        bincode::config::legacy()
                            .with_limit::<{ flow_commands_connect::command_connect_simple::MAX_FRAME_BYTES }>(),
                    )
                });
            let response = match response {
                Ok(r) => r,
                Err(bincode::error::DecodeError::Io { inner, .. })
                    if matches!(
                        inner.kind(),
                        std::io::ErrorKind::BrokenPipe
                            | std::io::ErrorKind::ConnectionReset
                            | std::io::ErrorKind::UnexpectedEof
                    ) =>
                {
                    if matches!(progress, ProgressMode::Live) && flow_utils_tty::spinner_used() {
                        let stderr = std::io::stderr();
                        let mut stderr = stderr.lock();
                        flow_utils_tty::print_clear_line(&mut stderr)
                            .expect("failed to clear spinner line");
                    }
                    return Err(());
                }
                Err(e) => {
                    if matches!(progress, ProgressMode::Live) && flow_utils_tty::spinner_used() {
                        let stderr = std::io::stderr();
                        let mut stderr = stderr.lock();
                        flow_utils_tty::print_clear_line(&mut stderr)
                            .expect("failed to clear spinner line");
                    }
                    panic!("Unknown exception reading from the server: {}", e)
                }
            };
            match response {
                MonitorToClientMessage::PleaseHold(server_status, watcher_status) => {
                    let status_string = {
                        if server_status::is_free(&server_status) {
                            // Let's ignore messages from the server that it is free.
                            // It's a confusing message for the user
                            if watcher_status.1 == file_watcher_status::StatusKind::Ready {
                                None
                            } else {
                                Some(file_watcher_status::string_of_status(&watcher_status))
                            }
                        } else {
                            Some(server_status::string_of_status(
                                use_emoji,
                                false,
                                &server_status,
                            ))
                        }
                    };
                    if let Some(status_string) = status_string {
                        match progress {
                            ProgressMode::Live => {
                                print_status(
                                    &format!("Please wait. {}", status_string),
                                    true,
                                    true,
                                );
                            }
                            ProgressMode::Throttled { interval } => {
                                let now = std::time::Instant::now();
                                if should_emit_keepalive(now, last_print, interval) {
                                    print_status(
                                        &format!("Please wait. {}", status_string),
                                        true,
                                        false,
                                    );
                                    last_print = Some(now);
                                }
                            }
                            ProgressMode::Silent => {}
                        }
                    }
                    continue;
                }
                MonitorToClientMessage::Data(response) => {
                    if matches!(progress, ProgressMode::Live) && flow_utils_tty::spinner_used() {
                        let stderr = std::io::stderr();
                        let mut stderr = stderr.lock();
                        flow_utils_tty::print_clear_line(&mut stderr)
                            .expect("failed to clear spinner line");
                    }
                    return Ok(response);
                }
                MonitorToClientMessage::ServerException(exn_str) => {
                    if flow_utils_tty::spinner_used() {
                        let stderr = std::io::stderr();
                        let mut stderr = stderr.lock();
                        flow_utils_tty::print_clear_line(&mut stderr)
                            .expect("failed to clear spinner line");
                    }
                    let msg = format!("Server threw an exception: {}", exn_str);
                    flow_common_exit_status::exit_with_msg(
                        flow_common_exit_status::FlowExitStatus::UnknownError,
                        &msg,
                    )
                }
            }
        }
    }

    if retries < 0 {
        flow_common_exit_status::exit_with_msg(
            flow_common_exit_status::FlowExitStatus::OutOfRetries,
            "Out of retries, exiting!",
        );
    }

    let version_mismatch_strategy = match connect_flags.on_mismatch {
        OnMismatchBehavior::ChooseNewest => {
            flow_server_env::socket_handshake::VersionMismatchStrategy::StopServerIfOlder
        }
        OnMismatchBehavior::StopServer => {
            flow_server_env::socket_handshake::VersionMismatchStrategy::AlwaysStopServer
        }
        OnMismatchBehavior::RestartClient => {
            flow_server_env::socket_handshake::VersionMismatchStrategy::ErrorClient
        }
        OnMismatchBehavior::ErrorClient => {
            flow_server_env::socket_handshake::VersionMismatchStrategy::ErrorClient
        }
    };
    let client_handshake = (
        flow_server_env::socket_handshake::ClientToMonitor1 {
            client_build_id: flow_server_env::socket_handshake::build_revision(),
            client_version: flow_common::flow_version::version().to_string(),
            is_stop_request: false,
            server_should_hangup_if_still_initializing: false,
            version_mismatch_strategy,
        },
        flow_server_env::socket_handshake::ClientToMonitor2 {
            client_type: flow_server_env::socket_handshake::ClientType::Ephemeral,
        },
    );

    let path = flow_server_files::server_files_js::config_file(flowconfig_name, root);
    // let the server enforce flowconfig warnings; we only care about the flowconfig
    // as far as it pertains to connecting to the server.
    let enforce_warnings = false;
    let flowconfig = read_config_or_exit(&path, enforce_warnings);

    // connect handles timeouts itself
    let tmp_dir_normalized = normalize_temp_dir(&connect_flags.temp_dir);
    flow_parser::file_key::set_project_root(&root.to_string_lossy());
    let flowlib_dir = flow_flowlib::libdir(
        flowlib_builtin_lib(flowconfig.options.builtin_lib),
        &tmp_dir_normalized,
    );
    flow_parser::file_key::set_flowlib_root(
        &flow_flowlib::path_of_libdir(&flowlib_dir).to_string_lossy(),
    );
    let tmp_dir_str = tmp_dir_normalized.to_string_lossy();
    let env = make_env(
        &flowconfig,
        flowconfig_name,
        connect_flags,
        root,
        &tmp_dir_str,
    );

    let (sockaddr, mut stream) =
        flow_commands_connect::command_connect::connect(&env, &client_handshake);

    let response = match send_command(&mut stream, request) {
        Ok(()) => wait_for_response(&mut stream, env.progress, env.emoji, root),
        Err(bincode::error::EncodeError::Io { inner, .. })
            if matches!(
                inner.kind(),
                std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::UnexpectedEof
            ) =>
        {
            if matches!(env.progress, ProgressMode::Live) && flow_utils_tty::spinner_used() {
                let stderr = std::io::stderr();
                let mut stderr = stderr.lock();
                flow_utils_tty::print_clear_line(&mut stderr)
                    .expect("failed to clear spinner line");
            }
            Err(())
        }
        Err(e) => {
            panic!("Error sending command to server: {}", e);
        }
    };

    match response {
        Ok(response) => response,
        Err(()) => {
            flow_commands_connect::command_connect_simple::close_connection(&sockaddr);
            print_status(
                &format!(
                    "Lost connection to the flow server ({} {} remaining)",
                    retries,
                    if retries == 1 { "retry" } else { "retries" },
                ),
                !matches!(env.progress, ProgressMode::Silent),
                matches!(env.progress, ProgressMode::Live),
            );
            connect_and_make_request_inner(
                flowconfig_name,
                connect_flags,
                root,
                request,
                retries - 1,
            )
        }
    }
}

/// Make a single ephemeral request to the daemon (starting one if needed), building the connect flags
/// from `args`. A `pub` wrapper over the `pub(crate)` connect helpers, for callers outside this crate.
pub fn connect_and_make_ephemeral_request(
    args: &arg_spec::Values,
    flowconfig_name: &str,
    root: &std::path::Path,
    request: &server_prot::request::Command,
) -> server_prot::response::Response {
    connect_and_make_request(flowconfig_name, &get_connect_flags(args), root, request)
}

pub fn connect_and_make_request(
    flowconfig_name: &str,
    connect_flags: &ConnectParams,
    root: &std::path::Path,
    request: &server_prot::request::Command,
) -> server_prot::response::Response {
    match connect_flags.timeout {
        None => connect_and_make_request_inner(
            flowconfig_name,
            connect_flags,
            root,
            request,
            connect_flags.retries,
        ),
        Some(timeout) => {
            let (tx, rx) = std::sync::mpsc::channel::<server_prot::response::Response>();
            let flowconfig_name_owned = flowconfig_name.to_string();
            let connect_flags_clone = connect_flags.clone();
            let root_owned = root.to_path_buf();
            let request_clone = request.clone();
            let initial_retries = connect_flags.retries;
            std::thread::Builder::new()
                .name("connect_and_make_request_timed".to_string())
                .spawn(move || {
                    let response = connect_and_make_request_inner(
                        &flowconfig_name_owned,
                        &connect_flags_clone,
                        &root_owned,
                        &request_clone,
                        initial_retries,
                    );
                    match tx.send(response) {
                        Ok(()) => {}
                        Err(_) => {}
                    }
                })
                .expect("failed to spawn connect_and_make_request_timed thread");
            match rx.recv_timeout(std::time::Duration::from_secs(timeout as u64)) {
                Ok(response) => response,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => flow_common_exit::exit(
                    flow_common_exit::FlowExitStatus::OutOfTime,
                    Some("Timeout exceeded, exiting"),
                ),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => flow_common_exit::exit(
                    flow_common_exit::FlowExitStatus::UnknownError,
                    Some("Inner connect thread panicked, exiting"),
                ),
            }
        }
    }
}

pub fn failwith_bad_response(
    request: &server_prot::request::Command,
    response: &server_prot::response::Response,
) -> ! {
    let msg = format!(
        "Bad response to {:?}: received {:?}",
        server_prot::request::to_string(request),
        server_prot::response::to_string(response),
    );
    flow_common_exit_status::exit_with_msg(
        flow_common_exit_status::FlowExitStatus::UnknownError,
        &msg,
    )
}

pub fn get_check_or_status_exit_code(
    errors: &flow_common_errors::error_utils::ConcreteLocPrintableErrorSet,
    warnings: &flow_common_errors::error_utils::ConcreteLocPrintableErrorSet,
    max_warnings: Option<i32>,
) -> FlowExitStatus {
    if errors.is_empty() {
        match max_warnings {
            Some(x) if warnings.cardinal() as i32 > x => FlowExitStatus::TypeError,
            None | Some(_) => FlowExitStatus::NoError,
        }
    } else {
        FlowExitStatus::TypeError
    }
}

pub fn choose_file_watcher(
    flowconfig: &FlowConfig,
    lazy_mode: Option<LazyMode>,
    file_watcher: Option<flow_config::FileWatcher>,
    file_watcher_debug: bool,
    sync_timeout: Option<u32>,
) -> FlowServerMonitorOptions::FileWatcher {
    let file_watcher = match (file_watcher, lazy_mode) {
        (None, Some(LazyMode::WatchmanDeprecated)) => flow_config::FileWatcher::Watchman,
        (Some(file_watcher), _) => file_watcher,
        (None, _) => flowconfig
            .options
            .file_watcher
            .unwrap_or(flow_config::FileWatcher::DFind),
    };
    match file_watcher {
        flow_config::FileWatcher::NoFileWatcher => FlowServerMonitorOptions::NoFileWatcher,
        flow_config::FileWatcher::DFind => FlowServerMonitorOptions::DFind,
        flow_config::FileWatcher::Watchman => {
            let sync_timeout = sync_timeout.or(flowconfig.options.watchman_sync_timeout);
            let defer_states = flowconfig.options.watchman_defer_states.clone();
            FlowServerMonitorOptions::Watchman(FlowServerMonitorOptions::WatchmanOptions {
                debug: file_watcher_debug,
                defer_states,
                sync_timeout,
            })
        }
        flow_config::FileWatcher::EdenFS => {
            let sync_timeout = sync_timeout.or(flowconfig.options.watchman_sync_timeout);
            let defer_states = flowconfig.options.watchman_defer_states.clone();
            let watchman_fallback = FlowServerMonitorOptions::WatchmanOptions {
                debug: file_watcher_debug,
                defer_states: defer_states.clone(),
                sync_timeout,
            };
            if !flow_edenfs_watcher::is_available() {
                return FlowServerMonitorOptions::Watchman(watchman_fallback);
            }
            // Watchman natively defers during hg operations via Defer_changes subscribe mode.
            // EdenFS requires explicit state names, so we hardcode the hg states here.
            let mut edenfs_defer_states =
                vec!["hg.update".to_string(), "hg.transaction".to_string()];
            edenfs_defer_states.extend(defer_states);
            FlowServerMonitorOptions::EdenFS(FlowServerMonitorOptions::EdenfsOptions {
                edenfs_debug: file_watcher_debug,
                edenfs_timeout_secs: flowconfig.options.file_watcher_edenfs_timeout,
                edenfs_throttle_time_ms: flowconfig.options.file_watcher_edenfs_throttle_time_ms,
                edenfs_defer_states,
                edenfs_max_commit_distance: match std::env::var("FLOW_EDENFS_MAX_COMMIT_DISTANCE")
                    .ok()
                {
                    Some(v) => v.parse::<u32>().unwrap_or(0),
                    None => flowconfig.options.file_watcher_edenfs_max_commit_distance,
                },
                edenfs_force_subprocess_mergebase: flowconfig
                    .options
                    .file_watcher_edenfs_subprocess_mergebase,
                edenfs_watchman_fallback: watchman_fallback,
            })
        }
    }
}

pub fn choose_file_watcher_mergebase_with(
    flowconfig: &FlowConfig,
    vcs: Option<Vcs>,
    mergebase_with: Option<&str>,
) -> String {
    match mergebase_with {
        Some(x) => x.to_string(),
        None => {
            let mergebase_with = match vcs {
                Some(Vcs::Git) => flowconfig.options.file_watcher_mergebase_with_git.clone(),
                Some(Vcs::Hg) => flowconfig.options.file_watcher_mergebase_with_hg.clone(),
                None => None,
            };
            let mergebase_with =
                mergebase_with.or_else(|| flowconfig.options.file_watcher_mergebase_with.clone());
            match mergebase_with {
                Some(x) => x,
                None => DEFAULT_FILE_WATCHER_MERGEBASE_WITH.to_string(),
            }
        }
    }
}

pub fn choose_file_watcher_timeout(
    flowconfig: &FlowConfig,
    cli_timeout: Option<u32>,
) -> Option<f64> {
    match cli_timeout.or(flowconfig.options.file_watcher_timeout) {
        Some(0) => None,
        Some(x) => Some(f64::from(x)),
        None => Some(f64::from(DEFAULT_FILE_WATCHER_TIMEOUT)),
    }
}

#[derive(Clone, Debug)]
pub struct CodemodParams {
    pub options_flags: OptionsFlags,
    pub saved_state_options_flags: SavedStateFlags,
    pub ignore_version: bool,
    pub write: bool,
    pub repeat: bool,
    pub log_level: Option<flow_hh_logger::Level>,
    pub root: Option<String>,
    pub input_file: Option<String>,
    pub base_flag: BaseFlags,
    pub anon: Option<Vec<String>>,
}

pub fn get_codemod_flags(args: &arg_spec::Values) -> CodemodParams {
    let options_flags = get_options_flags(args);
    let saved_state_options_flags = get_saved_state_flags(args);
    let ignore_version =
        flow_command_spec::get(args, "--ignore-version", &arg_spec::truthy()).unwrap();
    let write = flow_command_spec::get(args, "--write", &arg_spec::truthy()).unwrap();
    let repeat = flow_command_spec::get(args, "--repeat", &arg_spec::truthy()).unwrap();
    let log_level =
        flow_command_spec::get(args, "--log-level", &arg_spec::optional(log_level_flag())).unwrap();
    let root =
        flow_command_spec::get(args, "--root", &arg_spec::optional(arg_spec::string())).unwrap();
    let input_file = flow_command_spec::get(
        args,
        "--input-file",
        &arg_spec::optional(arg_spec::string()),
    )
    .unwrap();
    let base_flag = get_base_flags(args);
    let anon =
        flow_command_spec::get(args, "FILE...", &arg_spec::list_of(arg_spec::string())).unwrap();
    let anon = anon.filter(|v| !v.is_empty());

    if !write && repeat {
        let msg = "Error: cannot run codemod with --repeat flag unless --write is also passed";
        flow_common_exit::exit(FlowExitStatus::CommandlineUsageError, Some(msg));
    }

    CodemodParams {
        options_flags,
        saved_state_options_flags,
        ignore_version,
        write,
        repeat,
        log_level,
        root,
        input_file,
        base_flag,
        anon,
    }
}

fn log_level_flag() -> arg_spec::FlagType<Option<flow_hh_logger::Level>> {
    arg_spec::enum_flag(vec![
        ("off", flow_hh_logger::Level::Off),
        ("fatal", flow_hh_logger::Level::Fatal),
        ("error", flow_hh_logger::Level::Error),
        ("warn", flow_hh_logger::Level::Warn),
        ("info", flow_hh_logger::Level::Info),
        ("debug", flow_hh_logger::Level::Debug),
    ])
}

pub fn add_codemod_flags(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    let spec = add_options_flags(spec);
    let spec = add_saved_state_flags(spec);
    let spec = add_from_flag(spec);
    let spec = add_ignore_version_flag(spec);
    let spec = spec
        .flag("--write", &arg_spec::truthy(), "Edit files in place", None)
        .flag(
            "--repeat",
            &arg_spec::truthy(),
            "Run this codemod repeatedly until no more files change",
            None,
        )
        .flag(
            "--log-level",
            &arg_spec::optional(log_level_flag()),
            "Verbosity of logging (off|fatal|error|warn|info|debug)",
            Some("FLOW_LOG_LEVEL"),
        );
    let spec = add_root_flag(spec);
    let spec = add_input_file_flag(
        spec,
        "File containing a list of paths to transform. Incompatible with stdin and FILE...",
    );
    let spec = add_base_flags(spec);
    spec.anon("FILE...", &arg_spec::list_of(arg_spec::string()))
}

pub fn subcommand_spec<T: Clone + Send + Sync + 'static>(
    name: &str,
    doc: &str,
    visibility: flow_command_spec::Visibility,
    cmd_list: Vec<(&'static str, T, String)>,
) -> flow_command_spec::Spec {
    let mut command_info = cmd_list
        .iter()
        .filter(|(cmd, _, subdoc)| !cmd.is_empty() && !subdoc.is_empty())
        .map(|(cmd, _, subdoc)| (cmd.to_string(), subdoc.clone()))
        .collect::<Vec<_>>();
    command_info.sort_by(|(a, _), (b, _)| a.cmp(b));
    let command_info = flow_command_spec::format_two_columns(None, None, 1, &command_info);
    flow_command_spec::Spec::new(
        name,
        doc,
        visibility,
        format!(
            "Usage: {} {} SUBCOMMAND [OPTIONS]...\n\nValid values for SUBCOMMAND:\n{}\n",
            exe_name(),
            name,
            command_info,
        ),
    )
    .anon(
        "subcommand",
        &arg_spec::required(
            None,
            arg_spec::command_flag(
                cmd_list
                    .into_iter()
                    .map(|(cmd, subcommand, _)| (cmd, subcommand))
                    .collect(),
            ),
        ),
    )
}

#[cfg(test)]
mod tests {
    use flow_config::opts::BuiltinLib as ConfigBuiltinLib;

    use super::builtin_lib_arg;
    use super::config_builtin_lib_of_arg;

    #[test]
    fn builtin_lib_uses_canonical_names() {
        assert_eq!(
            config_builtin_lib_of_arg("default"),
            Some(ConfigBuiltinLib::FlowlibWithLibDomDts)
        );
        assert_eq!(
            config_builtin_lib_of_arg("core-only"),
            Some(ConfigBuiltinLib::Flowlib)
        );
        assert_eq!(
            builtin_lib_arg(ConfigBuiltinLib::FlowlibWithLibDomDts),
            "default"
        );
        assert_eq!(builtin_lib_arg(ConfigBuiltinLib::Flowlib), "core-only");
    }
}
