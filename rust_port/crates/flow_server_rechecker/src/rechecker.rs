/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Instant;

use dupe::Dupe;
use flow_common::options::Options;
use flow_common_utils::checked_set::CheckedSet;
use flow_heap::heap_state::CommittedHeap;
use flow_heap::parsing_heaps::ActiveTransaction;
use flow_profiling::memory_utils;
use flow_profiling::profiling_js;
use flow_server_env::lsp_prot;
use flow_server_env::monitor_rpc;
use flow_server_env::persistent_connection;
use flow_server_env::server_env;
use flow_server_env::server_monitor_listener_state;
use flow_server_env::server_monitor_listener_state::FindRefCommand;
use flow_server_env::server_monitor_listener_state::Priority;
use flow_server_env::server_monitor_listener_state::RecheckQueue;
use flow_server_env::server_monitor_listener_state::Updates;
use flow_server_env::server_monitor_listener_state::WorkloadChanges;
use flow_server_env::server_orchestrator;
use flow_server_env::server_status;
use flow_services_inference::type_service;
use flow_services_references::find_refs_types;
use flow_utils_concurrency::worker_cancel;

use crate::recheck_updates;

pub fn init_tokio_runtime() {
    flow_tokio_runtime::prewarm_blocking_pool();
}

pub struct ProfilingFinished {
    pub duration: f64,
}

impl ProfilingFinished {
    pub fn get_profiling_duration(&self) -> f64 {
        self.duration
    }
}

pub fn process_updates(
    skip_incompatible: bool,
    options: &Options,
    committed_heap: &Arc<CommittedHeap>,
    updates: &BTreeSet<String>,
) -> Updates {
    let transaction = ActiveTransaction::new(committed_heap.dupe());
    match recheck_updates::process_updates(skip_incompatible, options, &transaction, updates) {
        Ok(updates) => Updates::NormalUpdates(updates),
        Err(recheck_updates::Error::RecoverableShouldReinitNonLazily {
            msg,
            updates,
            changed_declaration_files,
        }) => {
            flow_hh_logger::info!("Libdef change detected: {}", msg);
            flow_hh_logger::info!("Will require full check reinit ({} updates)", updates.len());
            Updates::RequiredFullCheckReinit {
                updates,
                change: server_monitor_listener_state::IncompatibleLibChange {
                    changed_declaration_files,
                },
            }
        }
        Err(recheck_updates::Error::Unrecoverable { msg, exit_status }) => {
            flow_common_exit_status::exit_with_msg(exit_status, &msg);
        }
    }
}

fn send_start_recheck(env: &server_env::Env) {
    monitor_rpc::status_update(server_status::Event::RecheckStart);
    persistent_connection::send_start_recheck(&env.connections);
}

fn persistent_server_logging_context() -> lsp_prot::LoggingContext {
    flow_event_logger::get_context()
}

fn recheck(
    genv: &server_env::Genv,
    orchestrator: &server_orchestrator::ServerOrchestratorHandle,
    env: server_env::EnvRef,
    files_to_force: CheckedSet,
    find_ref_command: &mut Option<FindRefCommand>,
    incompatible_lib_change: Option<server_monitor_listener_state::IncompatibleLibChange>,
    changed_mergebase: Option<bool>,
    missed_changes: bool,
    will_be_checked_files: &mut CheckedSet,
    updates: CheckedSet,
    recheck_epoch: u64,
) -> Result<ProfilingFinished, type_service::RecheckError> {
    let options = &genv.options;
    let workers = genv.workers.as_ref().unwrap();

    let find_ref_request = match find_ref_command.as_ref() {
        Some(FindRefCommand { request, .. }) => request
            .downcast_ref::<find_refs_types::Request>()
            .expect("find ref request type should match rechecker")
            .clone(),
        None => find_refs_types::empty_request(),
    };

    let should_print_summary = options.profile && !options.quiet;
    let recheck_start = Instant::now();

    send_start_recheck(&env);

    if incompatible_lib_change.is_some() {
        flow_flowlib::extract_if_missing_or_exit(
            options.file_options.default_lib_dir.as_ref(),
            options.jsx_intrinsic_elements,
        );
    }

    let recheck_profiling = profiling_js::Running::new("Recheck");
    let recheck_result = profiling_js::with_current(&recheck_profiling, || {
        memory_utils::with_committed_heap(orchestrator.heap().dupe(), || {
            type_service::recheck(
                Some(orchestrator),
                workers,
                orchestrator.heap(),
                options,
                &updates,
                &find_ref_request,
                files_to_force,
                incompatible_lib_change,
                changed_mergebase,
                missed_changes,
                will_be_checked_files,
                env,
            )
        })
    });
    let (log_recheck_event, recheck_stats, find_ref_results, prepared) = match recheck_result {
        Ok((log_event, stats, results, prepared)) => (log_event, stats, results, prepared),
        Err(type_service::RecheckError::Canceled(changed_files)) => {
            return Err(type_service::RecheckError::Canceled(changed_files));
        }
        Err(type_service::RecheckError::TooSlow) => {
            unreachable!("TooSlow is handled inside type_service::recheck");
        }
    };
    orchestrator.commit_recheck(|| prepared.commit(), recheck_epoch);
    let recheck_profiling = recheck_profiling.finish();
    if should_print_summary {
        profiling_js::print_summary(&recheck_profiling);
    }

    if let Some(FindRefCommand {
        client_id,
        references_to_lsp_response,
        ..
    }) = find_ref_command.take()
    {
        let (response, metadata) = references_to_lsp_response(Box::new(find_ref_results));
        let metadata = lsp_prot::Metadata {
            server_logging_context: Some(persistent_server_logging_context()),
            ..metadata
        };
        if let Some(client) = persistent_connection::get_client(client_id) {
            persistent_connection::send_response((response, metadata), &client);
        }
    }

    let duration = recheck_start.elapsed().as_secs_f64();
    let profiling = ProfilingFinished { duration };

    log_recheck_event(&recheck_profiling.to_event_logger_json_properties());

    let lsp_stats = lsp_prot::RecheckStats {
        dependent_file_count: recheck_stats.dependent_file_count as i32,
        changed_file_count: recheck_stats.changed_file_count as i32,
        top_cycle: recheck_stats.top_cycle.map(|(f, s)| (f, s as i32)),
    };
    monitor_rpc::send_telemetry(lsp_prot::TelemetryFromServer::RecheckSummary {
        duration: profiling.get_profiling_duration(),
        stats: lsp_stats,
    });

    Ok(profiling)
}

