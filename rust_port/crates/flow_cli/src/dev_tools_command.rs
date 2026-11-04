/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::io;
use std::path::PathBuf;

use flow_command_spec::arg_spec;
use flow_dev_tools::ErrorCheckCommand;
use flow_dev_tools::check_type_only_changes;
use flow_dev_tools::runtests;
use flow_dev_tools::update_suppressions::Only;

#[derive(Clone)]
enum DevToolsSubcommand {
    AddComments,
    CheckTypeOnlyChanges,
    Runtests,
    UpdateSuppressions,
}

fn error_check_command_flag() -> arg_spec::FlagType<Option<ErrorCheckCommand>> {
    arg_spec::enum_flag(vec![
        ("check", ErrorCheckCommand::Check),
        ("full-check", ErrorCheckCommand::FullCheck),
        ("status", ErrorCheckCommand::Status),
    ])
}

fn only_flag() -> arg_spec::FlagType<Option<Only>> {
    arg_spec::enum_flag(vec![("add", Only::Add), ("remove", Only::Remove)])
}

fn get_optional_string(args: &arg_spec::Values, flag: &str) -> Option<String> {
    flow_command_spec::get(args, flag, &arg_spec::optional(arg_spec::string())).unwrap()
}

fn get_flowconfig_name(args: &arg_spec::Values) -> String {
    if args.contains_key("--flowconfigName") {
        flow_command_spec::get(
            args,
            "--flowconfigName",
            &arg_spec::required(Some(".flowconfig".to_string()), arg_spec::string()),
        )
        .unwrap()
    } else {
        flow_command_utils::get_base_flags(args).flowconfig_name
    }
}

fn get_bin(args: &arg_spec::Values) -> String {
    get_optional_string(args, "--bin")
        .map(|bin| flow_command_utils::expand_path(&bin))
        .unwrap_or_else(|| {
            std::env::current_exe()
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_else(|_| "flow".to_string())
        })
}

fn get_check(args: &arg_spec::Values) -> ErrorCheckCommand {
    let flag = if args.contains_key("-c") {
        "-c"
    } else {
        "--check"
    };
    flow_command_spec::get(
        args,
        flag,
        &arg_spec::required(Some(ErrorCheckCommand::Status), error_check_command_flag()),
    )
    .unwrap()
}

fn parse_single_root(args: &arg_spec::Values) -> PathBuf {
    let Some(root) = flow_command_spec::get(args, "ROOT", &arg_spec::string()).unwrap() else {
        flow_common_exit_status::exit_with_msg(
            flow_common_exit_status::FlowExitStatus::CommandlineUsageError,
            "Missing required ROOT argument",
        );
    };
    PathBuf::from(flow_command_utils::expand_path(&root))
}

fn parse_roots(args: &arg_spec::Values) -> Vec<PathBuf> {
    let roots = flow_command_spec::get(args, "ROOT...", &arg_spec::list_of(arg_spec::string()))
        .unwrap()
        .unwrap_or_default();
    if roots.is_empty() {
        flow_common_exit_status::exit_with_msg(
            flow_common_exit_status::FlowExitStatus::CommandlineUsageError,
            "Missing required ROOT argument",
        );
    }
    roots
        .into_iter()
        .map(|root| PathBuf::from(flow_command_utils::expand_path(&root)))
        .collect()
}

fn dev_tools_common_spec(name: &str, doc: &str, usage: String) -> flow_command_spec::Spec {
    flow_command_utils::add_base_flags(flow_command_spec::Spec::new(
        name,
        doc,
        flow_command_spec::Visibility::Public,
        usage,
    ))
    .flag(
        "--flowconfigName",
        &arg_spec::required(Some(".flowconfig".to_string()), arg_spec::string()),
        "Name of the flowconfig to use in checking",
        Some("FLOW_CONFIG_NAME"),
    )
    .flag(
        "--check",
        &arg_spec::required(Some(ErrorCheckCommand::Status), error_check_command_flag()),
        "The flow command to check flow errors (check|full-check|status)",
        None,
    )
    .flag(
        "-c",
        &arg_spec::required(Some(ErrorCheckCommand::Status), error_check_command_flag()),
        "",
        None,
    )
}

