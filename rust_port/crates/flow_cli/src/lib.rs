/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![feature(deref_patterns)]
#![feature(never_type)]

use std::path::Path;
use std::sync::Arc;
use std::sync::OnceLock;

use flow_common::options::Options;
use flow_config::LazyMode;
use flow_server_monitor::StartArgs;

mod agent_helper_command;
mod apply_code_action_command;
mod ast_command;
mod autocomplete_command;
mod autofix_command;
mod batch_coverage_command;
mod check_contents_command;
mod codemod_command;
mod config_command;
mod coverage_command;
mod cycle_command;
mod dev_tools_command;
mod dump_types_command;
mod env_builder_debug_command;
mod extra_commands;
pub use extra_commands::register_docs_url;
pub use extra_commands::register_extra_commands;
mod find_module_command;
mod force_recheck_command;
mod foreground_check_commands;
mod get_def_command;
mod glean_command;
mod glean_runner;
mod glean_schema;
mod graph_command;
mod init_command;
mod inlay_hint_command;
mod ls_command;
mod lsp_command;
mod offset_cache;
mod query_command;
mod save_state_command;
mod server_command;
mod shell_complete_command;
mod start_command;
mod status_command;
mod stop_command;
mod type_at_pos_command;
mod type_of_name_command;
mod version_command;

pub(crate) fn get_options_with_root_and_flowconfig_name(
    ignore_version: bool,
    root: &Path,
    flowconfig_name: &str,
    lazy_mode: Option<LazyMode>,
    options_flags: flow_command_utils::OptionsFlags,
    saved_state_options_flags: flow_command_utils::SavedStateFlags,
) -> Arc<Options> {
    let flowconfig_path = root.join(flowconfig_name);
    let flowconfig_path = flowconfig_path.to_string_lossy().to_string();
    let (flowconfig, flowconfig_hash) =
        flow_command_utils::read_config_and_hash_or_exit(&flowconfig_path, !ignore_version);
    Arc::new(flow_command_utils::make_options(
        flowconfig_name.to_string(),
        flowconfig_hash,
        flowconfig,
        lazy_mode,
        root.to_path_buf(),
        options_flags,
        saved_state_options_flags,
    ))
}

fn parse_lazy_mode(s: &str) -> Option<LazyMode> {
    match s {
        "fs" => Some(LazyMode::Lazy),
        "none" => Some(LazyMode::NonLazy),
        "watchman" => Some(LazyMode::WatchmanDeprecated),
        "true" => Some(LazyMode::Lazy),
        _ => None,
    }
}

fn build_monitor_options_and_start_args(
    args: &flow_server_monitor::DaemonizeArgs,
) -> (Arc<Options>, StartArgs) {
    let lazy_mode = args.lazy_mode.as_deref().and_then(parse_lazy_mode);
    let options_flags = flow_command_utils::OptionsFlags {
        all: args.all,
        debug: args.debug,
        flowconfig_flags: flow_command_utils::FlowconfigFlags {
            ignores: args.flowconfig_ignores.clone(),
            includes: args.flowconfig_includes.clone(),
            libs: args.flowconfig_libs.clone(),
            raw_lint_severities: args.flowconfig_raw_lint_severities.clone(),
            untyped: args.flowconfig_untyped.clone(),
            declarations: args.flowconfig_declarations.clone(),
        },
        include_warnings: args.include_warnings,
        max_warnings: args.max_warnings,
        max_workers: args.max_workers,
        merge_timeout: args.merge_timeout,
        munge_underscore_members: args.munge_underscore_members,
        builtin_lib: Some(args.builtin_lib.clone()),
        profile: args.profile,
        quiet: args.quiet,
        slow_to_check_logging: args.slow_to_check_logging,
        strip_root: args.strip_root,
        temp_dir: Some(args.temp_dir.clone()),
        verbose: args.verbose.clone(),
        wait_for_recheck: args.wait_for_recheck,
        vpn_less: args.vpn_less,
        include_suppressions: args.include_suppressions,
        estimate_recheck_time: args.estimate_recheck_time,
        distributed: args.distributed,
        no_autoimports: args.no_autoimports,
    };
    let saved_state_options_flags = flow_command_utils::SavedStateFlags {
        saved_state_fetcher: args.saved_state_fetcher,
        saved_state_force_recheck: args.saved_state_force_recheck,
        saved_state_no_fallback: args.saved_state_no_fallback,
        saved_state_skip_version_check: args.saved_state_skip_version_check,
        saved_state_verify: args.saved_state_verify,
    };

    let options = get_options_with_root_and_flowconfig_name(
        args.ignore_version,
        &args.root,
        &args.flowconfig_name,
        lazy_mode,
        options_flags,
        saved_state_options_flags,
    );

    let start_args = StartArgs {
        flowconfig_name: args.flowconfig_name.clone(),
        server_log_file: args.server_log_file.clone(),
        monitor_log_file: args.monitor_log_file.clone(),
        lazy_mode: args.lazy_mode.clone(),
        builtin_lib: args.builtin_lib.clone(),
        ignore_version: args.ignore_version,
        wait_for_recheck: args.wait_for_recheck,
        file_watcher: args.file_watcher.clone(),
        file_watcher_debug: args.file_watcher_debug,
        file_watcher_timeout: args.file_watcher_timeout,
        file_watcher_mergebase_with: args.file_watcher_mergebase_with.clone(),
        file_watcher_sync_timeout: args.file_watcher_sync_timeout,
        from: args.from.clone(),
        autostop: args.autostop,
        no_restart: args.no_restart,
        cli_overrides: flow_common::cli_overrides::CliOverrides {
            max_warnings: args.max_warnings,
            no_autoimports: args.no_autoimports,
            flowconfig_ignores: args.flowconfig_ignores.clone(),
            flowconfig_includes: args.flowconfig_includes.clone(),
            flowconfig_libs: args.flowconfig_libs.clone(),
            flowconfig_raw_lint_severities: args.flowconfig_raw_lint_severities.clone(),
            flowconfig_untyped: args.flowconfig_untyped.clone(),
            flowconfig_declarations: args.flowconfig_declarations.clone(),
        },
    };

    (options, start_args)
}

