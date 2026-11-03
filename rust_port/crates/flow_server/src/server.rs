/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use flow_cgroup as cgroup;
use flow_common::files::LibDir;
use flow_common::options::Options;
use flow_common::options::SavedStateFetcher;
use flow_common::options::SupportedOs;
use flow_common_errors::error_utils::ConcreteLocPrintableErrorSet;
use flow_common_errors::error_utils::PrintableError;
use flow_common_exit_status::FlowExitStatus;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_flowlib;
use flow_parser::ast::Program;
use flow_parser::file_key;
use flow_parser::file_key::FileKey;
use flow_parser::loc::Loc;
use flow_profiling::profiling_js;
use flow_server_env::error_collator;
use flow_server_env::monitor_rpc;
use flow_server_env::server_env::Env;
use flow_server_env::server_env::Genv;
use flow_server_env::server_orchestrator;
use flow_server_env::server_status;
use flow_services_inference::type_service::RecheckError;
use flow_typing_errors::intermediate_error::to_printable_error;

use crate::server_daemon;
use crate::server_env_build;

const SERVER_MAIN_THREAD_STACK_SIZE: usize = {
    if cfg!(debug_assertions) {
        256 * 1024 * 1024
    } else if cfg!(windows) {
        64 * 1024 * 1024
    } else {
        32 * 1024 * 1024
    }
};

pub type CheckOnceSuppressedErrors = Vec<(PrintableError<Loc>, BTreeSet<Loc>)>;
pub type CheckOnceCollatedErrors<'a> = (
    &'a ConcreteLocPrintableErrorSet,
    &'a ConcreteLocPrintableErrorSet,
    CheckOnceSuppressedErrors,
    Option<String>, /* lazy_msg */
);
pub type CheckOncePrintErrors<'a> = Box<dyn FnOnce(&ProfilingFinished) + 'a>;

type ProfilingRunning = profiling_js::Running;
pub type ProfilingFinished = profiling_js::Finished;

fn worker_threads(options: &Options) -> NonZeroUsize {
    usize::try_from(options.max_workers)
        .ok()
        .and_then(NonZeroUsize::new)
        .unwrap_or(NonZeroUsize::MIN)
}

fn with_profiling<F, R>(label: &str, should_print_summary: bool, f: F) -> (ProfilingFinished, R)
where
    F: FnOnce(&ProfilingRunning) -> R,
{
    profiling_js::with_profiling_sync(label, should_print_summary, f)
}

fn sample_init_memory(
    profiling: &ProfilingRunning,
    committed_heap: &Arc<flow_heap::heap_state::CommittedHeap>,
) {
    let hash_stats = committed_heap.hash_stats();
    let heap_size = committed_heap.heap_size();
    let memory_metrics = [
        ("heap.size", heap_size),
        ("hash_table.nonempty_slots", hash_stats.nonempty_slots),
        ("hash_table.used_slots", hash_stats.used_slots),
        ("hash_table.slots", hash_stats.slots),
    ];
    for (metric, value) in memory_metrics {
        profiling.legacy_sample_memory(&format!("init_done.{}", metric), value as f64);
    }
}

fn extract_flowlibs_or_exit(options: &Options) {
    match &options.file_options.default_lib_dir {
        Some(libdir) => {
            let flowlib_libdir = match libdir {
                LibDir::Prelude(path) => flow_flowlib::LibDir::Prelude(path.clone()),
                LibDir::Flowlib(path) => flow_flowlib::LibDir::Flowlib(path.clone()),
                LibDir::FlowlibWithLibDomDts(path) => {
                    flow_flowlib::LibDir::FlowlibWithLibDomDts(path.clone())
                }
                LibDir::Tslib(path) => flow_flowlib::LibDir::Tslib(path.clone()),
            };
            let extract_result = std::panic::catch_unwind(|| {
                flow_flowlib::extract(&flowlib_libdir, options.jsx_intrinsic_elements)
            });
            match extract_result {
                Ok(()) => {}
                Err(_) => {
                    let libdir_str = flow_flowlib::path_of_libdir(&flowlib_libdir)
                        .display()
                        .to_string();
                    let msg = format!(
                        "Could not extract flowlib files into {}: extract failed",
                        libdir_str
                    );
                    flow_common_exit_status::exit_with_msg(
                        FlowExitStatus::CouldNotExtractFlowlibs,
                        &msg,
                    );
                }
            }
        }
        None => {}
    }
}

