/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::io;
use std::io::Write as _;
use std::str::FromStr as _;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use crossbeam::channel::Receiver;
use crossbeam::channel::TryRecvError;
use flow_common_exit_status::FlowExitStatus;
use flow_common_socket::socket::Addr;
use flow_common_socket::socket::SocketStream;

pub type FilePath = std::path::PathBuf;
use flow_server_env::file_watcher_status;
use flow_server_env::lsp;
use flow_server_env::lsp::loc_to_lsp_range;
use flow_server_env::lsp_connect_params;
use flow_server_env::lsp_connect_params::ConnectParams;
use flow_server_env::lsp_helpers;
use flow_server_env::lsp_mapper;
use flow_server_env::lsp_prot;
use flow_server_env::lsp_prot::DocumentUri;
use flow_server_env::lsp_prot::LspId;
use flow_server_env::lsp_prot::Metadata;
use flow_server_env::lsp_prot::UriMap;
use flow_server_env::server_prot;
use flow_server_env::server_status;
use flow_server_env::socket_handshake;
use lsp_types::MessageType;
use lsp_types::NumberOrString;

use crate::lsp_writers;

type RawFd = std::os::raw::c_int;

fn lsp_exit_ok() -> ! {
    std::process::exit(0)
}

fn lsp_exit_bad() -> ! {
    std::process::exit(1)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WrappedId {
    pub server_id: i32,
    pub message_id: LspId,
}

pub fn encode_wrapped(wrapped_id: &WrappedId) -> LspId {
    let WrappedId {
        server_id,
        message_id,
    } = wrapped_id;
    match message_id {
        NumberOrString::Number(id) => NumberOrString::String(format!("{}:#{}", server_id, id)),
        NumberOrString::String(id) => NumberOrString::String(format!("{}:'{}", server_id, id)),
    }
}

pub fn decode_wrapped(lsp: &LspId) -> WrappedId {
    try_decode_wrapped(lsp)
        .unwrap_or_else(|_| panic!("Invalid message id {}", lsp_fmt::id_to_string(lsp)))
}

fn try_decode_wrapped(lsp: &LspId) -> Result<WrappedId, flow_server_env::lsp::error::T> {
    let s = match lsp {
        NumberOrString::Number(_) => {
            return Err(parse_error_exception("not a wrapped id"));
        }
        NumberOrString::String(s) => s,
    };
    let colon_pos = s
        .find(':')
        .ok_or_else(|| parse_error_exception(format!("Invalid message id {}", s)))?;
    let server_id: i32 = s[..colon_pos]
        .parse()
        .map_err(|_| parse_error_exception(format!("Invalid message id {}", s)))?;
    let rest = &s[colon_pos + 1..];
    if rest.is_empty() {
        return Err(parse_error_exception(format!("Invalid message id {}", s)));
    }
    let kind = &rest[..1];
    let message_str = &rest[1..];
    let message_id = if kind == "#" {
        NumberOrString::Number(
            message_str
                .parse()
                .map_err(|_| parse_error_exception(format!("Invalid message id {}", s)))?,
        )
    } else {
        NumberOrString::String(message_str.to_string())
    };
    Ok(WrappedId {
        server_id,
        message_id,
    })
}

pub type WrappedMap<V> = HashMap<WrappedId, V>;

pub struct ServerConn {
    pub client_id: lsp_prot::ClientId,
    pub sockaddr: Addr,
    pub stream: Mutex<SocketStream>,
    pub incoming: Mutex<Receiver<io::Result<lsp_prot::MessageFromServer>>>,
}

#[derive(Debug, Clone)]
pub enum ShowStatusT {
    NeverShown,
    Shown(Option<LspId>, lsp::show_status::Params),
}

// o_open_doc is guaranteed to be up-to-date with respect to the editor
// o_ast, if present, is guaranteed to be up-to-date. It gets computed lazily.
// o_unsaved if true means that this open file has unsaved changes to the buffer.
#[derive(Debug, Clone)]
pub struct OpenFileInfo {
    pub o_open_doc: lsp_types::TextDocumentItem,
    pub o_ast: Option<(
        flow_parser::ast::Program<flow_parser::loc::Loc, flow_parser::loc::Loc>,
        Option<Vec<lsp_types::Diagnostic>>,
    )>,
    pub o_unsaved: bool,
}

pub struct InitializedEnv {
    pub i_initialize_params: lsp_types::InitializeParams,
    pub i_connect_params: ConnectParams,
    pub i_root: FilePath,
    pub i_version: Option<String>,
    pub i_server_id: i32,
    pub i_can_autostart_after_version_mismatch: bool,
    pub i_outstanding_local_handlers: HashMap<LspId, LspHandler>,
    pub i_outstanding_local_requests: HashMap<LspId, lsp::LspRequest>,
    pub i_outstanding_requests_from_server: WrappedMap<lsp::LspRequest>,
    pub i_is_connected: bool,
    pub i_status: ShowStatusT,
    pub i_open_files: UriMap<OpenFileInfo>,
    pub i_errors: crate::lsp_errors::T,
    pub i_config: serde_json::Value,
    pub i_flowconfig: FlowConfig,
    pub i_file_options: Arc<FileOptions>,
}

impl Clone for InitializedEnv {
    fn clone(&self) -> Self {
        assert!(
            self.i_outstanding_local_handlers.is_empty(),
            "Cannot clone InitializedEnv with {} outstanding local handlers — \
             Box<dyn FnOnce> is not Clone. Restructure to avoid clone with outstanding handlers.",
            self.i_outstanding_local_handlers.len()
        );
        InitializedEnv {
            i_initialize_params: self.i_initialize_params.clone(),
            i_connect_params: self.i_connect_params.clone(),
            i_root: self.i_root.clone(),
            i_version: self.i_version.clone(),
            i_server_id: self.i_server_id,
            i_can_autostart_after_version_mismatch: self.i_can_autostart_after_version_mismatch,
            i_outstanding_local_handlers: HashMap::new(),
            i_outstanding_local_requests: self.i_outstanding_local_requests.clone(),
            i_outstanding_requests_from_server: self.i_outstanding_requests_from_server.clone(),
            i_is_connected: self.i_is_connected,
            i_status: self.i_status.clone(),
            i_open_files: self.i_open_files.clone(),
            i_errors: self.i_errors.clone(),
            i_config: self.i_config.clone(),
            i_flowconfig: self.i_flowconfig.clone(),
            i_file_options: self.i_file_options.clone(),
        }
    }
}

#[derive(Clone)]
pub struct DisconnectedEnv {
    pub d_ienv: InitializedEnv,
    pub d_autostart: bool,
    pub d_server_status: Option<(server_status::Status, file_watcher_status::Status)>,
}

pub struct ConnectedEnv {
    pub c_ienv: InitializedEnv,
    pub c_conn: ServerConn,
    pub c_server_status: (server_status::Status, Option<file_watcher_status::Status>),
    pub c_recent_summaries: Vec<(f64, lsp_prot::TelemetryFromServer)>,
    pub c_about_to_exit_code: Option<FlowExitStatus>,
    pub c_is_rechecking: bool,
    pub c_lazy_stats: Option<server_prot::response::LazyStats>,
    pub c_outstanding_requests_to_server: HashSet<LspId>,
}

pub enum ServerState {
    Disconnected(DisconnectedEnv),
    Connected(ConnectedEnv),
}

pub enum State {
    PreInit(ConnectParams),
    Initialized(ServerState),
    PostShutdown,
}

#[derive(Debug)]
pub struct RemoteExceptionData {
    pub message: String,
    pub stack: String,
}

#[derive(Debug)]
pub enum FlowLspError {
    ClientFatalConnectionException(RemoteExceptionData),
    ClientRecoverableConnectionException(RemoteExceptionData),
    ServerFatalConnectionException(RemoteExceptionData),
    RequestLspException {
        id: LspId,
        error: flow_server_env::lsp::error::T,
    },
    LspException(flow_server_env::lsp::error::T),
    ChangedFileNotOpen(DocumentUri),
}

impl fmt::Display for FlowLspError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FlowLspError::ClientFatalConnectionException(e) => {
                write!(f, "Client fatal connection exception: {}", e.message)
            }
            FlowLspError::ClientRecoverableConnectionException(e) => {
                write!(f, "Client recoverable connection exception: {}", e.message)
            }
            FlowLspError::ServerFatalConnectionException(e) => {
                write!(f, "Server fatal connection exception: {}", e.message)
            }
            FlowLspError::RequestLspException { error, .. } | FlowLspError::LspException(error) => {
                write!(f, "{}", error.message)
            }
            FlowLspError::ChangedFileNotOpen(uri) => {
                write!(f, "Changed file not open: {}", **uri)
            }
        }
    }
}

impl std::error::Error for FlowLspError {}

impl From<flow_server_env::lsp::error::T> for FlowLspError {
    fn from(e: flow_server_env::lsp::error::T) -> Self {
        FlowLspError::LspException(e)
    }
}

#[derive(Clone)]
pub enum Event {
    ServerMessage(lsp_prot::MessageFromServer),
    ClientMessage(lsp_prot::LspMessage, Metadata),
    Tick,
}

pub use flow_config::FlowConfig;

pub type FileOptions = flow_common::files::FileOptions;

pub enum LspResultHandler {
    ShowMessageHandler(
        Box<
            dyn FnOnce(
                Option<lsp_types::MessageActionItem>,
                &mut ServerState,
            ) -> Result<(), FlowLspError>,
        >,
    ),
    ShowStatusHandler(
        Box<dyn FnOnce(lsp::show_status::Result, &mut ServerState) -> Result<(), FlowLspError>>,
    ),
    ApplyWorkspaceEditHandler(
        Box<
            dyn FnOnce(
                lsp_types::ApplyWorkspaceEditResponse,
                &mut ServerState,
            ) -> Result<(), FlowLspError>,
        >,
    ),
    ConfigurationHandler(
        Box<dyn FnOnce(Vec<serde_json::Value>, &mut ServerState) -> Result<(), FlowLspError>>,
    ),
    VoidHandler,
}

pub struct LspHandler {
    pub on_response: LspResultHandler,
    pub on_error:
        Box<dyn FnOnce(lsp::error::T, String, &mut ServerState) -> Result<(), FlowLspError>>,
}

use crate::jsonrpc::JsonrpcMessage;
use crate::jsonrpc::JsonrpcQueue;

fn annotate_request_parse_error(message: &JsonrpcMessage, error: FlowLspError) -> FlowLspError {
    match (message.id.clone(), message.method_.is_empty(), error) {
        (Some(id), false, FlowLspError::LspException(error)) => {
            FlowLspError::RequestLspException { id, error }
        }
        (_, _, error) => error,
    }
}

pub type FileInput = flow_server_utils::file_input::FileInput;

pub type RageItem = lsp::rage::RageItem;

pub struct TrackEffect {
    pub changed_live_uri: Option<DocumentUri>,
}

pub enum LogNeeded {
    LogNeeded(Metadata),
    LogDeferred,
    LogNotNeeded,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
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

pub type InteractionState = crate::lsp_interaction::State;
pub type InteractionServerStatus = crate::lsp_interaction::ServerStatus;
pub type InteractionBufferStatus = crate::lsp_interaction::BufferStatus;
pub type InteractionTrigger = crate::lsp_interaction::Trigger;
pub type InteractionId = crate::lsp_interaction::Id;
pub type InteractionUx = crate::lsp_interaction::Ux;

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectError {
    ServerMissing,
    ServerSocketMissing,
    BuildIdMismatch(BuildIdMismatchKind),
    ServerBusy(ServerBusyKind),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BuildIdMismatchKind {
    ServerExited,
    ClientShouldError { server_version: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerBusyKind {
    NotResponding,
    TooManyClients,
    FailOnInit((server_status::Status, file_watcher_status::Status)),
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectError::ServerMissing => write!(f, "Server missing"),
            ConnectError::ServerSocketMissing => write!(f, "Server socket missing"),
            ConnectError::BuildIdMismatch(BuildIdMismatchKind::ServerExited) => {
                write!(f, "Server exited due to version mismatch")
            }
            ConnectError::BuildIdMismatch(BuildIdMismatchKind::ClientShouldError {
                server_version,
            }) => {
                write!(f, "Server is wrong version ({server_version})")
            }
            ConnectError::ServerBusy(ServerBusyKind::NotResponding) => {
                write!(f, "Server busy: not responding")
            }
            ConnectError::ServerBusy(ServerBusyKind::TooManyClients) => {
                write!(f, "Server busy: too many clients")
            }
            ConnectError::ServerBusy(ServerBusyKind::FailOnInit((
                server_status,
                watcher_status,
            ))) => {
                write!(
                    f,
                    "Server busy during init ({server_status:?}, {watcher_status:?})"
                )
            }
        }
    }
}

fn connect_temp_dir(connect_params: &ConnectParams) -> String {
    flow_command_utils::get_temp_dir(&connect_params.temp_dir)
}

fn classify_persistent_connect(
    flowconfig_name: &str,
    env: &DisconnectedEnv,
    client_handshake: &socket_handshake::ClientHandshake,
) -> Result<ServerConn, ConnectError> {
    let tmp_dir = connect_temp_dir(&env.d_ienv.i_connect_params);
    let conn_result = flow_commands_connect::command_connect_simple::connect_once(
        flowconfig_name,
        client_handshake,
        &tmp_dir,
        &env.d_ienv.i_root,
    );
    let (sockaddr, stream) = match conn_result {
        Ok((sockaddr, stream)) => (sockaddr, stream),
        Err(flow_commands_connect::command_connect_simple::CCSError::ServerMissing) => {
            return Err(ConnectError::ServerMissing);
        }
        Err(flow_commands_connect::command_connect_simple::CCSError::ServerSocketMissing) => {
            return Err(ConnectError::ServerSocketMissing);
        }
        Err(flow_commands_connect::command_connect_simple::CCSError::BuildIdMismatch(
            flow_commands_connect::command_connect_simple::MismatchBehavior::ServerExited,
        )) => {
            return Err(ConnectError::BuildIdMismatch(
                BuildIdMismatchKind::ServerExited,
            ));
        }
        Err(flow_commands_connect::command_connect_simple::CCSError::BuildIdMismatch(
            flow_commands_connect::command_connect_simple::MismatchBehavior::ClientShouldError {
                server_bin: _,
                server_version,
            },
        )) => {
            return Err(ConnectError::BuildIdMismatch(
                BuildIdMismatchKind::ClientShouldError { server_version },
            ));
        }
        Err(flow_commands_connect::command_connect_simple::CCSError::ServerBusy(
            flow_commands_connect::command_connect_simple::BusyReason::TooManyClients,
        )) => {
            return Err(ConnectError::ServerBusy(ServerBusyKind::TooManyClients));
        }
        Err(flow_commands_connect::command_connect_simple::CCSError::ServerBusy(
            flow_commands_connect::command_connect_simple::BusyReason::NotResponding,
        )) => {
            return Err(ConnectError::ServerBusy(ServerBusyKind::NotResponding));
        }
        Err(flow_commands_connect::command_connect_simple::CCSError::ServerBusy(
            flow_commands_connect::command_connect_simple::BusyReason::FailOnInit(
                server_status,
                watcher_status,
            ),
        )) => {
            return Err(ConnectError::ServerBusy(ServerBusyKind::FailOnInit((
                server_status,
                watcher_status,
            ))));
        }
    };
    stream
        .set_read_timeout(None)
        .map_err(|_| ConnectError::ServerBusy(ServerBusyKind::NotResponding))?;
    stream
        .set_write_timeout(None)
        .map_err(|_| ConnectError::ServerBusy(ServerBusyKind::NotResponding))?;
    let reader_stream = stream
        .try_clone()
        .map_err(|_| ConnectError::ServerBusy(ServerBusyKind::NotResponding))?;
    let (sender, receiver) =
        crossbeam::channel::unbounded::<io::Result<lsp_prot::MessageFromServer>>();
    std::thread::Builder::new()
        .name("flow_lsp_persistent_reader".to_string())
        .spawn(move || {
            let mut stream = reader_stream;
            loop {
                let result: io::Result<lsp_prot::MessageFromServer> =
                    flow_parser::loc::with_full_source_serde(|| {
                        flow_server_env::server_socket_rpc::receive_message(&mut stream)
                    });
                let was_err = result.is_err();
                if sender.send(result).is_err() {
                    return;
                }
                if was_err {
                    return;
                }
            }
        })
        .map_err(|_| ConnectError::ServerBusy(ServerBusyKind::NotResponding))?;
    let client_id = env.d_ienv.i_server_id + 1;
    Ok(ServerConn {
        client_id,
        sockaddr,
        stream: Mutex::new(stream),
        incoming: Mutex::new(receiver),
    })
}

fn send_request_with_metadata(
    conn: &ServerConn,
    request_with_metadata: &lsp_prot::RequestWithMetadata,
) -> Result<(), std::io::Error> {
    let mut stream = conn.stream.lock().unwrap();
    flow_parser::loc::with_full_source_serde(|| {
        flow_server_env::server_socket_rpc::send_message(&mut *stream, request_with_metadata)
    })
}

fn flow_cli_command(
    subcommand: &str,
    flowconfig_name: &str,
    connect_params: &ConnectParams,
    root: &FilePath,
) -> std::process::Command {
    let exe = std::env::args_os()
        .next()
        .unwrap_or_else(|| std::ffi::OsString::from("flow"));
    let mut command = std::process::Command::new(exe);
    command.arg(subcommand);
    command.arg("--flowconfig-name").arg(flowconfig_name);
    if let Some(from) = connect_params.from.as_deref() {
        command.arg("--from").arg(from);
    }
    if let Some(temp_dir) = connect_params.temp_dir.as_deref() {
        command.arg("--temp-dir").arg(temp_dir);
    }
    if subcommand == "start" {
        if connect_params.autostop {
            command.arg("--autostop");
        }
        if connect_params.quiet {
            command.arg("--quiet");
        }
    }
    command.arg(root);
    command
}

fn start_flow_server(
    flowconfig_name: &str,
    connect_params: &ConnectParams,
    root: &FilePath,
) -> Result<(), String> {
    let mut command = flow_cli_command("start", flowconfig_name, connect_params, root);
    let status = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| format!("Failed to start Flow server: {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Flow start exited with status {}", status))
    }
}