// Runs a function which should be canceled if we are notified about any file changes. After the
// work is canceled, post_cancel is called and its result returned
fn run_but_cancel_on_file_changes<T, ProcessUpdates, GetForced, F, PreCancel, PostCancel>(
    _options: &Options,
    recheck_queue: Arc<RecheckQueue>,
    process_updates: ProcessUpdates,
    get_forced: GetForced,
    priority: Priority,
    f: F,
    pre_cancel: PreCancel,
    post_cancel: PostCancel,
) -> T
where
    ProcessUpdates: Fn(bool, &BTreeSet<String>) -> Updates + Send + 'static,
    GetForced: Fn() -> CheckedSet + Send + 'static,
    F: FnOnce() -> Result<T, type_service::RecheckError>,
    PreCancel: FnOnce() + Send + 'static,
    PostCancel: FnOnce() -> T,
{
    let (stop_tx, stop_rx) = crossbeam::channel::bounded::<()>(1);
    let (result_tx, result_rx) = std::sync::mpsc::channel::<bool>();
    // Detached on purpose: the task reports `cancel_fired` via `result_tx`, then
    // drops its captured closures (holding a deep `Env` clone) on the blocking
    // pool, off the caller's critical path.
    flow_tokio_runtime::spawn_blocking(move || {
        let cancel_fired = match server_monitor_listener_state::wait_for_updates_for_recheck(
            &recheck_queue,
            &process_updates,
            &get_forced,
            priority,
            Some(&stop_rx),
        ) {
            Some(WorkloadChanges {
                num_files_to_prioritize,
                num_files_to_force,
                num_files_to_recheck,
            }) => {
                flow_hh_logger::info!(
                    "Canceling recheck to prioritize {}, force {}, and recheck {} additional files",
                    num_files_to_prioritize,
                    num_files_to_force,
                    num_files_to_recheck,
                );
                flow_event_logger::recheck_canceled(
                    match priority {
                        Priority::Priority => "true",
                        Priority::Normal => "",
                    },
                    num_files_to_prioritize as i32,
                    num_files_to_recheck as i32,
                    num_files_to_force as i32,
                );
                pre_cancel();
                worker_cancel::stop_workers();
                true
            }
            None => false,
        };
        match result_tx.send(cancel_fired) {
            Ok(()) => {}
            // The caller is no longer waiting (e.g. server shutdown).
            Err(std::sync::mpsc::SendError(_)) => {}
        }
    });

    let ret = f();
    match stop_tx.try_send(()) {
        Ok(()) => {}
        Err(crossbeam::channel::TrySendError::Full(())) => {}
        Err(crossbeam::channel::TrySendError::Disconnected(())) => {}
    }
    // `Err` (default `false`) means the cancel task ended without reporting (panic/shutdown).
    let cancel_fired: bool = result_rx.recv().unwrap_or_default();
    if cancel_fired && ret.is_ok() {
        worker_cancel::resume_workers();
    }
    match ret {
        Ok(ret) => ret,
        Err(type_service::RecheckError::Canceled(_changed_files)) => post_cancel(),
        Err(type_service::RecheckError::TooSlow) => {
            unreachable!("TooSlow is handled inside type_service::recheck");
        }
    }
}