pub fn string_of_saved_state_fetcher(options: &Options) -> &'static str {
    match options.saved_state_fetcher {
        SavedStateFetcher::DummyFetcher => "none",
        SavedStateFetcher::LocalFetcher => "local",
        SavedStateFetcher::ScmFetcher => "scm",
        SavedStateFetcher::FbFetcher => "fb",
    }
}

fn exit_on_recheck_error(error: RecheckError) -> ! {
    let msg = match error {
        RecheckError::TooSlow => "Unhandled exception: recheck was too slow".to_string(),
        RecheckError::Canceled(files) => {
            format!(
                "Unhandled exception: recheck canceled for files: {:?}",
                files
            )
        }
    };
    flow_common_exit_status::exit_with_msg(FlowExitStatus::UnknownError, &msg);
}

fn init(
    orchestrator: Option<&server_orchestrator::ServerOrchestratorHandle>,
    profiling: &ProfilingRunning,
    focus_targets: Option<FlowOrdSet<FileKey>>,
    genv: &Genv,
    committed_heap: &Arc<flow_heap::heap_state::CommittedHeap>,
) -> Result<
    (
        Env,
        Arc<flow_heap::heap_state::CommittedHeap>,
        Option<String>,
    ),
    RecheckError,
> {
    // write binary path and version to server log
    flow_hh_logger::info!(
        "executable={}",
        std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    );
    flow_hh_logger::info!("version={}", flow_common::flow_version::version());

    let Some(workers) = genv.workers.as_ref() else {
        flow_common_exit_status::exit_with_msg(
            FlowExitStatus::UnknownError,
            "Unhandled exception: workers are not initialized",
        );
    };
    let options = &genv.options;
    crate::multi_worker::set_report_canceled_callback(|total, finished| {
        flow_hh_logger::info!("Canceling progress {}/{}", finished, total);
        monitor_rpc::status_update(server_status::Event::CancelingProgress(
            server_status::Progress {
                total: Some(total),
                finished,
            },
        ));
    });

    monitor_rpc::status_update(server_status::Event::InitStart);

    extract_flowlibs_or_exit(options);

    let (env, init_heap, first_internal_error) =
        flow_profiling::memory_utils::with_committed_heap(Arc::clone(committed_heap), || {
            flow_services_inference::type_service::init(
                orchestrator,
                options,
                workers,
                committed_heap,
                focus_targets,
            )
        })?;

    // Report the heap init built, which is the one a saved state brought when there was one to
    // load. It is not in force yet: whoever owns `committed_heap` installs it.
    sample_init_memory(profiling, &init_heap);
    flow_event_logger::sharedmem_init_done(init_heap.heap_size() as u64);
    Ok((env, init_heap, first_internal_error))
}

async fn idle_logging_loop(_options: Arc<Options>, _start_time: f64) {
    let idle_period_in_seconds = 300.0_f64;
    async fn sample(profiling: &ProfilingRunning) {
        let cgroup_stats = match tokio::task::spawn_blocking(cgroup::get_stats).await {
            Ok(cgroup_stats) => cgroup_stats,
            Err(err) if err.is_panic() => std::panic::resume_unwind(err.into_panic()),
            Err(err) => Err(err.to_string()),
        };
        match cgroup_stats {
            Err(_) => {}
            Ok(cgroup::Stats {
                total,
                total_swap,
                anon,
                file,
                shmem,
            }) => {
                profiling.sample_memory(None, "cgroup_total", total as f64);
                profiling.sample_memory(None, "cgroup_swap", total_swap as f64);
                profiling.sample_memory(None, "cgroup_anon", anon as f64);
                profiling.sample_memory(None, "cgroup_shmem", shmem as f64);
                profiling.sample_memory(None, "cgroup_file", file as f64);
            }
        }
    }

    let should_print_summary = _options.profile && !_options.quiet;
    loop {
        let profiling = ProfilingRunning::new("Idle");
        tokio::select! {
            _ = async {
                loop {
                    tokio::join!(sample(&profiling), tokio::time::sleep(Duration::from_secs(1)));
                }
            } => {}
            _ = tokio::time::sleep(Duration::from_secs_f64(idle_period_in_seconds)) => {}
        }

        let profiling = profiling.finish();
        if should_print_summary {
            profiling_js::print_summary(&profiling);
        }

        let idle_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64()
            - _start_time;
        let profiling_props = profiling.to_json_properties();
        flow_event_logger::idle_heartbeat(idle_time, &profiling_props);
        flow_tokio_runtime::spawn(async {
            if let Err(err) = flow_event_logger_lwt::flush().await {
                flow_hh_logger::error!("Failed to flush Flow event logs: {}", err);
            }
        });
    }
}

