/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// OSS-facing facade for `FlowEventLogger`. In fbcode builds the public API is
// implemented by `flow_facebook_logging::flow_event_logger`, which mirrors
// `flow/src/facebook/logging/flowEventLogger.ml`. In OSS builds (where
// `crates/facebook/` is stripped by ShipIt) the `cfg(not(fbcode_build))`
// branch falls back to the in-file stub whose event-logging functions are
// no-ops while the context-setter helpers retain real bodies (mirroring the
// behaviour of `flow/src/stubs/flowEventLogger.ml`).
//
// NOTE on the facade shape (departs from the `flow_edenfs_watcher` "thin
// `pub use`" pattern): the FB-internal `flow_facebook_logging::flow_event_logger`
// surface — ported faithfully from `flowEventLogger.ml` — uses different
// function signatures and a different `Error` shape than the existing OSS
// stub. Existing callers across `flow_cli`, `flow_lsp`, `flow_server`,
// `flow_server_monitor`, and `flow_logging_utils` are written against the
// OSS-stub signatures. To keep the build green during the gradual migration
// (Step 5 of the logging port reconciles each caller against the FB-shaped
// API one site at a time), the FB arm of this facade exposes adapter
// functions whose signatures match the OSS stub but whose bodies translate
// into `flow_facebook_logging::flow_event_logger` calls. Once Step 5
// completes and every caller speaks the FB-shaped API directly, this facade
// can be slimmed down to the `pub use flow_facebook_logging::flow_event_logger::*`
// shape used by sibling facades.

const AGENT_INVOCATION_ID_ENV: &str = "META_3PAI_INVOCATION_ID";

/// Returns the ID for the current AI-agent invocation, when present.
pub fn agent_invocation_id() -> Option<String> {
    std::env::var(AGENT_INVOCATION_ID_ENV).ok()
}

#[cfg(not(fbcode_build))]
mod stub {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::sync::OnceLock;
    use std::time::SystemTime;

    use flow_common_exit_status::FlowExitStatus;

    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
    pub struct MonitorOptions {
        pub file_watcher: String,
        pub vcs: String,
    }

    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
    pub struct ServerOptions {
        pub lazy_mode: String,
        pub max_workers: i32,
        pub enabled_rollouts: Vec<String>,
        pub debug: bool,
        pub log_saving: BTreeMap<String, flow_common::options::LogSaving>,
        pub log_file: String,
    }

    // The logging context is a little bit of singleton state that we want each
    // Flow command to hold onto and log with each event. It will also be passed
    // from the client to the server and logged with server events.
    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
    pub struct LoggingContext {
        // The literal argv for this Flow command.
        pub argv: String,
        // The current working directory.
        pub cwd: String,
        // The Flow command (like check, status, etc).
        pub command: Option<String>,
        // Whether the root is an eden mount.
        pub eden: Option<bool>,
        // The --from flag that most commands should have.
        pub from: Option<String>,
        // The server monitor options.
        pub monitor_options: Option<MonitorOptions>,
        // The Flow root being operated upon. This is a path.
        pub root: Option<String>,
        // The name specified in the .flowconfig. This is some identifier.
        pub root_name: Option<String>,
        // The saved state which this server successfully loaded.
        pub saved_state_filename: Option<String>,
        // The server options.
        pub server_options: Option<ServerOptions>,
        // The time this command started.
        pub start_time: f64,
        // The AI-agent invocation ID, when present.
        pub agent_id: Option<String>,
        // The --reason flag that `agent-helper` commands require.
        pub reason: Option<String>,
    }

    // Persistent_context says what conditions were like when the message was received.
    #[derive(Clone, Debug)]
    pub struct PersistentContext {
        pub start_lsp_state: Option<String>,
        pub start_lsp_state_reason: Option<String>,
        pub start_server_status: Option<String>,
        pub start_watcher_status: Option<String>,
    }

    // Persistent_delay is an explanation of why a persistent command couldn't be
    // handled immediately: what was the server busy doing?
    #[derive(Clone, Debug)]
    pub struct PersistentDelay {
        pub init_duration: f64,
        pub command_count: i32,
        pub command_duration: f64,
        pub command_worst: Option<String>,
        pub command_worst_duration: Option<f64>,
        pub recheck_count: i32,
        pub recheck_dependent_files: i32,
        pub recheck_changed_files: i32,
        pub recheck_duration: f64,
        pub recheck_worst_duration: Option<f64>,
        pub recheck_worst_dependent_file_count: Option<i32>,
        pub recheck_worst_changed_file_count: Option<i32>,
        pub recheck_worst_cycle_leader: Option<String>,
        pub recheck_worst_cycle_size: Option<i32>,
    }

    pub type Error = (String, String);

