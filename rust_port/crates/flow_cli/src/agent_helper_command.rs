/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use flow_command_spec::arg_spec;
use flow_parser::loc::LOC_NONE;
use flow_parser::loc::Loc;
use flow_server_env::server_prot;

#[derive(Clone)]
enum AgentHelperSubcommand {
    InspectTypes,
    UnpackDocs,
}

const INSPECT_TYPES_DOC: &str = "Shows the hover type at each `// ^?` comment in a file";
const UNPACK_DOCS_DOC: &str = "Writes the Flow docs bundled into this binary to a directory";

fn subcommand_flag() -> arg_spec::FlagType<Option<(AgentHelperSubcommand, Vec<String>)>> {
    arg_spec::command_flag(vec![
        ("inspect-types", AgentHelperSubcommand::InspectTypes),
        ("unpack-docs", AgentHelperSubcommand::UnpackDocs),
    ])
}

fn add_reason_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--reason",
        &arg_spec::required(None, arg_spec::string()),
        "Why you are running this command (required, used by logging)",
        None,
    )
}

fn set_reason_from_flag(args: &arg_spec::Values) {
    let reason = flow_command_spec::get(
        args,
        "--reason",
        &arg_spec::required(None, arg_spec::string()),
    )
    .unwrap_or_else(|err| {
        flow_command_utils::die_bad_args(&flow_command_spec::error_message(&err))
    });
    if reason.trim().is_empty() {
        flow_command_utils::die_bad_args("--reason must not be empty");
    }
    flow_event_logger::set_reason(Some(reason));
}

fn inspect_types_spec() -> flow_command_spec::Spec {
    let exe_name = flow_command_utils::exe_name();
    let spec = flow_command_spec::Spec::new(
        "inspect-types",
        INSPECT_TYPES_DOC,
        flow_command_spec::Visibility::Public,
        format!(
            "Usage: {exe_name} agent-helper inspect-types --reason REASON [OPTION]... [FILE]\n\nEach `// ^?` comment queries the column its `^` points at on the line above.\n\ne.g. {exe_name} agent-helper inspect-types --reason \"why is x any\" foo.js\nor   {exe_name} agent-helper inspect-types --reason \"why is x any\" < foo.js\n"
        ),
    );
    let spec = add_reason_flag(spec);
    let spec = flow_command_utils::add_base_flags(spec);
    let spec = flow_command_utils::add_connect_and_json_flags(spec);
    let spec = flow_command_utils::add_root_flag(spec);
    let spec = flow_command_utils::add_strip_root_flag(spec);
    let spec = flow_command_utils::add_from_flag(spec);
    let spec = flow_command_utils::add_path_flag(spec);
    let spec = flow_command_utils::add_wait_for_recheck_flag(spec);
    spec.flag(
        "--show-refinements",
        &arg_spec::truthy(),
        "Also show where each queried value was refined",
        None,
    )
    .flag(
        "--show-invalidations",
        &arg_spec::truthy(),
        "Also show what invalidated each queried value's refinements",
        None,
    )
    .anon("file", &arg_spec::optional(arg_spec::string()))
}

// A query that matched nothing has no type loc, so report where the `^` pointed.
fn string_of_query_loc(strip_root: Option<&str>, cursor_loc: &Loc, type_loc: &Loc) -> String {
    if *type_loc != LOC_NONE {
        return flow_common::reason::string_of_loc(strip_root, type_loc);
    }
    let source = cursor_loc
        .source
        .as_ref()
        .map(|source| flow_common::reason::string_of_source(strip_root, source))
        .unwrap_or_default();
    format!(
        "{}:{}:{}",
        source,
        cursor_loc.start.line,
        cursor_loc.start.column + 1
    )
}

