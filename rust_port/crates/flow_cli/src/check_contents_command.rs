/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;

use flow_command_spec::arg_spec;
use flow_common_errors::error_utils::ConcreteLocPrintableErrorSet;
use flow_common_errors::error_utils::PrintableError;
use flow_parser::loc::Loc;
use flow_server_env::server_prot;
use flow_server_utils::file_input::FileInput;

//***********************************************************************
// flow check-contents command
//***********************************************************************

fn spec() -> flow_command_spec::Spec {
    let exe_name = flow_command_utils::exe_name();
    let spec = flow_command_spec::Spec::new(
        "check-contents",
        "Run typechecker on contents from stdin",
        flow_command_spec::Visibility::Public,
        format!(
            "Usage: {exe_name} check-contents [OPTION]... [FILE]\n\nRuns a flow check on the contents of stdin. If FILE is provided, then\ncheck-contents pretends that the contents of stdin come from FILE\n\ne.g. {exe_name} check-contents < foo.js\nor   {exe_name} check-contents foo.js < foo.js\n"
        ),
    );
    let spec = flow_command_utils::add_base_flags(spec);
    let spec = flow_command_utils::add_connect_and_json_flags(spec);
    let spec = flow_command_utils::add_json_version_flag(spec);
    let spec = flow_command_utils::add_offset_style_flag(spec);
    let spec = flow_command_utils::add_root_flag(spec);
    let spec = flow_command_utils::add_error_flags(spec);
    let spec = flow_command_utils::add_strip_root_flag(spec);
    let spec = flow_command_utils::add_verbose_flags(spec);
    let spec = flow_command_utils::add_from_flag(spec);
    let spec = flow_command_utils::add_wait_for_recheck_flag(spec);
    spec.flag(
        "--all",
        &arg_spec::truthy(),
        "Ignore absence of an @flow pragma",
        None,
    )
    .anon("filename", &arg_spec::optional(arg_spec::string()))
}

fn main(args: &arg_spec::Values) {
    let base_flags = flow_command_utils::get_base_flags(args);
    let (option_values, json, pretty) = flow_command_utils::get_connect_and_json_flags(args);
    let json_version = flow_command_utils::get_json_version(args);
    let offset_style = flow_command_utils::get_offset_style(args);
    let root =
        flow_command_spec::get(args, "--root", &arg_spec::optional(arg_spec::string())).unwrap();
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
    let verbose = flow_command_utils::verbose_flags(args);
    let wait_for_recheck = flow_command_spec::get(
        args,
        "--wait-for-recheck",
        &arg_spec::optional(arg_spec::bool_flag()),
    )
    .unwrap();
    let all = flow_command_spec::get(args, "--all", &arg_spec::truthy()).unwrap();
    let file =
        flow_command_spec::get(args, "filename", &arg_spec::optional(arg_spec::string())).unwrap();

    let file = flow_command_utils::get_file_from_filename_or_stdin(
        spec().name.as_str(),
        None,
        file.as_deref(),
    );
    let flowconfig_name = base_flags.flowconfig_name;
    let root = flow_command_utils::guess_root(
        &flowconfig_name,
        match root.as_deref() {
            Some(root) => Some(root),
            None => file.path_of_file_input(),
        },
    );
    // pretty implies json
    let json = json || json_version.is_some() || pretty;
    let offset_kind = flow_command_utils::offset_kind_of_offset_style(offset_style);
    if !option_values.quiet && verbose.is_some() {
        eprintln!("NOTE: --verbose writes to the server log file");
    }

    let include_warnings = error_flags.include_warnings;
    let request = server_prot::request::Command::CHECK_FILE {
        input: file.clone(),
        verbose,
        force: all,
        include_warnings,
        wait_for_recheck,
    };
    let server_response = flow_command_utils::connect_and_make_request(
        &flowconfig_name,
        &option_values,
        &root,
        &request,
    );
    let response = match server_response {
        server_prot::response::Response::CHECK_FILE(response) => response,
        response => flow_command_utils::failwith_bad_response(&request, &response),
    };
    let stdin_file = match &file {
        FileInput::FileContent(None, contents) => Some((PathBuf::from("-"), contents.to_string())),
        FileInput::FileContent(Some(path), contents) => {
            Some((PathBuf::from(path), contents.to_string()))
        }
        _ => None,
    };
    let strip_root = if strip_root { Some(root) } else { None };
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
                pretty,
                json_version
                    .unwrap_or(flow_common_errors::error_utils::json_output::JsonVersion::JsonV1),
                &stdin_file,
                offset_kind,
                errors,
                warnings,
            )
            .expect("failed to write json errors");
            out.flush().expect("failed to flush json errors");
        };
    match response {
        server_prot::response::StatusResponse::ERRORS {
            errors,
            warnings,
            suppressed_errors,
        } => {
            if json {
                print_json(&errors, &warnings, &suppressed_errors)
            } else {
                let stdout = std::io::stdout();
                let mut out = stdout.lock();
                flow_common_errors::error_utils::cli_output::print_errors(
                    &mut out,
                    &error_flags,
                    &stdin_file,
                    strip_root.as_deref(),
                    &errors,
                    &warnings,
                    None,
                )
                .expect("failed to write cli errors");
                out.flush().expect("failed to flush cli errors");
                // Return a successful exit code if there were only warnings.
                flow_common_exit_status::exit(flow_command_utils::get_check_or_status_exit_code(
                    &errors,
                    &warnings,
                    error_flags.max_warnings,
                ))
            }
        }
        server_prot::response::StatusResponse::NO_ERRORS => {
            if json {
                print_json(
                    &ConcreteLocPrintableErrorSet::empty(),
                    &ConcreteLocPrintableErrorSet::empty(),
                    &[],
                )
            } else {
                let stdout = std::io::stdout();
                let mut out = stdout.lock();
                writeln!(out, "No errors!").expect("failed to write success output");
                out.flush().expect("failed to flush success output");
            }
            flow_common_exit_status::exit(flow_common_exit_status::FlowExitStatus::NoError)
        }
        server_prot::response::StatusResponse::NOT_COVERED => {
            if json {
                print_json(
                    &ConcreteLocPrintableErrorSet::empty(),
                    &ConcreteLocPrintableErrorSet::empty(),
                    &[],
                )
            } else {
                let stdout = std::io::stdout();
                let mut out = stdout.lock();
                writeln!(out, "File is not @flow!").expect("failed to write not-covered output");
                out.flush().expect("failed to flush not-covered output");
            }
            flow_common_exit_status::exit(flow_common_exit_status::FlowExitStatus::NoError)
        }
    }
}

pub(crate) fn command() -> flow_command_spec::Command {
    flow_command_spec::command(spec(), main)
}