    fn logging_enabled() -> &'static Mutex<bool> {
        static LOGGING_ENABLED: OnceLock<Mutex<bool>> = OnceLock::new();
        LOGGING_ENABLED.get_or_init(|| Mutex::new(true))
    }

    pub fn disable_logging() {
        *logging_enabled()
            .lock()
            .expect("flow_event_logger logging_enabled mutex poisoned") = false;
    }

    fn context() -> &'static Mutex<LoggingContext> {
        static CONTEXT: OnceLock<Mutex<LoggingContext>> = OnceLock::new();
        CONTEXT.get_or_init(|| {
            let argv = std::env::args().fold(String::new(), |acc, arg| {
                if acc.is_empty() {
                    arg
                } else {
                    acc + " " + &arg
                }
            });
            let cwd = std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let start_time = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64();
            Mutex::new(LoggingContext {
                argv,
                cwd,
                command: None,
                eden: None,
                from: None,
                root: None,
                root_name: None,
                saved_state_filename: None,
                start_time,
                monitor_options: None,
                server_options: None,
                agent_id: None,
                reason: None,
            })
        })
    }

    pub fn get_context() -> LoggingContext {
        context()
            .lock()
            .expect("flow_event_logger context mutex poisoned")
            .clone()
    }

    // If you are using the logging context for application logic then you are a clown.
    // Shame. Shame on us all.
    #[expect(
        non_snake_case,
        reason = "preserves OCaml `get_from_I_AM_A_CLOWN` identifier name"
    )]
    pub fn get_from_I_AM_A_CLOWN() -> Option<String> {
        context()
            .lock()
            .expect("flow_event_logger context mutex poisoned")
            .from
            .clone()
    }

    pub fn get_from_i_am_a_clown() -> Option<String> {
        get_from_I_AM_A_CLOWN()
    }

    pub fn restore_context(restored_context: LoggingContext) {
        // This server might have been started by a monitor process which is already pretty old,
        // so the start_time might be way out of date.
        let start_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        *context()
            .lock()
            .expect("flow_event_logger context mutex poisoned") = LoggingContext {
            start_time,
            ..restored_context
        };
    }

    pub fn set_command(command: Option<String>) {
        context()
            .lock()
            .expect("flow_event_logger context mutex poisoned")
            .command = command;
    }

    pub fn set_eden(eden: Option<bool>) {
        context()
            .lock()
            .expect("flow_event_logger context mutex poisoned")
            .eden = eden;
    }

    pub fn set_from(from: Option<String>) {
        context()
            .lock()
            .expect("flow_event_logger context mutex poisoned")
            .from = from;
    }

    pub fn set_agent_id(agent_id: Option<String>) {
        context()
            .lock()
            .expect("flow_event_logger context mutex poisoned")
            .agent_id = agent_id;
    }

    pub fn set_agent_id_from_client_context(client_context: &LoggingContext) {
        context()
            .lock()
            .expect("flow_event_logger context mutex poisoned")
            .agent_id = client_context.agent_id.clone();
    }

    pub fn set_reason(reason: Option<String>) {
        context()
            .lock()
            .expect("flow_event_logger context mutex poisoned")
            .reason = reason;
    }

    pub fn set_root(root: Option<String>) {
        context()
            .lock()
            .expect("flow_event_logger context mutex poisoned")
            .root = root;
    }

    pub fn set_root_name(root_name: Option<String>) {
        context()
            .lock()
            .expect("flow_event_logger context mutex poisoned")
            .root_name = root_name;
    }

    pub fn set_server_config(
        _tls: Option<flow_common::options::LogSaving>,
        _flowconfig_name: &str,
        root: PathBuf,
        root_name: Option<&str>,
    ) {
        let mut context = context()
            .lock()
            .expect("flow_event_logger context mutex poisoned");
        context.root = Some(root.to_string_lossy().into_owned());
        context.root_name = root_name.map(ToOwned::to_owned);
    }

    pub fn set_saved_state_filename(fn_: String) {
        context()
            .lock()
            .expect("flow_event_logger context mutex poisoned")
            .saved_state_filename = Some(fn_);
    }

    pub fn set_monitor_options(file_watcher: String, vcs: String) {
        let monitor_options = Some(MonitorOptions { file_watcher, vcs });
        context()
            .lock()
            .expect("flow_event_logger context mutex poisoned")
            .monitor_options = monitor_options;
    }

    pub fn set_server_options(
        lazy_mode: &str,
        max_workers: i32,
        enabled_rollouts: &BTreeMap<String, String>,
        debug: bool,
        log_saving: &BTreeMap<String, flow_common::options::LogSaving>,
        log_file: &str,
    ) {
        let enabled_rollouts: Vec<String> = enabled_rollouts
            .iter()
            .map(|(rollout, group)| format!("{}={}", rollout, group))
            .collect();
        let server_options = Some(ServerOptions {
            lazy_mode: lazy_mode.to_string(),
            max_workers,
            enabled_rollouts,
            debug,
            log_saving: log_saving.clone(),
            log_file: log_file.to_string(),
        });
        context()
            .lock()
            .expect("flow_event_logger context mutex poisoned")
            .server_options = server_options;
    }

    #[expect(
        non_camel_case_types,
        reason = "preserves OCaml SCREAMING_SNAKE_CASE event constructor names from flowEventLogger.ml"
    )]
    #[derive(Clone, Debug)]
    pub enum Event {
        // When the server finishes initializing.
        INIT_END,
        // When any flow command starts.
        INIT_START,
        // When the server loses the lock.
        LOCK_LOST,
        // When the server fails to reaquire the lock.
        LOCK_STOLEN,
        // When the server is out of date.
        OUT_OF_DATE,
        // The response of "flow status".
        STATUS_RESPONSE,
        // When a command exits.
        EXIT,
        // When the server finishes a recheck.
        RECHECK,
        // When a recheck is canceled.
        RECHECK_CANCELED,
        // When the server finishes a series of rechecks where some may have been canceled.
        RECHECK_SERIES,
        // When the server loads from a newer saved state instead of rechecking after a rebase.
        // Followed by a recheck.
        REINIT,
        REINIT_FULL_CHECK,
        // The oom killer killed the server.
        MURDERED_BY_OOM_KILLER,
        // A command from an ephemeral client finished successfully.
        EPHEMERAL_COMMAND_SUCCESS,
        // A command from an ephemeral client failed.
        EPHEMERAL_COMMAND_FAILURE,
        // A command from a persistent client finished successfully.
        PERSISTENT_COMMAND_SUCCESS,
        // A command from a persistent client failed.
        PERSISTENT_COMMAND_FAILURE,
        // A non-command-related expected error, e.g. server shuts down.
        PERSISTENT_EXPECTED_ERROR,
        // A non-command-related unhandled exception.
        PERSISTENT_UNEXPECTED_ERROR,
        // We successfully fetched a saved state.
        SAVED_STATE_FB_FETCHER_SUCCESS,
        // We failed to fetch a saved state.
        SAVED_STATE_FB_FETCHER_ERROR,
        // Finished loading saved state.
        LOAD_SAVED_STATE_SUCCESS,
        // Tried and failed to load saved state.
        LOAD_SAVED_STATE_ERROR,
        // The server has been idle (not handling requests or rechecks) for N seconds.
        IDLE_HEARTBEAT,
        // The Flow LSP sent the client the live parse errors.
        LIVE_PARSE_ERRORS,
        // The Flow LSP sent the client the live non-parse errors.
        LIVE_NON_PARSE_ERRORS,
        // The Flow LSP failed to send the client the live non-parse errors.
        LIVE_NON_PARSE_ERRORS_FAILED,
        // The file watcher notified us that an event started.
        FILE_WATCHER_EVENT_STARTED,
        // The file watcher notified us that an event finished.
        FILE_WATCHER_EVENT_FINISHED,
        // The file watcher encountered an uncaught failure.
        FILE_WATCHER_UNCAUGHT_FAILURE,
        WATCHMAN_ERROR,
        WATCHMAN_WARNING,
        WATCHMAN_UNCAUGHT_FAILURE,
        WATCHMAN_CONNECTION_REESTABLISHED,
        WATCHMAN_CONNECTION_REESTABLISHMENT_FAILED,
        SHAREDMEM_GC_RAN,
        SHAREDMEM_INIT_DONE,
        SHAREDMEM_FAILED_ANONYMOUS_MEMFD_INIT,
        WORKER_EXCEPTION,
        DFIND_READY,
        PARSING_EXCEPTION,
        EDENFS_WATCHER_FALLBACK,
        EDENFS_WATCHER_ERROR,
        EDENFS_WATCHER_LOST_CHANGES,
        EDENFS_WATCHER_NON_EDEN_WWW,
    }

    pub fn string_of_event(event: &Event) -> &'static str {
        match event {
            Event::INIT_END => "INIT_END",
            Event::INIT_START => "INIT_START",
            Event::LOCK_LOST => "LOCK_LOST",
            Event::LOCK_STOLEN => "LOCK_STOLEN",
            Event::OUT_OF_DATE => "OUT_OF_DATE",
            Event::STATUS_RESPONSE => "STATUS_RESPONSE",
            Event::EXIT => "EXIT",
            Event::RECHECK => "RECHECK",
            Event::RECHECK_CANCELED => "RECHECK_CANCELED",
            Event::RECHECK_SERIES => "RECHECK_SERIES",
            Event::REINIT => "REINIT",
            Event::REINIT_FULL_CHECK => "REINIT_FULL_CHECK",
            Event::MURDERED_BY_OOM_KILLER => "MURDERED_BY_OOM_KILLER",
            Event::EPHEMERAL_COMMAND_SUCCESS => "EPHEMERAL_COMMAND_SUCCESS",
            Event::EPHEMERAL_COMMAND_FAILURE => "EPHEMERAL_COMMAND_FAILURE",
            Event::PERSISTENT_COMMAND_SUCCESS => "PERSISTENT_COMMAND_SUCCESS",
            Event::PERSISTENT_COMMAND_FAILURE => "PERSISTENT_COMMAND_FAILURE",
            Event::PERSISTENT_EXPECTED_ERROR => "PERSISTENT_EXPECTED_ERROR",
            Event::PERSISTENT_UNEXPECTED_ERROR => "PERSISTENT_UNEXPECTED_ERROR",
            Event::SAVED_STATE_FB_FETCHER_SUCCESS => "SAVED_STATE_FB_FETCHER_SUCCESS",
            Event::SAVED_STATE_FB_FETCHER_ERROR => "SAVED_STATE_FB_FETCHER_ERROR",
            Event::LOAD_SAVED_STATE_SUCCESS => "LOAD_SAVED_STATE_SUCCESS",
            Event::LOAD_SAVED_STATE_ERROR => "LOAD_SAVED_STATE_ERROR",
            Event::IDLE_HEARTBEAT => "IDLE_HEARTBEAT",
            Event::LIVE_PARSE_ERRORS => "LIVE_PARSE_ERRORS",
            Event::LIVE_NON_PARSE_ERRORS => "LIVE_NON_PARSE_ERRORS",
            Event::LIVE_NON_PARSE_ERRORS_FAILED => "LIVE_NON_PARSE_ERRORS_FAILED",
            Event::FILE_WATCHER_EVENT_STARTED => "FILE_WATCHER_EVENT_STARTED",
            Event::FILE_WATCHER_EVENT_FINISHED => "FILE_WATCHER_EVENT_FINISHED",
            Event::FILE_WATCHER_UNCAUGHT_FAILURE => "FILE_WATCHER_UNCAUGHT_FAILURE",
            Event::WATCHMAN_ERROR => "WATCHMAN_ERROR",
            Event::WATCHMAN_WARNING => "WATCHMAN_WARNING",
            Event::WATCHMAN_UNCAUGHT_FAILURE => "WATCHMAN_UNCAUGHT_FAILURE",
            Event::WATCHMAN_CONNECTION_REESTABLISHED => "WATCHMAN_CONNECTION_REESTABLISHED",
            Event::WATCHMAN_CONNECTION_REESTABLISHMENT_FAILED => {
                "WATCHMAN_CONNECTION_REESTABLISHMENT_FAILED"
            }
            Event::SHAREDMEM_GC_RAN => "SHAREDMEM_GC_RAN",
            Event::SHAREDMEM_INIT_DONE => "SHAREDMEM_INIT_DONE",
            Event::SHAREDMEM_FAILED_ANONYMOUS_MEMFD_INIT => "SHAREDMEM_FAILED_ANONYMOUS_MEMFD_INIT",
            Event::WORKER_EXCEPTION => "WORKER_EXCEPTION",
            Event::DFIND_READY => "DFIND_READY",
            Event::PARSING_EXCEPTION => "PARSING_EXCEPTION",
            Event::EDENFS_WATCHER_FALLBACK => "EDENFS_WATCHER_FALLBACK",
            Event::EDENFS_WATCHER_ERROR => "EDENFS_WATCHER_ERROR",
            Event::EDENFS_WATCHER_LOST_CHANGES => "EDENFS_WATCHER_LOST_CHANGES",
            Event::EDENFS_WATCHER_NON_EDEN_WWW => "EDENFS_WATCHER_NON_EDEN_WWW",
        }
    }

    pub fn status_response(_num_errors: i32) {}

    pub fn init_done(
        _first_internal_error: Option<&str>,
        _init_trigger: &str,
        _saved_state_fetcher: &str,
        _profiling: &serde_json::Value,
    ) {
    }

    pub fn init_flow_command(_init_id: &str) {}

    pub fn init_flow_command_if_needed() {}

    pub fn init_worker(_init_id: &str, _arg: &serde_json::Value) {}

    pub fn register_scuba_entry() {}

    pub fn should_log() -> bool {
        *logging_enabled()
            .lock()
            .expect("flow_event_logger logging_enabled mutex poisoned")
    }

    pub fn lock_lost(_arg: &str) {}

    pub fn lock_stolen(_arg: &str) {}

    pub fn out_of_date() {}

    pub fn exit(_error: Option<&Error>, _msg: &str, _status: &str) {}

    pub fn log_exit(_status: FlowExitStatus, _msg: Option<&str>) {}

    pub fn report_from_monitor_server_exit_due_to_signal(_signal: i32) {}

    #[expect(
        clippy::too_many_arguments,
        reason = "matches OCaml signature; line-by-line port forbids bundling into a struct"
    )]
    pub fn recheck(
        _modified_count: i32,
        _deleted_count: i32,
        _merged_dependency_count: i32,
        _dependent_file_count: i32,
        _merge_skip_count: i32,
        _check_skip_count: i32,
        _profiling: &serde_json::Value,
        _time_to_resolve_all_type_errors: Option<f64>,
        _time_to_resolve_all_type_errors_in_one_file: Option<f64>,
        _time_to_resolve_all_subtyping_errors: Option<f64>,
        _time_to_resolve_all_subtyping_errors_in_one_file: Option<f64>,
        _rounds_to_resolve_all_type_errors: Option<u32>,
        _rounds_to_resolve_all_type_errors_in_one_file: Option<u32>,
        _rounds_to_resolve_all_subtyping_errors: Option<u32>,
        _rounds_to_resolve_all_subtyping_errors_in_one_file: Option<u32>,
        _first_internal_error: Option<&str>,
        _slowest_file: Option<&str>,
        _num_slow_files: Option<i32>,
        _scm_changed_mergebase: Option<bool>,
    ) {
    }

    pub fn log_typing_errors(_data: &serde_json::Value) {}

    pub fn recheck_canceled(
        _num_files_to_prioritize: i32,
        _num_files_to_recheck: i32,
        _num_files_to_force: i32,
    ) {
    }

    pub fn recheck_series(_recheck_count: i32, _profiling: &serde_json::Value) {}

    pub fn reinit(_reason: &str, _profiling: &serde_json::Value) {}

    pub fn global_lib_reinit(
        _changed_file_count: usize,
        _old_global_count: usize,
        _new_global_count: usize,
        _inventory_file_count: usize,
        _deleted_file_count: usize,
        _profiling: &serde_json::Value,
    ) {
    }

    pub fn reinit_full_check(_profiling: &serde_json::Value) {}

    pub fn murdered_by_oom_killer(_arg: &str) {}

    pub fn ephemeral_command_success(
        _json_data: &serde_json::Value,
        _client_context: &LoggingContext,
        _profiling: &serde_json::Value,
    ) {
    }

    pub fn ephemeral_command_failure(
        _json_data: &serde_json::Value,
        _client_context: &LoggingContext,
    ) {
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "matches OCaml signature; line-by-line port forbids bundling into a struct"
    )]
    pub fn persistent_command_success(
        _server_logging_context: Option<&LoggingContext>,
        _request_id: &str,
        _method_name: &str,
        _request: &str,
        _extra_data: &[(String, serde_json::Value)],
        _client_context: &LoggingContext,
        _persistent_context: Option<&PersistentContext>,
        _persistent_delay: Option<&PersistentDelay>,
        _server_profiling: Option<&serde_json::Value>,
        _client_duration: Option<f64>,
        _wall_start: f64,
        _activity_key: Option<&serde_json::Value>,
        _error: Option<&Error>,
    ) {
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "matches OCaml signature; line-by-line port forbids bundling into a struct"
    )]
    pub fn persistent_command_failure(
        _server_logging_context: Option<&LoggingContext>,
        _request: &str,
        _extra_data: &[(String, serde_json::Value)],
        _client_context: &LoggingContext,
        _persistent_context: Option<&PersistentContext>,
        _persistent_delay: Option<&PersistentDelay>,
        _server_profiling: Option<&serde_json::Value>,
        _client_duration: Option<f64>,
        _wall_start: f64,
        _activity_key: Option<&serde_json::Value>,
        _error: &Error,
    ) {
    }

    pub fn persistent_expected_error(
        _request: Option<&str>,
        _client_context: &LoggingContext,
        _activity_key: Option<&serde_json::Value>,
        _error: &Error,
    ) {
    }

    pub fn persistent_unexpected_error(
        _request: Option<&str>,
        _client_context: &LoggingContext,
        _activity_key: Option<&serde_json::Value>,
        _error: &Error,
    ) {
    }

    pub fn saved_state_fb_fetcher_success(
        _repo_root: &str,
        _merge_base_hash: &str,
        _merge_base_timestamp: f64,
        _saved_state_hash: &str,
        _changed_files_count: i32,
        _saved_state_filename: &str,
        _profiling: &serde_json::Value,
    ) {
    }

    pub fn saved_state_fb_fetcher_error(_step: &str, _trace: &str, _profiling: &serde_json::Value) {
    }

    pub fn load_saved_state_success(_changed_files_count: i32) {}

    pub fn load_saved_state_error(
        _saved_state_filename: &str,
        _changed_files_count: i32,
        _invalid_reason: &str,
        _trace: &str,
    ) {
    }

    pub fn idle_heartbeat(_idle_time: f64, _profiling: &serde_json::Value) {}

    pub fn live_parse_errors(_request: &str, _data: &str, _wall_start: f64) {}

    pub fn live_non_parse_errors(_request: &str, _data: &str, _wall_start: f64) {}

    pub fn live_non_parse_errors_failed(_request: &str, _data: &str, _wall_start: f64) {}

    pub fn file_watcher_event_started(_name: &str, _data: &str) {}

    pub fn file_watcher_event_finished(_name: &str, _data: &str) {}

    pub fn file_watcher_uncaught_failure(_msg: &str) {}

    pub fn watchman_error(_request: Option<&str>, _response: Option<&str>, _msg: &str) {}

    pub fn watchman_warning(_warning: &str) {}

    pub fn watchman_uncaught_failure(_msg: &str) {}

    pub fn watchman_connection_reestablished(_downtime: f64) {}

    pub fn watchman_connection_reestablishment_failed(_msg: &str) {}

    pub fn sharedmem_gc_ran(_a: &str, _b: f64, _c: f64, _d: f64) {}

    pub fn sharedmem_init_done(_size: u64) {}

    pub fn sharedmem_failed_memfd_init(_msg: &str) {}

    pub fn worker_exception(_msg: &str) {}

    pub fn dfind_ready(_a: &str, _b: f64) {}

    pub fn parsing_exception(_msg: &str) {}

    pub fn set_file_watcher(_name: &str) {}

    pub fn edenfs_watcher_fallback(_msg: &str) {}

    pub fn edenfs_watcher_non_eden_www(_backtrace: &str) {}

    pub fn edenfs_watcher_error(_msg: &str, _backtrace: &str) {}

    pub fn edenfs_watcher_lost_changes(_msg: &str, _backtrace: &str) {}
}