fn print_inspect_types_item(
    strip_root: Option<&str>,
    show_refinements: bool,
    show_invalidations: bool,
    item: &server_prot::response::inspect_types::Item,
) {
    let server_prot::response::inspect_types::Item {
        cursor_loc,
        type_loc,
        type_str,
        refining_locs,
        refinement_invalidated,
    } = item;
    println!("{}", string_of_query_loc(strip_root, cursor_loc, type_loc));
    println!("{}", type_str.as_deref().unwrap_or("(unknown)"));
    if show_refinements && !refining_locs.is_empty() {
        let refining_locs: Vec<_> = refining_locs
            .iter()
            .map(|loc| flow_common::reason::string_of_loc(strip_root, loc))
            .collect();
        println!("Refined at {}", refining_locs.join(", "));
    }
    if show_invalidations && !refinement_invalidated.is_empty() {
        let invalidations: Vec<_> = refinement_invalidated
            .iter()
            .map(|(loc, reason)| {
                format!(
                    "{} at {}",
                    flow_common::refinement_invalidation::string_of_reason(*reason),
                    flow_common::reason::string_of_loc(strip_root, loc)
                )
            })
            .collect();
        println!("Refinement invalidated due to {}", invalidations.join(", "));
    }
}

fn json_of_inspect_types_item(
    strip_root: Option<&str>,
    show_refinements: bool,
    show_invalidations: bool,
    item: &server_prot::response::inspect_types::Item,
) -> serde_json::Value {
    let server_prot::response::inspect_types::Item {
        cursor_loc,
        type_loc,
        type_str,
        refining_locs,
        refinement_invalidated,
    } = item;
    let json_of_loc = |loc: &Loc| flow_common::reason::json_of_loc(strip_root, false, None, loc);
    let mut props = serde_json::Map::new();
    props.insert(
        "cursor".to_string(),
        serde_json::json!({
            "line": cursor_loc.start.line,
            "column": cursor_loc.start.column + 1,
        }),
    );
    props.insert(
        "loc".to_string(),
        if *type_loc == LOC_NONE {
            serde_json::Value::Null
        } else {
            json_of_loc(type_loc)
        },
    );
    props.insert("type".to_string(), serde_json::json!(type_str));
    if show_refinements {
        props.insert(
            "refinements".to_string(),
            serde_json::Value::Array(refining_locs.iter().map(json_of_loc).collect()),
        );
    }
    if show_invalidations {
        props.insert(
            "invalidations".to_string(),
            serde_json::Value::Array(
                refinement_invalidated
                    .iter()
                    .map(|(loc, reason)| {
                        serde_json::json!({
                            "loc": json_of_loc(loc),
                            "reason": flow_common::refinement_invalidation::string_of_reason(*reason),
                        })
                    })
                    .collect(),
            ),
        );
    }
    serde_json::Value::Object(props)
}

fn run_inspect_types(args: &arg_spec::Values) {
    set_reason_from_flag(args);
    let base_flags = flow_command_utils::get_base_flags(args);
    let (connect_flags, json, pretty) = flow_command_utils::get_connect_and_json_flags(args);
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
    let show_refinements =
        flow_command_spec::get(args, "--show-refinements", &arg_spec::truthy()).unwrap();
    let show_invalidations =
        flow_command_spec::get(args, "--show-invalidations", &arg_spec::truthy()).unwrap();
    let filename =
        flow_command_spec::get(args, "file", &arg_spec::optional(arg_spec::string())).unwrap();

    let file = flow_command_utils::get_file_from_filename_or_stdin(
        "agent-helper inspect-types",
        filename.as_deref(),
        path.as_deref(),
    );
    let flowconfig_name = base_flags.flowconfig_name;
    let root = flow_command_utils::find_a_root(&flowconfig_name, root_arg.as_deref(), Some(&file));
    let request =
        server_prot::request::Command::INSPECT_TYPES(server_prot::inspect_types_options::T {
            input: file,
            wait_for_recheck,
        });
    let response = flow_command_utils::connect_and_make_request(
        &flowconfig_name,
        &connect_flags,
        &root,
        &request,
    );
    match response {
        server_prot::response::Response::INSPECT_TYPES(Err(err)) => {
            if json || pretty {
                flow_hh_json::prerr_json_endline(pretty, &serde_json::json!({ "error": err }));
            } else {
                eprintln!("{}", err);
            }
        }
        server_prot::response::Response::INSPECT_TYPES(Ok(items)) => {
            let strip_root = if strip_root {
                Some(root.to_string_lossy().to_string())
            } else {
                None
            };
            if json || pretty {
                let items = items
                    .iter()
                    .map(|item| {
                        json_of_inspect_types_item(
                            strip_root.as_deref(),
                            show_refinements,
                            show_invalidations,
                            item,
                        )
                    })
                    .collect();
                flow_hh_json::print_json_endline(pretty, &serde_json::Value::Array(items));
                return;
            }
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    println!();
                }
                print_inspect_types_item(
                    strip_root.as_deref(),
                    show_refinements,
                    show_invalidations,
                    item,
                );
            }
        }
        response => {
            flow_command_utils::failwith_bad_response(&request, &response);
        }
    }
}