fn add_bin_flag(spec: flow_command_spec::Spec) -> flow_command_spec::Spec {
    spec.flag(
        "--bin",
        &arg_spec::optional(arg_spec::string()),
        "Path to the flow binary",
        None,
    )
}

fn add_comments_spec() -> flow_command_spec::Spec {
    add_bin_flag(dev_tools_common_spec(
        "add-comments",
        "Adds flow comments",
        format!(
            "Usage: {} dev-tools add-comments [OPTION]... ROOT\n\nQueries Flow for the errors for ROOT. The errors automatically have a comment added on the line before them.\n",
            flow_command_utils::exe_name()
        ),
    ))
    .flag(
        "--comment",
        &arg_spec::optional(arg_spec::string()),
        "Comment to add before the selected errors",
        None,
    )
    .flag("--all", &arg_spec::truthy(), "DEPRECATED", None)
    .flag(
        "--code",
        &arg_spec::optional(arg_spec::string()),
        "Only add comments for a specific code",
        None,
    )
    .anon("ROOT", &arg_spec::string())
}

fn check_type_only_changes_spec() -> flow_command_spec::Spec {
    flow_command_spec::Spec::new(
        "check-type-only-changes",
        "Checks that changes to JavaScript files only change types",
        flow_command_spec::Visibility::Public,
        format!(
            r#"Usage: {exe} dev-tools check-type-only-changes [OPTION]...

Compiles both sides of every changed JavaScript file to runnable JavaScript, erasing types and
lowering Flow runtime syntax as Babel would, and compares the results with locations and comments
stripped. Exits with code 1 if the runtime code of any file changed or failed to compile.

By default, the working copy is compared against the current commit.

Examples (Mercurial revisions; use HEAD and origin/main with Git):
  Uncommitted changes:
    {exe} dev-tools check-type-only-changes
  The current commit:
    {exe} dev-tools check-type-only-changes --rev .
  The stack and uncommitted changes since master:
    {exe} dev-tools check-type-only-changes --mergebase-with master
  Between two revisions:
    {exe} dev-tools check-type-only-changes --base REV1 --rev REV2
"#,
            exe = flow_command_utils::exe_name()
        ),
    )
    .flag(
        "--base",
        &arg_spec::optional(arg_spec::string()),
        "Revision to compare against (default: the parent of the checked revision, or the current commit for the working copy)",
        None,
    )
    .flag(
        "--mergebase-with",
        &arg_spec::optional(arg_spec::string()),
        "Compare against the merge base of the checked revision and this revision",
        None,
    )
    .flag(
        "--rev",
        &arg_spec::optional(arg_spec::string()),
        "Revision to check (default: the working copy)",
        None,
    )
    .flag(
        "--show-diff",
        &arg_spec::truthy(),
        "Print a diff of the compiled runtime code of each file whose runtime code changed",
        None,
    )
}

fn update_suppressions_spec() -> flow_command_spec::Spec {
    dev_tools_common_spec(
        "update-suppressions",
        "Adds and removes suppression comments",
        format!(
            "Usage: {} dev-tools update-suppressions [OPTION]... ROOT [ROOT...]\n\nRemoves unnecessary, and adds necessary, error suppression comments for ROOT.\n",
            flow_command_utils::exe_name()
        ),
    )
    .flag(
        "--include-flowtest",
        &arg_spec::truthy(),
        "Also remove comments from files that end in -flowtest.js or are in a __flowtests__ directory",
        None,
    )
    .flag(
        "--sites",
        &arg_spec::optional(arg_spec::string()),
        "Comma-delimited list of site names for each ROOT",
        None,
    )
    .flag(
        "--comment",
        &arg_spec::optional(arg_spec::string()),
        "Comment to include with the suppression. Automatically prepends $FlowFixMe",
        None,
    )
    .flag(
        "--only",
        &arg_spec::optional(only_flag()),
        "Use --only add to only add comments and --only remove to only remove comments",
        None,
    )
    .anon("ROOT...", &arg_spec::list_of(arg_spec::string()))
}

