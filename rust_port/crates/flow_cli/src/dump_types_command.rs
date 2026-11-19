/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use flow_command_spec::arg_spec;
use flow_common::dump_types_document::DocumentOptions;
use flow_common_errors::error_utils;
use flow_server_env::server_prot;

// ***********************************************************************
// flow dump-types command
// ***********************************************************************

fn spec() -> flow_command_spec::Spec {
    let exe_name = flow_command_utils::exe_name();
    // Outputs list of all types in the file
    let spec = flow_command_spec::Spec::new(
        "dump-types",
        "Outputs types for every expression in the file",
        flow_command_spec::Visibility::Internal,
        format!(
            "Usage: {exe_name} dump-types [OPTION]... [FILE]\n\ne.g. {exe_name} dump-types foo.js\nor   {exe_name} dump-types < foo.js\n"
        ),
    );
    let spec = flow_command_utils::add_base_flags(spec);
    let spec = flow_command_utils::add_connect_and_json_flags(spec);
    let spec = flow_command_utils::add_root_flag(spec);
    let spec = flow_command_utils::add_strip_root_flag(spec);
    let spec = flow_command_utils::add_from_flag(spec);
    let spec = flow_command_utils::add_path_flag(spec);
    let spec = flow_command_utils::add_wait_for_recheck_flag(spec);
    spec.flag(
        "--evaluate-type-destructors",
        &arg_spec::truthy(),
        "Use the result of type destructor evaluation if available",
        None,
    )
    .flag(
        "--for-tool",
        &arg_spec::optional_value_with_default(10, arg_spec::int()),
        "Print each type as JSON for programs to read: the type as Flow's checker represents it, cut off below DEPTH levels of nesting (default 10)",
        None,
    )
    .flag(
        "--dedup",
        &arg_spec::optional_value_with_default(256, arg_spec::int()),
        "Print each repeated type larger than N bytes (default 256) once, in a shared $defs table, and refer to it elsewhere. Types stay strings of JSON unless --json-direct is given. Requires --for-tool and --json",
        None,
    )
    .flag(
        "--json-direct",
        &arg_spec::truthy(),
        "Print each type as a JSON value rather than as a string of JSON, in an object whose \"schema\" field says so. Requires --for-tool and --json",
        None,
    )
    .flag(
        "--force-schema",
        &arg_spec::truthy(),
        "Always print an object whose \"schema\" field lists every option that shapes the output, even where the output would otherwise be a plain array. Readers that expect the plain array cannot read it. Requires --for-tool and --json",
        None,
    )
    .anon("file", &arg_spec::optional(arg_spec::string()))
}

fn types_to_json(
    file_content: Option<&String>,
    strip_root: Option<&str>,
    document: DocumentOptions,
    types: &[(flow_parser::loc::Loc, String)],
) -> Result<serde_json::Value, serde_json::Error> {
    let offset_table =
        file_content.map(|s| flow_parser::offset_utils::OffsetTable::make(s.as_str()));
    let entries = types
        .iter()
        .map(|(loc, t)| {
            let mut json = serde_json::Map::new();
            json.insert("type".to_string(), document.type_value(t)?);
            json.insert("reasons".to_string(), serde_json::Value::Array(vec![]));
            json.insert(
                "loc".to_string(),
                flow_common::reason::json_of_loc(strip_root, false, offset_table.as_ref(), loc),
            );
            json.extend(error_utils::deprecated_json_props_of_loc(strip_root, loc));
            Ok(serde_json::Value::Object(json))
        })
        .collect::<Result<_, _>>()?;
    Ok(serde_json::Value::Array(entries))
}