fn unpack_docs_spec() -> flow_command_spec::Spec {
    let spec = flow_command_spec::Spec::new(
        "unpack-docs",
        UNPACK_DOCS_DOC,
        flow_command_spec::Visibility::Public,
        format!(
            "Usage: {} agent-helper unpack-docs --reason REASON [OPTION]... DIR\n\nCreates DIR if needed and overwrites any docs already in it.\n",
            flow_command_utils::exe_name()
        ),
    );
    let spec = add_reason_flag(spec);
    let spec = flow_command_utils::add_from_flag(spec);
    spec.anon("DIR", &arg_spec::string())
}

fn run_unpack_docs(args: &arg_spec::Values) {
    set_reason_from_flag(args);
    let Some(dir) = flow_command_spec::get(args, "DIR", &arg_spec::string()).unwrap() else {
        flow_command_spec::usage(&unpack_docs_spec());
        flow_common_exit_status::exit(
            flow_common_exit_status::FlowExitStatus::CommandlineUsageError,
        );
    };
    if let Err(err) = std::fs::create_dir_all(&dir).and_then(|()| flow_docs::DOCS.extract(&dir)) {
        flow_common_exit_status::exit_with_msg(
            flow_common_exit_status::FlowExitStatus::UnknownError,
            &format!("Could not unpack the docs into {dir}: {err}"),
        );
    }
}

fn root_spec() -> flow_command_spec::Spec {
    flow_command_spec::Spec::new(
        "agent-helper",
        "Commands for coding agents working with Flow",
        flow_command_spec::Visibility::Public,
        format!(
            "Usage: {} agent-helper SUBCOMMAND [OPTION]...\n\nValid values for SUBCOMMAND:\n{}",
            flow_command_utils::exe_name(),
            flow_command_spec::format_two_columns(
                None,
                None,
                1,
                &[
                    ("inspect-types".to_string(), INSPECT_TYPES_DOC.to_string()),
                    ("unpack-docs".to_string(), UNPACK_DOCS_DOC.to_string()),
                ],
            ),
        ),
    )
    .anon("subcommand", &subcommand_flag())
}

pub(crate) fn command() -> flow_command_spec::Command {
    flow_command_spec::command(root_spec(), |args| {
        let (subcommand, argv) =
            match flow_command_spec::get(args, "subcommand", &subcommand_flag()) {
                Ok(Some(subcommand)) => subcommand,
                Ok(None) => {
                    flow_command_spec::usage(&root_spec());
                    flow_common_exit_status::exit(
                        flow_common_exit_status::FlowExitStatus::CommandlineUsageError,
                    );
                }
                Err(err) => {
                    flow_command_utils::die_bad_args(&flow_command_spec::error_message(&err))
                }
            };
        let command = match subcommand {
            AgentHelperSubcommand::InspectTypes => {
                flow_command_spec::command(inspect_types_spec(), run_inspect_types)
            }
            AgentHelperSubcommand::UnpackDocs => {
                flow_command_spec::command(unpack_docs_spec(), run_unpack_docs)
            }
        };
        flow_command_utils::run_command(&command, &argv);
    })
}