fn runtests_spec() -> flow_command_spec::Spec {
    flow_command_spec::Spec::new(
        "runtests",
        "Runs Flow's check tests (tests/ directory)",
        flow_command_spec::Visibility::Public,
        format!(
            r#"Usage: {} dev-tools runtests [OPTION]... [TEST_FILTER]

Runs Flow's bash-style tests from the tests/ directory using the cross-platform Rust runner."#,
            flow_command_utils::exe_name()
        ),
    )
    .flag(
        "--tests-dir",
        &arg_spec::optional(arg_spec::string()),
        "Path to tests directory",
        None,
    )
    .flag(
        "-d",
        &arg_spec::optional(arg_spec::string()),
        "Flow root containing tests/",
        None,
    )
    .flag(
        "--filter",
        &arg_spec::optional(arg_spec::string()),
        "Regular expression to filter test names",
        None,
    )
    .flag(
        "-f",
        &arg_spec::optional(arg_spec::string()),
        "Regular expression to filter test names",
        None,
    )
    .flag(
        "--test",
        &arg_spec::optional(arg_spec::string()),
        "Run a specific test",
        None,
    )
    .flag(
        "-t",
        &arg_spec::optional(arg_spec::string()),
        "Run a specific test",
        None,
    )
    .flag(
        "--run-test",
        &arg_spec::optional(arg_spec::string()),
        "Run a specific test (Buck/TPX compatibility)",
        None,
    )
    .flag(
        "--parallelism",
        &arg_spec::optional(arg_spec::uint()),
        "Number of tests to run in parallel",
        None,
    )
    .flag(
        "-p",
        &arg_spec::optional(arg_spec::uint()),
        "Number of tests to run in parallel",
        None,
    )
    .flag(
        "--check-only",
        &arg_spec::truthy(),
        "Only run full-check tests",
        None,
    )
    .flag("-c", &arg_spec::truthy(), "Only run full-check tests", None)
    .flag(
        "--saved-state",
        &arg_spec::truthy(),
        "Test using saved state",
        None,
    )
    .flag("-s", &arg_spec::truthy(), "Test using saved state", None)
    .flag(
        "--record",
        &arg_spec::truthy(),
        "Re-record failing tests to update expected output",
        None,
    )
    .flag("-r", &arg_spec::truthy(), "Re-record failing tests", None)
    .flag(
        "--quiet",
        &arg_spec::truthy(),
        "Quiet output (hides status, just prints results)",
        None,
    )
    .flag("-q", &arg_spec::truthy(), "Quiet output", None)
    .flag(
        "--verbose",
        &arg_spec::truthy(),
        "Verbose output (shows skipped tests)",
        None,
    )
    .flag("-v", &arg_spec::truthy(), "Verbose output", None)
    .flag(
        "--json",
        &arg_spec::truthy(),
        "Output results as a JSON map",
        None,
    )
    .flag(
        "-j",
        &arg_spec::truthy(),
        "Output results as a JSON map",
        None,
    )
    .flag(
        "--list",
        &arg_spec::truthy(),
        "List tests that will be run",
        None,
    )
    .flag(
        "-l",
        &arg_spec::truthy(),
        "List tests that will be run",
        None,
    )
    .flag(
        "--list-tests",
        &arg_spec::truthy(),
        "List tests that will be run (Buck/TPX compatibility)",
        None,
    )
    .anon("TEST_FILTER", &arg_spec::string())
}