pub(crate) enum RecheckOutcome {
    NothingToDo {
        completed_recheck_epoch: u64,
    },
    CompletedRecheck {
        profiling: ProfilingFinished,
        recheck_count: usize,
    },
}

// Result of a single recheck attempt. A canceled attempt yields `Restart` rather
// than recursing, so `recheck_single` can drive restarts in a loop (see there).
enum RecheckAttempt {
    Done(RecheckOutcome),
    Restart,
}

pub(crate) fn recheck_single(
    recheck_count: usize,
    genv: &server_env::Genv,
    orchestrator: &server_orchestrator::ServerOrchestratorHandle,
) -> RecheckOutcome {
    // Drive canceled-recheck restarts iteratively instead of recursively. Each
    // attempt that gets canceled returns `Restart` and asks the orchestrator for a
    // fresh `Env` rather than calling itself again; the canceled attempt's `Env`
    // snapshot is dropped before the next attempt starts. This
    // bounds both stack depth and the number of simultaneously live `Env`
    // snapshots to O(1) regardless of how many consecutive cancellations occur.
    let mut recheck_count = recheck_count;
    loop {
        match recheck_single_attempt(recheck_count, genv, orchestrator) {
            RecheckAttempt::Done(outcome) => return outcome,
            RecheckAttempt::Restart => recheck_count += 1,
        }
    }
}