fn kill_stale_server(
    flowconfig_name: &str,
    connect_params: &ConnectParams,
    root: &FilePath,
) -> Result<(), String> {
    let output = flow_cli_command("stop", flowconfig_name, connect_params, root)
        .arg("--quiet")
        .output()
        .map_err(|e| format!("Failed to stop stale Flow server: {}", e))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err(format!("Flow stop exited with status {}", output.status))
        } else {
            Err(stderr)
        }
    }
}

fn flow_event_logger_persistent_context(
    metadata: &Metadata,
) -> flow_event_logger::PersistentContext {
    flow_event_logger::PersistentContext {
        start_lsp_state: metadata.start_lsp_state.clone(),
        start_lsp_state_reason: metadata.start_lsp_state_reason.clone(),
        start_server_status: metadata
            .start_server_status
            .as_ref()
            .map(|status| server_status::string_of_status(false, true, status)),
        start_watcher_status: metadata
            .start_watcher_status
            .as_ref()
            .map(file_watcher_status::string_of_status),
    }
}

fn flow_event_logger_persistent_delay(
    persistent_delay: &PersistentDelay,
) -> flow_event_logger::PersistentDelay {
    flow_event_logger::PersistentDelay {
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
    }
}

#[allow(dead_code)]
const PREAMBLE_SENTINEL: u8 = 0x8E;
const _PREAMBLE_CORE_SIZE: usize = 4;
#[allow(dead_code)]
const PREAMBLE_SIZE: usize = _PREAMBLE_CORE_SIZE + 1;

fn server_fatal_connection_exception(message: impl Into<String>) -> FlowLspError {
    FlowLspError::ServerFatalConnectionException(RemoteExceptionData {
        message: message.into(),
        stack: String::new(),
    })
}

fn make_lsp_exception(
    code: flow_server_env::lsp::error::Code,
    message: impl Into<String>,
) -> FlowLspError {
    FlowLspError::LspException(flow_server_env::lsp::error::T {
        code,
        message: message.into(),
        data: None,
    })
}

pub(crate) fn parse_error_exception(message: impl Into<String>) -> flow_server_env::lsp::error::T {
    flow_server_env::lsp::error::T {
        code: flow_server_env::lsp::error::Code::ParseError,
        message: message.into(),
        data: None,
    }
}

fn internal_error_exception(message: impl Into<String>) -> FlowLspError {
    make_lsp_exception(flow_server_env::lsp::error::Code::InternalError, message)
}

pub fn sys_utils_select_non_intr(
    _read_fds: &[RawFd],
    _write_fds: &[RawFd],
    _except_fds: &[RawFd],
    _timeout: f64,
) -> Result<(Vec<RawFd>, Vec<RawFd>, Vec<RawFd>), std::io::Error> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "sys_utils_select_non_intr: requires unsafe for libc::poll syscall",
    ))
}

pub fn flow_version() -> &'static str {
    flow_common::flow_version::version()
}

pub fn read_flowconfig_from_disk(flowconfig_name: &str, root: &FilePath) -> FlowConfig {
    let config_path = flow_server_files::server_files_js::config_file(flowconfig_name, root);
    match flow_config::get(&config_path) {
        Ok((flowconfig, _warnings, _hash)) => flowconfig,
        Err(flow_config::Error(line, msg)) => {
            eprintln!(".flowconfig:{} {}", line, msg);
            flow_common_exit_status::exit(
                flow_common_exit_status::FlowExitStatus::InvalidFlowconfig,
            );
        }
    }
}

pub fn lsp_interaction_init() {
    crate::lsp_interaction::init();
}

pub fn log_flusher_run() {
    std::thread::Builder::new()
        .name("flow_lsp_log_flusher".to_string())
        .spawn(|| {
            loop {
                std::thread::sleep(Duration::from_secs(5));
                let event_logger_result = flow_tokio_runtime::block_on(async {
                    let event_logger_flush = flow_event_logger_lwt::flush();
                    let interaction_flush = crate::lsp_interaction::flush();
                    let (event_logger_result, ()) =
                        tokio::join!(event_logger_flush, interaction_flush);
                    event_logger_result
                });
                if let Err(err) = event_logger_result {
                    eprintln!("Failed to flush Flow event logs: {}", err);
                }
            }
        })
        .expect("failed to spawn LSP log flusher");
}

use flow_lsp::lsp_fmt;

use crate::jsonrpc::get_next_request_id;
use crate::lsp_interaction;

fn sys_utils_realpath(path: &str) -> Option<String> {
    flow_common::files::cached_canonicalize(std::path::Path::new(path))
        .ok()
        .map(|p| p.to_string_lossy().to_string())
}

fn json_truncate(
    json: &serde_json::Value,
    max_string_length: usize,
    max_child_count: usize,
) -> serde_json::Value {
    match json {
        serde_json::Value::String(s) => {
            if s.len() > max_string_length {
                let truncate_to = s
                    .char_indices()
                    .map(|(idx, _)| idx)
                    .take_while(|idx| *idx <= max_string_length)
                    .last()
                    .unwrap_or(0);
                serde_json::Value::String(s[..truncate_to].to_string())
            } else {
                json.clone()
            }
        }
        serde_json::Value::Array(arr) => {
            let truncated: Vec<serde_json::Value> = arr
                .iter()
                .take(max_child_count)
                .map(|v| json_truncate(v, max_string_length, max_child_count))
                .collect();
            serde_json::Value::Array(truncated)
        }
        serde_json::Value::Object(map) => {
            let truncated: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .take(max_child_count)
                .map(|(k, v)| {
                    (
                        k.clone(),
                        json_truncate(v, max_string_length, max_child_count),
                    )
                })
                .collect();
            serde_json::Value::Object(truncated)
        }
        _ => json.clone(),
    }
}

pub(crate) fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

pub fn string_of_server_state(state: &ServerState) -> &'static str {
    match state {
        ServerState::Disconnected(_) => "Disconnected",
        ServerState::Connected(_) => "Connected",
    }
}

pub fn string_of_state(state: &State) -> &'static str {
    match state {
        State::PreInit(_) => "Pre_init",
        State::Initialized(server_state) => string_of_server_state(server_state),
        State::PostShutdown => "Post_shutdown",
    }
}

fn to_stdout_fn() -> impl FnMut(serde_json::Value) {
    |json| to_stdout(&json)
}

fn to_stdout(json: &serde_json::Value) {
    let s = format!("{}\r\n\r\n", serde_json::to_string(json).unwrap());
    let content_length = s.len();
    let mut stdout = io::stdout().lock();
    write!(stdout, "Content-Length: {}\r\n\r\n{}", content_length, s).unwrap();
    stdout.flush().unwrap();
}

fn get_ienv(state: &ServerState) -> &InitializedEnv {
    match state {
        ServerState::Connected(cenv) => &cenv.c_ienv,
        ServerState::Disconnected(denv) => &denv.d_ienv,
    }
}

fn get_ienv_mut(state: &mut ServerState) -> &mut InitializedEnv {
    match state {
        ServerState::Connected(cenv) => &mut cenv.c_ienv,
        ServerState::Disconnected(denv) => &mut denv.d_ienv,
    }
}

fn update_ienv(state: &mut ServerState, f: impl FnOnce(&mut InitializedEnv)) {
    f(get_ienv_mut(state));
}

fn update_recent_summaries(cenv: &mut ConnectedEnv, summary: lsp_prot::TelemetryFromServer) {
    let new_time = now();
    cenv.c_recent_summaries.push((new_time, summary));
    cenv.c_recent_summaries
        .retain(|(t, _)| *t >= new_time - 120.0);
}

#[allow(dead_code)]
fn log_of_summaries(
    root: &FilePath,
    summaries: &[lsp_prot::TelemetryFromServer],
) -> PersistentDelay {
    let mut acc = PersistentDelay::default();
    for event in summaries {
        match event {
            lsp_prot::TelemetryFromServer::InitSummary { duration } => {
                acc.init_duration += duration;
            }
            lsp_prot::TelemetryFromServer::CommandSummary {
                name: cmd,
                duration,
            } => {
                let is_worst = match acc.command_worst_duration {
                    None => true,
                    Some(d) => *duration >= d,
                };
                if is_worst {
                    acc.command_worst = Some(cmd.clone());
                    acc.command_worst_duration = Some(*duration);
                }
                acc.command_count += 1;
                acc.command_duration += duration;
            }
            lsp_prot::TelemetryFromServer::RecheckSummary { stats, duration } => {
                let lsp_prot::RecheckStats {
                    dependent_file_count,
                    changed_file_count,
                    ref top_cycle,
                } = *stats;
                let is_worst = match acc.recheck_worst_duration {
                    None => true,
                    Some(d) => *duration >= d,
                };
                if is_worst {
                    acc.recheck_worst_duration = Some(*duration);
                    acc.recheck_worst_dependent_file_count = Some(dependent_file_count);
                    acc.recheck_worst_changed_file_count = Some(changed_file_count);
                    acc.recheck_worst_cycle_size = top_cycle.as_ref().map(|(_, size)| *size);
                    acc.recheck_worst_cycle_leader = top_cycle
                        .as_ref()
                        .map(|(f, _)| flow_common::files::relative_path(root, &f.to_absolute()));
                }
                acc.recheck_count += 1;
                acc.recheck_dependent_files += dependent_file_count;
                acc.recheck_changed_files += changed_file_count;
                acc.recheck_duration += duration;
            }
        }
    }
    acc
}

#[allow(dead_code)]
fn get_root(state: &ServerState) -> &FilePath {
    &get_ienv(state).i_root
}

fn get_flowconfig(state: &ServerState) -> &FlowConfig {
    &get_ienv(state).i_flowconfig
}

fn get_initialize_params(state: &ServerState) -> &lsp_types::InitializeParams {
    &get_ienv(state).i_initialize_params
}

fn command_key_of_ienv(ienv: &InitializedEnv) -> String {
    let path = lsp_helpers::get_root(&ienv.i_initialize_params)
        .expect("initialized LSP state must have a root");
    let path = path.to_string_lossy();
    let file_uri = lsp_helpers::path_to_lsp_uri(path.as_ref(), "");
    format!("org.flow:{}", file_uri.as_str())
}

fn command_key_of_server_state(state: &ServerState) -> String {
    command_key_of_ienv(get_ienv(state))
}

fn command_key_of_state(state: &State) -> String {
    match state {
        State::PreInit(_) | State::PostShutdown => String::new(),
        State::Initialized(server_state) => command_key_of_server_state(server_state),
    }
}

fn get_open_files(state: &ServerState) -> &UriMap<OpenFileInfo> {
    &get_ienv(state).i_open_files
}

fn update_open_file(
    state: &mut ServerState,
    uri: DocumentUri,
    open_file_info: Option<OpenFileInfo>,
) {
    update_ienv(state, |ienv| match open_file_info {
        Some(info) => {
            ienv.i_open_files.insert(uri, info);
        }
        None => {
            ienv.i_open_files.remove(&uri);
        }
    });
}

fn update_errors(
    state: &mut ServerState,
    f: impl FnOnce(crate::lsp_errors::T) -> crate::lsp_errors::T,
) {
    update_ienv(state, |ienv| {
        let errors = std::mem::replace(&mut ienv.i_errors, crate::lsp_errors::empty());
        ienv.i_errors = f(errors);
    });
}

// Meta-specific property that is set on requests and notifications by our
// internal VS Code extension to provide end-to-end telemetry.
// https://fburl.com/code/wdoqo479
fn new_metadata(state: &State, message: &JsonrpcMessage) -> Metadata {
    let (start_lsp_state, start_lsp_state_reason, start_server_status, start_watcher_status) =
        match state {
            State::Initialized(ServerState::Connected(cenv)) => {
                let (s, w) = &cenv.c_server_status;
                (None, None, Some(s.clone()), w.clone())
            }
            State::Initialized(ServerState::Disconnected(denv))
                if denv.d_server_status.is_some() =>
            {
                let (s, w) = denv.d_server_status.as_ref().unwrap();
                (
                    Some(string_of_state(state).to_string()),
                    None,
                    Some(s.clone()),
                    Some(w.clone()),
                )
            }
            State::Initialized(ServerState::Disconnected(denv)) => (
                Some(string_of_state(state).to_string()),
                Some(denv.d_ienv.i_status.clone()),
                None,
                None,
            ),
            State::PreInit(_) | State::PostShutdown => {
                (Some(string_of_state(state).to_string()), None, None, None)
            }
        };
    let start_lsp_state_reason: Option<String> = match start_lsp_state_reason {
        None | Some(ShowStatusT::NeverShown) => None,
        Some(ShowStatusT::Shown(_, params)) => Some(params.request.message.clone()),
    };
    let lsp_id = message.id.clone();
    let activity_key = message.json.get("activityKey").cloned();
    Metadata {
        start_wall_time: message.timestamp,
        start_server_status,
        start_watcher_status,
        start_json_truncated: json_truncate(&message.json, 256, 4),
        start_lsp_state,
        start_lsp_state_reason,
        lsp_method_name: message.method_.clone(),
        lsp_id,
        activity_key,
        ..lsp_prot::empty_metadata()
    }
}

#[allow(dead_code)]
fn edata_of_exception(exn: &dyn std::error::Error) -> RemoteExceptionData {
    RemoteExceptionData {
        message: exn.to_string(),
        stack: String::new(),
    }
}

fn selectively_omit_errors(
    request_name: &str,
    response: lsp_prot::LspMessage,
) -> lsp_prot::LspMessage {
    use flow_server_env::lsp::LspMessage;
    use flow_server_env::lsp::LspResult;
    match &response {
        LspMessage::ResponseMessage(id, LspResult::ErrorResult(_, _)) => {
            let new_response = match request_name {
                "textDocument/completion" => Some(LspResult::CompletionResult(
                    lsp_types::CompletionResponse::List(lsp_types::CompletionList {
                        is_incomplete: false,
                        items: vec![],
                    }),
                )),
                "textDocument/signatureHelp" => Some(LspResult::SignatureHelpResult(None)),
                "textDocument/documentHighlight" => {
                    Some(LspResult::DocumentHighlightResult(vec![]))
                }
                _ => None,
            };
            match new_response {
                Some(result) => LspMessage::ResponseMessage(id.clone(), result),
                None => response,
            }
        }
        _ => response,
    }
}