async fn gc_loop(orchestrator: server_orchestrator::ServerOrchestratorHandle) {
    loop {
        tokio::task::yield_now().await;
        let done = orchestrator.collect_heap_slice(10000);
        if done {
            break;
        }
    }
}

fn serve(genv: &Genv, orchestrator: &server_orchestrator::ServerOrchestratorHandle) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");
    loop {
        monitor_rpc::status_update(server_status::Event::Ready);

        let options = &genv.options;

        let start_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        runtime.block_on(async {
            let orchestrator_for_gc = orchestrator.clone();
            let idle_thread = async {
                tokio::join!(
                    idle_logging_loop(Arc::clone(options), start_time),
                    gc_loop(orchestrator_for_gc),
                );
            };
            let wait_thread =
                flow_server_env::server_monitor_listener_state::wait_for_recheck_async(
                    orchestrator.recheck(),
                );
            tokio::select! {
                biased;
                _ = wait_thread => {}
                _ = idle_thread => {}
            }
        });

        let _profiling = flow_server_rechecker::rechecker::recheck_loop(genv, orchestrator);
        // Flush the logs asynchronously
        flow_tokio_runtime::spawn(async {
            if let Err(err) = flow_event_logger_lwt::flush().await {
                flow_hh_logger::error!("Failed to flush Flow event logs: {}", err);
            }
        });
    }
}

pub fn create_program_init(options: Arc<Options>) -> Genv {
    flow_utils_concurrency::thread_pool::init_rayon_global_thread_pool(worker_threads(&options));

    file_key::set_project_root(&options.root.display().to_string());
    match &options.file_options.default_lib_dir {
        Some(
            LibDir::Flowlib(path)
            | LibDir::FlowlibWithLibDomDts(path)
            | LibDir::Prelude(path)
            | LibDir::Tslib(path),
        ) => {
            file_key::set_flowlib_root(&path.display().to_string());
        }
        None => {}
    }

    server_env_build::make_genv(options)
}

fn detect_linux_distro() -> Option<String> {
    let contents = match std::fs::read_to_string("/etc/os-release") {
        Ok(c) => c,
        Err(_) => return None,
    };
    let lines: Vec<&str> = contents.lines().collect();

    let id_line = lines.iter().find(|line| line.starts_with("ID="));
    match id_line {
        Some(line) => {
            let id = &line[3..];
            let id = id.trim();
            let id = if id.len() >= 2 && id.starts_with('"') && id.ends_with('"') {
                &id[1..id.len() - 1]
            } else {
                id
            };
            Some(id.to_string())
        }
        None => None,
    }
}

pub fn check_supported_operating_system(options: &Options) {
    let supported_os_list = &options.supported_operating_systems;
    let current_os_unsupported = match supported_os_list.as_slice() {
        [] => false,
        _ => !supported_os_list.iter().any(|os| match os {
            SupportedOs::CentOS => {
                matches!(detect_linux_distro().as_deref(), Some("centos"))
            }
        }),
    };
    if current_os_unsupported {
        let current_os = match detect_linux_distro() {
            Some(distro) => distro,
            None => "Unknown".to_string(),
        };
        let msg = format!(
            "This operating system ({}) is not supported by this Flow configuration.",
            current_os
        );
        flow_hh_logger::info!("{}", msg);
        flow_common_exit_status::exit(FlowExitStatus::InvalidFlowconfig);
    }
}