fn root_spec() -> flow_command_spec::Spec {
    flow_command_spec::Spec::new(
        "dev-tools",
        "Runs Flow developer tools",
        flow_command_spec::Visibility::Public,
        format!(
            "Usage: {} dev-tools SUBCOMMAND [OPTION]...\n\nValid values for SUBCOMMAND:\n{}",
            flow_command_utils::exe_name(),
            flow_command_spec::format_two_columns(
                None,
                None,
                1,
                &[
                    ("add-comments".to_string(), "Adds flow comments".to_string(),),
                    (
                        "check-type-only-changes".to_string(),
                        "Checks that changes to JavaScript files only change types".to_string(),
                    ),
                    ("runtests".to_string(), "Runs Flow check tests".to_string()),
                    (
                        "update-suppressions".to_string(),
                        "Adds and removes suppression comments".to_string(),
                    ),
                ],
            ),
        ),
    )
    .anon(
        "subcommand",
        &arg_spec::required(
            None,
            arg_spec::command_flag(vec![
                ("add-comments", DevToolsSubcommand::AddComments),
                (
                    "check-type-only-changes",
                    DevToolsSubcommand::CheckTypeOnlyChanges,
                ),
                ("runtests", DevToolsSubcommand::Runtests),
                (
                    "update-suppressions",
                    DevToolsSubcommand::UpdateSuppressions,
                ),
            ]),
        ),
    )
}

pub(crate) fn command() -> flow_command_spec::Command {
    flow_command_spec::command(root_spec(), |args| {
        let (subcommand, argv) = flow_command_spec::get(
            args,
            "subcommand",
            &arg_spec::required(
                None,
                arg_spec::command_flag(vec![
                    ("add-comments", DevToolsSubcommand::AddComments),
                    (
                        "check-type-only-changes",
                        DevToolsSubcommand::CheckTypeOnlyChanges,
                    ),
                    ("runtests", DevToolsSubcommand::Runtests),
                    (
                        "update-suppressions",
                        DevToolsSubcommand::UpdateSuppressions,
                    ),
                ]),
            ),
        )
        .unwrap();
        let command = match subcommand {
            DevToolsSubcommand::AddComments => {
                flow_command_spec::command(add_comments_spec(), |args| {
                    run_or_exit(run_add_comments(args))
                })
            }
            DevToolsSubcommand::CheckTypeOnlyChanges => flow_command_spec::command(
                check_type_only_changes_spec(),
                run_check_type_only_changes,
            ),
            DevToolsSubcommand::Runtests => {
                flow_command_spec::command(runtests_spec(), run_runtests)
            }
            DevToolsSubcommand::UpdateSuppressions => {
                flow_command_spec::command(update_suppressions_spec(), |args| {
                    run_or_exit(run_update_suppressions(args))
                })
            }
        };
        flow_command_utils::run_command(&command, &argv);
    })
}

fn run_runtests(args: &arg_spec::Values) {
    let json_output = flow_command_spec::get(args, "--json", &arg_spec::truthy()).unwrap()
        || flow_command_spec::get(args, "-j", &arg_spec::truthy()).unwrap();
    let args = runtests::Args {
        current_version: flow_common::flow_version::version().to_owned(),
        tests_dir: get_optional_string(args, "--tests-dir"),
        dir: get_optional_string(args, "-d"),
        filter: get_optional_string(args, "--filter").or_else(|| get_optional_string(args, "-f")),
        test: get_optional_string(args, "--test").or_else(|| get_optional_string(args, "-t")),
        run_test: get_optional_string(args, "--run-test"),
        positional_filter: flow_command_spec::get(args, "TEST_FILTER", &arg_spec::string())
            .unwrap(),
        parallelism: flow_command_spec::get(
            args,
            "--parallelism",
            &arg_spec::optional(arg_spec::uint()),
        )
        .unwrap()
        .or_else(|| {
            flow_command_spec::get(args, "-p", &arg_spec::optional(arg_spec::uint())).unwrap()
        }),
        check_only: flow_command_spec::get(args, "--check-only", &arg_spec::truthy()).unwrap()
            || flow_command_spec::get(args, "-c", &arg_spec::truthy()).unwrap(),
        saved_state: flow_command_spec::get(args, "--saved-state", &arg_spec::truthy()).unwrap()
            || flow_command_spec::get(args, "-s", &arg_spec::truthy()).unwrap(),
        record: flow_command_spec::get(args, "--record", &arg_spec::truthy()).unwrap()
            || flow_command_spec::get(args, "-r", &arg_spec::truthy()).unwrap(),
        quiet: flow_command_spec::get(args, "--quiet", &arg_spec::truthy()).unwrap()
            || flow_command_spec::get(args, "-q", &arg_spec::truthy()).unwrap(),
        verbose: flow_command_spec::get(args, "--verbose", &arg_spec::truthy()).unwrap()
            || flow_command_spec::get(args, "-v", &arg_spec::truthy()).unwrap(),
        json_output,
        list: flow_command_spec::get(args, "--list", &arg_spec::truthy()).unwrap()
            || flow_command_spec::get(args, "-l", &arg_spec::truthy()).unwrap(),
        list_tests: flow_command_spec::get(args, "--list-tests", &arg_spec::truthy()).unwrap(),
    };
    match runtests::run(args) {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(err) => flow_common_exit_status::exit_with_msg(
            flow_common_exit_status::FlowExitStatus::UnknownError,
            &format!("runtests failed: {err}"),
        ),
    }
}