fn handle_response(
    types: Vec<(flow_parser::loc::Loc, String)>,
    defs: Option<String>,
    json: bool,
    document: DocumentOptions,
    file_content: Option<&String>,
    pretty: bool,
    strip_root: Option<&str>,
) {
    if json {
        // The server produced both the types and `$defs`, so a parse failure is unreachable in
        // practice; report it rather than emit a wrong shape.
        let types_json = match types_to_json(file_content, strip_root, document, &types) {
            Ok(types_json) => types_json,
            Err(err) => {
                eprintln!("dump-types: could not parse a type as JSON: {err}");
                return;
            }
        };
        let defs_json = match defs.map(|defs| serde_json::from_str(&defs)).transpose() {
            Ok(defs_json) => defs_json,
            Err(err) => {
                eprintln!("dump-types: could not re-parse $defs: {err}");
                return;
            }
        };
        flow_hh_json::print_json_endline(pretty, &document.document(types_json, defs_json));
    } else {
        let out = types
            .into_iter()
            .map(|(loc, ty)| {
                format!(
                    "{}: {}",
                    flow_common::reason::string_of_loc(strip_root, &loc),
                    ty
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        println!("{}", out);
    }
}

fn handle_error(
    err: String,
    file_content: Option<&String>,
    json: bool,
    document: DocumentOptions,
    pretty: bool,
    strip_root: Option<&str>,
    dedup_threshold: Option<usize>,
) {
    if json {
        flow_hh_json::prerr_json_endline(
            pretty,
            &serde_json::json!({
                "error": err,
            }),
        );
        // Also output an empty payload on stdout, for JSON parsers. It has to keep the shape the
        // caller asked for, so a `--dedup` consumer is not handed a bare array on the error path.
        let empty_defs = dedup_threshold.map(|_| "{}".to_owned());
        handle_response(
            vec![],
            empty_defs,
            json,
            document,
            file_content,
            pretty,
            strip_root,
        );
    } else {
        eprintln!("{}", err);
    }
}

/// Which of `dump-types`' modifiers an invocation passed, and what they modify.
struct Modifiers {
    for_tool: bool,
    json: bool,
    dedup: bool,
    document: DocumentOptions,
    path: bool,
    file: bool,
}

/// Exits with a usage error naming the first modifier that would be ignored.
fn reject_inapplicable_modifiers(m: &Modifiers) {
    // Each shapes the `--for-tool --json` payload, and only that.
    let for_tool_json_modifiers = [
        ("--dedup", m.dedup),
        ("--json-direct", m.document.json_direct),
        ("--force-schema", m.document.force_schema),
    ];
    if !(m.for_tool && m.json)
        && let Some((flag, _)) = for_tool_json_modifiers.iter().find(|(_, used)| *used)
    {
        let missing = [("--for-tool", m.for_tool), ("--json", m.json)]
            .into_iter()
            .filter(|(_, present)| !present)
            .map(|(flag, _)| flag)
            .collect::<Vec<_>>();
        usage_error(&format!("{flag} requires {}", missing.join(" and ")));
    }
    if m.path && m.file {
        usage_error("--path applies only to a file read from stdin, not to a FILE argument");
    }
}

fn usage_error(msg: &str) -> ! {
    flow_common_exit_status::exit_with_msg(
        flow_common_exit_status::FlowExitStatus::CommandlineUsageError,
        &format!("dump-types: {msg}"),
    )
}

fn main(args: &arg_spec::Values) {
    let base_flags = flow_command_utils::get_base_flags(args);
    let flowconfig_name = base_flags.flowconfig_name;
    let connect_flags = flow_command_utils::get_connect_flags(args);
    let json_flags = flow_command_utils::get_json_flags(args);
    let root_arg =
        flow_command_spec::get(args, "--root", &arg_spec::optional(arg_spec::string())).unwrap();
    let strip_root = flow_command_spec::get(args, "--strip-root", &arg_spec::truthy()).unwrap();
    let path =
        flow_command_spec::get(args, "--path", &arg_spec::optional(arg_spec::string())).unwrap();
    let wait_for_recheck = flow_command_spec::get(
        args,
        "--wait-for-recheck",
        &arg_spec::optional(arg_spec::bool_flag()),
    )
    .unwrap();
    let evaluate_type_destructors =
        flow_command_spec::get(args, "--evaluate-type-destructors", &arg_spec::truthy()).unwrap();
    let for_tool = flow_command_spec::get(
        args,
        "--for-tool",
        &arg_spec::optional_value_with_default(10, arg_spec::int()),
    )
    .unwrap();
    // Opt-in: without it the output shape is exactly what it has always been.
    let dedup_threshold: Option<usize> = flow_command_spec::get(
        args,
        "--dedup",
        &arg_spec::optional_value_with_default(256, arg_spec::int()),
    )
    .unwrap()
    .map(|n: i32| n.max(0) as usize);
    let filename =
        flow_command_spec::get(args, "file", &arg_spec::optional(arg_spec::string())).unwrap();

    let document = DocumentOptions {
        json_direct: flow_command_spec::get(args, "--json-direct", &arg_spec::truthy()).unwrap(),
        force_schema: flow_command_spec::get(args, "--force-schema", &arg_spec::truthy()).unwrap(),
    };

    let json = json_flags.json || json_flags.pretty;

    reject_inapplicable_modifiers(&Modifiers {
        for_tool: for_tool.is_some(),
        json,
        dedup: dedup_threshold.is_some(),
        document,
        path: path.is_some(),
        file: filename.is_some(),
    });
    let file = flow_command_utils::get_file_from_filename_or_stdin(
        "dump-types",
        filename.as_deref(),
        path.as_deref(),
    );
    let file_content = file.content_of_file_input().ok();
    let root = flow_command_utils::guess_root(
        &flowconfig_name,
        match root_arg.as_deref() {
            Some(root) => Some(root),
            None => file.path_of_file_input(),
        },
    );
    let strip_root = if strip_root {
        Some(root.to_string_lossy().to_string())
    } else {
        None
    };
    let request = server_prot::request::Command::DUMP_TYPES {
        input: file,
        evaluate_type_destructors,
        for_tool,
        dedup_threshold,
        wait_for_recheck,
    };
    let response = flow_command_utils::connect_and_make_request(
        &flowconfig_name,
        &connect_flags,
        &root,
        &request,
    );
    match response {
        server_prot::response::Response::DUMP_TYPES(Err(err)) => {
            handle_error(
                err,
                file_content.as_ref(),
                json,
                document,
                json_flags.pretty,
                strip_root.as_deref(),
                dedup_threshold,
            );
        }
        server_prot::response::Response::DUMP_TYPES(Ok((response, defs))) => {
            handle_response(
                response,
                defs,
                json,
                document,
                file_content.as_ref(),
                json_flags.pretty,
                strip_root.as_deref(),
            );
        }
        response => {
            flow_command_utils::failwith_bad_response(&request, &response);
        }
    }
}

pub(crate) fn command() -> flow_command_spec::Command {
    flow_command_spec::command(spec(), main)
}
