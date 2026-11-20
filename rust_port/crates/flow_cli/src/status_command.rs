/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeSet;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use flow_command_spec::arg_spec;
use flow_command_utils::ConnectParams;
use flow_command_utils::OffsetStyle;
use flow_common_errors::error_utils::ConcreteLocPrintableErrorSet;
use flow_common_errors::error_utils::PrintableError;
use flow_common_errors::error_utils::cli_output;
use flow_common_errors::error_utils::json_output;
use flow_parser::loc::Loc;
use flow_server_env::server_prot;

struct StatusArgs {
    root: PathBuf,
    output_json: bool,
    output_json_version: Option<json_output::JsonVersion>,
    offset_style: Option<OffsetStyle>,
    pretty: bool,
    error_flags: cli_output::ErrorFlags,
    strip_root: bool,
    show_lazy_status: bool,
}

fn spec(
    name: &str,
    doc: &str,
    visibility: flow_command_spec::Visibility,
    usage: String,
    explicit: bool,
) -> flow_command_spec::Spec {
    let spec = flow_command_spec::Spec::new(name, doc, visibility, usage);
    let spec = flow_command_utils::add_base_flags(spec);
    let spec = flow_command_utils::add_connect_and_json_flags(spec);
    let spec = flow_command_utils::add_json_version_flag(spec);
    let spec = flow_command_utils::add_offset_style_flag(spec);
    let spec = flow_command_utils::add_error_flags(spec);
    let spec = flow_command_utils::add_strip_root_flag(spec);
    let spec = flow_command_utils::add_from_flag(spec);
    let spec = spec.flag(
        "--show-lazy-status",
        &arg_spec::truthy(),
        "Show whether the server is in lazy mode",
        None,
    );
    let spec = if explicit {
        spec
    } else {
        spec.flag(
            "--version",
            &arg_spec::truthy(),
            "Print version number and exit",
            None,
        )
        .flag(
            "--help-all",
            &arg_spec::truthy(),
            "Show all commands including internal and experimental ones",
            None,
        )
    };
    spec.anon("root", &arg_spec::optional(arg_spec::string()))
}

fn request_status(
    flowconfig_name: &str,
    root: &Path,
    connect_flags: &ConnectParams,
    include_warnings: bool,
) -> (
    server_prot::response::StatusResponse,
    server_prot::response::LazyStats,
) {
    let request = server_prot::request::Command::STATUS { include_warnings };
    match flow_command_utils::connect_and_make_request(
        flowconfig_name,
        connect_flags,
        root,
        &request,
    ) {
        server_prot::response::Response::STATUS {
            status_response,
            lazy_stats,
        } => (status_response, lazy_stats),
        response => flow_command_utils::failwith_bad_response(&request, &response),
    }
}

pub(crate) fn status_json(flowconfig_name: &str, root: &Path) -> io::Result<serde_json::Value> {
    let (response, _) = request_status(flowconfig_name, root, &ConnectParams::default(), true);
    match response {
        server_prot::response::StatusResponse::ERRORS {
            errors,
            warnings,
            suppressed_errors,
        } => Ok(json_output::full_status_json_of_errors(
            None,
            &suppressed_errors,
            json_output::JsonVersion::JsonV1,
            &None,
            flow_parser::offset_utils::OffsetKind::Utf8,
            &errors,
            &warnings,
        )(vec![])),
        server_prot::response::StatusResponse::NO_ERRORS => {
            Ok(serde_json::json!({"passed": true, "errors": []}))
        }
        server_prot::response::StatusResponse::NOT_COVERED => {
            Err(io::Error::other("Flow server did not cover this root"))
        }
    }
}

