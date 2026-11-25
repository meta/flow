/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::path::Path;

use flow_command_spec::arg_spec;
use flow_parser::loc_sig::LocSig;
use flow_server_env::server_prot;
use flow_server_utils::file_input::FileInput;
use flow_services_export::export_index;

fn spec() -> flow_command_spec::Spec {
    let exe_name = flow_command_utils::exe_name();
    let spec = flow_command_spec::Spec::new(
        "type-of-name-experimental",
        "Shows the type of a variable or export by name",
        flow_command_spec::Visibility::Experimental,
        format!(
            "Usage: {} type-of-name-experimental [OPTION]... FILE NAME [NAME...]\n\ne.g. {} type-of-name foo.js myVariable\ne.g. {} type-of-name foo.js UserCard DataDisplay ProductItem\ne.g. {} type-of-name foo.js UserCard.name UserCard.email\n",
            exe_name, exe_name, exe_name, exe_name,
        ),
    );
    let spec = flow_command_utils::add_base_flags(spec);
    let spec = flow_command_utils::add_connect_flags(spec);
    let spec = flow_command_utils::add_root_flag(spec);
    let spec = flow_command_utils::add_strip_root_flag(spec);
    let spec = flow_command_utils::add_verbose_flags(spec);
    let spec = flow_command_utils::add_from_flag(spec);
    let spec = flow_command_utils::add_path_flag(spec);
    let spec = flow_command_utils::add_wait_for_recheck_flag(spec);
    spec.flag(
        "--expand-component-props",
        &arg_spec::truthy(),
        "[DEPRECATED: now default] Expand rest props of components",
        None,
    )
    .flag(
        "--unexpand-component-props",
        &arg_spec::truthy(),
        "Show spread types without expanding (e.g., ...UserCardProps)",
        None,
    )
    .flag(
        "--hide-references",
        &arg_spec::truthy(),
        "Hide definition information of names within types",
        None,
    )
    .flag(
        "--exact-match-only",
        &arg_spec::truthy(),
        "Only show results that match the name exactly",
        None,
    )
    .anon("args", &arg_spec::list_of(arg_spec::string()))
}

fn handle_response(
    strip_root: Option<&str>,
    hide_references: bool,
    query_name: &str,
    response: server_prot::response::infer_type_of_name::T,
) {
    let server_prot::response::infer_type_of_name::T {
        loc,
        actual_name,
        type_,
        refs,
        documentation,
        prop_docs,
        source,
    } = response;

    let match_exactness = if actual_name == query_name {
        format!("exact match '{}'", actual_name)
    } else {
        format!(
            "approximate match '{}' (instead of '{}')",
            actual_name, query_name
        )
    };
    let range = match source {
        export_index::Source::Global => {
            " defined as a library definition (no need to import)".to_string()
        }
        export_index::Source::Builtin(specifier) => {
            format!("defined in module `{}`", specifier.display())
        }
        export_index::Source::FileKey(file) => {
            if loc == flow_parser::loc::Loc::none() {
                format!(" defined at {}", file.to_absolute())
            } else {
                format!(
                    " defined at {}",
                    flow_common::reason::range_string_of_loc(strip_root, &loc)
                )
            }
        }
    };
    let str_of_loc = |loc: &flow_parser::loc::Loc| {
        loc.source
            .as_ref()
            .map(|_| flow_common::reason::string_of_loc(strip_root, loc))
    };
    let type_str = if type_.contains('\n') {
        format!("\n```\n{}\n```", type_)
    } else {
        format!(" `{}` ", type_)
    };
    let refs = if hide_references {
        String::new()
    } else {
        match refs {
            None => String::new(),
            Some(refs) => {
                let refs = refs
                    .iter()
                    .filter_map(|(name, loc, summary)| {
                        str_of_loc(loc).map(|loc| {
                            let summary = summary
                                .as_ref()
                                .map(|summary| format!(" {}", summary))
                                .unwrap_or_default();
                            format!("'{}' is defined at {}{}", name, loc, summary)
                        })
                    })
                    .collect::<Vec<_>>();
                match refs.as_slice() {
                    [] => String::new(),
                    _ => format!("\nwhere\n{}\n", refs.join("\n")),
                }
            }
        }
    };
    let doc = documentation
        .as_ref()
        .map(|documentation| format!("\nand documentation:\n{}\n", documentation))
        .unwrap_or_default();
    let prop_docs_str = match prop_docs {
        None => String::new(),
        Some(docs) => {
            let lines = docs
                .iter()
                .map(|doc| format!("  {} - {}", doc.prop_name, doc.description))
                .collect::<Vec<_>>();
            match lines.as_slice() {
                [] => String::new(),
                _ => format!("\nProp documentation:\n{}\n", lines.join("\n")),
            }
        }
    };
    println!(
        "Found {}{} with type{}{}{}{}",
        match_exactness, range, type_str, refs, doc, prop_docs_str
    );
}