/// Maps the monitor/user startup cause to the `init_trigger` Scuba column on
/// `INIT_END`.
///
/// `MonitorRestart(None)` includes monitor-driven restarts without a specific
/// reason (EdenFS watcher retry, non-fatal signaled exit, kill_by_monitor that
/// doesn't take down the monitor).
///
/// `MonitorRestart(Some(OutOfSharedMemory))` is currently unreachable: the
/// monitor exits on OOM rather than restarting (see `process_server_exit` in
/// flow_server_monitor_server.rs). The arm is kept for type exhaustiveness in
/// case the policy changes.
pub fn string_of_init_trigger(start_cause: server_status::StartCause) -> &'static str {
    use server_status::RestartReason;
    use server_status::StartCause;
    match start_cause {
        StartCause::UserInitiated => "user_initiated",
        StartCause::MonitorRestart(None) => "monitor_restart",
        StartCause::MonitorRestart(Some(RestartReason::ServerOutOfDate)) => "server_out_of_date",
        StartCause::MonitorRestart(Some(RestartReason::OutOfSharedMemory)) => {
            "out_of_shared_memory"
        }
        StartCause::MonitorRestart(Some(RestartReason::Restart)) => "restart_on_reinit",
    }
}

fn run(
    options: Arc<Options>,
    monitor_channels: Option<monitor_rpc::Channels>,
    start_cause: server_status::StartCause,
) {
    flow_tokio_runtime::init_worker_threads(worker_threads(&options));

    // Check if the current operating system is supported
    check_supported_operating_system(&options);

    match monitor_channels {
        Some(channels) => monitor_rpc::init(channels),
        None => monitor_rpc::disable(),
    }

    flow_server_rechecker::rechecker::init_tokio_runtime();

    let committed_heap = Arc::new(flow_heap::heap_state::CommittedHeap::new());
    let server_orchestrator =
        server_orchestrator::ServerOrchestrator::new(options.clone(), committed_heap);
    let orchestrator = server_orchestrator.handle();

    let genv_arc = Arc::new(create_program_init(options.clone()));
    let listener_running = matches!(monitor_rpc::state(), monitor_rpc::StateKind::Initialized);
    if listener_running {
        let genv_for_listener = genv_arc.clone();
        let orchestrator_for_listener = orchestrator.clone();
        std::thread::spawn(move || {
            let callbacks = flow_server_env::server_monitor_listener::CommandHandlerCallbacks {
                enqueue_or_handle_ephemeral:
                    flow_server_command_handler::command_handler::enqueue_or_handle_ephemeral,
                enqueue_persistent:
                    flow_server_command_handler::command_handler::enqueue_persistent,
            };
            flow_server_env::server_monitor_listener::listen_for_messages(
                &genv_for_listener,
                &orchestrator_for_listener,
                &callbacks,
            );
        });
    }

    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    flow_hh_logger::info!("Initializing Server (This might take some time)");

    let should_print_summary = options.profile && !options.quiet;
    let (profiling, init_result) = with_profiling("Init", should_print_summary, |profiling| {
        init(
            Some(&orchestrator),
            profiling,
            None,
            &genv_arc,
            orchestrator.heap(),
        )
    });
    let (env, init_heap, first_internal_error) = match init_result {
        Ok(result) => result,
        Err(error) => exit_on_recheck_error(error),
    };
    let _orchestrator_owner = server_orchestrator.start(Arc::new(env), init_heap);
    let init_duration = profiling.get_profiling_duration();

    monitor_rpc::send_telemetry(
        flow_server_env::lsp_prot::TelemetryFromServer::InitSummary {
            duration: init_duration,
        },
    );
    monitor_rpc::status_update(server_status::Event::FinishingUp);

    let saved_state_fetcher = string_of_saved_state_fetcher(&options);
    let init_trigger = string_of_init_trigger(start_cause);

    let profiling_props = profiling.to_event_logger_json_properties();
    flow_event_logger::init_done(
        first_internal_error.as_deref(),
        init_trigger,
        saved_state_fetcher,
        &profiling_props,
    );

    flow_hh_logger::info!("Server is READY");

    let t_prime = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    flow_hh_logger::info!("Took {} seconds to initialize.", t_prime - t);

    serve(&genv_arc, &orchestrator);
}

fn exit_msg_of_exception(error: &dyn std::fmt::Display, msg: &str) -> String {
    let bt = format!("{}", error);
    if bt.is_empty() {
        msg.to_string()
    } else {
        format!("{}:\n{}", msg, bt)
    }
}

// Most recent panic's backtrace, stashed at the panic site for the `catch_unwind`
// thread to read back (rayon's `resume_unwind` skips the hook). Last-write-wins.
// Kept unsymbolized so the hook stays cheap; `log_worker_exception` formats it.
static LAST_PANIC_BACKTRACE: std::sync::Mutex<Option<std::backtrace::Backtrace>> =
    std::sync::Mutex::new(None);