#[allow(dead_code)]
fn get_next_event_from_server(
    _flowconfig_name: &str,
    cenv: &ConnectedEnv,
) -> Result<Option<Event>, FlowLspError> {
    let incoming = cenv.c_conn.incoming.lock().unwrap();
    match incoming.try_recv() {
        Ok(Ok(message)) => Ok(Some(Event::ServerMessage(message))),
        Ok(Err(err)) => Err(server_reader_error(err)),
        Err(TryRecvError::Empty) => Ok(None),
        Err(TryRecvError::Disconnected) => Err(server_fatal_connection_exception(
            "Server connection reader thread exited".to_string(),
        )),
    }
}

fn server_reader_error(err: std::io::Error) -> FlowLspError {
    let msg = match err.kind() {
        std::io::ErrorKind::UnexpectedEof
        | std::io::ErrorKind::ConnectionReset
        | std::io::ErrorKind::ConnectionAborted
        | std::io::ErrorKind::BrokenPipe => "End_of_file".to_string(),
        _ => err.to_string(),
    };
    server_fatal_connection_exception(msg)
}

fn event_from_client_message(
    state: &State,
    message: JsonrpcMessage,
    parser: &dyn Fn(&JsonrpcMessage) -> Result<lsp_prot::LspMessage, FlowLspError>,
) -> Result<Event, FlowLspError> {
    let lsp_message =
        parser(&message).map_err(|error| annotate_request_parse_error(&message, error))?;
    let metadata = new_metadata(state, &message);
    Ok(Event::ClientMessage(lsp_message, metadata))
}

#[allow(dead_code)]
async fn get_next_event_from_client(
    state: &State,
    client: &JsonrpcQueue,
    parser: &dyn Fn(&JsonrpcMessage) -> Result<lsp_prot::LspMessage, FlowLspError>,
) -> Result<Event, FlowLspError> {
    let message = client.get_message()?;
    event_from_client_message(state, message, parser)
}

fn get_next_event_sync(
    _flowconfig_name: &str,
    state: &State,
    client: &JsonrpcQueue,
    parser: &dyn Fn(&JsonrpcMessage) -> Result<lsp_prot::LspMessage, FlowLspError>,
) -> Result<Event, FlowLspError> {
    let tick_deadline = std::time::Instant::now() + Duration::from_secs(1);
    if let Some(message) = client.try_get_message() {
        return event_from_client_message(state, message?, parser);
    }

    let server_receiver = match state {
        State::Initialized(ServerState::Connected(cenv)) => {
            Some(cenv.c_conn.incoming.lock().unwrap().clone())
        }
        _ => None,
    };

    let now = std::time::Instant::now();
    if now >= tick_deadline {
        return Ok(Event::Tick);
    }
    let timeout = tick_deadline - now;

    match server_receiver {
        Some(server_receiver) => {
            // OCaml selects the raw client/server file descriptors. The Rust port
            // reads both descriptors on background threads, so a server status burst
            // can keep the server channel ready after the raw socket would already
            // have yielded back to the event loop. Prefer queued client work here so
            // LSP requests and $/cancelRequest notifications are not starved behind
            // queued PleaseHold/status notifications.
            let mut select = crossbeam::channel::Select::new_biased();
            let client_oper = select.recv(client.receiver());
            let server_oper = select.recv(&server_receiver);
            match select.select_timeout(timeout) {
                Ok(oper) if oper.index() == client_oper => match oper.recv(client.receiver()) {
                    Ok(message) => event_from_client_message(state, message?, parser),
                    Err(_) => Err(FlowLspError::ClientFatalConnectionException(
                        RemoteExceptionData {
                            message: "End_of_file".to_string(),
                            stack: String::new(),
                        },
                    )),
                },
                Ok(oper) if oper.index() == server_oper => match oper.recv(&server_receiver) {
                    Ok(Ok(message)) => Ok(Event::ServerMessage(message)),
                    Ok(Err(err)) => Err(server_reader_error(err)),
                    Err(_) => Err(server_fatal_connection_exception(
                        "Server connection reader thread exited".to_string(),
                    )),
                },
                Ok(_) => unreachable!("unexpected LSP select operation"),
                Err(_) => Ok(Event::Tick),
            }
        }
        None => match client.get_message_timeout(timeout) {
            Some(Ok(message)) => event_from_client_message(state, message, parser),
            Some(Err(e)) => Err(e),
            None => Ok(Event::Tick),
        },
    }
}

#[allow(dead_code)]
async fn get_next_event(
    flowconfig_name: &str,
    state: &State,
    client: &JsonrpcQueue,
    parser: &dyn Fn(&JsonrpcMessage) -> Result<lsp_prot::LspMessage, FlowLspError>,
) -> Result<Event, FlowLspError> {
    get_next_event_sync(flowconfig_name, state, client, parser)
}

fn convert_to_client_uris(state: &State, event: Event) -> Event {
    match state {
        State::PreInit(_) | State::PostShutdown => event,
        State::Initialized(server_state) => {
            let ienv = get_ienv(server_state);
            let i_initialize_params = &ienv.i_initialize_params;
            let i_root = &ienv.i_root;
            match event {
                Event::ServerMessage(msg) => {
                    let client_path = lsp_helpers::get_root(i_initialize_params)
                        .expect("initialized LSP state must have a root");
                    let client_path = client_path.to_string_lossy();
                    let client_root = format!(
                        "{}/",
                        lsp_helpers::path_to_lsp_uri(client_path.as_ref(), "").as_str()
                    );
                    let server_path = i_root.to_string_lossy();
                    let server_root = format!(
                        "{}/",
                        lsp_helpers::path_to_lsp_uri(server_path.as_ref(), "").as_str()
                    );
                    let mut mapper = lsp_mapper::default_mapper();
                    let client_root_clone = client_root.clone();
                    let server_root_clone = server_root.clone();
                    mapper.of_document_uri = Box::new(move |_mapper, uri| {
                        let uri_str = uri.as_str().to_string();
                        if let Some(relative) = uri_str.strip_prefix(&server_root_clone) {
                            let new_uri_str = format!("{}{}", client_root_clone, relative);
                            lsp_types::Uri::from_str(&new_uri_str).unwrap_or(uri)
                        } else {
                            uri
                        }
                    });
                    #[allow(clippy::arc_with_non_send_sync)]
                    let mapper = std::sync::Arc::new(mapper);
                    let server_mapper = lsp_prot::default_message_from_server_mapper(mapper);
                    Event::ServerMessage((server_mapper.of_message_from_server)(
                        &server_mapper,
                        msg,
                    ))
                }
                Event::ClientMessage(msg, metadata) => Event::ClientMessage(msg, metadata),
                Event::Tick => Event::Tick,
            }
        }
    }
}

fn convert_to_server_uris(request: lsp_prot::Request) -> lsp_prot::Request {
    let server_uri_of_client_uri = |uri: DocumentUri| -> DocumentUri {
        let path =
            lsp_helpers::lsp_uri_to_path(&uri).expect("convert_to_server_uris: invalid file url");
        let path = FilePath::from(path);
        let canonical = flow_common::files::cached_canonicalize(&path).unwrap_or(path);
        let canonical = canonical.to_string_lossy();
        lsp_helpers::path_to_lsp_uri(canonical.as_ref(), "")
    };
    let mut client_to_server_mapper = lsp_mapper::default_mapper();
    client_to_server_mapper.of_document_uri =
        Box::new(move |_mapper, uri| server_uri_of_client_uri(uri));
    match request {
        lsp_prot::Request::Subscribe => lsp_prot::Request::Subscribe,
        lsp_prot::Request::LspToServer(msg) => {
            let mapped = (client_to_server_mapper.of_lsp_message)(&client_to_server_mapper, msg);
            lsp_prot::Request::LspToServer(mapped)
        }
        lsp_prot::Request::LiveErrorsRequest(uri) => {
            let mapped_uri =
                (client_to_server_mapper.of_document_uri)(&client_to_server_mapper, uri);
            lsp_prot::Request::LiveErrorsRequest(mapped_uri)
        }
    }
}

fn send_request_to_client(
    ienv: &mut InitializedEnv,
    id: LspId,
    request: lsp::LspRequest,
    on_response: LspResultHandler,
    on_error: Box<dyn FnOnce(lsp::error::T, String, &mut ServerState) -> Result<(), FlowLspError>>,
) {
    let json = {
        let key = command_key_of_ienv(ienv);
        lsp_fmt::print_lsp(
            true,
            &key,
            &lsp_prot::LspMessage::RequestMessage(id.clone(), request.clone()),
        )
    };
    to_stdout(&json);

    let handlers = LspHandler {
        on_response,
        on_error,
    };
    ienv.i_outstanding_local_requests
        .insert(id.clone(), request);
    ienv.i_outstanding_local_handlers.insert(id, handlers);
}

fn show_status_params_eq(a: &lsp::show_status::Params, b: &lsp::show_status::Params) -> bool {
    a.request.typ == b.request.typ
        && a.request.message == b.request.message
        && a.progress == b.progress
        && a.total == b.total
        && a.short_message == b.short_message
}

fn should_send_status(ienv: &InitializedEnv, status: &lsp::show_status::Params) -> (bool, bool) {
    let use_status = lsp_helpers::supports_status(&ienv.i_initialize_params);
    let (will_dismiss_old, will_show_new) = match (use_status, &ienv.i_status, status) {
        (_, ShowStatusT::Shown(_, existing_status), status)
            if show_status_params_eq(existing_status, status) =>
        {
            (false, false)
        }
        (true, _, _) => (false, true),
        (false, ShowStatusT::Shown(_, existing), status)
            if existing.request.typ == MessageType::ERROR
                && status.request.typ == MessageType::ERROR =>
        {
            (false, false)
        }
        (false, ShowStatusT::Shown(id, _), status) if status.request.typ == MessageType::ERROR => {
            (id.is_some(), true)
        }
        (false, ShowStatusT::Shown(id, _), _) => (id.is_some(), false),
        (false, ShowStatusT::NeverShown, status) if status.request.typ == MessageType::ERROR => {
            (false, true)
        }
        (false, ShowStatusT::NeverShown, _) => (false, false),
    };
    (will_dismiss_old, will_show_new)
}

fn show_status(
    ienv: &mut InitializedEnv,
    type_: MessageType,
    message: &str,
    short_message: Option<&str>,
    progress: Option<i32>,
    total: Option<i32>,
    titles: &[&str],
    handler: Option<Box<dyn FnOnce(&str, &mut ServerState)>>,
    background_color: Option<lsp::show_status::ShowStatusBackgroundColor>,
) {
    let use_status = lsp_helpers::supports_status(&ienv.i_initialize_params);
    let actions: Vec<lsp_types::MessageActionItem> = titles
        .iter()
        .map(|title| lsp_types::MessageActionItem {
            title: title.to_string(),
            properties: Default::default(),
        })
        .collect();
    let params = lsp::show_status::Params {
        request: lsp_types::ShowMessageRequestParams {
            typ: type_,
            message: message.to_string(),
            actions: Some(actions),
        },
        short_message: short_message.map(|s| s.to_string()),
        progress,
        total,
        background_color,
    };
    let (will_dismiss_old, will_show_new) = should_send_status(ienv, &params);
    // dismiss the old one
    if will_dismiss_old {
        if let ShowStatusT::Shown(Some(id), existing_params) = &ienv.i_status {
            let existing_params = existing_params.clone();
            let id = id.clone();
            let notification =
                lsp::LspNotification::CancelRequestNotification(lsp_types::CancelParams { id });
            let json = {
                let key = command_key_of_ienv(ienv);
                lsp_fmt::print_lsp(
                    true,
                    &key,
                    &lsp_prot::LspMessage::NotificationMessage(notification),
                )
            };
            to_stdout(&json);
            ienv.i_status = ShowStatusT::Shown(None, existing_params);
        }
    }
    // show the new one
    if !will_show_new {
        return;
    }
    let id = NumberOrString::Number(get_next_request_id());
    let request = if use_status {
        lsp::LspRequest::ShowStatusRequest(params.clone())
    } else {
        lsp::LspRequest::ShowMessageRequestRequest(params.request.clone())
    };
    let mark_ienv_shown = |id: LspId, future_ienv: &mut InitializedEnv| {
        if let ShowStatusT::Shown(Some(ref future_id), ref future_params) = future_ienv.i_status {
            if *future_id == id {
                let future_params = future_params.clone();
                future_ienv.i_status = ShowStatusT::Shown(None, future_params);
            }
        }
    };
    let on_error: Box<
        dyn FnOnce(lsp::error::T, String, &mut ServerState) -> Result<(), FlowLspError>,
    > = {
        let id = id.clone();
        Box::new(move |_e, _msg, state| {
            update_ienv(state, |ienv| mark_ienv_shown(id.clone(), ienv));
            Ok(())
        })
    };
    let handle_result: Box<
        dyn FnOnce(lsp::show_status::Result, &mut ServerState) -> Result<(), FlowLspError>,
    > = {
        let id = id.clone();
        Box::new(move |result, state| {
            update_ienv(state, |ienv| mark_ienv_shown(id.clone(), ienv));
            if let Some(title) = result.map(|item| item.title) {
                if let Some(handler) = handler {
                    handler(&title, state);
                }
            }
            Ok(())
        })
    };
    let on_response = if use_status {
        LspResultHandler::ShowStatusHandler(handle_result)
    } else {
        LspResultHandler::ShowMessageHandler(handle_result)
    };
    send_request_to_client(ienv, id.clone(), request, on_response, on_error);
    ienv.i_status = ShowStatusT::Shown(Some(id), params);
}

// calls realpath on every DocumentUri
fn send_to_server(
    env: &ConnectedEnv,
    request: lsp_prot::Request,
    metadata: &Metadata,
) -> Result<(), FlowLspError> {
    // calls realpath on every DocumentUri, because we want the server to only run once, on
    // the canonical files, even if there are multiple clients operating on various symlinks.
    let request = convert_to_server_uris(request);
    let request_with_metadata: lsp_prot::RequestWithMetadata = (request, metadata.clone());
    send_request_with_metadata(&env.c_conn, &request_with_metadata).map_err(|err| {
        let msg = match err.kind() {
            std::io::ErrorKind::UnexpectedEof
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::BrokenPipe => "End_of_file".to_string(),
            _ => err.to_string(),
        };
        server_fatal_connection_exception(msg)
    })
}

fn send_lsp_to_server(
    cenv: &ConnectedEnv,
    metadata: &Metadata,
    message: lsp_prot::LspMessage,
) -> Result<(), FlowLspError> {
    send_to_server(cenv, lsp_prot::Request::LspToServer(message), metadata)
}

fn send_configuration_to_server(
    method_name: &str,
    settings: serde_json::Value,
    cenv: &ConnectedEnv,
) -> Result<(), FlowLspError> {
    use flow_server_env::lsp::LspMessage;
    use flow_server_env::lsp::LspNotification;
    let metadata = Metadata {
        start_wall_time: now(),
        start_json_truncated: serde_json::json!({"method": method_name}),
        lsp_method_name: method_name.to_string(),
        ..lsp_prot::empty_metadata()
    };
    let msg = LspMessage::NotificationMessage(LspNotification::DidChangeConfigurationNotification(
        lsp_types::DidChangeConfigurationParams { settings },
    ));
    send_lsp_to_server(cenv, &metadata, msg)
}

fn request_configuration(ienv: &mut InitializedEnv) {
    let id = NumberOrString::Number(get_next_request_id());
    let request = lsp::LspRequest::ConfigurationRequest(lsp_types::ConfigurationParams {
        items: vec![lsp_types::ConfigurationItem {
            scope_uri: None,
            section: Some("flow".to_string()),
        }],
    });
    let on_response = LspResultHandler::ConfigurationHandler(Box::new(|configs, state| {
        if configs.len() == 1 {
            let i_config = configs
                .into_iter()
                .next()
                .expect("configuration response length was checked");
            // update the lsp process's cache
            let config_clone = i_config.clone();
            update_ienv(state, |ienv| {
                ienv.i_config = config_clone;
            });
            // forward the notification to the server process
            if let ServerState::Connected(cenv) = state {
                send_configuration_to_server("synthetic/didChangeConfiguration", i_config, cenv)?;
            }
        }
        Ok(())
    }));
    let on_error: Box<
        dyn FnOnce(lsp::error::T, String, &mut ServerState) -> Result<(), FlowLspError>,
    > = Box::new(|_e, _msg, _state| Ok(()));
    send_request_to_client(ienv, id, request, on_response, on_error);
}

