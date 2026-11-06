/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::HashMap;
use std::fs;
use std::future::Future;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;

use regex::Regex;
#[cfg(unix)]
use tokio::fs::symlink as symlink_dir;
#[cfg(unix)]
use tokio::fs::symlink as symlink_file;
#[cfg(windows)]
use tokio::fs::symlink_dir;
#[cfg(windows)]
use tokio::fs::symlink_file;

use super::AnnotateExportsOptions;
use super::ExecOptions;
use super::check_test_config::TestVariant;
use super::check_test_helpers::TestContext;
use super::check_test_helpers::TestContextOptions;
use super::check_website::WebsiteTestOptions;
use super::check_website::run_website_test;
use super::diff_output;
use super::exec_file;
use super::exec_shell;
use super::exists;
use super::parse_test_config;
use super::record_output;
use super::run_annotate_exports;
use super::split_shell_args;

static TEMP_ID: AtomicU64 = AtomicU64::new(0);

fn make_temp_parent() -> io::Result<PathBuf> {
    loop {
        let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("flow_check_test_{}_{}", std::process::id(), id));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
}

async fn create_symlink(target: &Path, source_link: &Path, link: &Path) -> io::Result<()> {
    let metadata_target = if target.is_absolute() {
        target.to_path_buf()
    } else {
        source_link
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(target)
    };
    let metadata = tokio::fs::metadata(metadata_target).await?;
    if metadata.is_dir() {
        symlink_dir(target, link).await
    } else {
        symlink_file(target, link).await
    }
}

// Return codes matching runtests-common.sh
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(i32)]
pub(super) enum TestStatus {
    Success = 0,
    Failure = 1,
    Skip = 2,
    MissingFiles = 3,
    Error = 4,
    MissingAllOption = 5,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TestResult {
    pub(super) status: TestStatus,
    pub(super) name: String,
    pub(super) diff: Option<String>,
}

// Recursively copy directory contents (preserving symlinks).
// Limits concurrency to avoid exhausting file descriptors when
// copying test directories with many files in a parallel runner.
const COPY_CONCURRENCY: usize = 32;

pub(super) fn copy_dir<'a>(
    src: &'a Path,
    dst: &'a Path,
) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'a>> {
    Box::pin(async move {
        tokio::fs::create_dir_all(dst).await?;
        let mut read_dir = tokio::fs::read_dir(src).await?;
        let mut entries = Vec::new();
        while let Some(entry) = read_dir.next_entry().await? {
            entries.push(entry.file_name());
        }
        for batch in entries.chunks(COPY_CONCURRENCY) {
            let batch = batch.iter().map(|entry| {
                let src_path = src.join(entry);
                let dst_path = dst.join(entry);
                async move {
                    let metadata = tokio::fs::symlink_metadata(&src_path).await?;
                    if metadata.file_type().is_symlink() {
                        let link_target = tokio::fs::read_link(&src_path).await?;
                        create_symlink(&link_target, &src_path, &dst_path).await?;
                    } else if metadata.is_dir() {
                        copy_dir(&src_path, &dst_path).await?;
                    } else {
                        tokio::fs::copy(&src_path, &dst_path).await?;
                        let mut permissions = metadata.permissions();
                        #[cfg(unix)]
                        permissions.set_mode(permissions.mode() | 0o200);
                        #[cfg(windows)]
                        permissions.set_readonly(false);
                        tokio::fs::set_permissions(&dst_path, permissions).await?;
                    }
                    Ok::<(), io::Error>(())
                }
            });
            futures::future::try_join_all(batch).await?;
        }
        Ok(())
    })
}

#[derive(Clone)]
pub(super) struct RunOneTestOptions {
    pub(super) test_dir: PathBuf,
    pub(super) flow_bin: PathBuf,
    pub(super) version: String,
    pub(super) check_only: bool,
    pub(super) saved_state: bool,
    pub(super) record: bool,
    pub(super) scripts_dir: PathBuf,
    pub(super) env: HashMap<String, String>,
}