// Stashes each panic's backtrace in `LAST_PANIC_BACKTRACE`. Capture only, no logging:
// a panic in a hook aborts the process. Idempotent; chains to the prev hook.
pub fn install_panic_hook() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let bt = std::backtrace::Backtrace::force_capture();
            if let Ok(mut guard) = LAST_PANIC_BACKTRACE.lock() {
                *guard = Some(bt);
            }
            prev(info);
        }));
    });
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else {
        "unknown panic".to_string()
    }
}

// Logs a panic to the `WORKER_EXCEPTION` Scuba event with the backtrace stashed by
// `install_panic_hook`. Call from a `catch_unwind` boundary, never from the hook.
pub fn log_worker_exception(err_msg: &str) {
    // Read through poison: it just means a thread panicked here, which is what we log.
    let backtrace = match LAST_PANIC_BACKTRACE.lock() {
        Ok(mut guard) => guard.take(),
        Err(poisoned) => poisoned.into_inner().take(),
    };
    let data = match backtrace {
        Some(bt) => format!("{err_msg}\n{bt}"),
        None => err_msg.to_string(),
    };
    flow_event_logger::worker_exception(&data);
}

pub fn run_from_daemonize(
    options: Arc<Options>,
    monitor_channels: Option<monitor_rpc::Channels>,
    start_cause: server_status::StartCause,
) {
    install_panic_hook();
    let result = std::thread::Builder::new()
        .name("flow_server_main".to_string())
        .stack_size(SERVER_MAIN_THREAD_STACK_SIZE)
        .spawn(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run(options, monitor_channels, start_cause);
            }))
        })
        .expect("failed to spawn flow_server_main thread")
        .join()
        .unwrap_or_else(Err);
    match result {
        Ok(()) => {}
        Err(e) => {
            let err_msg = panic_message(&*e);
            let err_display: &dyn std::fmt::Display = &err_msg;
            let status = flow_common_exit_status::exit_status_for_panic_message(&err_msg);
            let log_msg = match status {
                FlowExitStatus::OutOfSharedMemory => {
                    let msg = exit_msg_of_exception(err_display, "Out of shared memory");
                    flow_hh_logger::info!("{}", msg);
                    msg
                }
                FlowExitStatus::HashTableFull => {
                    let msg = exit_msg_of_exception(err_display, "Hash table is full");
                    flow_hh_logger::info!("{}", msg);
                    msg
                }
                FlowExitStatus::HeapFull => {
                    let msg = exit_msg_of_exception(err_display, "Heap is full");
                    flow_hh_logger::info!("{}", msg);
                    msg
                }
                FlowExitStatus::KilledByMonitor => {
                    let msg = "Monitor died unexpectedly";
                    flow_hh_logger::info!("{}", msg);
                    msg.to_string()
                }
                _ => {
                    let msg = format!("Unhandled exception: {}", err_msg);
                    flow_hh_logger::info!("{}", msg);
                    log_worker_exception(&err_msg);
                    msg
                }
            };
            flow_common_exit_status::exit_with_log_msg(status, &log_msg);
        }
    }
}

// Foreground type check for `flow full-check` / `flow focus-check`. Logs a
// `WORKER_EXCEPTION` on panic, then resumes the unwind for the CLI to classify.
pub fn check_once<'a, FormatErrors>(
    options: Arc<Options>,
    format_errors: FormatErrors,
    focus_targets: Option<FlowOrdSet<FileKey>>,
) -> (ConcreteLocPrintableErrorSet, ConcreteLocPrintableErrorSet)
where
    FormatErrors: for<'b> FnOnce(CheckOnceCollatedErrors<'b>) -> CheckOncePrintErrors<'a>,
{
    install_panic_hook();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        check_once_inner(options, format_errors, focus_targets)
    }));
    match result {
        Ok(errors_and_warnings) => errors_and_warnings,
        Err(payload) => {
            log_worker_exception(&panic_message(&*payload));
            std::panic::resume_unwind(payload);
        }
    }
}