fn subscribe_to_config_changes(ienv: &mut InitializedEnv) {
    let id = NumberOrString::Number(get_next_request_id());
    let request = lsp::LspRequest::RegisterCapabilityRequest(lsp::register_capability::Params {
        registrations: vec![lsp::register_capability::Registration {
            id: "didChangeConfiguration".to_string(),
            method: "workspace/didChangeConfiguration".to_string(),
            register_options: lsp::register_capability::Options::DidChangeConfiguration,
        }],
    });
    let on_response = LspResultHandler::VoidHandler;
    let on_error: Box<
        dyn FnOnce(lsp::error::T, String, &mut ServerState) -> Result<(), FlowLspError>,
    > = Box::new(|_e, _msg, _state| Ok(()));
    send_request_to_client(ienv, id, request, on_response, on_error);
}

fn do_initialize(params: &lsp_types::InitializeParams) -> lsp_types::InitializeResult {
    let supported_kinds = lsp_helpers::supports_code_action_kinds(params);
    let supports_quickfixes = supported_kinds.iter().any(|k| k == "quickfix");
    let supports_refactor_extract = supported_kinds.iter().any(|k| k == "refactor.extract");
    let supports_source_actions = supported_kinds.iter().any(|k| k == "source");
    let mut supported_code_action_kinds = vec![];
    if supports_source_actions {
        supported_code_action_kinds.push(lsp_types::CodeActionKind::new(
            "source.addMissingImports.flow",
        ));
        supported_code_action_kinds.push(lsp_types::CodeActionKind::new(
            "source.organizeImports.flow",
        ));
    }
    if supports_refactor_extract {
        supported_code_action_kinds.push(lsp_types::CodeActionKind::REFACTOR_EXTRACT);
    }
    if supports_quickfixes {
        supported_code_action_kinds.push(lsp_types::CodeActionKind::QUICKFIX);
    }
    let code_action_provider = if !supported_code_action_kinds.is_empty() {
        Some(lsp_types::CodeActionProviderCapability::Options(
            lsp_types::CodeActionOptions {
                code_action_kinds: Some(supported_code_action_kinds),
                ..Default::default()
            },
        ))
    } else {
        Some(lsp_types::CodeActionProviderCapability::Simple(false))
    };
    let text_document_sync = Some(lsp_types::TextDocumentSyncCapability::Options(
        lsp_types::TextDocumentSyncOptions {
            open_close: Some(true),
            change: Some(lsp_types::TextDocumentSyncKind::INCREMENTAL),
            will_save: Some(false),
            will_save_wait_until: Some(false),
            save: Some(lsp_types::TextDocumentSyncSaveOptions::SaveOptions(
                lsp_types::SaveOptions {
                    include_text: Some(false),
                },
            )),
        },
    ));
    let server_snippet_text_edit = lsp_helpers::supports_experimental_snippet_text_edit(params);
    let experimental = serde_json::json!({
        "strictCompletionOrder": true,
        "autoCloseJsx": true,
        "pasteProvider": true,
        "renameFileImports": true,
        "snippetTextEdit": server_snippet_text_edit,
    });
    lsp_types::InitializeResult {
        capabilities: lsp_types::ServerCapabilities {
            text_document_sync,
            hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
            completion_provider: Some(lsp_types::CompletionOptions {
                resolve_provider: Some(false),
                trigger_characters: Some(vec![
                    ".".to_string(),
                    "[".to_string(),
                    " ".to_string(),
                    "<".to_string(),
                    "/".to_string(),
                    "\"".to_string(),
                    "'".to_string(),
                    "#".to_string(),
                    "*".to_string(),
                ]),
                completion_item: Some(lsp_types::CompletionOptionsCompletionItem {
                    label_details_support: Some(true),
                }),
                ..Default::default()
            }),
            signature_help_provider: Some(lsp_types::SignatureHelpOptions {
                trigger_characters: Some(vec!["(".to_string(), ",".to_string(), "{".to_string()]),
                ..Default::default()
            }),
            definition_provider: Some(lsp_types::OneOf::Left(true)),
            type_definition_provider: Some(lsp_types::TypeDefinitionProviderCapability::Simple(
                false,
            )),
            references_provider: Some(lsp_types::OneOf::Left(true)),
            document_highlight_provider: Some(lsp_types::OneOf::Left(true)),
            document_symbol_provider: Some(lsp_types::OneOf::Left(true)),
            workspace_symbol_provider: Some(lsp_types::OneOf::Left(true)),
            code_action_provider,
            code_lens_provider: None,
            document_formatting_provider: Some(lsp_types::OneOf::Left(false)),
            document_range_formatting_provider: Some(lsp_types::OneOf::Left(false)),
            document_on_type_formatting_provider: None,
            rename_provider: Some(lsp_types::OneOf::Right(lsp_types::RenameOptions {
                prepare_provider: Some(true),
                work_done_progress_options: Default::default(),
            })),
            document_link_provider: None,
            execute_command_provider: Some(lsp_types::ExecuteCommandOptions {
                commands: vec![
                    "log".to_string(),
                    "source.addMissingImports".to_string(),
                    "source.organizeImports".to_string(),
                ],
                ..Default::default()
            }),
            implementation_provider: Some(lsp_types::ImplementationProviderCapability::Simple(
                false,
            )),
            selection_range_provider: Some(lsp_types::SelectionRangeProviderCapability::Simple(
                true,
            )),
            linked_editing_range_provider: Some(
                lsp_types::LinkedEditingRangeServerCapabilities::Simple(true),
            ),
            workspace: Some(lsp_types::WorkspaceServerCapabilities {
                file_operations: Some(Default::default()),
                ..Default::default()
            }),
            experimental: Some(experimental),
            ..Default::default()
        },
        server_info: Some(lsp_types::ServerInfo {
            name: "Flow".to_string(),
            version: Some(flow_version().to_string()),
        }),
    }
}

fn message_with_flow_and_root_name_prefix(flowconfig: &FlowConfig, msg: &str) -> String {
    match &flowconfig.options.root_name {
        None => format!("Flow: {}", msg),
        Some(root) => format!("Flow ({}): {}", root, msg),
    }
}

fn show_connected_status(cenv: &mut ConnectedEnv) {
    let flowconfig = cenv.c_ienv.i_flowconfig.clone();
    let message_with_prefix = |msg: &str| message_with_flow_and_root_name_prefix(&flowconfig, msg);
    let (type_, message, short_message, progress, total) = if cenv.c_is_rechecking {
        let (ref server_status, _) = cenv.c_server_status;
        if !server_status::is_free(server_status) {
            let (sm, progress, total) = server_status::get_progress(server_status);
            let message = message_with_prefix(&server_status::string_of_status(
                false,
                false,
                server_status,
            ));
            (
                MessageType::WARNING,
                message,
                sm.map(|s| message_with_prefix(&s)),
                progress,
                total,
            )
        } else {
            (
                MessageType::WARNING,
                message_with_prefix("Server is rechecking..."),
                None,
                None,
                None,
            )
        }
    } else {
        let (_, ref watcher_status) = cenv.c_server_status;
        match watcher_status {
            Some((_, file_watcher_status::StatusKind::Deferred { reason })) => {
                let message = format!("Waiting for {} to finish", reason);
                let short_message = Some(message_with_prefix("blocked"));
                (MessageType::WARNING, message, short_message, None, None)
            }
            _ => {
                let message = match &cenv.c_lazy_stats {
                    Some(stats) if stats.checked_files < stats.total_files && stats.lazy_mode => {
                        format!(
                            "Flow is ready. Checking {}/{} files (lazy mode)",
                            stats.checked_files, stats.total_files,
                        )
                    }
                    _ => "Flow is ready.".to_string(),
                };
                let short_message = Some(message_with_prefix("ready"));
                (MessageType::INFO, message, short_message, None, None)
            }
        }
    };
    show_status(
        &mut cenv.c_ienv,
        type_,
        &message,
        short_message.as_deref(),
        progress,
        total,
        &[],
        None,
        None,
    );
}

fn show_connected(mut env: ConnectedEnv) -> ServerState {
    // report that we're connected to telemetry/connectionStatus
    let i_is_connected = lsp_writers::notify_connection_status(
        &env.c_ienv.i_initialize_params,
        to_stdout,
        env.c_ienv.i_is_connected,
        true,
    );
    env.c_ienv.i_is_connected = i_is_connected;
    // show green status
    show_connected_status(&mut env);
    ServerState::Connected(env)
}

fn show_connecting(reason: &ConnectError, mut env: DisconnectedEnv) -> ServerState {
    if *reason == ConnectError::ServerMissing {
        lsp_writers::log_info(to_stdout, "Starting Flow server");
    }
    let flowconfig = env.d_ienv.i_flowconfig.clone();
    let message_with_prefix = |msg: &str| message_with_flow_and_root_name_prefix(&flowconfig, msg);
    let (message, short_message, progress, total) = match (reason, &env.d_server_status) {
        (ConnectError::ServerMissing, _) => {
            (message_with_prefix("Server starting"), None, None, None)
        }
        (ConnectError::ServerSocketMissing, _) => {
            (message_with_prefix("Server starting?"), None, None, None)
        }
        (ConnectError::BuildIdMismatch(BuildIdMismatchKind::ServerExited), _) => (
            message_with_prefix("Server was wrong version and exited"),
            None,
            None,
            None,
        ),
        (ConnectError::BuildIdMismatch(BuildIdMismatchKind::ClientShouldError { .. }), _) => (
            message_with_prefix("Server is wrong version"),
            None,
            None,
            None,
        ),
        (ConnectError::ServerBusy(ServerBusyKind::TooManyClients), _) => {
            (message_with_prefix("Server busy"), None, None, None)
        }
        (ConnectError::ServerBusy(_), None) => {
            (message_with_prefix("Server busy"), None, None, None)
        }
        (ConnectError::ServerBusy(_), Some((server_status, watcher_status))) => {
            if !server_status::is_free(server_status) {
                let (sm, progress, total) = server_status::get_progress(server_status);
                (
                    message_with_prefix(&server_status::string_of_status(
                        false,
                        false,
                        server_status,
                    )),
                    sm,
                    progress,
                    total,
                )
            } else {
                (
                    message_with_prefix(&file_watcher_status::string_of_status(watcher_status)),
                    None,
                    None,
                    None,
                )
            }
        }
    };
    show_status(
        &mut env.d_ienv,
        MessageType::WARNING,
        &message,
        short_message.as_deref(),
        progress,
        total,
        &[],
        None,
        None,
    );
    ServerState::Disconnected(env)
}

fn show_disconnected(
    code: Option<&FlowExitStatus>,
    message: Option<&str>,
    mut env: DisconnectedEnv,
) -> ServerState {
    // report that we're disconnected to telemetry/connectionStatus
    let i_is_connected = lsp_writers::notify_connection_status(
        &env.d_ienv.i_initialize_params,
        to_stdout,
        env.d_ienv.i_is_connected,
        false,
    );
    env.d_ienv.i_is_connected = i_is_connected;
    let flowconfig = env.d_ienv.i_flowconfig.clone();
    let message_with_prefix = |msg: &str| message_with_flow_and_root_name_prefix(&flowconfig, msg);
    // show red status
    let message = message
        .map(|m| m.to_string())
        .unwrap_or_else(|| message_with_prefix("server is stopped"));
    let message = match code {
        Some(code) => format!(
            "{} [{}]",
            message,
            flow_common_exit_status::to_string(*code)
        ),
        None => message,
    };
    let handler: Box<dyn FnOnce(&str, &mut ServerState)> =
        Box::new(|r: &str, state: &mut ServerState| {
            if let ServerState::Disconnected(e) = state {
                if r == "Restart" {
                    e.d_autostart = true;
                }
            }
        });
    show_status(
        &mut env.d_ienv,
        MessageType::ERROR,
        &message,
        None,
        None,
        None,
        &["Restart"],
        Some(handler),
        Some(lsp::show_status::ShowStatusBackgroundColor::Error),
    );
    ServerState::Disconnected(env)
}

fn close_conn(env: &ConnectedEnv) {
    if let Ok(stream) = env.c_conn.stream.lock() {
        match stream.shutdown() {
            Ok(()) | Err(_) => (),
        }
    }
    flow_commands_connect::command_connect_simple::close_connection(&env.c_conn.sockaddr);
}

fn track_to_server(
    state: &mut ServerState,
    c: &lsp_prot::LspMessage,
) -> Result<TrackEffect, FlowLspError> {
    use flow_server_env::lsp::LspMessage;
    use flow_server_env::lsp::LspNotification;
    let changed_live_uri = match c {
        LspMessage::NotificationMessage(LspNotification::DidOpenNotification(params)) => {
            let o_open_doc = params.text_document.clone();
            let uri = params.text_document.uri.clone();
            update_open_file(
                state,
                uri.clone(),
                Some(OpenFileInfo {
                    o_open_doc,
                    o_ast: None,
                    o_unsaved: false,
                }),
            );
            Some(uri)
        }
        LspMessage::NotificationMessage(LspNotification::DidCloseNotification(params)) => {
            let uri = params.text_document.uri.clone();
            update_open_file(state, uri.clone(), None);
            update_errors(state, |errors| {
                crate::lsp_errors::clear_all_live_errors_and_send(
                    &mut |v| to_stdout(&v),
                    &uri,
                    errors,
                )
            });
            None
        }
        LspMessage::NotificationMessage(LspNotification::DidChangeNotification(params)) => {
            let uri = params.text_document.uri.clone();
            let open_files = get_open_files(state);
            let o_open_doc = match open_files.get(&uri) {
                Some(info) => info.o_open_doc.clone(),
                None => return Err(FlowLspError::ChangedFileNotOpen(uri)),
            };
            let text = lsp_helpers::apply_changes_unsafe(&o_open_doc.text, &params.content_changes);
            let o_open_doc = lsp_types::TextDocumentItem {
                uri: uri.clone(),
                language_id: o_open_doc.language_id.clone(),
                version: params.text_document.version,
                text,
            };
            update_open_file(
                state,
                uri.clone(),
                Some(OpenFileInfo {
                    o_open_doc,
                    o_ast: None,
                    o_unsaved: true,
                }),
            );
            if let ServerState::Connected(_) = state {
                update_errors(state, |errors| {
                    crate::lsp_errors::update_errors_due_to_change_and_send(
                        &mut |v| to_stdout(&v),
                        params,
                        errors,
                    )
                });
            }
            Some(uri)
        }
        LspMessage::NotificationMessage(LspNotification::DidSaveNotification(params)) => {
            let uri = params.text_document.uri.clone();
            let open_files = get_open_files(state);
            let open_file = open_files.get(&uri).cloned();
            if let Some(mut open_file) = open_file {
                open_file.o_unsaved = false;
                update_open_file(state, uri.clone(), Some(open_file));
            }
            Some(uri)
        }
        _ => None,
    };
    match (state, c) {
        (ServerState::Connected(env), LspMessage::RequestMessage(id, _)) => {
            env.c_outstanding_requests_to_server.insert(id.clone());
        }
        (ServerState::Connected(env), LspMessage::ResponseMessage(id, _)) => {
            let wrapped = decode_wrapped(id);
            env.c_ienv
                .i_outstanding_requests_from_server
                .remove(&wrapped);
        }
        _ => {}
    }
    Ok(TrackEffect { changed_live_uri })
}

fn track_from_server(state: &mut ServerState, c: &lsp_prot::LspMessage) {
    use flow_server_env::lsp::LspMessage;
    match (state, c) {
        (ServerState::Connected(env), LspMessage::ResponseMessage(id, _)) => {
            env.c_outstanding_requests_to_server.remove(id);
        }
        (ServerState::Connected(env), LspMessage::RequestMessage(id, params)) => {
            let wrapped = WrappedId {
                server_id: env.c_ienv.i_server_id,
                message_id: id.clone(),
            };
            env.c_ienv
                .i_outstanding_requests_from_server
                .insert(wrapped, params.clone());
        }
        (_, _) => {}
    }
}

fn lsp_document_item_to_flow(
    open_doc: &lsp_types::TextDocumentItem,
) -> Result<FileInput, FlowLspError> {
    let uri = &open_doc.uri;
    let fn_ = lsp_helpers::lsp_uri_to_path(uri).map_err(FlowLspError::LspException)?;
    let fn_ = sys_utils_realpath(&fn_).unwrap_or(fn_);
    Ok(FileInput::FileContent(
        Some(fn_),
        open_doc.text.clone().into(),
    ))
}