#[cfg(not(fbcode_build))]
pub use stub::*;

// In fbcode the facade re-exports the OSS-shaped public API by adapting it to
// the FB-internal `flow_facebook_logging::flow_event_logger` surface. Types
// whose shapes diverge between OSS and FB are duplicated locally inside
// `fb_facade` so callers see the OSS-shaped API in both arms; the adapter
// functions translate to the FB-shaped types/calls underneath.
#[cfg(fbcode_build)]
mod fb_facade {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::sync::OnceLock;
    use std::time::SystemTime;

    use flow_common_exit_status::FlowExitStatus;
    use flow_facebook_logging::flow_event_logger as fb;
    use flow_facebook_logging::scuba;

    // Local OSS-shaped types. The shapes intentionally match the OSS stub
    // (serde derives, OSS field set) so callers — written against the OSS
    // facade — compile unchanged in both arms. Step 5 of the logging port
    // will migrate callers to use the FB-internal types directly, after
    // which this duplication can go away.

    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
    pub struct MonitorOptions {
        pub file_watcher: String,
        pub vcs: String,
    }

    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
    pub struct ServerOptions {
        pub lazy_mode: String,
        pub max_workers: i32,
        pub enabled_rollouts: Vec<String>,
        pub debug: bool,
        pub log_saving: BTreeMap<String, flow_common::options::LogSaving>,
        pub log_file: String,
    }

    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
    pub struct LoggingContext {
        pub argv: String,
        pub cwd: String,
        pub command: Option<String>,
        pub eden: Option<bool>,
        pub from: Option<String>,
        pub monitor_options: Option<MonitorOptions>,
        pub root: Option<String>,
        pub root_name: Option<String>,
        pub saved_state_filename: Option<String>,
        pub server_options: Option<ServerOptions>,
        pub start_time: f64,
        pub agent_id: Option<String>,
        pub reason: Option<String>,
    }