// This ref is an estimate of the files which will be checked by the time the recheck is done.
// As the recheck progresses, the estimate will get better. We use this estimate to prevent
// canceling the recheck to force a file which we were already going to check.
// This early estimate is not a very good estimate, since it's missing new dependents and
// dependencies. However it should be good enough to prevent rechecks continuously restarting as
// the server gets spammed with autocomplete requests.
fn recheck_single_attempt(
    recheck_count: usize,
    genv: &server_env::Genv,
    orchestrator: &server_orchestrator::ServerOrchestratorHandle,
) -> RecheckAttempt {
    // Every attempt reads the committed state from its owner rather than carrying an `Env` of its
    // own, so any update that landed while the previous attempt ran is already applied.
    let server_orchestrator::RecheckSnapshot {
        env,
        completed_recheck_epoch,
    } = orchestrator.begin_recheck();
    let options = &genv.options;

    let will_be_checked_files = Arc::new(RwLock::new(env.checked_files.dupe()));
    let (priority, workload) = {
        let process_updates = |skip_incompatible: bool, updates: &BTreeSet<String>| -> Updates {
            process_updates(skip_incompatible, options, orchestrator.heap(), updates)
        };
        let get_forced = || {
            will_be_checked_files
                .read()
                .expect("will_be_checked_files lock should not be poisoned")
                .dupe()
        };
        server_monitor_listener_state::get_and_clear_recheck_workload(
            orchestrator.recheck(),
            &process_updates,
            &get_forced,
        )
    };

    let did_change_mergebase = workload.metadata.changed_mergebase.unwrap_or(false);
    let missed_changes = workload.metadata.missed_changes;
    let incompatible_lib_change = workload.incompatible_lib_change.dupe();
    let recheck_epoch = workload.recheck_epoch;

    let mut files_to_recheck_set = CheckedSet::empty();
    files_to_recheck_set.add(
        Some(workload.files_to_recheck.dupe()),
        None,
        Some(workload.files_to_prioritize.dupe()),
    );

    if missed_changes && !did_change_mergebase {
        files_to_recheck_set.add(Some(env.checked_files.focused().clone()), None, None);
    }

    if incompatible_lib_change.is_none()
        && !did_change_mergebase
        && files_to_recheck_set.is_empty()
        && workload.files_to_force.is_empty()
    {
        return RecheckAttempt::Done(RecheckOutcome::NothingToDo {
            completed_recheck_epoch: completed_recheck_epoch.max(recheck_epoch),
        });
    }

    // One pre-recheck `Env` ref, shared by the workload loop and the cancel-path
    // `process_updates`.
    let env_snapshot = env.dupe();

    let options_for_cancel = options.dupe();
    let committed_heap_for_cancel = orchestrator.heap().dupe();
    let process_updates_for_cancel =
        move |skip_incompatible: bool, updates: &BTreeSet<String>| -> Updates {
            crate::rechecker::process_updates(
                skip_incompatible,
                &options_for_cancel,
                &committed_heap_for_cancel,
                updates,
            )
        };
    let will_be_checked_files_for_cancel = will_be_checked_files.clone();
    let get_forced_for_cancel = move || {
        will_be_checked_files_for_cancel
            .read()
            .expect("will_be_checked_files lock should not be poisoned")
            .dupe()
    };

    // The canceled recheck, or a preceding sequence of canceled rechecks where none completed,
    // may have introduced garbage into shared memory. Since we immediately start another
    // recheck, we should first check whether we need to compact. Otherwise, the heap could
    // potentially grow unbounded.
    // The constant budget provided here should be sufficient to fully scan a 5G heap within 5
    // iterations. We want to avoid the scenario where repeatedly cancelled rechecks cause the
    // heap to grow faster than we can scan. An algorithmic approach to determine the amount of
    // work based on the allocation rate would be better.
    // Adding files_to_force to will_be_checked_files makes sure that future requests for
    // the same files doesn't cause us to cancel a check that was already working on
    // files are definitely checked, so we can add them now.
    worker_cancel::resume_workers();
    will_be_checked_files
        .write()
        .expect("will_be_checked_files lock should not be poisoned")
        .union(workload.files_to_force.dupe());

    let workload_cell = std::rc::Rc::new(std::cell::RefCell::new(Some(workload)));
    let workload_cell_for_recheck = workload_cell.clone();
    let will_be_checked_files_for_recheck = will_be_checked_files.clone();
    let f = move || {
        let mut workload = workload_cell_for_recheck
            .borrow_mut()
            .take()
            .expect("recheck workload should be available");
        let mut will_be_checked_files_for_recheck = will_be_checked_files_for_recheck
            .read()
            .expect("will_be_checked_files lock should not be poisoned")
            .dupe();
        match recheck(
            genv,
            orchestrator,
            env,
            workload.files_to_force.dupe(),
            &mut workload.find_ref_command,
            incompatible_lib_change,
            workload.metadata.changed_mergebase,
            missed_changes,
            &mut will_be_checked_files_for_recheck,
            files_to_recheck_set,
            recheck_epoch,
        ) {
            Ok(profiling) => Ok(RecheckAttempt::Done(RecheckOutcome::CompletedRecheck {
                profiling,
                recheck_count,
            })),
            Err(type_service::RecheckError::Canceled(_changed_files)) => {
                *workload_cell_for_recheck.borrow_mut() = Some(workload);
                Err(type_service::RecheckError::Canceled(_changed_files))
            }
            Err(type_service::RecheckError::TooSlow) => Err(type_service::RecheckError::TooSlow),
        }
    };
    let workload_cell_for_cancel = workload_cell.clone();
    let outcome = run_but_cancel_on_file_changes(
        options,
        orchestrator.recheck().dupe(),
        process_updates_for_cancel,
        get_forced_for_cancel,
        priority,
        f,
        || {},
        move || {
            let workload = workload_cell_for_cancel
                .borrow_mut()
                .take()
                .expect("canceled recheck should leave workload available");
            flow_hh_logger::info!(
                "Recheck successfully canceled. Restarting the recheck to include new file changes"
            );
            let _done = orchestrator.collect_heap_slice(256000);
            server_monitor_listener_state::requeue_workload(orchestrator.recheck(), workload);
            RecheckAttempt::Restart
        },
    );
    // Drop this attempt's snapshot on the blocking pool before the driver starts
    // the next attempt; if it's the last reference, the deep `Env` `Drop` runs
    // there instead of on the serve loop.
    flow_tokio_runtime::spawn_blocking(move || drop(env_snapshot));
    outcome
}

// It's not obvious to Mr Gabe how we should merge together the profiling info from multiple
// rechecks. But something is better than nothing...
pub fn recheck_loop(
    genv: &server_env::Genv,
    orchestrator: &server_orchestrator::ServerOrchestratorHandle,
) -> Vec<ProfilingFinished> {
    let mut profiling_list: Vec<ProfilingFinished> = Vec::new();
    loop {
        let should_print_summary = genv.options.profile;
        let recheck_series_profiling = profiling_js::Running::new("RecheckSeries");
        let recheck_result = recheck_single(1, genv, orchestrator);
        let recheck_series_profiling = recheck_series_profiling.finish();
        if should_print_summary {
            flow_hh_logger::info!(
                "RecheckSeries: wall duration {:.3}s",
                recheck_series_profiling.get_profiling_duration()
            );
        }
        match recheck_result {
            RecheckOutcome::NothingToDo {
                completed_recheck_epoch,
            } => {
                orchestrator.finish_recheck(completed_recheck_epoch);
                return profiling_list;
            }
            RecheckOutcome::CompletedRecheck {
                profiling: recheck_profiling,
                recheck_count,
            } => {
                flow_event_logger::recheck_series(
                    recheck_count as i32,
                    &recheck_series_profiling.to_event_logger_json_properties(),
                );
                profiling_list.push(recheck_profiling);
            }
        }
    }
}