fn run_check_type_only_changes(args: &arg_spec::Values) {
    let base = match (
        get_optional_string(args, "--base"),
        get_optional_string(args, "--mergebase-with"),
    ) {
        (None, None) => check_type_only_changes::Base::Parent,
        (Some(base), None) => check_type_only_changes::Base::Revision(base),
        (None, Some(other)) => check_type_only_changes::Base::MergeBaseWith(other),
        (Some(_), Some(_)) => flow_common_exit_status::exit_with_msg(
            flow_common_exit_status::FlowExitStatus::CommandlineUsageError,
            "--base and --mergebase-with cannot be used together",
        ),
    };
    let result = std::env::current_dir().and_then(|dir| {
        check_type_only_changes::run(check_type_only_changes::Args {
            dir,
            base,
            rev: get_optional_string(args, "--rev"),
            show_diff: flow_command_spec::get(args, "--show-diff", &arg_spec::truthy()).unwrap(),
        })
    });
    match result {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(err) => flow_common_exit_status::exit_with_msg(
            flow_common_exit_status::FlowExitStatus::UnknownError,
            &format!("check-type-only-changes failed: {err}"),
        ),
    }
}

fn run_or_exit(result: io::Result<()>) {
    if let Err(err) = result {
        let msg = format!("dev-tools failed: {}", err);
        flow_common_exit_status::exit_with_msg(
            flow_common_exit_status::FlowExitStatus::UnknownError,
            &msg,
        );
    }
}

fn run_add_comments(args: &arg_spec::Values) -> io::Result<()> {
    flow_dev_tools::comment::add_comments::runner(flow_dev_tools::comment::add_comments::Args {
        bin: get_bin(args),
        flowconfig_name: get_flowconfig_name(args),
        comment: get_optional_string(args, "--comment"),
        error_code: get_optional_string(args, "--code"),
        error_check_command: get_check(args),
        root: parse_single_root(args),
    })
}

fn run_update_suppressions(args: &arg_spec::Values) -> io::Result<()> {
    let flowconfig_name = get_flowconfig_name(args);
    let error_check_command = get_check(args);
    flow_dev_tools::update_suppressions::runner(flow_dev_tools::update_suppressions::Args {
        comment: get_optional_string(args, "--comment").unwrap_or_default(),
        roots: parse_roots(args),
        root_names: get_optional_string(args, "--sites")
            .unwrap_or_default()
            .split(',')
            .map(|site| site.trim().to_string())
            .collect(),
        include_flowtest: flow_command_spec::get(args, "--include-flowtest", &arg_spec::truthy())
            .unwrap(),
        only: flow_command_spec::get(args, "--only", &arg_spec::optional(only_flag())).unwrap(),
        load_root: move |root: &std::path::Path| {
            let root = flow_command_utils::guess_root(&flowconfig_name, root.to_str());
            let files = crate::ls_command::get_all_flow_files(&flowconfig_name, &root);
            let result = match error_check_command {
                ErrorCheckCommand::Status => {
                    crate::status_command::status_json(&flowconfig_name, &root)?
                }
                ErrorCheckCommand::Check | ErrorCheckCommand::FullCheck => {
                    crate::foreground_check_commands::full_check_json(&flowconfig_name, &root)
                }
            };
            Ok((files, result))
        },
    })
}