fn build_server_daemon_options(
    args: &flow_server::server_daemon::ServerDaemonArgs,
) -> Arc<Options> {
    let lazy_mode = args.lazy_mode.as_deref().and_then(parse_lazy_mode);
    let options_flags = flow_command_utils::OptionsFlags {
        all: args.all,
        debug: args.debug,
        flowconfig_flags: flow_command_utils::FlowconfigFlags {
            ignores: args.cli_overrides.flowconfig_ignores.clone(),
            includes: args.cli_overrides.flowconfig_includes.clone(),
            libs: args.cli_overrides.flowconfig_libs.clone(),
            raw_lint_severities: args.cli_overrides.flowconfig_raw_lint_severities.clone(),
            untyped: args.cli_overrides.flowconfig_untyped.clone(),
            declarations: args.cli_overrides.flowconfig_declarations.clone(),
        },
        include_warnings: args.include_warnings,
        max_warnings: args.cli_overrides.max_warnings,
        max_workers: args.max_workers,
        merge_timeout: args.merge_timeout,
        munge_underscore_members: args.munge_underscore_members,
        builtin_lib: Some(args.builtin_lib.clone()),
        profile: args.profile,
        quiet: args.quiet,
        slow_to_check_logging: args.slow_to_check_logging,
        strip_root: args.strip_root,
        temp_dir: Some(args.temp_dir.clone()),
        verbose: args.verbose.clone(),
        wait_for_recheck: args.wait_for_recheck,
        vpn_less: args.vpn_less,
        include_suppressions: args.include_suppressions,
        estimate_recheck_time: args.estimate_recheck_time,
        distributed: args.distributed,
        no_autoimports: args.cli_overrides.no_autoimports,
    };
    let saved_state_options_flags = flow_command_utils::SavedStateFlags {
        saved_state_fetcher: args.saved_state_fetcher,
        saved_state_force_recheck: args.saved_state_force_recheck,
        saved_state_no_fallback: args.saved_state_no_fallback,
        saved_state_skip_version_check: args.saved_state_skip_version_check,
        saved_state_verify: args.saved_state_verify,
    };

    get_options_with_root_and_flowconfig_name(
        args.ignore_version,
        &args.root,
        &args.flowconfig_name,
        lazy_mode,
        options_flags,
        saved_state_options_flags,
    )
}

fn register_daemon_entry_points() {
    static REGISTERED: OnceLock<()> = OnceLock::new();
    REGISTERED.get_or_init(|| {
        flow_dfind::dfind_server::register();
        flow_server_monitor::flow_server_monitor_daemon::register(
            build_monitor_options_and_start_args,
        );
        flow_server::server_daemon::register(build_server_daemon_options);
        flow_event_logger::register_scuba_entry();
    });
}

pub(crate) fn check_entry_point() {
    register_daemon_entry_points();
    flow_daemon::check_entry_point();
}

fn explicit_commands() -> Vec<flow_command_spec::Command> {
    vec![
        agent_helper_command::command(),
        apply_code_action_command::command(),
        ast_command::command(),
        autocomplete_command::command(),
        autofix_command::command(),
        foreground_check_commands::full_check_command(),
        foreground_check_commands::focus_check_command(),
        check_contents_command::command(),
        codemod_command::command(),
        config_command::command(),
        coverage_command::command(),
        batch_coverage_command::command(),
        cycle_command::command(),
        dev_tools_command::command(),
        query_command::command(),
        env_builder_debug_command::command(),
        graph_command::command(),
        dump_types_command::command(),
        find_module_command::command(),
        force_recheck_command::command(),
        get_def_command::command(),
        glean_command::command(),
        init_command::command(),
        inlay_hint_command::command(),
        lsp_command::command(),
        ls_command::command(),
        save_state_command::command(),
        server_command::command(),
        start_command::command(),
        stop_command::command(),
        type_at_pos_command::command(),
        type_of_name_command::command(),
        version_command::command(),
    ]
}