fn main(args: &arg_spec::Values) {
    let base_flags = flow_command_utils::get_base_flags(args);
    let connect_flags = flow_command_utils::get_connect_flags(args);
    let root_arg =
        flow_command_spec::get(args, "--root", &arg_spec::optional(arg_spec::string())).unwrap();
    let strip_root = flow_command_spec::get(args, "--strip-root", &arg_spec::truthy()).unwrap();
    let verbose = flow_command_utils::verbose_flags(args);
    let path =
        flow_command_spec::get(args, "--path", &arg_spec::optional(arg_spec::string())).unwrap();
    let wait_for_recheck = flow_command_spec::get(
        args,
        "--wait-for-recheck",
        &arg_spec::optional(arg_spec::bool_flag()),
    )
    .unwrap();
    let _expand_component_props_deprecated =
        flow_command_spec::get(args, "--expand-component-props", &arg_spec::truthy()).unwrap();
    let unexpanded =
        flow_command_spec::get(args, "--unexpand-component-props", &arg_spec::truthy()).unwrap();
    let hide_references =
        flow_command_spec::get(args, "--hide-references", &arg_spec::truthy()).unwrap();
    let exact_match_only =
        flow_command_spec::get(args, "--exact-match-only", &arg_spec::truthy()).unwrap();
    let raw_args = flow_command_spec::get(args, "args", &arg_spec::list_of(arg_spec::string()))
        .unwrap()
        .unwrap_or_default();

    let (file, names) = match raw_args.as_slice() {
        [file, names @ ..] if !names.is_empty() => (file.clone(), names.to_vec()),
        _ => {
            eprintln!(
                "{}",
                flow_command_spec::command(spec(), |_| {}).string_of_usage()
            );
            eprintln!("Expected FILE followed by one or more NAMEs");
            flow_common_exit_status::exit(
                flow_common_exit_status::FlowExitStatus::CommandlineUsageError,
            );
        }
    };
    let file_input = if Path::new(&file).exists() {
        flow_command_utils::get_file_from_filename_or_stdin(
            "type-of-name-experimental",
            Some(&file),
            path.as_deref(),
        )
    } else {
        // A FILE that does not exist yet is checked as an empty @flow file.
        let path = flow_command_utils::get_path_of_file(&file);
        FileInput::FileContent(Some(path), "// @flow\n".into())
    };
    let root_dir = file_input
        .path_of_file_input()
        .and_then(|path| Path::new(path).ancestors().find(|dir| dir.exists()))
        .map(|dir| dir.to_string_lossy().into_owned());
    let root = flow_command_utils::find_a_root(
        &base_flags.flowconfig_name,
        root_arg.as_deref().or(root_dir.as_deref()),
        Some(&file_input),
    );
    let strip_root = if strip_root { Some(root.clone()) } else { None };
    if verbose.is_some() {
        eprintln!("NOTE: --verbose writes to the server log file");
    }
    let expand_component_props = !unexpanded;
    let request =
        server_prot::request::Command::TYPE_OF_NAME(server_prot::type_of_name_options::T {
            input: file_input,
            names: names.clone(),
            verbose,
            wait_for_recheck,
            expand_component_props,
            exact_match_only,
            strip_root: strip_root.clone(),
        });
    let response = flow_command_utils::connect_and_make_request(
        &base_flags.flowconfig_name,
        &connect_flags,
        &root,
        &request,
    );
    match response {
        server_prot::response::Response::TYPE_OF_NAME(results) => {
            let mut has_error = false;
            assert_eq!(
                names.len(),
                results.len(),
                "TYPE_OF_NAME result count should match requested names",
            );
            for (index, (query_name, result)) in names.iter().zip(results).enumerate() {
                if index > 0 {
                    println!();
                }
                match result {
                    Err(err) => {
                        has_error = true;
                        eprintln!("{}", err);
                    }
                    Ok(response) => {
                        let strip_root_string = strip_root
                            .as_ref()
                            .map(|root| root.to_string_lossy().to_string());
                        handle_response(
                            strip_root_string.as_deref(),
                            hide_references,
                            query_name,
                            response,
                        );
                    }
                }
            }
            if has_error {
                flow_common_exit_status::exit(flow_common_exit_status::FlowExitStatus::TypeError);
            }
        }
        response => {
            flow_command_utils::failwith_bad_response(&request, &response);
        }
    }
}

pub(crate) fn command() -> flow_command_spec::Command {
    flow_command_spec::command(spec(), main)
}