fn diagnostic_of_parse_error(
    loc: &flow_parser::loc::Loc,
    parse_error: &flow_parser::parse_error::ParseError,
) -> lsp_types::Diagnostic {
    lsp_types::Diagnostic {
        range: loc_to_lsp_range(loc),
        severity: Some(lsp_types::DiagnosticSeverity::ERROR),
        code: Some(lsp_types::NumberOrString::String("ParseError".to_string())),
        source: Some("Flow".to_string()),
        tags: None,
        message: parse_error.to_string(),
        related_information: None,
        code_description: None,
        data: None,
    }
}

fn live_syntax_errors_enabled(state: &ServerState) -> bool {
    let params = get_initialize_params(state);
    params
        .initialization_options
        .as_ref()
        .and_then(|opts| opts.get("liveSyntaxErrors"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

fn parse_and_cache(
    state: &mut ServerState,
    uri: &DocumentUri,
) -> Result<
    (
        flow_parser::ast::Program<flow_parser::loc::Loc, flow_parser::loc::Loc>,
        Option<Vec<lsp_types::Diagnostic>>,
    ),
    FlowLspError,
> {
    let flowconfig = get_flowconfig(state);
    let use_strict = flowconfig.options.modules_are_use_strict;
    let module_ref_prefix = flowconfig.options.haste_module_ref_prefix.clone();
    let parse_options = Some(flow_parser::ParseOptions {
        use_strict,
        module_ref_prefix: module_ref_prefix.map(|s| s.into()),
        ..flow_parser::PERMISSIVE_PARSE_OPTIONS
    });

    let parse = |file: &FileInput| -> (
        flow_parser::ast::Program<flow_parser::loc::Loc, flow_parser::loc::Loc>,
        Option<Vec<lsp_types::Diagnostic>>,
    ) {
        let content = file.content_of_file_input_unsafe();
        let filename_opt = file.path_of_file_input();
        let filekey = filename_opt.map(flow_parser::file_key::FileKey::source_file_of_absolute);
        let (program, errors) = match filekey {
            Some(fk) => flow_parser::parse_program_file::<()>(
                false,
                None,
                parse_options.clone(),
                fk,
                Ok(content.as_str()),
            ),
            None => flow_parser::parse_program_without_file(
                false,
                None,
                parse_options.clone(),
                Ok(content.as_str()),
            ),
        };
        (
            program,
            if live_syntax_errors_enabled(state) {
                Some(
                    errors
                        .iter()
                        .map(|(loc, err)| diagnostic_of_parse_error(loc, err))
                        .collect(),
                )
            } else {
                None
            },
        )
    };

    let open_file_info_to_parse = match get_open_files(state).get(uri) {
        Some(info) => match &info.o_ast {
            Some(o_ast) => return Ok(o_ast.clone()),
            None => Some((info.o_open_doc.clone(), info.o_unsaved)),
        },
        None => None,
    };
    match open_file_info_to_parse {
        Some((o_open_doc, o_unsaved)) => {
            let file = lsp_document_item_to_flow(&o_open_doc)?;
            let o_ast = parse(&file);
            let open_file_info = Some(OpenFileInfo {
                o_open_doc,
                o_ast: Some(o_ast.clone()),
                o_unsaved,
            });
            update_open_file(state, uri.clone(), open_file_info);
            Ok(o_ast)
        }
        None => {
            let fn_ = lsp_helpers::lsp_uri_to_path(uri).map_err(FlowLspError::LspException)?;
            let fn_ = sys_utils_realpath(&fn_).unwrap_or(fn_);
            let file = FileInput::FileName(fn_);
            let (open_ast, _) = parse(&file);
            Ok((open_ast, None))
        }
    }
}

fn do_document_symbol(
    state: &mut ServerState,
    _id: &LspId,
    params: &lsp_types::DocumentSymbolParams,
) -> Result<(), FlowLspError> {
    use flow_server_env::lsp::LspMessage;
    use flow_server_env::lsp::LspResult;
    use flow_server_env::lsp::document_symbol_result;
    let uri = &params.text_document.uri;
    let (ast, _live_parse_errors) = parse_and_cache(state, uri)?;
    let supports_hierarchical =
        lsp_helpers::supports_hierarchical_document_symbol(get_initialize_params(state));
    let result = if supports_hierarchical {
        document_symbol_result::T::DocumentSymbol(
            crate::document_symbol_provider::provide_document_symbols(&ast),
        )
    } else {
        document_symbol_result::T::SymbolInformation(
            crate::document_symbol_provider::provide_symbol_information(uri, &ast),
        )
    };
    let response =
        LspMessage::ResponseMessage(_id.clone(), LspResult::DocumentSymbolResult(result));
    let key = command_key_of_server_state(state);
    let json = lsp_fmt::print_lsp(true, &key, &response);
    to_stdout(&json);
    Ok(())
}

fn do_selection_range(
    state: &mut ServerState,
    _id: &LspId,
    params: &lsp_types::SelectionRangeParams,
) -> Result<(), FlowLspError> {
    use flow_server_env::lsp::LspMessage;
    use flow_server_env::lsp::LspResult;
    let uri = &params.text_document.uri;
    let positions = &params.positions;
    let (ast, _live_parse_errors) = parse_and_cache(state, uri)?;
    let response = crate::selection_range_provider::provide_selection_ranges(positions, &ast);
    let lsp_result = match response {
        Ok(ranges) => LspResult::SelectionRangeResult(ranges),
        Err(msg) => LspResult::ErrorResult(
            lsp::error::T {
                code: lsp::error::Code::InternalError,
                message: msg,
                data: None,
            },
            String::new(),
        ),
    };
    let outgoing = LspMessage::ResponseMessage(_id.clone(), lsp_result);
    let key = command_key_of_server_state(state);
    let json = lsp_fmt::print_lsp(false, &key, &outgoing);
    to_stdout(&json);
    Ok(())
}

fn do_rage(flowconfig_name: &str, state: &ServerState) -> Vec<RageItem> {
    // Mirrors OCaml `do_rage` in `flowLsp.ml`: collect log files and the LSP
    // adapter state. Uses `add_file` to read each file (capping at 10MB) and
    // `add_string` for the stringified state.
    fn add_file(items: &mut Vec<RageItem>, file: &std::path::Path) {
        const MAX_LEN: usize = 10 * 1024 * 1024;
        let title = Some(file.to_string_lossy().to_string());
        let data = if file.exists() {
            match std::fs::read_to_string(file) {
                Ok(data) => {
                    if data.len() <= MAX_LEN {
                        data
                    } else {
                        data[data.len() - MAX_LEN..].to_string()
                    }
                }
                Err(e) => format!("Failed to read file: {}", e),
            }
        } else {
            format!("File not found: {}", file.display())
        };
        items.push(RageItem { title, data });
    }
    let mut items: Vec<RageItem> = vec![];
    let ienv = get_ienv(state);
    let root = ienv.i_root.clone();
    let tmp_dir = connect_temp_dir(&ienv.i_connect_params);
    let server_log_file =
        flow_server_files::server_files_js::log_file(flowconfig_name, &tmp_dir, &root);
    let monitor_log_file =
        flow_server_files::server_files_js::monitor_log_file(flowconfig_name, &tmp_dir, &root);
    add_file(&mut items, std::path::Path::new(&server_log_file));
    add_file(&mut items, std::path::Path::new(&monitor_log_file));
    // Also pick up the rotated `.old` siblings — useful if the user reports a
    // bug after a crash.
    add_file(
        &mut items,
        std::path::Path::new(&format!("{}.old", server_log_file)),
    );
    add_file(
        &mut items,
        std::path::Path::new(&format!("{}.old", monitor_log_file)),
    );
    items.push(RageItem {
        title: None,
        data: format!(
            "LSP adapter state: {}\n",
            crate::rage_print::string_of_state(state)
        ),
    });
    items
}

fn parse_json(state: &State, msg: &JsonrpcMessage) -> Result<lsp_prot::LspMessage, FlowLspError> {
    let json = &msg.json;
    let internal_error = |message: String| flow_server_env::lsp::error::T {
        code: flow_server_env::lsp::error::Code::InternalError,
        message,
        data: None,
    };
    let outstanding = |id: &LspId| -> Result<lsp::LspRequest, flow_server_env::lsp::error::T> {
        let ienv = match state {
            State::PreInit(_) => {
                return Err(internal_error(format!(
                    "Unexpected LSP response before init: {}",
                    json
                )));
            }
            State::PostShutdown => {
                return Err(internal_error(format!(
                    "Unexpected LSP response after shutdown: {}",
                    json
                )));
            }
            State::Initialized(server_state) => get_ienv(server_state),
        };
        if let Some(msg) = ienv.i_outstanding_local_requests.get(id) {
            return Ok(msg.clone());
        }
        let wrapped = try_decode_wrapped(id)?;
        if let Some(msg) = ienv.i_outstanding_requests_from_server.get(&wrapped) {
            return Ok(msg.clone());
        }
        Err(internal_error(format!(
            "Wasn't expecting a response to request {}",
            lsp_fmt::id_to_string(id)
        )))
    };
    Ok(lsp_fmt::parse_lsp(json, &outstanding)?)
}

fn with_timer<T>(f: impl FnOnce() -> T) -> (f64, T) {
    let start = now();
    let ret = f();
    let duration = now() - start;
    (duration, ret)
}

fn collect_interaction_state(state: &ServerState) -> InteractionState {
    let time = now();
    let files = get_open_files(state);
    let buffer_status = if files.is_empty() {
        InteractionBufferStatus::NoOpenBuffers
    } else if files.values().any(|file| file.o_unsaved) {
        InteractionBufferStatus::UnsavedBuffers
    } else {
        InteractionBufferStatus::NoUnsavedBuffers
    };
    let server_status = match state {
        ServerState::Disconnected(disconnected_env) => {
            if disconnected_env.d_server_status.is_none() {
                InteractionServerStatus::Stopped
            } else {
                InteractionServerStatus::Initializing
            }
        }
        ServerState::Connected(connected_env) => {
            if connected_env.c_is_rechecking {
                InteractionServerStatus::Rechecking
            } else {
                InteractionServerStatus::Ready
            }
        }
    };
    InteractionState {
        time,
        server_status,
        buffer_status,
    }
}

fn gc_pending_interactions(state: &State) {
    use std::sync::atomic::AtomicU64;
    use std::sync::atomic::Ordering;
    static NEXT_GC: AtomicU64 = AtomicU64::new(0);
    match state {
        State::PreInit(_) | State::PostShutdown => {}
        State::Initialized(server_state) => {
            let next_gc = f64::from_bits(NEXT_GC.load(Ordering::SeqCst));
            if now() >= next_gc {
                let new_next_gc = lsp_interaction::gc(&|| collect_interaction_state(server_state));
                NEXT_GC.store(new_next_gc.to_bits(), Ordering::SeqCst);
            }
        }
    }
}

fn start_interaction(trigger: InteractionTrigger, state: &ServerState) -> InteractionId {
    let start_state = collect_interaction_state(state);
    lsp_interaction::start(start_state, trigger)
}

fn log_interaction(ux: InteractionUx, state: &ServerState, id: InteractionId) {
    let end_state = collect_interaction_state(state);
    lsp_interaction::log(end_state, ux, id)
}

fn dismiss_tracks(state: &mut ServerState) {
    let key = command_key_of_server_state(state);
    let decline_request_to_server = |id: &LspId| {
        let e = flow_server_env::lsp::error::T {
            code: flow_server_env::lsp::error::Code::RequestCancelled,
            message: "Connection to server has been lost".to_string(),
            data: None,
        };
        let result = flow_server_env::lsp::LspResult::ErrorResult(e, String::new());
        let json = lsp_fmt::print_lsp_response(true, &key, id, &result);
        to_stdout(&json);
    };
    lsp_interaction::dismiss_tracks(collect_interaction_state(state));
    match state {
        ServerState::Connected(env) => {
            let server_id = env.c_ienv.i_server_id;
            for wrapped in env.c_ienv.i_outstanding_requests_from_server.keys() {
                if server_id == wrapped.server_id {
                    let id = encode_wrapped(wrapped);
                    let notification =
                        flow_server_env::lsp::LspNotification::CancelRequestNotification(
                            lsp_types::CancelParams { id },
                        );
                    let json = lsp_fmt::print_lsp_notification(&notification);
                    to_stdout(&json);
                }
            }
            for id in &env.c_outstanding_requests_to_server.clone() {
                decline_request_to_server(id);
            }
            env.c_outstanding_requests_to_server.clear();
            update_errors(state, |errors| {
                crate::lsp_errors::clear_all_errors_and_send(&mut |v| to_stdout(&v), errors)
            });
        }
        ServerState::Disconnected(_) => {}
    }
}

fn do_live_diagnostics(
    state: &mut ServerState,
    trigger: Option<InteractionTrigger>,
    metadata: &Metadata,
    uri: &DocumentUri,
) -> Result<(), FlowLspError> {
    let file_path = lsp_helpers::lsp_uri_to_path(uri).map_err(FlowLspError::LspException)?;
    let (is_ignored, _) =
        flow_common::files::is_ignored(&get_ienv(state).i_file_options, &file_path);
    if is_ignored {
        return Ok(());
    }
    let trigger = trigger.unwrap_or(InteractionTrigger::UnknownTrigger);
    if let ServerState::Connected(cenv) = &*state {
        let mut metadata = metadata.clone();
        metadata.interaction_tracking_id = Some(start_interaction(trigger.clone(), state));
        if let Err(err) = send_to_server(
            cenv,
            lsp_prot::Request::LiveErrorsRequest(uri.clone()),
            &metadata,
        ) {
            eprintln!("{}", err);
        }
    }

    let interaction_id = start_interaction(trigger, state);
    let (_, live_parse_errors) = parse_and_cache(state, uri)?;
    let ux = match live_parse_errors {
        None => InteractionUx::ErroredPushingLiveParseErrors,
        Some(live_parse_errors) => {
            update_errors(state, |errors| {
                crate::lsp_errors::set_live_parse_errors_and_send(
                    &mut |v| to_stdout(&v),
                    uri,
                    live_parse_errors,
                    errors,
                )
            });
            InteractionUx::PushedLiveParseErrors(uri.clone())
        }
    };
    log_interaction(ux, state, interaction_id);
    Ok(())
}

fn get_local_request_handler(
    ienv: &mut InitializedEnv,
    id: &LspId,
    result: lsp::LspResult,
) -> Result<Option<Box<dyn FnOnce(&mut ServerState) -> Result<(), FlowLspError>>>, FlowLspError> {
    let Some(handler) = ienv.i_outstanding_local_handlers.get(id) else {
        return Ok(None);
    };
    let is_mistyped = !matches!(
        (&result, &handler.on_response),
        (
            lsp::LspResult::ShowMessageRequestResult(_),
            LspResultHandler::ShowMessageHandler(_),
        ) | (
            lsp::LspResult::ShowStatusResult(_),
            LspResultHandler::ShowStatusHandler(_),
        ) | (
            lsp::LspResult::ApplyWorkspaceEditResult(_),
            LspResultHandler::ApplyWorkspaceEditHandler(_),
        ) | (
            lsp::LspResult::ConfigurationResult(_),
            LspResultHandler::ConfigurationHandler(_),
        ) | (
            lsp::LspResult::RegisterCapabilityResult,
            LspResultHandler::VoidHandler,
        ) | (lsp::LspResult::ErrorResult(_, _), _)
    );
    if is_mistyped {
        return Err(internal_error_exception(format!(
            "Response {} has mistyped handler",
            lsp_fmt::result_name_to_string(&result)
        )));
    }
    let handler = ienv
        .i_outstanding_local_handlers
        .remove(id)
        .expect("handler existed during typed validation");
    ienv.i_outstanding_local_requests.remove(id);
    let LspHandler {
        on_response,
        on_error,
    } = handler;
    let handler: Box<dyn FnOnce(&mut ServerState) -> Result<(), FlowLspError>> =
        match (result, on_response) {
            (
                lsp::LspResult::ShowMessageRequestResult(result),
                LspResultHandler::ShowMessageHandler(handle),
            ) => Box::new(move |state| handle(result, state)),
            (
                lsp::LspResult::ShowStatusResult(result),
                LspResultHandler::ShowStatusHandler(handle),
            ) => Box::new(move |state| handle(result, state)),
            (
                lsp::LspResult::ApplyWorkspaceEditResult(result),
                LspResultHandler::ApplyWorkspaceEditHandler(handle),
            ) => Box::new(move |state| handle(result, state)),
            (
                lsp::LspResult::ConfigurationResult(result),
                LspResultHandler::ConfigurationHandler(handle),
            ) => Box::new(move |state| handle(result, state)),
            (lsp::LspResult::RegisterCapabilityResult, LspResultHandler::VoidHandler) => {
                Box::new(|_state| Ok(()))
            }
            (lsp::LspResult::ErrorResult(e, msg), _) => {
                Box::new(move |state| on_error(e, msg, state))
            }
            (result, _) => {
                return Err(internal_error_exception(format!(
                    "Response {} has mistyped handler",
                    lsp_fmt::result_name_to_string(&result)
                )));
            }
        };
    Ok(Some(handler))
}

fn try_connect(
    version_mismatch_strategy: &socket_handshake::VersionMismatchStrategy,
    flowconfig_name: &str,
    env: DisconnectedEnv,
) -> ServerState {
    let flowconfig = read_flowconfig_from_disk(flowconfig_name, &env.d_ienv.i_root);
    // If the version in .flowconfig has changed under our feet then we mustn't
    // connect. We'll terminate and trust the editor to relaunch an ok version.
    let current_version = flowconfig.version.clone();
    if env.d_ienv.i_version != current_version {
        let prev_version_str = env.d_ienv.i_version.as_deref().unwrap_or("[None]");
        let current_version_str = current_version.as_deref().unwrap_or("[None]");
        let message = format!(
            "\nVersion in flowconfig that spawned the existing flow server: {}\nVersion in flowconfig currently: {}\n",
            prev_version_str, current_version_str
        );
        lsp_writers::telemetry_log(to_stdout, &message);
        lsp_exit_bad();
    }
    let _start_env_flowconfig = &flowconfig;
    let _start_env_flowconfig_name = flowconfig_name;
    let _start_env_connect_params = &env.d_ienv.i_connect_params;
    let _start_env_root = &env.d_ienv.i_root;

    let client_handshake = lsp_connect_params::persistent_client_handshake(
        version_mismatch_strategy,
        env.d_ienv.i_initialize_params.clone(),
    );

    let conn = classify_persistent_connect(flowconfig_name, &env, &client_handshake);

    #[allow(unreachable_code)]
    match conn {
        Ok(conn) => {
            let i_server_id = env.d_ienv.i_server_id + 1;
            // this flag is set to false to prevent restart loops when the flowconfig changes.
            // once we successfully reconnect, it should be reset back to true (the default).
            let i_can_autostart_after_version_mismatch = true;
            let DisconnectedEnv { mut d_ienv, .. } = env;
            d_ienv.i_server_id = i_server_id;
            d_ienv.i_can_autostart_after_version_mismatch = i_can_autostart_after_version_mismatch;
            let new_env = ConnectedEnv {
                c_ienv: d_ienv,
                c_conn: conn,
                c_server_status: (server_status::INITIAL_STATUS, None),
                c_about_to_exit_code: None,
                c_is_rechecking: false,
                c_lazy_stats: None,
                c_outstanding_requests_to_server: HashSet::new(),
                c_recent_summaries: vec![],
            };
            let server_status_for_metadata = new_env.c_server_status.0.clone();
            let watcher_status_for_metadata = new_env.c_server_status.1.clone();
            let make_metadata = |method_name: &str| -> Metadata {
                Metadata {
                    start_wall_time: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs_f64(),
                    start_server_status: Some(server_status_for_metadata.clone()),
                    start_watcher_status: watcher_status_for_metadata.clone(),
                    start_json_truncated: serde_json::json!({"method": method_name}),
                    lsp_method_name: method_name.to_string(),
                    ..lsp_prot::empty_metadata()
                }
            };
            // send the initial messages to the server
            let metadata = make_metadata("synthetic/subscribe");
            if let Err(err) = send_to_server(&new_env, lsp_prot::Request::Subscribe, &metadata) {
                eprintln!("{}", err);
                close_conn(&new_env);
                let disconnected = DisconnectedEnv {
                    d_ienv: new_env.c_ienv,
                    d_autostart: false,
                    d_server_status: None,
                };
                return show_disconnected(None, Some(&err.to_string()), disconnected);
            }
            let settings = new_env.c_ienv.i_config.clone();
            if let Err(err) =
                send_configuration_to_server("synthetic/configuration", settings, &new_env)
            {
                eprintln!("{}", err);
                close_conn(&new_env);
                let disconnected = DisconnectedEnv {
                    d_ienv: new_env.c_ienv,
                    d_autostart: false,
                    d_server_status: None,
                };
                return show_disconnected(None, Some(&err.to_string()), disconnected);
            }
            let metadata = make_metadata("synthetic/open");
            for open_file_info in new_env.c_ienv.i_open_files.values() {
                let msg = lsp::LspMessage::NotificationMessage(
                    lsp::LspNotification::DidOpenNotification(
                        lsp_types::DidOpenTextDocumentParams {
                            text_document: open_file_info.o_open_doc.clone(),
                        },
                    ),
                );
                if let Err(err) = send_lsp_to_server(&new_env, &metadata, msg) {
                    eprintln!("{}", err);
                    close_conn(&new_env);
                    let disconnected = DisconnectedEnv {
                        d_ienv: new_env.c_ienv,
                        d_autostart: false,
                        d_server_status: None,
                    };
                    return show_disconnected(None, Some(&err.to_string()), disconnected);
                }
            }

            let open_uris: Vec<_> = new_env.c_ienv.i_open_files.keys().cloned().collect();
            // close the old UI and bring up the new
            let new_state = show_connected(new_env);
            // Generate live errors for the newly opened files
            let mut state = new_state;
            for uri in open_uris {
                if let Err(err) = do_live_diagnostics(
                    &mut state,
                    Some(InteractionTrigger::ServerConnected),
                    &metadata,
                    &uri,
                ) {
                    eprintln!("{}", err);
                }
            }
            state
        }
        // Server_missing means the lock file is absent, because the server isn't running
        Err(ref reason @ ConnectError::ServerMissing) => {
            let d_autostart_was_set = env.d_autostart;
            let DisconnectedEnv { d_ienv, .. } = env;
            let new_env = DisconnectedEnv {
                d_ienv,
                d_autostart: false,
                d_server_status: None,
            };
            if d_autostart_was_set {
                match start_flow_server(
                    flowconfig_name,
                    &new_env.d_ienv.i_connect_params,
                    &new_env.d_ienv.i_root,
                ) {
                    Ok(()) => show_connecting(reason, new_env),
                    Err(msg) => show_disconnected(None, Some(&msg), new_env),
                }
            } else {
                show_disconnected(None, None, new_env)
            }
        }
        // Server_socket_missing means the server is present but lacks its sock
        // file. There's a tiny race possibility that the server has created a
        // lock but not yet created a sock file. More likely is that the server
        // is an old version of the server which doesn't even create the right
        // sock file. We'll kill the server now so we can start a new one next.
        // And if it was in that race? bad luck...
        Err(ref reason @ ConnectError::ServerSocketMissing) => {
            match kill_stale_server(
                flowconfig_name,
                &env.d_ienv.i_connect_params,
                &env.d_ienv.i_root,
            ) {
                Ok(()) => show_connecting(
                    reason,
                    DisconnectedEnv {
                        d_server_status: None,
                        ..env
                    },
                ),
                Err(_) => {
                    let msg = "An old version of the Flow server is running. Please stop it.";
                    show_disconnected(
                        None,
                        Some(msg),
                        DisconnectedEnv {
                            d_server_status: None,
                            ..env
                        },
                    )
                }
            }
        }
        // The server exited due to a version mismatch between the lsp and the server.
        Err(ref reason @ ConnectError::BuildIdMismatch(BuildIdMismatchKind::ServerExited)) => {
            if env.d_autostart {
                show_connecting(
                    reason,
                    DisconnectedEnv {
                        d_server_status: None,
                        ..env
                    },
                )
            } else {
                // We shouldn't hit this case. When `env.d_autostart` is `false`, we ask the server NOT to
                // die on a version mismatch.
                let msg = message_with_flow_and_root_name_prefix(
                    &env.d_ienv.i_flowconfig,
                    "the server was the wrong version",
                );
                show_disconnected(
                    None,
                    Some(&msg),
                    DisconnectedEnv {
                        d_server_status: None,
                        ..env
                    },
                )
            }
        }
        // The server and the lsp are different binaries and can't talk to each other. The server is not
        // stopping (either because we asked it not to stop or because it is newer than this client). In
        // this case, our best option is to stop the lsp and let the IDE start a new lsp with a newer
        // binary
        Err(ConnectError::BuildIdMismatch(BuildIdMismatchKind::ClientShouldError {
            ref server_version,
            ..
        })) => {
            let cmp = flow_common_semver::semver::compare(
                server_version,
                flow_common::flow_version::version(),
            );
            match cmp {
                Ok(n) if n < 0 => {
                    eprintln!(
                        "Flow: the running server is an older version of Flow ({}) than the LSP ({}), but we're not allowed to stop it",
                        server_version,
                        flow_common::flow_version::version()
                    );
                }
                Ok(0) => {
                    eprintln!(
                        "Flow: the running server is a different binary with the same version ({})",
                        flow_common::flow_version::version()
                    );
                }
                Ok(_) => {
                    eprintln!(
                        "Flow: the running server is a newer version of Flow ({}) than the LSP ({})",
                        server_version,
                        flow_common::flow_version::version()
                    );
                }
                Err(_) => {
                    eprintln!(
                        "Flow: unable to compare server version ({}) with LSP version ({})",
                        server_version,
                        flow_common::flow_version::version()
                    );
                }
            }
            eprintln!(
                "LSP is exiting. Hopefully the IDE will start an LSP with the same binary as the server"
            );
            lsp_exit_bad();
        }
        // While the server is busy initializing, sometimes we get Server_busy.Fail_on_init
        // with a server-status telling us how far it is through init. And sometimes we get
        // just ServerStatus.Not_responding if the server was just too busy to give us a
        // status update. These are cases where the right version of the server is running
        // but it's not speaking to us just now. So we'll keep trying until it's ready.
        Err(ref reason @ ConnectError::ServerBusy(ServerBusyKind::FailOnInit(ref st))) => {
            show_connecting(
                reason,
                DisconnectedEnv {
                    d_server_status: Some(st.clone()),
                    ..env
                },
            )
        }
        // The following codes mean the right version of the server is running so
        // we'll retry. They provide no information about the d_server_status of
        // the server, so we'll leave it as it was before.
        Err(ref reason @ ConnectError::ServerBusy(ServerBusyKind::NotResponding))
        | Err(ref reason @ ConnectError::ServerBusy(ServerBusyKind::TooManyClients)) => {
            show_connecting(reason, env)
        }
    }
}

pub fn run(flowconfig_name: &str, connect_params: ConnectParams) {
    let client = JsonrpcQueue::new();
    let state = State::PreInit(connect_params);
    initial_lwt_thread_sync(flowconfig_name, &client, state);
}

#[allow(dead_code)]
async fn initial_lwt_thread(flowconfig_name: &str, client: JsonrpcQueue, state: State) {
    initial_lwt_thread_sync(flowconfig_name, &client, state);
}

fn initial_lwt_thread_sync(flowconfig_name: &str, client: &JsonrpcQueue, state: State) {
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("Uncaught async exception: {}", info);
        eprintln!("{}", msg);
        std::process::exit(FlowExitStatus::UnknownError as i32);
    }));

    lsp_interaction_init();

    log_flusher_run();

    main_loop_sync(flowconfig_name, client, state);
}