fn registered_commands() -> Vec<flow_command_spec::Command> {
    let explicit = explicit_commands();
    let extra = extra_commands::extra_commands();
    let mut commands = Vec::with_capacity(explicit.len() + extra.len());
    commands.extend(explicit);
    commands.extend(extra);
    commands
}

fn all_commands() -> Vec<flow_command_spec::Command> {
    let registered = registered_commands();
    let mut commands = Vec::with_capacity(registered.len() + 4);
    commands.push(shell_complete_command::command());
    commands.push(status_command::default_command(default_command_info()));
    commands.push(status_command::check_command());
    commands.push(status_command::status_command());
    commands.extend(registered);
    commands
}

fn find_command(subcmd: &str) -> Option<flow_command_spec::Command> {
    all_commands()
        .into_iter()
        .find(|command| command.name() == subcmd)
}

fn startup_max_workers(arguments: &[String]) -> Option<std::num::NonZeroUsize> {
    let (subcmd, rest) = arguments.split_first()?;
    let subcmd = subcmd.to_ascii_lowercase();
    if subcmd != "start" && subcmd != "server" {
        return None;
    }
    let mut rest = rest.iter();
    while let Some(arg) = rest.next() {
        if let Some(value) = arg.strip_prefix("--max-workers=") {
            return value
                .parse::<usize>()
                .ok()
                .and_then(std::num::NonZeroUsize::new);
        }
        if arg == "--max-workers" {
            return rest
                .next()
                .and_then(|value| value.parse::<usize>().ok())
                .and_then(std::num::NonZeroUsize::new);
        }
    }
    None
}

fn flow_shell_main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();

    let max_workers = startup_max_workers(&arguments).or_else(|| {
        std::env::var("FLOW_MAX_WORKERS")
            .ok()
            .and_then(|workers_str| workers_str.parse::<std::num::NonZeroUsize>().ok())
    });
    if let Some(count) = max_workers {
        flow_utils_concurrency::thread_pool::init_thread_pool(
            flow_utils_concurrency::thread_pool::ThreadCount::NumThreads(count),
        );
    }

    let default_command = status_command::default_command(default_command_info());
    let (command, argv) = match arguments.split_first() {
        None => (default_command, vec![]),
        Some((subcmd, rest)) => {
            let subcmd = subcmd.to_ascii_lowercase();
            match find_command(&subcmd) {
                Some(command) => (command, rest.to_vec()),
                None => (default_command, arguments),
            }
        }
    };
    let command_string = command.name().to_string();
    flow_event_logger::set_command(Some(command_string));
    let init_id = flow_common_utils::random_id::short_string();
    flow_event_logger::init_flow_command(&init_id);
    flow_command_utils::run_command(&command, &argv);
}

pub fn main() {
    // Log an EXIT telemetry event on every exit below; the logger's at-exit
    // hook delivers it.
    flow_common_exit_status::set_exit_hook(flow_event_logger::log_exit);

    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }

    if flow_common_utils::utils_js::in_flow_test() {
        flow_logging_utils::disable_logging();
    }

    check_entry_point();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(flow_shell_main));
    match result {
        Ok(()) => {
            flow_common_exit_status::exit(flow_common_exit_status::FlowExitStatus::NoError);
        }
        Err(err) => {
            let err_msg = if let Some(s) = err.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = err.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "unknown panic".to_string()
            };
            if err_msg.contains("Out_of_shared_memory") || err_msg.contains("Out of shared memory")
            {
                flow_common_exit_status::exit_with_msg(
                    flow_common_exit_status::FlowExitStatus::OutOfSharedMemory,
                    "Out of shared memory",
                );
            } else {
                flow_common_exit_status::exit_with_msg(
                    flow_common_exit_status::FlowExitStatus::UnknownError,
                    &format!("Unhandled exception: {}", err_msg),
                );
            }
        }
    }
}

fn default_command_info() -> Vec<status_command::CommandInfo> {
    let mut commands = Vec::with_capacity(registered_commands().len() + 2);
    commands.push(status_command::check_command());
    commands.push(status_command::status_command());
    commands.extend(registered_commands());
    commands
        .into_iter()
        .filter_map(|command| {
            let name = command.name().to_string();
            let doc = command.doc().to_string();
            let visibility = command.visibility();
            if name.is_empty() {
                None
            } else {
                Some(status_command::CommandInfo {
                    name,
                    doc,
                    visibility,
                })
            }
        })
        .collect()
}