fn check_status(flowconfig_name: &str, args: &StatusArgs, connect_flags: &ConnectParams) {
    let (response, lazy_stats) = request_status(
        flowconfig_name,
        &args.root,
        connect_flags,
        args.error_flags.include_warnings,
    );
    let strip_root = if args.strip_root {
        Some(args.root.clone())
    } else {
        None
    };
    let offset_kind = flow_command_utils::offset_kind_of_offset_style(args.offset_style);
    let print_json =
        |errors: &ConcreteLocPrintableErrorSet,
         warnings: &ConcreteLocPrintableErrorSet,
         suppressed_errors: &[(PrintableError<Loc>, BTreeSet<Loc>)]| {
            let strip_root = strip_root
                .as_deref()
                .map(|root| root.to_string_lossy().into_owned());
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            flow_common_errors::error_utils::json_output::print_errors_with_offset_kind(
                &mut out,
                strip_root.as_deref(),
                suppressed_errors,
                args.pretty,
                args.output_json_version
                    .unwrap_or(json_output::JsonVersion::JsonV1),
                &None,
                offset_kind,
                errors,
                warnings,
            )
            .expect("failed to write json errors");
            out.flush().expect("failed to flush json errors");
        };
    let lazy_msg = args.show_lazy_status.then(|| {
        if lazy_stats.lazy_mode {
            format!(
                "The Flow server is currently in lazy mode and is only checking {}/{} files.\nTo learn more, visit flow.org/en/docs/lang/lazy-modes",
                lazy_stats.checked_files, lazy_stats.total_files
            )
        } else {
            "The Flow server is not in lazy mode.".to_string()
        }
    });
    match response {
        server_prot::response::StatusResponse::ERRORS {
            errors,
            warnings,
            suppressed_errors,
        } => {
            let error_flags = &args.error_flags;
            let from = flow_event_logger::get_from_i_am_a_clown();
            if args.output_json {
                print_json(&errors, &warnings, &suppressed_errors)
            } else if matches!(from.as_deref(), Some("vim") | Some("emacs")) {
                let strip_root = strip_root
                    .as_deref()
                    .map(|root| root.to_string_lossy().into_owned());
                let stdout = std::io::stdout();
                let mut out = stdout.lock();
                flow_common_errors::error_utils::vim_emacs_output::print_errors(
                    strip_root.as_deref(),
                    &mut out,
                    &errors,
                    &warnings,
                )
                .expect("failed to write vim/emacs errors");
                out.flush().expect("failed to flush vim/emacs errors");
            } else {
                let mut cli_errors = errors.clone();
                for (error, _) in &suppressed_errors {
                    cli_errors.add(error.clone());
                }
                let stdout = std::io::stdout();
                let mut out = stdout.lock();
                flow_common_errors::error_utils::cli_output::print_errors(
                    &mut out,
                    error_flags,
                    &None,
                    strip_root.as_deref(),
                    &cli_errors,
                    &warnings,
                    lazy_msg.as_deref(),
                )
                .expect("failed to write cli errors");
                out.flush().expect("failed to flush cli errors");
            }
            flow_common_exit_status::exit(flow_command_utils::get_check_or_status_exit_code(
                &errors,
                &warnings,
                error_flags.max_warnings,
            ))
        }
        server_prot::response::StatusResponse::NO_ERRORS => {
            if args.output_json {
                print_json(
                    &ConcreteLocPrintableErrorSet::empty(),
                    &ConcreteLocPrintableErrorSet::empty(),
                    &[],
                )
            } else {
                println!("No errors!");
                if let Some(msg) = &lazy_msg {
                    println!("\n{}", msg);
                }
            }
            flow_common_exit_status::exit(flow_common_exit_status::FlowExitStatus::NoError)
        }
        server_prot::response::StatusResponse::NOT_COVERED => {
            let msg = "Why on earth did the server respond with NOT_COVERED?";
            eprintln!("{}", msg);
            flow_common_exit_status::exit(flow_common_exit_status::FlowExitStatus::UnknownError)
        }
    }
}