#[allow(dead_code)]
async fn main_loop(flowconfig_name: &str, client: &JsonrpcQueue, state: State) {
    main_loop_sync(flowconfig_name, client, state);
}

fn main_loop_sync(flowconfig_name: &str, client: &JsonrpcQueue, mut state: State) {
    loop {
        std::thread::yield_now();
        gc_pending_interactions(&state);

        state = match get_next_event_sync(flowconfig_name, &state, client, &|message| {
            parse_json(&state, message)
        }) {
            Ok(event) => main_handle(flowconfig_name, state, event),
            Err(e) => {
                let err: Box<dyn std::error::Error> = Box::new(e);
                main_handle_error(err, state, None)
            }
        };
    }
}

#[allow(clippy::result_large_err)]
fn main_handle(flowconfig_name: &str, state: State, event: Event) -> State {
    let event_for_error = event.clone();
    let (client_duration, result) =
        with_timer(|| main_handle_unsafe(flowconfig_name, state, event));
    match result {
        Ok((state, LogNeeded::LogNeeded(mut metadata))) => {
            let client_duration_val = if metadata.client_duration.is_none() {
                Some(client_duration)
            } else {
                metadata.client_duration
            };
            metadata.client_duration = client_duration_val;
            main_log_command(&state, &metadata);
            state
        }
        Ok((state, _)) => state,
        Err((state, exn)) => main_handle_error(exn, state, Some(event_for_error)),
    }
}