pub(super) fn run_one_test(opts: RunOneTestOptions) -> io::Result<TestResult> {
    let RunOneTestOptions {
        test_dir,
        flow_bin,
        version,
        check_only,
        saved_state,
        record,
        scripts_dir,
        env: initial_env,
    } = opts;

    let name = test_dir
        .file_name()
        .ok_or_else(|| io::Error::other("test directory has no basename"))?
        .to_string_lossy()
        .into_owned();
    let exp_file_name = format!("{name}.exp");

    // Windows symlink skip
    if cfg!(windows) && (name == "symlink" || name == "node_tests") {
        return Ok(TestResult {
            status: TestStatus::Skip,
            name,
            diff: None,
        });
    }

    // Check required files — .exp file is always required.
    // For .flowconfig, we require it OR .testconfig (since .testconfig may
    // specify a cwd: where the .flowconfig lives, e.g. in a subdirectory).
    let has_exp_file = exists(&test_dir.join(&exp_file_name));
    let has_flowconfig = exists(&test_dir.join(".flowconfig"));
    let has_testconfig = exists(&test_dir.join(".testconfig"));
    if !has_exp_file || (!has_flowconfig && !has_testconfig) {
        return Ok(if name == "auxiliary" || name == "callable" {
            TestResult {
                status: TestStatus::Skip,
                name,
                diff: None,
            }
        } else {
            TestResult {
                status: TestStatus::MissingFiles,
                name,
                diff: None,
            }
        });
    }

    // Create temp directory
    let tmp_parent = make_temp_parent()?;
    let tmp_dir = tmp_parent.join(&name);
    let mut work_dir = tmp_dir.clone(); // hoisted so `finally` can stop the flow server
    let mut server_started = false; // track whether we started a flow server
    let mut env = HashMap::new();

    let result = (|| -> io::Result<TestResult> {
        fs::create_dir_all(&tmp_dir)?;

        // Copy test directory to temp
        flow_tokio_runtime::block_on(copy_dir(&test_dir, &tmp_dir))?;

        // Copy fs.sh equivalent (parent's fs.sh) - not needed for JS but maintain structure
        let fs_sh_path = test_dir.join("..").join("fs.sh");
        if exists(&fs_sh_path) {
            fs::copy(fs_sh_path, tmp_parent.join("fs.sh"))?;
        }

        // Move exp file out of test dir (same filesystem, so rename is safe)
        let exp_src = tmp_dir.join(&exp_file_name);
        let exp_dst = tmp_parent.join(&exp_file_name);
        if exists(&exp_src) {
            fs::rename(exp_src, &exp_dst)?;
        }

        let out_file = tmp_parent.join(format!("{name}.out"));
        let log_file = tmp_parent.join(format!("{name}.log"));
        let monitor_log_file = tmp_parent.join(format!("{name}.monitor_log"));
        let err_file = tmp_parent.join(format!("{name}.err"));
        let diff_file = tmp_parent.join(format!("{name}.diff"));

        // Parse config
        let config = parse_test_config(&tmp_dir)?;

        // Handle cwd - determine working directory
        work_dir = if config.cwd.is_empty() {
            tmp_dir
        } else {
            tmp_dir.join(&config.cwd)
        };

        // Determine flowlib - check .flowconfig in the working directory (after cwd applied)
        // Matches bash behavior: the check happens after `pushd "$cwd"`
        let mut use_prelude = true;
        let flowconfig_path = work_dir.join(".flowconfig"); // JS: const flowconfigPath = join(workDir, '.flowconfig');
        if exists(&flowconfig_path) {
            let flowconfig_content = fs::read_to_string(flowconfig_path)?;
            if flowconfig_content.contains("builtin_lib") {
                use_prelude = false;
            }

            let all_re = Regex::new(r"(?m)^[ \t]*all=(true|false)\b").map_err(io::Error::other)?;
            if !all_re.is_match(&flowconfig_content) {
                return Ok(TestResult {
                    status: TestStatus::MissingAllOption,
                    name,
                    diff: None,
                });
            }
        }

        // Skip conditions
        if saved_state && config.skip_saved_state {
            return Ok(TestResult {
                status: TestStatus::Skip,
                name,
                diff: None,
            });
        }
        if !saved_state && config.saved_state_only {
            return Ok(TestResult {
                status: TestStatus::Skip,
                name,
                diff: None,
            });
        }
        if !initial_env.contains_key("FLOW_GIT_BINARY") && config.git {
            return Ok(TestResult {
                status: TestStatus::Skip,
                name,
                diff: None,
            });
        }
        if cfg!(windows) && config.skip_windows {
            return Ok(TestResult {
                status: TestStatus::Skip,
                name,
                diff: None,
            });
        }
        if check_only && !config.is_check_only() {
            return Ok(TestResult {
                status: TestStatus::Skip,
                name,
                diff: None,
            });
        }

        // Set up environment
        // Include tests_bin in PATH so the flow server can find helper scripts
        // (e.g. fetch_saved_state.sh for --saved-state-fetcher local).
        // Matches bash's `PATH="$THIS_DIR/scripts/tests_bin:$PATH"`.
        env = initial_env.clone(); // JS: env = {...process.env,
        let tests_bin_dir = scripts_dir.join("tests_bin"); // JS: const testsBinDir = join(resolve(__dirname, '../../../../scripts'), 'tests_bin');
        env.insert("FLOW_TEMP_DIR".to_owned(), tmp_parent.display().to_string());
        if use_prelude {
            env.insert("FLOW_BUILTIN_LIB".to_owned(), "prelude".to_owned());
        }
        env.insert("IN_FLOW_TEST".to_owned(), "1".to_owned());
        env.insert("FLOW_LOG_LEVEL".to_owned(), "debug".to_owned());
        env.insert("FLOW_LOG_FILE".to_owned(), log_file.display().to_string());
        env.insert(
            "FLOW_MONITOR_LOG_FILE".to_owned(),
            monitor_log_file.display().to_string(),
        );
        env.insert("FLOW".to_owned(), flow_bin.display().to_string());
        env.insert("VERSION".to_owned(), version.clone());
        let mut paths = vec![tests_bin_dir];
        if let Some(path) = env.get("PATH") {
            paths.extend(std::env::split_paths(path));
        }
        env.insert(
            "PATH".to_owned(),
            std::env::join_paths(paths)
                .map_err(io::Error::other)?
                .to_string_lossy()
                .into_owned(),
        );

        let mut return_status = TestStatus::Success;
        let mut out_content = String::new();
        let mut err_content = String::new();

        // Execute test based on mode
        if config.variant != TestVariant::Standard {
            env.insert("FLOW_BUILTIN_LIB".to_owned(), "default".to_owned());
            let flow_root = scripts_dir
                .parent()
                .ok_or_else(|| io::Error::other("scripts directory has no parent"))?;
            let command_result = run_website_test(WebsiteTestOptions {
                variant: config.variant,
                flow_bin: &flow_bin,
                website_dir: &flow_root.join("website"),
                work_dir: &work_dir,
                env: &env,
                record,
            })?;
            out_content = command_result.stdout;
            err_content = command_result.stderr;
            if command_result.code != 0 {
                return_status = TestStatus::Error;
            }
        } else if config.cmd.trim() == "full-check" {
            // Full-check mode
            if saved_state {
                return Ok(TestResult {
                    status: TestStatus::Skip,
                    name,
                    diff: None,
                });
            }
            let mut args = vec!["full-check".to_owned(), ".".to_owned()];
            if use_prelude {
                args.extend(["--builtin-lib".to_owned(), "prelude".to_owned()]);
            }
            args.extend(["--strip-root".to_owned(), "--show-all-errors".to_owned()]);
            let command_result = exec_file(
                &flow_bin.to_string_lossy(),
                &args,
                &ExecOptions {
                    cwd: Some(work_dir.clone()),
                    env: Some(env.clone()),
                    ..ExecOptions::default()
                },
                None,
            )?;
            out_content = command_result.stdout; // JS: outContent = result.stdout;
            if config.ignore_stderr {
                err_content = command_result.stderr;
            } else {
                out_content.push_str(&command_result.stderr);
            }
            if config.ignore_stderr && command_result.code != 0 && command_result.code != 2 {
                err_content.push_str(&format!(
                    "flow full-check return code: {}\n",
                    command_result.code
                ));
                return_status = TestStatus::Error;
            }
        } else if split_shell_args(config.cmd.trim())?
            .first()
            .map(String::as_str)
            == Some("annotate-exports")
        {
            // Annotate-exports mode
            // Mark serverStarted because runAnnotateExports starts a flow server
            // internally. If it throws before stopping the server, the finally
            // block will clean it up.
            server_started = true;
            let command_args = config
                .cmd
                .trim()
                .strip_prefix("annotate-exports")
                .unwrap_or("")
                .trim_start()
                .to_owned();
            let annotate_result = run_annotate_exports(AnnotateExportsOptions {
                flow_bin: flow_bin.clone(),
                test_dir: work_dir.clone(),
                use_prelude,
                cmd_args: command_args,
                log_file: log_file.clone(),
                monitor_log_file: monitor_log_file.clone(),
                wait_for_recheck: config.wait_for_recheck.clone(),
                file_watcher: config.file_watcher.clone(),
                env: env.clone(),
            })?;
            // runAnnotateExports stops the server internally, so clear the flag
            // to avoid a redundant stop in the finally block.
            server_started = false;
            out_content = annotate_result.output;
            err_content = annotate_result.stderr;
            // Match bash behavior: only set RUNTEST_ERROR when ignore_stderr
            // is true. When ignore_stderr is false, the error exit code is
            // not treated as fatal — the test is judged by its output diff.
            if config.ignore_stderr && annotate_result.error_code != 0 {
                err_content.push_str(&format!(
                    "flow codemod return code: {}\n",
                    annotate_result.error_code
                ));
                return_status = TestStatus::Error;
            }
        } else if !config.shell.is_empty() {
            // JS: } else {
            // Shell/script mode
            // Run as bash script (original test.sh)
            let bash_test_prelude = r#"set -e
to_bash_path() {
  if command -v cygpath > /dev/null 2>&1; then
    cygpath -u "$1"
  else
    printf "%s\n" "$1"
  fi
}

FLOW="$(to_bash_path "$_CT_FLOW_BIN")"
ERR_FILE="$(to_bash_path "$_CT_ERR_FILE")"
SCRIPT_PATH="$(to_bash_path "$_CT_SCRIPT_PATH")"
THIS_DIR="$(to_bash_path "$_CT_THIS_DIR")"

export FLOW
export EXIT_OK=0
export EXIT_ONE=1
export EXIT_ERRS=2
export EXIT_INVALID_FLOWCONFIG=8
export EXIT_SERVER_ALREADY_RUNNING=11
export EXIT_COULD_NOT_FIND_FLOWCONFIG=12
export EXIT_USAGE=64
export FLOW_LOG_FILE="$_CT_LOG_FILE"
export FLOW_MONITOR_LOG_FILE="$_CT_MONITOR_LOG_FILE"
export FLOW_TEMP_DIR="$_CT_TEMP_DIR"
export IN_FLOW_TEST=1
export FLOW_LOG_LEVEL=debug

# Variables needed by start_flow_unsafe / create_saved_state
saved_state=$_CT_SAVED_STATE
flowlib="$_CT_FLOWLIB"
abs_log_file="$_CT_LOG_FILE"
abs_monitor_log_file="$_CT_MONITOR_LOG_FILE"
wait_for_recheck="$_CT_WAIT_FOR_RECHECK"
file_watcher="$_CT_FILE_WATCHER"

SAVED_OPTION="$(set +o | grep errexit)"

assert_exit_on_line() {
  (
    set -e
    _assert_exit__line=$1; shift
    _assert_exit__ret=0
    _assert_exit__code=$1; shift
    "$@" ||  _assert_exit__ret=$?
    eval "$SAVED_OPTION"
    if [ "$_assert_exit__ret" -eq "$_assert_exit__code" ]; then
      return 0
    else
      echo "\`$(basename "$1") ${*:2}\` expected to exit code $_assert_exit__code but got $_assert_exit__ret (line $_assert_exit__line)"
      return 1
    fi
  )
  return $?
}
assert_exit() { assert_exit_on_line "${BASH_LINENO[0]}" "$@"; }
assert_ok() { assert_exit_on_line "${BASH_LINENO[0]}" "$EXIT_OK" "$@"; }
assert_one() { assert_exit_on_line "${BASH_LINENO[0]}" "$EXIT_ONE" "$@"; }
assert_errors() { assert_exit_on_line "${BASH_LINENO[0]}" "$EXIT_ERRS" "$@"; }
assert_server_already_running() { assert_exit_on_line "${BASH_LINENO[0]}" "$EXIT_SERVER_ALREADY_RUNNING" "$@"; }

query_at_pos() {
  local query=$1 file=$2 line=$3 col=$4
  shift 4
  local flags=("$@")
  printf "%s:%s:%s\n" "$file" "$line" "$col"
  echo "Flags:" "${flags[@]}"
  assert_ok "$FLOW" "$query" "$file" "$line" "$col" --strip-root "${flags[@]}" < "$file"
  printf "\n"
}

queries_in_file() {
  local query=$1 file=$2
  shift 2
  local arg_flags_array=("$@")
  # A query line queries the closest line above it that is not a query line
  # itself, so several queries on one line stack under it.
  awk 'BEGIN { target = 0 } /^\/\/.*\^/{ print NR, target; next } { target = NR }' "$file" | while read -r line target; do
    local linep="$line""p"
    local col
    col=$(sed -n "$linep" "$file" | awk -F'^' '{ print $1}' | wc -c)
    col="$((col))"
    local line_flags_array=()
    IFS=" " read -r -a line_flags_array <<< "$(sed -n "$linep" "$file" | awk -F'^' '{ print $2 }')"
    if [ -n "$col" ]; then
      local all_flags=("${arg_flags_array[@]}" "${line_flags_array[@]}")
      query_at_pos "$query" "$file" "$target" "$col" "${all_flags[@]}"
    fi
  done
}

show_skipping_stats() {
  printf "\n========Skipping stats========\n"
  grep -o "Merge skipped [0-9]\+ of [0-9]\+ modules" $1 | tail -n 1
  grep -o "Check will skip [0-9]\+ of [0-9]\+ files" $1 | tail -n 1
}

create_saved_state() {
  local root="$1"
  local flowconfig_name="$2"
  (
    set -e
    "$FLOW" start "$root" \
      $flowlib --wait \
      --wait-for-recheck "$wait_for_recheck" \
      --lazy-mode none \
      --file-watcher "$file_watcher" \
      --flowconfig-name "$flowconfig_name" \
      --log-file "$abs_log_file" \
      --monitor-log-file "$abs_monitor_log_file"

    local SAVED_STATE_FILENAME="$root/.flow.saved_state"
    local CHANGES_FILENAME="$root/.flow.saved_state_file_changes"
    assert_ok "$FLOW" save-state \
      --root "$root" \
      --out "$SAVED_STATE_FILENAME" \
      --flowconfig-name "$flowconfig_name"
    assert_ok "$FLOW" stop --flowconfig-name "$flowconfig_name" "$root"
    touch "$CHANGES_FILENAME"
  ) > /dev/null 2>&1
  return $?
}

start_flow_unsafe() {
  local root=$1; shift
  if [ ! -d "$root" ]; then
    printf "Invalid root directory '%s'\n" "$root" >&2
    return 1
  fi
  if [[ "$saved_state" -eq 1 ]]; then
    local flowconfig_name=".flowconfig"
    for ((i=1; i<=$#; i++)); do
      opt="${!i}"
      if [ "$opt" = "--flowconfig-name" ]; then
        ((i++))
        flowconfig_name=${!i}
      fi
    done
    if create_saved_state "$root" "$flowconfig_name"; then
      PATH="$THIS_DIR/tests_bin:$PATH" \
      "$FLOW" start "$root" \
        $flowlib --wait \
        --wait-for-recheck "$wait_for_recheck" \
        --saved-state-fetcher "local" \
        --saved-state-no-fallback \
        --file-watcher "$file_watcher" \
        --log-file "$abs_log_file" \
        --monitor-log-file "$abs_monitor_log_file" \
        "$@"
      return $?
    else
      printf "Failed to generate saved state\n" >&2
      return 1
    fi
  else
    PATH="$THIS_DIR/tests_bin:$PATH" \
    "$FLOW" start "$root" \
      $flowlib --wait \
      --wait-for-recheck "$wait_for_recheck" \
      --file-watcher "$file_watcher" \
      --log-file "$abs_log_file" \
      --monitor-log-file "$abs_monitor_log_file" \
      "$@"
    return $?
  fi
}

start_flow() {
  assert_ok start_flow_unsafe "$@"
}

# Auto-start server if configured
auto_start_enabled=$_CT_AUTO_START
if [ "$auto_start_enabled" -eq 1 ]; then
  set +e
  start_flow_unsafe . $_CT_START_ARGS > /dev/null 2>> "$ERR_FILE"
  code=$?
  set -e
  if [ $code -ne 0 ]; then
    printf "flow start exited code %s\n" "$code"
    exit $code
  fi
fi

# Run script in subshell (matching original bash behavior where shell
# scripts run in a ( ... ) subshell, so failures don't skip cleanup)
set +e
(
  set -e
  source "$SCRIPT_PATH" "$FLOW"
)
_script_exit=$?
set -e

# Stop server - always attempt, matching bash's
# "stop server, even if we didn't start it"
"$FLOW" stop . 1> /dev/null 2>&1 || true

exit $_script_exit
"#;

            // Run as bash script (original test.sh)
            let script_path = work_dir.join(&config.shell);
            let mut script_env = env.clone();
            // Pass all dynamic values via environment variables to avoid
            // shell injection when paths contain spaces or special characters.
            for (key, value) in [
                ("_CT_FLOW_BIN", flow_bin.display().to_string()),
                ("_CT_LOG_FILE", log_file.display().to_string()),
                (
                    "_CT_MONITOR_LOG_FILE",
                    monitor_log_file.display().to_string(),
                ),
                ("_CT_TEMP_DIR", tmp_parent.display().to_string()),
                (
                    "_CT_SAVED_STATE",
                    if saved_state { "1" } else { "0" }.to_owned(),
                ),
                (
                    "_CT_FLOWLIB",
                    if use_prelude {
                        " --builtin-lib prelude"
                    } else {
                        ""
                    }
                    .to_owned(),
                ),
                ("_CT_WAIT_FOR_RECHECK", config.wait_for_recheck.clone()),
                ("_CT_FILE_WATCHER", config.file_watcher.clone()),
                ("_CT_THIS_DIR", scripts_dir.display().to_string()),
                ("_CT_ERR_FILE", err_file.display().to_string()),
                ("_CT_START_ARGS", config.start_args.clone()),
                (
                    "_CT_AUTO_START",
                    if config.auto_start { "1" } else { "0" }.to_owned(),
                ), // JS: _CT_AUTO_START: config.auto_start ? '1' : '0',
                ("_CT_SCRIPT_PATH", script_path.display().to_string()), // JS: _CT_SCRIPT_PATH: scriptPath,
            ] {
                script_env.insert(key.to_owned(), value); // JS: const scriptEnv: {[string]: string | void} = {...bashEnv, ...};
            }

            // Build a bash command that includes all helper functions and variables
            // matching what run-one-test provides.
            // All dynamic values are read from environment variables set above
            // to avoid shell injection issues with paths containing spaces.
            let args = vec!["-c".to_owned(), bash_test_prelude.to_owned()];
            let command_result = exec_file(
                "bash",
                &args,
                &ExecOptions {
                    cwd: Some(work_dir.clone()),
                    env: Some(script_env),
                    ..ExecOptions::default()
                },
                None,
            )?;
            // The bash script handles its own server start/stop via the
            // inline `"$FLOW" stop . ...` at the end, so the server is
            // already stopped when we get here — no safety cleanup needed.
            server_started = false;
            out_content = command_result.stdout;
            if config.ignore_stderr {
                err_content = command_result.stderr;
            } else {
                out_content.push_str(&command_result.stderr);
            }
            if command_result.code != 0 {
                // When auto-start fails, the inline bash script already prints
                // "flow start exited code N" and exits — the original bash
                // runner skips the test script entirely and does NOT print an
                // additional shell exit line.  But when the test script itself
                // fails, the original bash runner DOES print
                // "<shell> exited code N". Distinguish the two cases by
                // checking whether stdout already contains the start-failure
                // message.
                if !out_content.contains("flow start exited code") {
                    // JS: if (!/flow start exited code/.test(outContent)) {
                    out_content.push_str(&format!(
                        "{} exited code {}\n",
                        config.shell, command_result.code
                    ));
                }
                return_status = TestStatus::Error;
            }
        } else {
            // General cmd mode
            if config.auto_start {
                let mut start_args = vec!["start".to_owned(), ".".to_owned()]; // JS: const startArgs = ['start', '.'];
                if use_prelude {
                    start_args.extend(["--builtin-lib".to_owned(), "prelude".to_owned()]);
                }
                start_args.push("--wait".to_owned());
                if saved_state {
                    // Handle saved state start
                    let extra_start_args = split_shell_args(&config.start_args)?;
                    let mut flowconfig_name = ".flowconfig";
                    for pair in extra_start_args.windows(2) {
                        if pair[0] == "--flowconfig-name" {
                            flowconfig_name = &pair[1];
                        }
                    }
                    // Create saved state first
                    let ctx = TestContext::new(TestContextOptions {
                        flow_bin: &flow_bin,
                        test_dir: &work_dir,
                        log_file: &log_file,
                        monitor_log_file: &monitor_log_file,
                        use_prelude,
                        wait_for_recheck: &config.wait_for_recheck,
                        file_watcher: &config.file_watcher,
                        env: &env,
                    });
                    let ok = ctx.create_saved_state(&work_dir, flowconfig_name);
                    if !ok {
                        out_content = "Failed to generate saved state\n".to_owned();
                        return_status = TestStatus::Error;
                    } else {
                        start_args.extend([
                            "--wait-for-recheck".to_owned(),
                            config.wait_for_recheck.clone(),
                            "--saved-state-fetcher".to_owned(),
                            "local".to_owned(),
                            "--saved-state-no-fallback".to_owned(),
                            "--file-watcher".to_owned(),
                            config.file_watcher.clone(),
                            "--log-file".to_owned(),
                            log_file.display().to_string(),
                            "--monitor-log-file".to_owned(),
                            monitor_log_file.display().to_string(),
                        ]);
                        start_args.extend(extra_start_args);
                        let start_result = exec_file(
                            &flow_bin.to_string_lossy(),
                            &start_args,
                            &ExecOptions {
                                cwd: Some(work_dir.clone()),
                                env: Some(env.clone()),
                                ..ExecOptions::default()
                            },
                            None,
                        )?;
                        err_content.push_str(&start_result.stderr);
                        if start_result.code != 0 {
                            out_content = format!("flow start exited code {}\n", start_result.code);
                            return_status = TestStatus::Error;
                        } else {
                            server_started = true;
                        }
                    }
                } else {
                    let extra_start_args = split_shell_args(&config.start_args)?;
                    start_args.extend([
                        "--wait-for-recheck".to_owned(),
                        config.wait_for_recheck.clone(),
                        "--file-watcher".to_owned(),
                        config.file_watcher.clone(),
                        "--log-file".to_owned(),
                        log_file.display().to_string(),
                        "--monitor-log-file".to_owned(),
                        monitor_log_file.display().to_string(),
                    ]);
                    start_args.extend(extra_start_args);
                    let start_result = exec_file(
                        &flow_bin.to_string_lossy(),
                        &start_args,
                        &ExecOptions {
                            cwd: Some(work_dir.clone()),
                            env: Some(env.clone()),
                            ..ExecOptions::default()
                        },
                        None,
                    )?;
                    if start_result.code != 0 {
                        out_content = format!("flow start exited code {}\n", start_result.code);
                        return_status = TestStatus::Error;
                    } else {
                        server_started = true;
                    }
                }
            }

            if return_status != TestStatus::Error {
                // Run the cmd.
                // The bash runner uses `eval "$FLOW $cmd"` which processes shell
                // constructs (pipes, redirections, etc.). Detect metacharacters
                // and use shell execution when needed; otherwise use the safer
                // execFile path.
                let cmd_trimmed = config.cmd.trim();
                let stdin_data = if config.stdin.is_empty() {
                    None
                } else {
                    Some(fs::read_to_string(work_dir.join(&config.stdin))?)
                };
                let shell_meta = Regex::new(r"[|;&<>()$`]").map_err(io::Error::other)?;
                let command_result = if shell_meta.is_match(cmd_trimmed) {
                    let quoted_flow = flow_bin.display().to_string().replace('"', "\\\"");
                    exec_shell(
                        &format!("\"{quoted_flow}\" {cmd_trimmed}"),
                        &ExecOptions {
                            cwd: Some(work_dir.clone()),
                            env: Some(env.clone()),
                            timeout: None,
                            max_buffer: None,
                        },
                        stdin_data.as_deref(),
                    )?
                } else {
                    exec_file(
                        &flow_bin.to_string_lossy(),
                        &split_shell_args(cmd_trimmed)?,
                        &ExecOptions {
                            cwd: Some(work_dir.clone()),
                            env: Some(env.clone()),
                            ..ExecOptions::default()
                        },
                        stdin_data.as_deref(),
                    )?
                };
                out_content = command_result.stdout;
                if config.ignore_stderr {
                    err_content.push_str(&command_result.stderr);
                } else {
                    out_content.push_str(&command_result.stderr);
                }

                // Stop server if one was started
                if server_started {
                    let stop_result = exec_file(
                        &flow_bin.to_string_lossy(),
                        &["stop".to_owned(), ".".to_owned()],
                        &ExecOptions {
                            cwd: Some(work_dir.clone()),
                            env: Some(env.clone()),
                            ..ExecOptions::default()
                        },
                        None,
                    )?;
                    if stop_result.code == 0 {
                        server_started = false;
                    }
                }
            }
        }

        // Write output files
        fs::write(&out_file, out_content)?;
        if !err_content.is_empty() {
            // Use appendFileSync because in bash script mode, the inline script
            // may have already written server stderr to errFile via redirect
            // (2>> "$_CT_ERR_FILE"). writeFileSync would overwrite that output.
            use std::io::Write;
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&err_file)?;
            file.write_all(err_content.as_bytes())?;
        }
        // Diff output
        if return_status == TestStatus::Success {
            let diff = diff_output(&exp_dst, &out_file, &version)?;
            if !diff.is_empty() {
                fs::write(&diff_file, diff)?;
            }
        }

        // Handle results
        if return_status != TestStatus::Success {
            // Copy artifacts back to source dir for debugging
            copy_artifact(&out_file, &test_dir.join(format!("{name}.out")))?;
            copy_artifact(&log_file, &test_dir.join(format!("{name}.log")))?;
            copy_artifact(
                &monitor_log_file,
                &test_dir.join(format!("{name}.monitor_log")),
            )?;
            copy_artifact(&err_file, &test_dir.join(format!("{name}.err")))?;
            return Ok(TestResult {
                status: return_status,
                name,
                diff: None,
            });
        }
        if exists(&diff_file) {
            let diff_content = fs::read_to_string(&diff_file)?;
            if !diff_content.is_empty() {
                // Test failed - copy artifacts back
                copy_artifact(&out_file, &test_dir.join(format!("{name}.out")))?;
                copy_artifact(&log_file, &test_dir.join(format!("{name}.log")))?;
                copy_artifact(
                    &monitor_log_file,
                    &test_dir.join(format!("{name}.monitor_log")),
                )?;
                copy_artifact(&err_file, &test_dir.join(format!("{name}.err")))?;
                copy_artifact(&diff_file, &test_dir.join(format!("{name}.diff")))?;
                if record {
                    record_output(&out_file, &test_dir.join(exp_file_name), &version)?;
                    // Clean up after recording
                    clean_artifact(&test_dir.join(format!("{name}.out")))?;
                    clean_artifact(&test_dir.join(format!("{name}.err")))?;
                    clean_artifact(&test_dir.join(format!("{name}.diff")))?;
                }
                return Ok(TestResult {
                    status: TestStatus::Failure,
                    name: name.to_owned(),
                    diff: Some(diff_content),
                });
            }
        }

        clean_artifact(&test_dir.join(format!("{name}.out")))?;
        clean_artifact(&test_dir.join(format!("{name}.log")))?;
        clean_artifact(&test_dir.join(format!("{name}.monitor_log")))?;
        clean_artifact(&test_dir.join(format!("{name}.err")))?;
        clean_artifact(&test_dir.join(format!("{name}.diff")))?;
        Ok(TestResult {
            status: TestStatus::Success,
            name,
            diff: None,
        })
    })();

    // Clean up temp directory
    // Stop any running flow server in the working directory.
    // Only attempt this if a server was actually started, to avoid
    // spawning a useless process for every full-check test.
    if server_started {
        exec_file(
            &flow_bin.to_string_lossy(),
            &["stop".to_owned(), ".".to_owned()],
            &ExecOptions {
                cwd: Some(work_dir),
                env: Some(env),
                timeout: Some(Duration::from_secs(30)),
                ..ExecOptions::default()
            },
            None,
        )?;
    }
    if let Err(error) = fs::remove_dir_all(tmp_parent) {
        eprintln!("Failed to clean up test directory: {error}");
    }
    result
}

fn copy_artifact(src: &Path, dst: &Path) -> io::Result<()> {
    if exists(src) {
        let content = fs::read(src)?;
        if !content.is_empty() {
            fs::write(dst, content)?;
        } else {
            // JS: } else {
            // Source is empty — clean the destination to match bash's
            // `[ -s file ] && mv file dest || rm -f dest/file` pattern.
            clean_artifact(dst)?;
        }
    } else {
        clean_artifact(dst)?;
    }
    Ok(())
}

fn clean_artifact(path: &Path) -> io::Result<()> {
    if exists(path) {
        fs::remove_file(path)?;
    }
    Ok(())
}