fn main(args: &arg_spec::Values, command_info: &[CommandInfo]) {
    let version = flow_command_spec::get(args, "--version", &arg_spec::truthy()).unwrap_or(false);
    if version {
        flow_command_utils::print_version();
        flow_common_exit_status::exit(flow_common_exit_status::FlowExitStatus::NoError);
    }

    let help_all_flag =
        flow_command_spec::get(args, "--help-all", &arg_spec::truthy()).unwrap_or(false);
    if help_all_flag {
        let by_visibility = |visibility| {
            let mut result: Vec<_> = command_info
                .iter()
                .filter(|command| command.visibility == visibility)
                .map(|command| (command.name.clone(), command.doc.clone()))
                .collect();
            result.sort_by(|(a, _), (b, _)| a.cmp(b));
            result
        };
        let public = by_visibility(flow_command_spec::Visibility::Public);
        let experimental = by_visibility(flow_command_spec::Visibility::Experimental);
        let internal = by_visibility(flow_command_spec::Visibility::Internal);
        let col_width = 1 + public
            .iter()
            .chain(experimental.iter())
            .chain(internal.iter())
            .map(|(name, _)| name.len())
            .max()
            .unwrap_or(0);
        let fmt = |commands: &[(String, String)]| {
            flow_command_spec::format_two_columns(None, Some(col_width), 1, commands)
        };
        let mut help_all = format!(
            "Documentation: {}\n\nUsage: {} [COMMAND] \n\nValid values for COMMAND:\n{}\n",
            crate::extra_commands::docs_url(),
            flow_command_utils::exe_name(),
            fmt(&public),
        );
        if !experimental.is_empty() {
            help_all.push_str(&format!(
                "\nExperimental commands (may change or be removed):\n{}\n",
                fmt(&experimental),
            ));
        }
        if !internal.is_empty() {
            help_all.push_str(&format!(
                "\nInternal commands (not part of the public API):\n{}\n",
                fmt(&internal),
            ));
        }
        print!("{}", help_all);
        flow_common_exit_status::exit(flow_common_exit_status::FlowExitStatus::NoError);
    }

    let base_flags = flow_command_utils::get_base_flags(args);
    let flowconfig_name = base_flags.flowconfig_name;

    let root_arg =
        flow_command_spec::get(args, "root", &arg_spec::optional(arg_spec::string())).unwrap();
    let root = flow_command_utils::guess_root(&flowconfig_name, root_arg.as_deref());

    let (connect_flags, json, pretty) = flow_command_utils::get_connect_and_json_flags(args);
    let json_version = flow_command_utils::get_json_version(args);
    let offset_style = flow_command_utils::get_offset_style(args);
    let json = json || json_version.is_some() || pretty;

    let error_flags_args = flow_command_utils::get_error_flags_args(args);
    let error_flags = flow_command_utils::collect_error_flags(
        error_flags_args.color,
        error_flags_args.include_warnings,
        error_flags_args.max_warnings,
        error_flags_args.one_line,
        error_flags_args.list_files,
        error_flags_args.show_all_errors,
        error_flags_args.show_all_branches,
        error_flags_args.unicode,
        error_flags_args.message_width,
    );
    let strip_root = flow_command_spec::get(args, "--strip-root", &arg_spec::truthy()).unwrap();
    let show_lazy_status =
        flow_command_spec::get(args, "--show-lazy-status", &arg_spec::truthy()).unwrap();

    let status_args = StatusArgs {
        root,
        output_json: json,
        output_json_version: json_version,
        offset_style,
        pretty,
        error_flags,
        strip_root,
        show_lazy_status,
    };

    check_status(&flowconfig_name, &status_args, &connect_flags)
}

pub(crate) struct CommandInfo {
    pub(crate) name: String,
    pub(crate) doc: String,
    pub(crate) visibility: flow_command_spec::Visibility,
}

pub(crate) fn status_command() -> flow_command_spec::Command {
    flow_command_spec::command(
        spec(
            "status",
            "(default) Shows current Flow errors by asking the Flow server",
            flow_command_spec::Visibility::Public,
            format!(
                "Usage: {} status [OPTION]... [ROOT]\nShows current Flow errors by asking the Flow server.\n\nFlow will search upward for a .flowconfig file, beginning at ROOT.\nROOT is assumed to be the current directory if unspecified.\nA server will be started if none is running over ROOT.\n\nStatus command options:",
                flow_command_utils::exe_name()
            ),
            true,
        ),
        |args| main(args, &[]),
    )
}

pub(crate) fn check_command() -> flow_command_spec::Command {
    flow_command_spec::command(
        spec(
            "check",
            "(default) Shows current Flow errors by asking the Flow server",
            flow_command_spec::Visibility::Public,
            format!(
                "Usage: {} check [OPTION]... [ROOT]\nShows current Flow errors by asking the Flow server.\n\nFlow will search upward for a .flowconfig file, beginning at ROOT.\nROOT is assumed to be the current directory if unspecified.\nA server will be started if none is running over ROOT.\n\nCheck command options:",
                flow_command_utils::exe_name()
            ),
            true,
        ),
        |args| main(args, &[]),
    )
}

pub(crate) fn default_command(command_info: Vec<CommandInfo>) -> flow_command_spec::Command {
    let mut public_command_info = command_info
        .iter()
        .filter(|command| command.visibility == flow_command_spec::Visibility::Public)
        .map(|command| (command.name.clone(), command.doc.clone()))
        .collect::<Vec<_>>();
    public_command_info.sort_by(|(a, _), (b, _)| a.cmp(b));
    let cmd_usage = flow_command_spec::format_two_columns(None, None, 1, &public_command_info);
    flow_command_spec::command(
        spec(
            "default",
            "",
            flow_command_spec::Visibility::Internal,
            format!(
                "Documentation: {}\n\nUsage: {} [COMMAND] \n\nValid values for COMMAND:\n{}\n\nDefault values if unspecified:\n  COMMAND\tstatus\n\nStatus command options:",
                crate::extra_commands::docs_url(),
                flow_command_utils::exe_name(),
                cmd_usage
            ),
            false,
        ),
        move |args| main(args, &command_info),
    )
}