fn main_handle_initialized_unsafe(
    flowconfig_name: &str,
    state: &mut ServerState,
    event: Event,
) -> Result<LogNeeded, Box<dyn std::error::Error>> {
    use flow_server_env::lsp::LspMessage;
    use flow_server_env::lsp::LspNotification;
    use flow_server_env::lsp::LspRequest;
    use flow_server_env::lsp::LspResult;
    use lsp_prot::MessageFromServer;
    use lsp_prot::NotificationFromServer;
    use lsp_prot::Response;
    match event {
        Event::ClientMessage(
            ref msg @ LspMessage::NotificationMessage(
                LspNotification::DidChangeConfigurationNotification(ref params),
            ),
            metadata,
        ) => {
            let settings = &params.settings;
            match settings {
                serde_json::Value::Null => {
                    update_ienv(state, |ienv| {
                        request_configuration(ienv);
                    });
                }
                i_config => {
                    let i_config = i_config.clone();
                    update_ienv(state, |ienv| {
                        ienv.i_config = i_config;
                    });
                    match state {
                        ServerState::Connected(cenv) => {
                            send_lsp_to_server(cenv, &metadata, msg.clone())?;
                        }
                        ServerState::Disconnected(_) => {}
                    }
                }
            }
            Ok(LogNeeded::LogNotNeeded)
        }
        Event::ClientMessage(LspMessage::ResponseMessage(id, result), metadata) => {
            let ienv = get_ienv_mut(state);
            match get_local_request_handler(ienv, &id, result.clone())? {
                Some(handler) => {
                    handler(state)?;
                    Ok(LogNeeded::LogNotNeeded)
                }
                None => match state {
                    ServerState::Connected(_cenv) => {
                        let c = LspMessage::ResponseMessage(id.clone(), result.clone());
                        track_to_server(state, &c)?;
                        let wrapped = decode_wrapped(&id);
                        if let ServerState::Connected(cenv) = state {
                            if wrapped.server_id == cenv.c_ienv.i_server_id {
                                let c =
                                    LspMessage::ResponseMessage(wrapped.message_id.clone(), result);
                                send_lsp_to_server(cenv, &metadata, c)?;
                            }
                        }
                        Ok(LogNeeded::LogNotNeeded)
                    }
                    ServerState::Disconnected(_) => {
                        let msg = format!(
                            "Response {:?} has missing handler",
                            lsp_fmt::denorm_message_to_string(&LspMessage::ResponseMessage(
                                id, result
                            ))
                        );
                        Err(internal_error_exception(msg).into())
                    }
                },
            }
        }
        Event::ClientMessage(
            LspMessage::RequestMessage(id, LspRequest::DocumentSymbolRequest(params)),
            metadata,
        ) => {
            let interaction_id =
                start_interaction(InteractionTrigger::DocumentSymbol(id.clone()), state);
            do_document_symbol(state, &id, &params)?;
            log_interaction(InteractionUx::Responded, state, interaction_id);
            Ok(LogNeeded::LogNeeded(metadata))
        }
        Event::ClientMessage(
            LspMessage::RequestMessage(id, LspRequest::SelectionRangeRequest(params)),
            metadata,
        ) => {
            let interaction_id =
                start_interaction(InteractionTrigger::SelectionRange(id.clone()), state);
            do_selection_range(state, &id, &params)?;
            log_interaction(InteractionUx::Responded, state, interaction_id);
            Ok(LogNeeded::LogNeeded(metadata))
        }
        Event::ClientMessage(c, metadata) if matches!(state, ServerState::Connected(_)) => {
            let TrackEffect { changed_live_uri } = track_to_server(state, &c)?;
            let trigger = lsp_interaction::trigger_of_lsp_msg(&c);
            let interaction_tracking_id = trigger
                .as_ref()
                .map(|trigger| start_interaction(trigger.clone(), state));
            if let ServerState::Connected(cenv) = state {
                let mut fwd_metadata = metadata.clone();
                fwd_metadata.interaction_tracking_id = interaction_tracking_id;
                send_lsp_to_server(cenv, &fwd_metadata, c.clone())?;
            }
            if let Some(ref uri) = changed_live_uri {
                do_live_diagnostics(state, trigger, &metadata, uri)?;
            }
            Ok(LogNeeded::LogDeferred)
        }
        Event::ClientMessage(LspMessage::RequestMessage(id, LspRequest::RageRequest), metadata) => {
            let result = do_rage(flowconfig_name, state);
            let response = LspMessage::ResponseMessage(id, LspResult::RageResult(result));
            let key = command_key_of_server_state(state);
            let json = lsp_fmt::print_lsp(true, &key, &response);
            to_stdout(&json);
            Ok(LogNeeded::LogNeeded(metadata))
        }
        Event::ClientMessage(
            c @ LspMessage::NotificationMessage(
                LspNotification::DidOpenNotification(_)
                | LspNotification::DidChangeNotification(_)
                | LspNotification::DidSaveNotification(_)
                | LspNotification::DidCloseNotification(_),
            ),
            mut metadata,
        ) => {
            let trigger = lsp_interaction::trigger_of_lsp_msg(&c);
            let interaction_id = trigger
                .as_ref()
                .map(|trigger| start_interaction(trigger.clone(), state));
            let (client_duration, result) = with_timer(|| -> Result<(), FlowLspError> {
                let TrackEffect { changed_live_uri } = track_to_server(state, &c)?;
                if let Some(ref uri) = changed_live_uri {
                    do_live_diagnostics(state, trigger.clone(), &metadata, uri)?;
                }
                Ok(())
            });
            result?;
            if let Some(id) = interaction_id {
                log_interaction(InteractionUx::Responded, state, id);
            }
            metadata.client_duration = Some(client_duration);
            Ok(LogNeeded::LogNeeded(metadata))
        }
        Event::ClientMessage(
            LspMessage::NotificationMessage(LspNotification::CancelRequestNotification(_)),
            _metadata,
        ) if matches!(state, ServerState::Disconnected(_)) => Ok(LogNeeded::LogNotNeeded),
        Event::ClientMessage(c, mut metadata) if matches!(state, ServerState::Disconnected(_)) => {
            let interaction_id = lsp_interaction::trigger_of_lsp_msg(&c)
                .map(|trigger| start_interaction(trigger, state));
            track_to_server(state, &c)?;
            let method_ = lsp_fmt::denorm_message_to_string(&c);
            let err_msg = format!("Server not connected; can't handle {}", method_);
            if let Some(id) = interaction_id {
                log_interaction(InteractionUx::Errored, state, id);
            }
            match c {
                LspMessage::RequestMessage(id, _request) => {
                    let e = lsp::error::T {
                        code: lsp::error::Code::RequestCancelled,
                        message: err_msg.clone(),
                        data: None,
                    };
                    let outgoing =
                        LspMessage::ResponseMessage(id, LspResult::ErrorResult(e, String::new()));
                    let key = command_key_of_server_state(state);
                    to_stdout(&lsp_fmt::print_lsp(false, &key, &outgoing));
                    metadata.error_info =
                        Some((lsp_prot::ErrorKind::ExpectedError, err_msg, String::new()));
                    Ok(LogNeeded::LogNeeded(metadata))
                }
                _ => Err(err_msg.into()),
            }
        }
        Event::ServerMessage(MessageFromServer::NotificationFromServer(
            NotificationFromServer::ServerExit(exit_code),
        )) => {
            if let ServerState::Connected(cenv) = state {
                cenv.c_about_to_exit_code = Some(exit_code);
            }
            Ok(LogNeeded::LogNotNeeded)
        }
        Event::ServerMessage(MessageFromServer::RequestResponse((
            Response::LspFromServer(msg),
            mut metadata,
        ))) => {
            let ux = match msg {
                None => InteractionUx::Responded,
                Some(outgoing) => {
                    track_from_server(state, &outgoing);
                    let (outgoing, ux) = match &outgoing {
                        LspMessage::RequestMessage(id, _request) => {
                            let server_id = if let ServerState::Connected(cenv) = &state {
                                cenv.c_ienv.i_server_id
                            } else {
                                0
                            };
                            let wrapped = WrappedId {
                                server_id,
                                message_id: id.clone(),
                            };
                            let encoded_outgoing = LspMessage::RequestMessage(
                                encode_wrapped(&wrapped),
                                _request.clone(),
                            );
                            (encoded_outgoing, InteractionUx::Responded)
                        }
                        LspMessage::ResponseMessage(id, LspResult::RageResult(items)) => {
                            metadata.client_duration = None;
                            let mut combined_items = items.clone();
                            combined_items.extend(do_rage(flowconfig_name, state));
                            let rage_outgoing = LspMessage::ResponseMessage(
                                id.clone(),
                                LspResult::RageResult(combined_items),
                            );
                            (rage_outgoing, InteractionUx::Responded)
                        }
                        LspMessage::ResponseMessage(_, LspResult::ErrorResult(e, _)) => {
                            let ux = if e.code == lsp::error::Code::RequestCancelled {
                                InteractionUx::Canceled
                            } else {
                                InteractionUx::Errored
                            };
                            (outgoing.clone(), ux)
                        }
                        _ => (outgoing.clone(), InteractionUx::Responded),
                    };
                    let outgoing = selectively_omit_errors(&metadata.lsp_method_name, outgoing);
                    let key = command_key_of_server_state(state);
                    to_stdout(&lsp_fmt::print_lsp(false, &key, &outgoing));
                    ux
                }
            };
            if let Some(id) = metadata.interaction_tracking_id {
                log_interaction(ux, state, id);
            }
            Ok(LogNeeded::LogNeeded(metadata))
        }
        Event::ServerMessage(MessageFromServer::RequestResponse((
            Response::UncaughtException {
                request,
                exception_constructor,
                stack,
            },
            mut metadata,
        ))) => {
            metadata.error_info = Some((
                lsp_prot::ErrorKind::UnexpectedError,
                exception_constructor.clone(),
                stack.clone(),
            ));
            let outgoing: Option<LspMessage> = match request {
                lsp_prot::Request::LspToServer(LspMessage::RequestMessage(id, _)) => {
                    let e = lsp::error::T {
                        code: lsp::error::Code::UnknownErrorCode,
                        message: "Flow encountered an unexpected error while handling this request. See the Flow logs for more details.".to_string(),
                        data: None,
                    };
                    Some(LspMessage::ResponseMessage(
                        id,
                        LspResult::ErrorResult(e, stack.clone()),
                    ))
                }
                lsp_prot::Request::Subscribe | lsp_prot::Request::LspToServer(_) => {
                    let code = lsp::error::code_to_enum(lsp::error::Code::UnknownErrorCode);
                    let text = format!("{} [{}]\n{}", exception_constructor, code, stack);
                    Some(LspMessage::NotificationMessage(
                        LspNotification::TelemetryNotification(lsp_types::LogMessageParams {
                            typ: lsp_types::MessageType::ERROR,
                            message: text,
                        }),
                    ))
                }
                lsp_prot::Request::LiveErrorsRequest(_) => None,
            };
            let key = command_key_of_server_state(state);
            if let Some(outgoing) = outgoing {
                let outgoing = selectively_omit_errors(&metadata.lsp_method_name, outgoing);
                to_stdout(&lsp_fmt::print_lsp(false, &key, &outgoing));
            }
            if let Some(id) = metadata.interaction_tracking_id {
                log_interaction(InteractionUx::Errored, state, id);
            }
            Ok(LogNeeded::LogNeeded(metadata))
        }
        Event::ServerMessage(MessageFromServer::NotificationFromServer(
            NotificationFromServer::Errors {
                diagnostics,
                errors_reason,
            },
        )) => {
            let end_state = collect_interaction_state(state);
            lsp_interaction::log_pushed_errors(end_state, &errors_reason);
            let is_rechecking = if let ServerState::Connected(cenv) = &state {
                cenv.c_is_rechecking
            } else {
                false
            };
            if is_rechecking {
                update_errors(state, |errors| {
                    crate::lsp_errors::add_streamed_server_errors_and_send(
                        &mut to_stdout_fn(),
                        diagnostics,
                        errors,
                    )
                });
            } else {
                update_errors(state, |errors| {
                    crate::lsp_errors::set_finalized_server_errors_and_send(
                        &mut to_stdout_fn(),
                        diagnostics,
                        errors,
                    )
                });
            }
            Ok(LogNeeded::LogNotNeeded)
        }
        Event::ServerMessage(MessageFromServer::RequestResponse((
            Response::LiveErrorsResponse(Ok(live_resp)),
            metadata,
        ))) => {
            let lsp_prot::LiveErrorsResponse {
                live_diagnostics,
                live_errors_uri: ref uri,
            } = live_resp;
            let file_is_still_open = get_open_files(state).contains_key(uri);
            if file_is_still_open {
                if let Some(id) = metadata.interaction_tracking_id {
                    log_interaction(
                        InteractionUx::PushedLiveNonParseErrors(uri.clone()),
                        state,
                        id,
                    );
                }
                let request =
                    serde_json::to_string(&metadata.start_json_truncated).unwrap_or_default();
                let data = serde_json::json!({
                    "uri": uri.to_string(),
                    "error_count": live_diagnostics.len(),
                })
                .to_string();
                flow_event_logger::live_non_parse_errors(&request, &data, metadata.start_wall_time);
                let uri_clone = uri.clone();
                update_errors(state, |errors| {
                    crate::lsp_errors::set_live_non_parse_errors_and_send(
                        &mut to_stdout_fn(),
                        &uri_clone,
                        live_diagnostics,
                        errors,
                    )
                });
            } else {
                if let Some(id) = metadata.interaction_tracking_id {
                    log_interaction(InteractionUx::ErroredPushingLiveNonParseErrors, state, id);
                }
                let request =
                    serde_json::to_string(&metadata.start_json_truncated).unwrap_or_default();
                let data = serde_json::json!({
                    "uri": uri.to_string(),
                    "reason": "File no longer open",
                })
                .to_string();
                flow_event_logger::live_non_parse_errors_failed(
                    &request,
                    &data,
                    metadata.start_wall_time,
                );
            }
            Ok(LogNeeded::LogNotNeeded)
        }
        Event::ServerMessage(MessageFromServer::RequestResponse((
            Response::LiveErrorsResponse(Err(live_err)),
            metadata,
        ))) => {
            let ux = match live_err.live_errors_failure_kind {
                lsp_prot::ErrorResponseKind::CanceledErrorResponse => {
                    InteractionUx::CanceledPushingLiveNonParseErrors
                }
                lsp_prot::ErrorResponseKind::ErroredErrorResponse => {
                    InteractionUx::ErroredPushingLiveNonParseErrors
                }
            };
            if let Some(id) = metadata.interaction_tracking_id {
                log_interaction(ux, state, id);
            }
            let request = serde_json::to_string(&metadata.start_json_truncated).unwrap_or_default();
            let data = serde_json::json!({
                "uri": live_err.live_errors_failure_uri.to_string(),
                "reason": live_err.live_errors_failure_reason,
            })
            .to_string();
            flow_event_logger::live_non_parse_errors_failed(
                &request,
                &data,
                metadata.start_wall_time,
            );
            Ok(LogNeeded::LogNotNeeded)
        }
        Event::ServerMessage(MessageFromServer::NotificationFromServer(
            NotificationFromServer::StartRecheck,
        )) => {
            let start_state = collect_interaction_state(state);
            lsp_interaction::recheck_start(start_state);
            if let ServerState::Connected(cenv) = state {
                cenv.c_is_rechecking = true;
                cenv.c_lazy_stats = None;
                show_connected_status(cenv);
            }
            Ok(LogNeeded::LogNotNeeded)
        }
        Event::ServerMessage(MessageFromServer::NotificationFromServer(
            NotificationFromServer::EndRecheck(lazy_stats),
        )) => {
            if let ServerState::Connected(cenv) = state {
                cenv.c_is_rechecking = false;
                cenv.c_lazy_stats = Some(lazy_stats);
                show_connected_status(cenv);
            }
            let open_file_uris: Vec<DocumentUri> = get_open_files(state).keys().cloned().collect();
            let method_name = "synthetic/endRecheck";
            if let ServerState::Connected(cenv) = &*state {
                let mut metadata = lsp_prot::empty_metadata();
                metadata.start_wall_time = now();
                metadata.start_server_status = Some(cenv.c_server_status.0.clone());
                metadata.start_watcher_status = cenv.c_server_status.1.clone();
                metadata.start_json_truncated = serde_json::json!({"method": method_name});
                metadata.lsp_method_name = method_name.to_string();
                for uri in open_file_uris {
                    if let Err(err) =
                        send_to_server(cenv, lsp_prot::Request::LiveErrorsRequest(uri), &metadata)
                    {
                        eprintln!("{}", err);
                    }
                }
            }
            Ok(LogNeeded::LogNotNeeded)
        }
        Event::ServerMessage(MessageFromServer::NotificationFromServer(
            NotificationFromServer::Telemetry(telemetry_event),
        )) => {
            if let ServerState::Connected(cenv) = state {
                update_recent_summaries(cenv, telemetry_event);
            }
            Ok(LogNeeded::LogNotNeeded)
        }
        Event::ServerMessage(MessageFromServer::NotificationFromServer(
            NotificationFromServer::PleaseHold(server_status, watcher_status),
        )) => {
            if let ServerState::Connected(cenv) = state {
                cenv.c_server_status = (server_status, Some(watcher_status));
                show_connected_status(cenv);
            }
            Ok(LogNeeded::LogNotNeeded)
        }
        Event::Tick if matches!(state, ServerState::Disconnected(_)) => {
            let version_mismatch_strategy = socket_handshake::VersionMismatchStrategy::ErrorClient;
            if let ServerState::Disconnected(env) = &mut *state {
                let saved_handlers = std::mem::take(&mut env.d_ienv.i_outstanding_local_handlers);
                let placeholder = env.clone();
                env.d_ienv.i_outstanding_local_handlers = saved_handlers;
                let owned_state = std::mem::replace(state, ServerState::Disconnected(placeholder));
                let new_state = match owned_state {
                    ServerState::Disconnected(env) => {
                        try_connect(&version_mismatch_strategy, flowconfig_name, env)
                    }
                    ServerState::Connected(_) => unreachable!(),
                };
                *state = new_state;
            }
            Ok(LogNeeded::LogNotNeeded)
        }
        Event::Tick => Ok(LogNeeded::LogNotNeeded),
        _ => unreachable!(),
    }
}