    #[derive(Clone, Debug)]
    pub struct PersistentContext {
        pub start_lsp_state: Option<String>,
        pub start_lsp_state_reason: Option<String>,
        pub start_server_status: Option<String>,
        pub start_watcher_status: Option<String>,
    }

    #[derive(Clone, Debug)]
    pub struct PersistentDelay {
        pub init_duration: f64,
        pub command_count: i32,
        pub command_duration: f64,
        pub command_worst: Option<String>,
        pub command_worst_duration: Option<f64>,
        pub recheck_count: i32,
        pub recheck_dependent_files: i32,
        pub recheck_changed_files: i32,
        pub recheck_duration: f64,
        pub recheck_worst_duration: Option<f64>,
        pub recheck_worst_dependent_file_count: Option<i32>,
        pub recheck_worst_changed_file_count: Option<i32>,
        pub recheck_worst_cycle_leader: Option<String>,
        pub recheck_worst_cycle_size: Option<i32>,
    }

    pub type Error = (String, String);

    #[expect(
        non_camel_case_types,
        reason = "preserves OCaml SCREAMING_SNAKE_CASE event constructor names from flowEventLogger.ml"
    )]
    #[derive(Clone, Debug)]
    pub enum Event {
        INIT_END,
        INIT_START,
        LOCK_LOST,
        LOCK_STOLEN,
        OUT_OF_DATE,
        STATUS_RESPONSE,
        EXIT,
        RECHECK,
        RECHECK_CANCELED,
        RECHECK_SERIES,
        REINIT,
        REINIT_FULL_CHECK,
        MURDERED_BY_OOM_KILLER,
        EPHEMERAL_COMMAND_SUCCESS,
        EPHEMERAL_COMMAND_FAILURE,
        PERSISTENT_COMMAND_SUCCESS,
        PERSISTENT_COMMAND_FAILURE,
        PERSISTENT_EXPECTED_ERROR,
        PERSISTENT_UNEXPECTED_ERROR,
        SAVED_STATE_FB_FETCHER_SUCCESS,
        SAVED_STATE_FB_FETCHER_ERROR,
        LOAD_SAVED_STATE_SUCCESS,
        LOAD_SAVED_STATE_ERROR,
        IDLE_HEARTBEAT,
        LIVE_PARSE_ERRORS,
        LIVE_NON_PARSE_ERRORS,
        LIVE_NON_PARSE_ERRORS_FAILED,
        FILE_WATCHER_EVENT_STARTED,
        FILE_WATCHER_EVENT_FINISHED,
        FILE_WATCHER_UNCAUGHT_FAILURE,
        WATCHMAN_ERROR,
        WATCHMAN_WARNING,
        WATCHMAN_UNCAUGHT_FAILURE,
        WATCHMAN_CONNECTION_REESTABLISHED,
        WATCHMAN_CONNECTION_REESTABLISHMENT_FAILED,
        SHAREDMEM_GC_RAN,
        SHAREDMEM_INIT_DONE,
        SHAREDMEM_FAILED_ANONYMOUS_MEMFD_INIT,
        WORKER_EXCEPTION,
        DFIND_READY,
        PARSING_EXCEPTION,
        EDENFS_WATCHER_FALLBACK,
        EDENFS_WATCHER_ERROR,
        EDENFS_WATCHER_LOST_CHANGES,
        EDENFS_WATCHER_NON_EDEN_WWW,
    }

    pub fn string_of_event(event: &Event) -> &'static str {
        match event {
            Event::INIT_END => "INIT_END",
            Event::INIT_START => "INIT_START",
            Event::LOCK_LOST => "LOCK_LOST",
            Event::LOCK_STOLEN => "LOCK_STOLEN",
            Event::OUT_OF_DATE => "OUT_OF_DATE",
            Event::STATUS_RESPONSE => "STATUS_RESPONSE",
            Event::EXIT => "EXIT",
            Event::RECHECK => "RECHECK",
            Event::RECHECK_CANCELED => "RECHECK_CANCELED",
            Event::RECHECK_SERIES => "RECHECK_SERIES",
            Event::REINIT => "REINIT",
            Event::REINIT_FULL_CHECK => "REINIT_FULL_CHECK",
            Event::MURDERED_BY_OOM_KILLER => "MURDERED_BY_OOM_KILLER",
            Event::EPHEMERAL_COMMAND_SUCCESS => "EPHEMERAL_COMMAND_SUCCESS",
            Event::EPHEMERAL_COMMAND_FAILURE => "EPHEMERAL_COMMAND_FAILURE",
            Event::PERSISTENT_COMMAND_SUCCESS => "PERSISTENT_COMMAND_SUCCESS",
            Event::PERSISTENT_COMMAND_FAILURE => "PERSISTENT_COMMAND_FAILURE",
            Event::PERSISTENT_EXPECTED_ERROR => "PERSISTENT_EXPECTED_ERROR",
            Event::PERSISTENT_UNEXPECTED_ERROR => "PERSISTENT_UNEXPECTED_ERROR",
            Event::SAVED_STATE_FB_FETCHER_SUCCESS => "SAVED_STATE_FB_FETCHER_SUCCESS",
            Event::SAVED_STATE_FB_FETCHER_ERROR => "SAVED_STATE_FB_FETCHER_ERROR",
            Event::LOAD_SAVED_STATE_SUCCESS => "LOAD_SAVED_STATE_SUCCESS",
            Event::LOAD_SAVED_STATE_ERROR => "LOAD_SAVED_STATE_ERROR",
            Event::IDLE_HEARTBEAT => "IDLE_HEARTBEAT",
            Event::LIVE_PARSE_ERRORS => "LIVE_PARSE_ERRORS",
            Event::LIVE_NON_PARSE_ERRORS => "LIVE_NON_PARSE_ERRORS",
            Event::LIVE_NON_PARSE_ERRORS_FAILED => "LIVE_NON_PARSE_ERRORS_FAILED",
            Event::FILE_WATCHER_EVENT_STARTED => "FILE_WATCHER_EVENT_STARTED",
            Event::FILE_WATCHER_EVENT_FINISHED => "FILE_WATCHER_EVENT_FINISHED",
            Event::FILE_WATCHER_UNCAUGHT_FAILURE => "FILE_WATCHER_UNCAUGHT_FAILURE",
            Event::WATCHMAN_ERROR => "WATCHMAN_ERROR",
            Event::WATCHMAN_WARNING => "WATCHMAN_WARNING",
            Event::WATCHMAN_UNCAUGHT_FAILURE => "WATCHMAN_UNCAUGHT_FAILURE",
            Event::WATCHMAN_CONNECTION_REESTABLISHED => "WATCHMAN_CONNECTION_REESTABLISHED",
            Event::WATCHMAN_CONNECTION_REESTABLISHMENT_FAILED => {
                "WATCHMAN_CONNECTION_REESTABLISHMENT_FAILED"
            }
            Event::SHAREDMEM_GC_RAN => "SHAREDMEM_GC_RAN",
            Event::SHAREDMEM_INIT_DONE => "SHAREDMEM_INIT_DONE",
            Event::SHAREDMEM_FAILED_ANONYMOUS_MEMFD_INIT => "SHAREDMEM_FAILED_ANONYMOUS_MEMFD_INIT",
            Event::WORKER_EXCEPTION => "WORKER_EXCEPTION",
            Event::DFIND_READY => "DFIND_READY",
            Event::PARSING_EXCEPTION => "PARSING_EXCEPTION",
            Event::EDENFS_WATCHER_FALLBACK => "EDENFS_WATCHER_FALLBACK",
            Event::EDENFS_WATCHER_ERROR => "EDENFS_WATCHER_ERROR",
            Event::EDENFS_WATCHER_LOST_CHANGES => "EDENFS_WATCHER_LOST_CHANGES",
            Event::EDENFS_WATCHER_NON_EDEN_WWW => "EDENFS_WATCHER_NON_EDEN_WWW",
        }
    }

    // Convert OSS-shaped `LoggingContext` into the FB-internal
    // `fb::LoggingContext` (same fields, different derives).
    fn to_fb_logging_context(ctx: LoggingContext) -> fb::LoggingContext {
        fb::LoggingContext {
            argv: ctx.argv,
            cwd: ctx.cwd,
            command: ctx.command,
            eden: ctx.eden,
            from: ctx.from,
            monitor_options: ctx.monitor_options.map(|m| fb::MonitorOptions {
                file_watcher: m.file_watcher,
                vcs: m.vcs,
            }),
            root: ctx.root,
            root_name: ctx.root_name,
            saved_state_filename: ctx.saved_state_filename,
            server_options: ctx.server_options.map(|s| fb::ServerOptions {
                lazy_mode: s.lazy_mode,
                max_workers: s.max_workers,
                enabled_rollouts: s.enabled_rollouts,
                debug: s.debug,
                log_saving: s.log_saving,
                log_file: s.log_file,
            }),
            start_time: ctx.start_time,
            agent_id: ctx.agent_id,
            reason: ctx.reason,
        }
    }

    fn from_fb_logging_context(ctx: fb::LoggingContext) -> LoggingContext {
        LoggingContext {
            argv: ctx.argv,
            cwd: ctx.cwd,
            command: ctx.command,
            eden: ctx.eden,
            from: ctx.from,
            monitor_options: ctx.monitor_options.map(|m| MonitorOptions {
                file_watcher: m.file_watcher,
                vcs: m.vcs,
            }),
            root: ctx.root,
            root_name: ctx.root_name,
            saved_state_filename: ctx.saved_state_filename,
            server_options: ctx.server_options.map(|s| ServerOptions {
                lazy_mode: s.lazy_mode,
                max_workers: s.max_workers,
                enabled_rollouts: s.enabled_rollouts,
                debug: s.debug,
                log_saving: s.log_saving,
                log_file: s.log_file,
            }),
            start_time: ctx.start_time,
            agent_id: ctx.agent_id,
            reason: ctx.reason,
        }
    }

    fn to_fb_error(err: &Error) -> fb::Error {
        let (msg, stack) = err;
        fb::Error(msg.clone(), fb::Callstack(stack.clone()))
    }

    // The OSS facade exposes `should_log` whose behaviour is gated on a local
    // `logging_enabled` flag. Mirror that flag here so `disable_logging` and
    // `should_log` interact correctly even when callers haven't yet been
    // migrated to the FB-internal `event_logger::should_log` surface.
    fn logging_enabled() -> &'static Mutex<bool> {
        static LOGGING_ENABLED: OnceLock<Mutex<bool>> = OnceLock::new();
        LOGGING_ENABLED.get_or_init(|| Mutex::new(true))
    }

    // SHIM: clears the local OSS-style flag and forwards to FB-internal disable_logging.
    pub fn disable_logging() {
        *logging_enabled()
            .lock()
            .expect("flow_event_logger logging_enabled mutex poisoned") = false;
        fb::disable_logging();
    }

    // SHIM: fetches FB-internal LoggingContext and converts it to the OSS-shaped struct.
    pub fn get_context() -> LoggingContext {
        from_fb_logging_context(fb::get_context())
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    #[expect(
        non_snake_case,
        reason = "preserves OCaml identifier casing from flowEventLogger.ml"
    )]
    pub fn get_from_I_AM_A_CLOWN() -> Option<String> {
        fb::get_from_I_AM_A_CLOWN()
    }

    // SHIM: lowercase alias retained for OSS callers; not exposed on FB-internal side.
    pub fn get_from_i_am_a_clown() -> Option<String> {
        get_from_I_AM_A_CLOWN()
    }

    // SHIM: converts OSS-shaped LoggingContext into FB-internal shape before forwarding.
    pub fn restore_context(restored_context: LoggingContext) {
        fb::restore_context(to_fb_logging_context(restored_context));
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn set_command(command: Option<String>) {
        fb::set_command(command);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn set_eden(eden: Option<bool>) {
        fb::set_eden(eden);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn set_from(from: Option<String>) {
        fb::set_from(from);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn set_agent_id(agent_id: Option<String>) {
        fb::set_agent_id(agent_id);
    }

    // SHIM: clones the OSS-shaped LoggingContext and converts it to FB-internal shape.
    pub fn set_agent_id_from_client_context(client_context: &LoggingContext) {
        let fb_ctx = to_fb_logging_context(client_context.clone());
        fb::set_agent_id_from_client_context(&fb_ctx);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn set_reason(reason: Option<String>) {
        fb::set_reason(reason);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn set_root(root: Option<String>) {
        fb::set_root(root);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn set_root_name(root_name: Option<String>) {
        fb::set_root_name(root_name);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn set_server_config(
        tls: Option<flow_common::options::LogSaving>,
        flowconfig_name: &str,
        root: PathBuf,
        root_name: Option<&str>,
    ) {
        fb::set_server_config(tls, flowconfig_name, root, root_name);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn set_saved_state_filename(fn_: String) {
        fb::set_saved_state_filename(fn_);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn set_monitor_options(file_watcher: String, vcs: String) {
        fb::set_monitor_options(file_watcher, vcs);
    }

    // SHIM: clones &str/&BTreeMap into owned values FB-internal expects.
    pub fn set_server_options(
        lazy_mode: &str,
        max_workers: i32,
        enabled_rollouts: &BTreeMap<String, String>,
        debug: bool,
        log_saving: &BTreeMap<String, flow_common::options::LogSaving>,
        log_file: &str,
    ) {
        fb::set_server_options(
            lazy_mode.to_string(),
            max_workers,
            enabled_rollouts.clone(),
            debug,
            log_saving.clone(),
            log_file.to_string(),
        );
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn status_response(num_errors: i32) {
        fb::status_response(num_errors);
    }

    // SHIM: clones first_internal_error from &str to String so it matches the
    // FB-internal signature. Argument order and remaining types align.
    pub fn init_done(
        first_internal_error: Option<&str>,
        init_trigger: &str,
        saved_state_fetcher: &str,
        profiling: &serde_json::Value,
    ) {
        fb::init_done(
            first_internal_error.map(|s| s.to_string()),
            init_trigger,
            saved_state_fetcher,
            profiling,
        );
    }

    // SHIM: clones init_id from &str to the String FB-internal expects.
    pub fn init_flow_command(init_id: &str) {
        fb::init_flow_command(init_id.to_string());
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn init_flow_command_if_needed() {
        fb::init_flow_command_if_needed();
    }

    // SHIM: discards the OSS-stub's unused `_arg: &serde_json::Value` parameter
    // and synthesises a `time: f64` from `SystemTime::now()`. FB-internal
    // expects `(init_id: String, time: f64)`. This produces a slightly drifted
    // worker init time relative to the real start; Step 5's caller migration
    // will route the actual time directly to the FB-internal API and remove
    // this approximation.
    pub fn init_worker(init_id: &str, _arg: &serde_json::Value) {
        let time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        fb::init_worker(init_id.to_string(), time);
    }

    pub fn register_scuba_entry() {
        scuba::table::of_name("flow_server_events");
        scuba::table::of_name("flow_typing_errors");
        scuba::table::of_name("flow_interactions");
        scuba::entry();
    }

    // SHIM: ANDs the local OSS-style flag with the FB-internal should_log so
    // the OSS-arm's `disable_logging` semantics keep working alongside FB's.
    pub fn should_log() -> bool {
        *logging_enabled()
            .lock()
            .expect("flow_event_logger logging_enabled mutex poisoned")
            && fb::should_log()
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn lock_lost(arg: &str) {
        fb::lock_lost(arg);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn lock_stolen(arg: &str) {
        fb::lock_stolen(arg);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn out_of_date() {
        fb::out_of_date();
    }

    // SHIM: maps OSS `(Option<&Error>, &str, &str)` to FB
    // `(Option<&fb::Error>, Option<String>, &str)`. The status is already the
    // symbolic string for the FB exit_status column; an empty `msg` becomes
    // `None`.
    pub fn exit(error: Option<&Error>, msg: &str, status: &str) {
        let fb_err = error.map(to_fb_error);
        let msg_owned = if msg.is_empty() {
            None
        } else {
            Some(msg.to_string())
        };
        fb::exit(fb_err.as_ref(), msg_owned, status);
    }

    /// Log an EXIT event for `status`, for the `flow_common_exit_status` exit
    /// hook. The sample is delivered by the logger's at-exit flush.
    pub fn log_exit(status: FlowExitStatus, msg: Option<&str>) {
        exit(
            None,
            msg.unwrap_or(""),
            flow_common_exit_status::to_string(status),
        )
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn report_from_monitor_server_exit_due_to_signal(signal: i32) {
        fb::report_from_monitor_server_exit_due_to_signal(signal);
    }

    // SHIM: clones `Option<&str>` into `Option<String>` for FB and unwraps
    // `Option<i32>` (defaulting absent to 0) into FB's required i32. Other
    // arg shapes match.
    #[expect(
        clippy::too_many_arguments,
        reason = "OSS-shaped API mirrors flowEventLogger.mli's labelled recheck arguments"
    )]
    pub fn recheck(
        modified_count: i32,
        deleted_count: i32,
        merged_dependency_count: i32,
        dependent_file_count: i32,
        merge_skip_count: i32,
        check_skip_count: i32,
        profiling: &serde_json::Value,
        time_to_resolve_all_type_errors: Option<f64>,
        time_to_resolve_all_type_errors_in_one_file: Option<f64>,
        time_to_resolve_all_subtyping_errors: Option<f64>,
        time_to_resolve_all_subtyping_errors_in_one_file: Option<f64>,
        rounds_to_resolve_all_type_errors: Option<u32>,
        rounds_to_resolve_all_type_errors_in_one_file: Option<u32>,
        rounds_to_resolve_all_subtyping_errors: Option<u32>,
        rounds_to_resolve_all_subtyping_errors_in_one_file: Option<u32>,
        first_internal_error: Option<&str>,
        slowest_file: Option<&str>,
        num_slow_files: Option<i32>,
        scm_changed_mergebase: Option<bool>,
    ) {
        fb::recheck(
            modified_count,
            deleted_count,
            merged_dependency_count,
            dependent_file_count,
            merge_skip_count,
            check_skip_count,
            profiling,
            time_to_resolve_all_type_errors,
            time_to_resolve_all_type_errors_in_one_file,
            time_to_resolve_all_subtyping_errors,
            time_to_resolve_all_subtyping_errors_in_one_file,
            rounds_to_resolve_all_type_errors,
            rounds_to_resolve_all_type_errors_in_one_file,
            rounds_to_resolve_all_subtyping_errors,
            rounds_to_resolve_all_subtyping_errors_in_one_file,
            first_internal_error.map(|s| s.to_string()),
            slowest_file.map(|s| s.to_string()),
            num_slow_files.unwrap_or(0),
            scm_changed_mergebase,
        );
    }

    // SHIM: serialises the OSS `&serde_json::Value` request payload into the
    // string FB-internal expects. Compact JSON serialisation.
    pub fn log_typing_errors(data: &serde_json::Value) {
        let s = data.to_string();
        fb::log_typing_errors(&s);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn recheck_canceled(
        num_files_to_prioritize: i32,
        num_files_to_recheck: i32,
        num_files_to_force: i32,
    ) {
        fb::recheck_canceled(
            num_files_to_prioritize,
            num_files_to_recheck,
            num_files_to_force,
        );
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn recheck_series(recheck_count: i32, profiling: &serde_json::Value) {
        fb::recheck_series(recheck_count, profiling);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn reinit(reason: &str, profiling: &serde_json::Value) {
        fb::reinit(reason, profiling);
    }

    pub fn global_lib_reinit(
        changed_file_count: usize,
        old_global_count: usize,
        new_global_count: usize,
        inventory_file_count: usize,
        deleted_file_count: usize,
        profiling: &serde_json::Value,
    ) {
        fb::global_lib_reinit(
            changed_file_count,
            old_global_count,
            new_global_count,
            inventory_file_count,
            deleted_file_count,
            profiling,
        );
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn reinit_full_check(profiling: &serde_json::Value) {
        fb::reinit_full_check(profiling);
    }

    // SHIM: discards the unused OSS `_arg: &str` parameter; FB-internal takes
    // no arguments.
    pub fn murdered_by_oom_killer(_arg: &str) {
        fb::murdered_by_oom_killer();
    }

    // SHIM: clones the OSS LoggingContext into FB shape and wraps the
    // `&serde_json::Value` into `Some(...)` for FB's optional parameter.
    pub fn ephemeral_command_success(
        json_data: &serde_json::Value,
        client_context: &LoggingContext,
        profiling: &serde_json::Value,
    ) {
        let fb_ctx = to_fb_logging_context(client_context.clone());
        fb::ephemeral_command_success(Some(json_data), &fb_ctx, profiling);
    }

    // SHIM: clones the OSS LoggingContext into FB shape and wraps the
    // `&serde_json::Value` into `Some(...)` for FB's optional parameter.
    pub fn ephemeral_command_failure(
        json_data: &serde_json::Value,
        client_context: &LoggingContext,
    ) {
        let fb_ctx = to_fb_logging_context(client_context.clone());
        fb::ephemeral_command_failure(Some(json_data), &fb_ctx);
    }

    // SHIM: clones LoggingContexts/PersistentContext/PersistentDelay into
    // FB-internal shapes and wraps the OSS `Error` into an
    // `fb::Error(String, Callstack(...))`.
    #[expect(
        clippy::too_many_arguments,
        reason = "OSS-shaped API mirrors flowEventLogger.mli's labelled persistent_command_success arguments"
    )]
    pub fn persistent_command_success(
        server_logging_context: Option<&LoggingContext>,
        request_id: &str,
        method_name: &str,
        request: &str,
        extra_data: &[(String, serde_json::Value)],
        client_context: &LoggingContext,
        persistent_context: Option<&PersistentContext>,
        persistent_delay: Option<&PersistentDelay>,
        server_profiling: Option<&serde_json::Value>,
        client_duration: Option<f64>,
        wall_start: f64,
        activity_key: Option<&serde_json::Value>,
        error: Option<&Error>,
    ) {
        let fb_server = server_logging_context.map(|ctx| to_fb_logging_context(ctx.clone()));
        let fb_client = to_fb_logging_context(client_context.clone());
        let fb_pctx = persistent_context.map(|persistent_context| fb::PersistentContext {
            start_lsp_state: persistent_context.start_lsp_state.clone(),
            start_lsp_state_reason: persistent_context.start_lsp_state_reason.clone(),
            start_server_status: persistent_context.start_server_status.clone(),
            start_watcher_status: persistent_context.start_watcher_status.clone(),
        });
        let fb_pdelay = persistent_delay.map(|persistent_delay| fb::PersistentDelay {
            init_duration: persistent_delay.init_duration,
            command_count: persistent_delay.command_count,
            command_duration: persistent_delay.command_duration,
            command_worst: persistent_delay.command_worst.clone(),
            command_worst_duration: persistent_delay.command_worst_duration,
            recheck_count: persistent_delay.recheck_count,
            recheck_dependent_files: persistent_delay.recheck_dependent_files,
            recheck_changed_files: persistent_delay.recheck_changed_files,
            recheck_duration: persistent_delay.recheck_duration,
            recheck_worst_duration: persistent_delay.recheck_worst_duration,
            recheck_worst_dependent_file_count: persistent_delay.recheck_worst_dependent_file_count,
            recheck_worst_changed_file_count: persistent_delay.recheck_worst_changed_file_count,
            recheck_worst_cycle_leader: persistent_delay.recheck_worst_cycle_leader.clone(),
            recheck_worst_cycle_size: persistent_delay.recheck_worst_cycle_size,
        });
        let extra_data_vec = extra_data.to_vec();
        let fb_err = error.map(to_fb_error);
        fb::persistent_command_success(
            fb_server,
            request_id.to_string(),
            method_name.to_string(),
            request,
            extra_data_vec,
            &fb_client,
            fb_pctx.as_ref(),
            fb_pdelay.as_ref(),
            server_profiling,
            client_duration,
            wall_start,
            activity_key,
            fb_err.as_ref(),
        );
    }

    // SHIM: same translations as `persistent_command_success`; the OSS Error
    // is required (non-optional) so it goes through `to_fb_error` directly.
    #[expect(
        clippy::too_many_arguments,
        reason = "OSS-shaped API mirrors flowEventLogger.mli's labelled persistent_command_failure arguments"
    )]
    pub fn persistent_command_failure(
        server_logging_context: Option<&LoggingContext>,
        request: &str,
        extra_data: &[(String, serde_json::Value)],
        client_context: &LoggingContext,
        persistent_context: Option<&PersistentContext>,
        persistent_delay: Option<&PersistentDelay>,
        server_profiling: Option<&serde_json::Value>,
        client_duration: Option<f64>,
        wall_start: f64,
        activity_key: Option<&serde_json::Value>,
        error: &Error,
    ) {
        let fb_server = server_logging_context.map(|ctx| to_fb_logging_context(ctx.clone()));
        let fb_client = to_fb_logging_context(client_context.clone());
        let fb_pctx = persistent_context.map(|persistent_context| fb::PersistentContext {
            start_lsp_state: persistent_context.start_lsp_state.clone(),
            start_lsp_state_reason: persistent_context.start_lsp_state_reason.clone(),
            start_server_status: persistent_context.start_server_status.clone(),
            start_watcher_status: persistent_context.start_watcher_status.clone(),
        });
        let fb_pdelay = persistent_delay.map(|persistent_delay| fb::PersistentDelay {
            init_duration: persistent_delay.init_duration,
            command_count: persistent_delay.command_count,
            command_duration: persistent_delay.command_duration,
            command_worst: persistent_delay.command_worst.clone(),
            command_worst_duration: persistent_delay.command_worst_duration,
            recheck_count: persistent_delay.recheck_count,
            recheck_dependent_files: persistent_delay.recheck_dependent_files,
            recheck_changed_files: persistent_delay.recheck_changed_files,
            recheck_duration: persistent_delay.recheck_duration,
            recheck_worst_duration: persistent_delay.recheck_worst_duration,
            recheck_worst_dependent_file_count: persistent_delay.recheck_worst_dependent_file_count,
            recheck_worst_changed_file_count: persistent_delay.recheck_worst_changed_file_count,
            recheck_worst_cycle_leader: persistent_delay.recheck_worst_cycle_leader.clone(),
            recheck_worst_cycle_size: persistent_delay.recheck_worst_cycle_size,
        });
        let extra_data_vec = extra_data.to_vec();
        let fb_err = to_fb_error(error);
        fb::persistent_command_failure(
            fb_server,
            request,
            extra_data_vec,
            &fb_client,
            fb_pctx.as_ref(),
            fb_pdelay.as_ref(),
            server_profiling,
            client_duration,
            wall_start,
            activity_key,
            &fb_err,
        );
    }

    // SHIM: clones the optional request string and wraps OSS Error tuple into
    // FB Error struct.
    pub fn persistent_expected_error(
        request: Option<&str>,
        client_context: &LoggingContext,
        activity_key: Option<&serde_json::Value>,
        error: &Error,
    ) {
        let fb_client = to_fb_logging_context(client_context.clone());
        let fb_err = to_fb_error(error);
        let request_owned = request.map(|request| request.to_string());
        fb::persistent_expected_error(request_owned, &fb_client, activity_key, &fb_err);
    }

    // SHIM: same translations as `persistent_expected_error`.
    pub fn persistent_unexpected_error(
        request: Option<&str>,
        client_context: &LoggingContext,
        activity_key: Option<&serde_json::Value>,
        error: &Error,
    ) {
        let fb_client = to_fb_logging_context(client_context.clone());
        let fb_err = to_fb_error(error);
        let request_owned = request.map(|request| request.to_string());
        fb::persistent_unexpected_error(request_owned, &fb_client, activity_key, &fb_err);
    }

    // SHIM: truncates the OSS `merge_base_timestamp: f64` to the `i64`
    // FB-internal expects. Other arg shapes match.
    pub fn saved_state_fb_fetcher_success(
        repo_root: &str,
        merge_base_hash: &str,
        merge_base_timestamp: f64,
        saved_state_hash: &str,
        changed_files_count: i32,
        saved_state_filename: &str,
        profiling: &serde_json::Value,
    ) {
        fb::saved_state_fb_fetcher_success(
            repo_root,
            merge_base_hash,
            merge_base_timestamp as i64,
            saved_state_hash,
            changed_files_count,
            saved_state_filename,
            profiling,
        );
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn saved_state_fb_fetcher_error(step: &str, trace: &str, profiling: &serde_json::Value) {
        fb::saved_state_fb_fetcher_error(step, trace, profiling);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn load_saved_state_success(changed_files_count: i32) {
        fb::load_saved_state_success(changed_files_count);
    }

    // SHIM: maps OSS `trace: &str` to FB's `Option<String>` by treating an
    // empty string as `None` and any non-empty trace as `Some(...)`.
    pub fn load_saved_state_error(
        saved_state_filename: &str,
        changed_files_count: i32,
        invalid_reason: &str,
        trace: &str,
    ) {
        let trace_owned = if trace.is_empty() {
            None
        } else {
            Some(trace.to_string())
        };
        fb::load_saved_state_error(
            saved_state_filename,
            changed_files_count,
            invalid_reason,
            trace_owned,
        );
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn idle_heartbeat(idle_time: f64, profiling: &serde_json::Value) {
        fb::idle_heartbeat(idle_time, profiling);
    }

    pub fn live_parse_errors(request: &str, data: &str, wall_start: f64) {
        fb::live_parse_errors(request, data, wall_start);
    }

    // SHIM: same translation as `live_parse_errors`.
    pub fn live_non_parse_errors(request: &str, data: &str, wall_start: f64) {
        fb::live_non_parse_errors(request, data, wall_start);
    }

    // SHIM: same translation as `live_parse_errors`.
    pub fn live_non_parse_errors_failed(request: &str, data: &str, wall_start: f64) {
        fb::live_non_parse_errors_failed(request, data, wall_start);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn file_watcher_event_started(name: &str, data: &str) {
        fb::file_watcher_event_started(name, data);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn file_watcher_event_finished(name: &str, data: &str) {
        fb::file_watcher_event_finished(name, data);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn file_watcher_uncaught_failure(msg: &str) {
        fb::file_watcher_uncaught_failure(msg);
    }

    // SHIM: clones `Option<&str>` request/response into `Option<String>` for FB.
    pub fn watchman_error(request: Option<&str>, response: Option<&str>, msg: &str) {
        fb::watchman_error(
            request.map(|s| s.to_string()),
            response.map(|s| s.to_string()),
            msg,
        );
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn watchman_warning(warning: &str) {
        fb::watchman_warning(warning);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn watchman_uncaught_failure(msg: &str) {
        fb::watchman_uncaught_failure(msg);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn watchman_connection_reestablished(downtime: f64) {
        fb::watchman_connection_reestablished(downtime);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn watchman_connection_reestablishment_failed(msg: &str) {
        fb::watchman_connection_reestablishment_failed(msg);
    }

    // SHIM: maps OSS `effort: &str` to FB's `SharedmemGcEffort` enum
    // ("always"/"always_TEST" → AlwaysTest, else Aggressive — discarded
    // strings match the OSS stub which ignored the value entirely) and casts
    // OSS `f64` sizes to FB's `i64`.
    pub fn sharedmem_gc_ran(a: &str, b: f64, c: f64, d: f64) {
        let effort = match a {
            "always" | "always_TEST" => fb::SharedmemGcEffort::AlwaysTest,
            _ => fb::SharedmemGcEffort::Aggressive,
        };
        fb::sharedmem_gc_ran(effort, b as i64, c as i64, d);
    }

    // SHIM: casts OSS `size: u64` to FB's `i64`.
    pub fn sharedmem_init_done(size: u64) {
        fb::sharedmem_init_done(size as i64);
    }

    // SHIM: discards the unused OSS `_msg: &str` parameter; FB-internal takes
    // no arguments.
    pub fn sharedmem_failed_memfd_init(_msg: &str) {
        fb::sharedmem_failed_memfd_init();
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn worker_exception(msg: &str) {
        fb::worker_exception(msg);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn dfind_ready(a: &str, b: f64) {
        fb::dfind_ready(a, b);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn parsing_exception(msg: &str) {
        fb::parsing_exception(msg);
    }

    // SHIM: clones `name: &str` into the String FB-internal expects.
    pub fn set_file_watcher(name: &str) {
        fb::set_file_watcher(name.to_string());
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn edenfs_watcher_fallback(msg: &str) {
        fb::edenfs_watcher_fallback(msg);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn edenfs_watcher_non_eden_www(backtrace: &str) {
        fb::edenfs_watcher_non_eden_www(backtrace);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn edenfs_watcher_error(msg: &str, backtrace: &str) {
        fb::edenfs_watcher_error(msg, backtrace);
    }

    // SHIM: forwards directly; FB and OSS share identical signature.
    pub fn edenfs_watcher_lost_changes(msg: &str, backtrace: &str) {
        fb::edenfs_watcher_lost_changes(msg, backtrace);
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn register_scuba_entry_registers_ocaml_top_level_scuba_tables() {
            register_scuba_entry();

            let categories = flow_facebook_logging::scuba::table::post_init_array();
            assert!(
                categories
                    .iter()
                    .any(|category| category == "perfpipe_flow_server_events"),
                "EventLogger daemon child needs the Flow events table registered before dispatch"
            );
            assert!(
                categories
                    .iter()
                    .any(|category| category == "perfpipe_flow_typing_errors"),
                "EventLogger daemon child needs the typing errors table registered before dispatch"
            );
            assert!(
                categories
                    .iter()
                    .any(|category| category == "perfpipe_flow_interactions"),
                "FlowInteractionLogger daemon child needs the interactions table registered before dispatch"
            );
        }
    }
}

#[cfg(fbcode_build)]
pub use fb_facade::*;