fn check_once_inner<'a, FormatErrors>(
    options: Arc<Options>,
    format_errors: FormatErrors,
    focus_targets: Option<FlowOrdSet<FileKey>>,
) -> (ConcreteLocPrintableErrorSet, ConcreteLocPrintableErrorSet)
where
    FormatErrors: for<'b> FnOnce(CheckOnceCollatedErrors<'b>) -> CheckOncePrintErrors<'a>,
{
    flow_daemon::pid_log::disable();
    monitor_rpc::disable();

    flow_event_logger::set_eden(Some(flow_common_vcs::eden::is_eden(&options.root)));
    flow_logging_utils::set_server_options(&options);

    let genv = create_program_init(Arc::clone(&options));
    // `flow full-check` drives its own loop, so there is no orchestrator to own the heap; this
    // frame does, for as long as the check runs. Nothing else can see it, so there is nothing to
    // publish either: whichever heap init comes back with is simply the one this frame goes on to
    // read errors out of.
    let committed_heap = Arc::new(flow_heap::heap_state::CommittedHeap::new());
    let should_print_summary = options.profile && !options.quiet;

    let (profiling, init_result) = with_profiling("Init", should_print_summary, |profiling| {
        // `flow full-check` runs without a server, so there is no recheck queue to schedule into.
        let (env, committed_heap, first_internal_error) =
            init(None, profiling, focus_targets, &genv, &committed_heap)?;
        let (errors, warnings, suppressed_errors) = error_collator::get(&env);
        let lazy_stats = env.lazy_stats(&options);
        let lazy_msg = if lazy_stats.lazy_mode {
            Some(format!(
                "Checked {}/{} files.",
                lazy_stats.checked_files, lazy_stats.total_files
            ))
        } else {
            None
        };

        let print_errors = profiling.with_timer(false, "FormatErrors", || {
            let transaction =
                flow_heap::parsing_heaps::ActiveTransaction::new(committed_heap.clone());
            let loc_of_aloc = |aloc: &flow_aloc::ALoc| -> Loc { transaction.loc_of_aloc(aloc) };
            let get_ast =
                |file: &FileKey| -> Option<Arc<Program<Loc, Loc>>> { transaction.get_ast(file) };
            let strip_root = if options.strip_root {
                Some(options.root.as_path())
            } else {
                None
            };

            let suppressed_errors = if options.include_suppressions {
                suppressed_errors
                    .into_iter()
                    .map(|(e, loc_set)| {
                        (
                            to_printable_error(&loc_of_aloc, get_ast, strip_root, e),
                            loc_set,
                        )
                    })
                    .collect()
            } else {
                vec![]
            };
            let collated_errors = (&errors, &warnings, suppressed_errors, lazy_msg);
            format_errors(collated_errors)
        });
        Ok((print_errors, errors, warnings, first_internal_error))
    });
    let (print_errors, errors, warnings, first_internal_error) = match init_result {
        Ok(result) => result,
        Err(error) => exit_on_recheck_error(error),
    };

    print_errors(&profiling);

    monitor_rpc::send_telemetry(
        flow_server_env::lsp_prot::TelemetryFromServer::InitSummary {
            duration: profiling.get_profiling_duration(),
        },
    );
    monitor_rpc::status_update(server_status::Event::FinishingUp);

    let saved_state_fetcher = string_of_saved_state_fetcher(&options);
    // `check_once` backs `flow full-check`; it's always invoked by the user.
    let init_trigger = string_of_init_trigger(server_status::StartCause::UserInitiated);

    let profiling_props = profiling.to_event_logger_json_properties();
    flow_event_logger::init_done(
        first_internal_error.as_deref(),
        init_trigger,
        saved_state_fetcher,
        &profiling_props,
    );

    (errors, warnings)
}

pub fn daemonize(
    init_id: &str,
    log_file: &str,
    argv: &[String],
    lazy_mode: Option<String>,
    builtin_lib: String,
    ignore_version: bool,
    file_watcher_pid: Option<u32>,
    start_cause: server_status::StartCause,
    options: Arc<Options>,
    cli_overrides: &flow_common::cli_overrides::CliOverrides,
) -> Result<flow_daemon::Handle<(), ()>, String> {
    server_daemon::daemonize(
        init_id,
        log_file,
        argv,
        lazy_mode,
        builtin_lib,
        ignore_version,
        options,
        file_watcher_pid,
        start_cause,
        cli_overrides,
    )
}