#[allow(clippy::result_large_err)]
fn main_handle_unsafe(
    flowconfig_name: &str,
    state: State,
    event: Event,
) -> Result<(State, LogNeeded), (State, Box<dyn std::error::Error>)> {
    use flow_server_env::lsp::LspMessage;
    use flow_server_env::lsp::LspNotification;
    use flow_server_env::lsp::LspRequest;
    let event = convert_to_client_uris(&state, event);
    match (state, event) {
        (
            State::PreInit(i_connect_params),
            Event::ClientMessage(
                LspMessage::RequestMessage(id, LspRequest::InitializeRequest(i_initialize_params)),
                metadata,
            ),
        ) => {
            let error_state = State::PreInit(i_connect_params.clone());
            let i_root = match lsp_helpers::get_root(&i_initialize_params) {
                Ok(i_root) => i_root,
                Err(err) => return Err((error_state, Box::new(FlowLspError::LspException(err)))),
            };
            let i_root = flow_common::files::cached_canonicalize(&i_root).unwrap_or(i_root);
            flow_parser::file_key::set_project_root(&i_root.to_string_lossy());

            let lsp_temp_dir: FilePath =
                flow_command_utils::normalize_temp_dir(&i_connect_params.temp_dir);

            match flow_flowlib::libdir(flow_flowlib::BuiltinLib::Flowlib, &lsp_temp_dir) {
                flow_flowlib::LibDir::Prelude(path) => {
                    flow_parser::file_key::set_flowlib_root(&path.to_string_lossy());
                }
                flow_flowlib::LibDir::Flowlib(path) => {
                    flow_parser::file_key::set_flowlib_root(&path.to_string_lossy());
                }
                flow_flowlib::LibDir::FlowlibWithLibDomDts(path) => {
                    flow_parser::file_key::set_flowlib_root(&path.to_string_lossy());
                }
                flow_flowlib::LibDir::Tslib(path) => {
                    flow_parser::file_key::set_flowlib_root(&path.to_string_lossy());
                }
            }

            let flowconfig = read_flowconfig_from_disk(flowconfig_name, &i_root);

            let mut d_ienv = InitializedEnv {
                i_initialize_params: i_initialize_params.clone(),
                i_connect_params,
                i_root: i_root.clone(),
                i_version: flowconfig.version.clone(),
                i_can_autostart_after_version_mismatch: true,
                i_server_id: 0,
                i_outstanding_local_requests: HashMap::new(),
                i_outstanding_local_handlers: HashMap::new(),
                i_outstanding_requests_from_server: WrappedMap::new(),
                i_is_connected: false,
                i_status: ShowStatusT::NeverShown,
                i_open_files: UriMap::new(),
                i_errors: crate::lsp_errors::empty(),
                i_config: serde_json::Value::Null,
                i_flowconfig: flowconfig.clone(),
                i_file_options: flow_command_utils::file_options_of_flowconfig(
                    &i_root,
                    &flowconfig,
                ),
            };

            flow_interaction_logger::set_server_config(
                flowconfig.options.log_saving.get("timeout").copied(),
                flowconfig_name,
                i_root.clone(),
                flowconfig.options.root_name.as_deref(),
            );

            let required_version = flowconfig.version.clone();
            if let Err(msg) = flow_command_utils::check_version(&required_version) {
                return Err((
                    error_state,
                    Box::new(FlowLspError::LspException(flow_server_env::lsp::error::T {
                        code: flow_server_env::lsp::error::Code::ServerErrorStart,
                        message: msg,
                        data: Some(serde_json::json!({ "retry": false })),
                    })),
                ));
            }

            let result = do_initialize(&i_initialize_params);
            let response = lsp::LspResult::InitializeResult(result);
            let key = command_key_of_ienv(&d_ienv);
            let json = lsp_fmt::print_lsp_response(true, &key, &id, &response);
            to_stdout(&json);

            if lsp_helpers::supports_configuration(&i_initialize_params) {
                request_configuration(&mut d_ienv);
                subscribe_to_config_changes(&mut d_ienv);
            }

            let version_mismatch_strategy =
                socket_handshake::VersionMismatchStrategy::AlwaysStopServer;

            let env = DisconnectedEnv {
                d_ienv,
                d_autostart: true,
                d_server_status: None,
            };

            let server_state = try_connect(&version_mismatch_strategy, flowconfig_name, env);
            Ok((
                State::Initialized(server_state),
                LogNeeded::LogNeeded(metadata),
            ))
        }
        (
            state,
            Event::ClientMessage(
                LspMessage::NotificationMessage(LspNotification::InitializedNotification),
                _metadata,
            ),
        ) => Ok((state, LogNeeded::LogNotNeeded)),
        (
            state,
            Event::ClientMessage(
                LspMessage::NotificationMessage(LspNotification::SetTraceNotification),
                _metadata,
            ),
        )
        | (
            state,
            Event::ClientMessage(
                LspMessage::NotificationMessage(LspNotification::LogTraceNotification),
                _metadata,
            ),
        ) => Ok((state, LogNeeded::LogNotNeeded)),
        (
            state,
            Event::ClientMessage(
                LspMessage::RequestMessage(id, LspRequest::ShutdownRequest),
                _metadata,
            ),
        ) => {
            if let State::Initialized(ServerState::Connected(env)) = &state {
                close_conn(env);
            }
            let key = command_key_of_state(&state);
            let response =
                lsp_prot::LspMessage::ResponseMessage(id, lsp::LspResult::ShutdownResult);
            let json = lsp_fmt::print_lsp(true, &key, &response);
            to_stdout(&json);
            Ok((State::PostShutdown, LogNeeded::LogNotNeeded))
        }
        (
            state,
            Event::ClientMessage(
                LspMessage::NotificationMessage(LspNotification::ExitNotification),
                _metadata,
            ),
        ) => {
            if matches!(state, State::PostShutdown) {
                lsp_exit_ok()
            } else {
                lsp_exit_bad()
            }
        }
        (state @ State::PreInit(_), Event::ClientMessage(_, _)) => {
            Err((state, "Server not initialized".into()))
        }
        (State::Initialized(mut server_state), event) => {
            match main_handle_initialized_unsafe(flowconfig_name, &mut server_state, event) {
                Ok(log_needed) => Ok((State::Initialized(server_state), log_needed)),
                Err(exn) => Err((State::Initialized(server_state), exn)),
            }
        }
        (state @ State::PostShutdown, Event::ClientMessage(_, _metadata)) => {
            Err((state, "Server shutting down".into()))
        }
        (state @ State::PreInit(_), Event::ServerMessage(_))
        | (state @ State::PostShutdown, Event::ServerMessage(_)) => {
            Ok((state, LogNeeded::LogNotNeeded))
        }
        (state, Event::Tick) => Ok((state, LogNeeded::LogNotNeeded)),
    }
}

fn main_log_command(_state: &State, metadata: &Metadata) {
    let request = serde_json::to_string(&metadata.start_json_truncated).unwrap_or_default();
    let request_id = metadata
        .lsp_id
        .as_ref()
        .map(lsp_fmt::id_to_string)
        .unwrap_or_else(|| "notif".to_string());
    let client_context = flow_event_logger::get_context();
    let persistent_context = flow_event_logger_persistent_context(metadata);
    let persistent_delay = match _state {
        State::Initialized(ServerState::Connected(cenv)) => {
            let delays: Vec<_> = cenv
                .c_recent_summaries
                .iter()
                .filter_map(|(time, summary)| {
                    if *time > metadata.start_wall_time {
                        Some(summary.clone())
                    } else {
                        None
                    }
                })
                .collect();
            if delays.is_empty() {
                None
            } else {
                let persistent_delay = log_of_summaries(&cenv.c_ienv.i_root, &delays);
                Some(flow_event_logger_persistent_delay(&persistent_delay))
            }
        }
        State::PreInit(_)
        | State::PostShutdown
        | State::Initialized(ServerState::Disconnected(_)) => None,
    };
    match &metadata.error_info {
        None => {
            flow_event_logger::persistent_command_success(
                metadata.server_logging_context.as_ref(),
                &request_id,
                &metadata.lsp_method_name,
                &request,
                metadata.extra_data.as_slice(),
                &client_context,
                Some(&persistent_context),
                persistent_delay.as_ref(),
                metadata.server_profiling.as_ref(),
                metadata.client_duration,
                metadata.start_wall_time,
                metadata.activity_key.as_ref(),
                None,
            );
        }
        Some((lsp_prot::ErrorKind::ExpectedError, msg, stack)) => {
            let error = (msg.clone(), stack.clone());
            flow_event_logger::persistent_command_success(
                metadata.server_logging_context.as_ref(),
                &request_id,
                &metadata.lsp_method_name,
                &request,
                metadata.extra_data.as_slice(),
                &client_context,
                Some(&persistent_context),
                persistent_delay.as_ref(),
                metadata.server_profiling.as_ref(),
                metadata.client_duration,
                metadata.start_wall_time,
                metadata.activity_key.as_ref(),
                Some(&error),
            );
        }
        Some((lsp_prot::ErrorKind::UnexpectedError, msg, stack)) => {
            let error = (msg.clone(), stack.clone());
            flow_event_logger::persistent_command_failure(
                metadata.server_logging_context.as_ref(),
                &request,
                metadata.extra_data.as_slice(),
                &client_context,
                Some(&persistent_context),
                persistent_delay.as_ref(),
                metadata.server_profiling.as_ref(),
                metadata.client_duration,
                metadata.start_wall_time,
                metadata.activity_key.as_ref(),
                &error,
            );
        }
    }
}

fn main_log_error(expected: bool, msg: &str, stack: &str, event: Option<&Event>) {
    let client_context = flow_event_logger::get_context();
    let (request, activity_key) = match event {
        Some(Event::ClientMessage(_, metadata)) => (
            Some(serde_json::to_string(&metadata.start_json_truncated).unwrap_or_default()),
            metadata.activity_key.as_ref(),
        ),
        Some(Event::ServerMessage(_)) | Some(Event::Tick) | None => (None, None),
    };
    let error = (msg.to_string(), stack.to_string());
    if expected {
        flow_event_logger::persistent_expected_error(
            request.as_deref(),
            &client_context,
            activity_key,
            &error,
        );
    } else {
        flow_event_logger::persistent_unexpected_error(
            request.as_deref(),
            &client_context,
            activity_key,
            &error,
        );
    }
}

// log the error
// report that we're disconnected to telemetry/connectionStatus
fn main_handle_error(
    exn: Box<dyn std::error::Error>,
    mut state: State,
    event: Option<Event>,
) -> State {
    let stack = format!("{:?}", exn);
    let _msg = exn.to_string();
    if let State::PostShutdown = &state {
        if exn
            .downcast_ref::<FlowLspError>()
            .is_some_and(|e| matches!(e, FlowLspError::ServerFatalConnectionException(_)))
        {
            return state;
        }
    }

    match exn.downcast::<FlowLspError>() {
        Ok(flow_err) => match *flow_err {
            FlowLspError::ServerFatalConnectionException(edata) => {
                let full_stack = format!("{}---\n{}", edata.stack, stack);
                main_log_error(
                    true,
                    &format!("[Server fatal] {}", edata.message),
                    &full_stack,
                    event.as_ref(),
                );

                if let State::Initialized(ServerState::Connected(ref mut env)) = state {
                    close_conn(env);
                    let i_is_connected = lsp_writers::notify_connection_status(
                        &env.c_ienv.i_initialize_params,
                        to_stdout,
                        env.c_ienv.i_is_connected,
                        false,
                    );
                    env.c_ienv.i_is_connected = i_is_connected;
                }

                let code = match &state {
                    State::Initialized(ServerState::Connected(cenv)) => cenv.c_about_to_exit_code,
                    _ => None,
                };
                let code_str = code
                    .map(flow_common_exit_status::to_string)
                    .unwrap_or_default();
                let report = format!(
                    "Server fatal exception: [{}] {}\n{}",
                    code_str, edata.message, full_stack
                );
                lsp_writers::telemetry_error(to_stdout, &report);

                if let State::Initialized(ref mut server_state) = state {
                    dismiss_tracks(server_state);
                }

                let (d_autostart, d_ienv) = match state {
                    State::Initialized(ServerState::Connected(ConnectedEnv {
                        c_ienv,
                        c_about_to_exit_code:
                            Some(
                                FlowExitStatus::FlowconfigChanged
                                | FlowExitStatus::ServerOutOfDate
                                | FlowExitStatus::LockStolen,
                            ),
                        ..
                    })) => {
                        let previous = c_ienv.i_can_autostart_after_version_mismatch;
                        let mut d_ienv = c_ienv;
                        d_ienv.i_can_autostart_after_version_mismatch = false;
                        (previous, d_ienv)
                    }
                    State::Initialized(ServerState::Connected(ConnectedEnv {
                        c_ienv,
                        c_about_to_exit_code: Some(FlowExitStatus::FileWatcherMissedChanges),
                        ..
                    })) => (true, c_ienv),
                    State::Initialized(ServerState::Connected(ConnectedEnv { c_ienv, .. })) => {
                        (false, c_ienv)
                    }
                    State::Initialized(ServerState::Disconnected(DisconnectedEnv {
                        d_ienv,
                        ..
                    })) => (false, d_ienv),
                    State::PreInit(_) | State::PostShutdown => {
                        panic!("Unexpected server error in inapplicable state")
                    }
                };
                let env = DisconnectedEnv {
                    d_ienv,
                    d_autostart,
                    d_server_status: None,
                };
                State::Initialized(ServerState::Disconnected(env))
            }
            FlowLspError::ClientRecoverableConnectionException(edata) => {
                let stack = format!("{}---\n{}", edata.stack, stack);
                main_log_error(
                    true,
                    &format!("[Client recoverable] {}", edata.message),
                    &stack,
                    event.as_ref(),
                );
                let report = format!("Client exception: {}\n{}", edata.message, stack);
                lsp_writers::telemetry_error(to_stdout, &report);
                state
            }
            FlowLspError::ClientFatalConnectionException(edata) => {
                let stack = format!("{}---\n{}", edata.stack, stack);
                main_log_error(
                    true,
                    &format!("[Client fatal] {}", edata.message),
                    &stack,
                    event.as_ref(),
                );
                eprintln!("Client fatal exception: {}\n{}", edata.message, stack);
                lsp_exit_bad()
            }
            FlowLspError::RequestLspException { id, error } => {
                main_log_error(
                    true,
                    &format!("[FlowLSP] {}", error.message),
                    &stack,
                    event.as_ref(),
                );
                let key = command_key_of_state(&state);
                let json = lsp_fmt::print_lsp_response(
                    true,
                    &key,
                    &id,
                    &lsp::LspResult::ErrorResult(error, stack),
                );
                to_stdout(&json);
                state
            }
            FlowLspError::ChangedFileNotOpen(uri) => {
                main_log_error(
                    false,
                    &format!("Changed file not open: {}", *uri),
                    &stack,
                    event.as_ref(),
                );
                state
            }
            FlowLspError::LspException(e) => {
                main_log_error(
                    true,
                    &format!("[FlowLSP] {}", e.message),
                    &stack,
                    event.as_ref(),
                );
                let code = lsp::error::code_to_enum(e.code);
                let text = format!("FlowLSP exception {} [{}]\n{}", e.message, code, stack);
                match event.as_ref() {
                    Some(Event::ClientMessage(
                        lsp_prot::LspMessage::RequestMessage(id, _request),
                        _metadata,
                    )) => {
                        let key = command_key_of_state(&state);
                        let json = lsp_fmt::print_lsp_response(
                            true,
                            &key,
                            id,
                            &lsp::LspResult::ErrorResult(e, stack),
                        );
                        to_stdout(&json);
                    }
                    _ => {
                        lsp_writers::telemetry_error(to_stdout, &text);
                    }
                }
                state
            }
        },
        Err(other) => {
            let e = lsp_fmt::error_of_exn(&*other);
            main_log_error(
                true,
                &format!("[FlowLSP] {}", e.message),
                &stack,
                event.as_ref(),
            );
            let code = lsp::error::code_to_enum(e.code);
            let text = format!("FlowLSP exception {} [{}]\n{}", e.message, code, stack);
            match event.as_ref() {
                Some(Event::ClientMessage(
                    lsp_prot::LspMessage::RequestMessage(id, _request),
                    _metadata,
                )) => {
                    let key = command_key_of_state(&state);
                    let json = lsp_fmt::print_lsp_response(
                        true,
                        &key,
                        id,
                        &lsp::LspResult::ErrorResult(e, stack),
                    );
                    to_stdout(&json);
                }
                _ => {
                    lsp_writers::telemetry_error(to_stdout, &text);
                }
            }
            state
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_with_multibyte_char_crossing_limit() -> String {
        let mut text = "a".repeat(255);
        text.push('█');
        text.push_str(" trailing text");
        text
    }

    #[test]
    fn json_truncate_does_not_split_multibyte_chars() {
        let json = serde_json::Value::String(text_with_multibyte_char_crossing_limit());

        let truncated = json_truncate(&json, 256, 4);

        assert_eq!(truncated, serde_json::Value::String("a".repeat(255)));
    }

    #[test]
    fn json_truncate_keeps_exact_char_boundary() {
        let mut text = "a".repeat(256);
        text.push('█');
        let json = serde_json::Value::String(text);

        let truncated = json_truncate(&json, 256, 4);

        assert_eq!(truncated, serde_json::Value::String("a".repeat(256)));
    }

    #[test]
    fn json_truncate_does_not_split_nested_lsp_text() {
        let json = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "languageId": "javascript",
                    "text": text_with_multibyte_char_crossing_limit(),
                    "uri": "file:///tmp/example.js",
                    "version": 1,
                },
            },
        });

        let truncated = json_truncate(&json, 256, 4);

        let text = truncated
            .pointer("/params/textDocument/text")
            .and_then(serde_json::Value::as_str)
            .expect("truncated JSON should keep the nested LSP text field");
        assert_eq!(text, "a".repeat(255));
    }
}
