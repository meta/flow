/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::any::Any;
use std::collections::BTreeSet;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use crossbeam::channel::Receiver;
use crossbeam::channel::Sender;
use dupe::Dupe;
use flow_common_utils::checked_set::CheckedSet;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_parser::file_key::FileKey;
use flow_parser::loc::Loc;

use crate::monitor_prot::FileWatcherMetadata;
use crate::monitor_prot::empty_file_watcher_metadata;
use crate::monitor_prot::merge_file_watcher_metadata;
use crate::server_orchestrator::Control;

/// The work still owed to the rechecker: pushes that have arrived, and the workload they have
/// been merged into. Owned by `ServerOrchestrator`, reachable only through its handle.
///
/// The locks stay because three threads push here and two block on it, so this cannot live in the
/// executor's single-threaded `State`.
pub struct RecheckQueue {
    stream: Mutex<Vec<RecheckMsg>>,
    acc: Mutex<Option<RecheckWorkload>>,
    next_epoch: AtomicU64,
    /// The highest epoch of a push that holds serial commands. The executor reads this instead of
    /// keeping a copy of its own.
    latest_pending_epoch: AtomicU64,
    notify: (Sender<()>, Receiver<()>),
    /// Senders for the in-progress recheck-cancel monitors. Each in-progress
    /// recheck registers a sender here so a new force-recheck or file-watcher
    /// update arriving DURING the recheck can preempt it. This is the Rust port
    /// equivalent of OCaml's `Lwt.pick` between the recheck thread and the
    /// file-watcher listener inside `run_but_cancel_on_file_changes`. Each push to
    /// the recheck stream notifies every registered sender so the corresponding
    /// monitor thread can call `worker_cancel::stop_workers()`.
    push_subscribers: Mutex<Vec<Sender<()>>>,
    /// Tells the executor that queued work changed. It is parked on its control channel and
    /// cannot poll.
    control: Sender<Control>,
}

impl RecheckQueue {
    pub(crate) fn new(control: Sender<Control>) -> Self {
        Self {
            stream: Mutex::new(Vec::new()),
            acc: Mutex::new(None),
            next_epoch: AtomicU64::new(0),
            latest_pending_epoch: AtomicU64::new(0),
            notify: crossbeam::channel::unbounded(),
            push_subscribers: Mutex::new(Vec::new()),
            control,
        }
    }

    /// The highest epoch pushed so far that holds serial commands.
    pub(crate) fn latest_pending_epoch(&self) -> u64 {
        self.latest_pending_epoch.load(Ordering::Relaxed)
    }

    /// A completed recheck can carry an epoch above every serial-command-holding push, because a
    /// workload merges reprioritisation pushes too.
    pub(crate) fn raise_pending_epoch(&self, epoch: u64) {
        self.latest_pending_epoch
            .fetch_max(epoch, Ordering::Relaxed);
    }
}

// Outstanding cancellation requests are lodged here as soon as they arrive
// from the monitor (NOT FIFO) as well as being lodged in the normal FIFO
// queue. (1) if there was a workload sent prior to the cancellation request
// but we're only just FIFO getting to it now, it can peek to see whether
// that a cancellation request came after it, and not bother starting.
// (2) by the time the FIFO queue gets around to seeing the cancellation
// request in the normal FIFO queue, then we know it can no longer cancel
// any future requests, so we'll remove it from the set.
// Observe that our cancellation handling is best-effort only... we won't
// cancel something already underway, and we might start something even while
// the cancellation request is queued up somewhere between monitor and server.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CancellationRequestId {
    Number(i32),
    String(String),
}

impl From<&crate::lsp_prot::LspId> for CancellationRequestId {
    fn from(id: &crate::lsp_prot::LspId) -> Self {
        match id {
            lsp_types::NumberOrString::Number(n) => Self::Number(*n),
            lsp_types::NumberOrString::String(s) => Self::String(s.clone()),
        }
    }
}

static CANCELLATION_REQUESTS: Mutex<BTreeSet<CancellationRequestId>> = Mutex::new(BTreeSet::new());

pub fn cancellation_requests() -> &'static Mutex<BTreeSet<CancellationRequestId>> {
    &CANCELLATION_REQUESTS
}

pub type FindRefResponseTransformer =
    Box<dyn FnOnce(Box<dyn Any + Send>) -> crate::lsp_prot::ResponseWithMetadata + Send>;

pub struct FindRefCommand {
    pub request: Box<dyn Any + Send>,
    pub client_id: crate::lsp_prot::ClientId,
    pub references_to_lsp_response: FindRefResponseTransformer,
}

struct RecheckMsg {
    recheck_epoch: u64,
    file_watcher_metadata: Option<FileWatcherMetadata>,
    files: RecheckFiles,
}

enum RecheckFiles {
    ChangedFiles(BTreeSet<String>, bool),
    FilesToForceFocusedAndRecheck {
        files: BTreeSet<String>,
        /// Normally, a recheck will abort if certain files change in an
        /// incompatible way that can't be handled incrementally. But when
        /// starting a lazy server, we just read the flowconfig, libs, etc.
        /// as part of the init so we shouldn't fail if they are included
        /// in the files changed since mergebase.
        skip_incompatible: bool,
    },
    GlobalFindRef {
        def_locs: Vec<Loc>,
        find_ref_command: FindRefCommand,
    },
    DependenciesToPrioritize(FlowOrdSet<FileKey>),
    FilesToReinit {
        files_to_prioritize: FlowOrdSet<FileKey>,
        files_to_recheck: FlowOrdSet<FileKey>,
        files_to_force: CheckedSet,
    },
}

fn push_recheck_msg_with_metadata(
    queue: &RecheckQueue,
    metadata: Option<FileWatcherMetadata>,
    files: RecheckFiles,
) -> u64 {
    let recheck_epoch = {
        let mut stream = queue.stream.lock().unwrap();
        let recheck_epoch = queue.next_epoch.fetch_add(1, Ordering::Relaxed) + 1;
        stream.push(RecheckMsg {
            recheck_epoch,
            file_watcher_metadata: metadata,
            files,
        });
        recheck_epoch
    };
    queue.notify.0.send(()).unwrap();
    notify_recheck_push_subscribers(queue);
    recheck_epoch
}

fn push_recheck_msg_marking_pending(
    queue: &RecheckQueue,
    metadata: Option<FileWatcherMetadata>,
    files: RecheckFiles,
) -> u64 {
    let recheck_epoch = {
        let mut stream = queue.stream.lock().unwrap();
        let recheck_epoch = queue.next_epoch.fetch_add(1, Ordering::Relaxed) + 1;
        stream.push(RecheckMsg {
            recheck_epoch,
            file_watcher_metadata: metadata,
            files,
        });
        queue.raise_pending_epoch(recheck_epoch);
        queue
            .control
            .send(Control::RecheckPending)
            .expect("the command control channel should stay open");
        recheck_epoch
    };
    queue.notify.0.send(()).unwrap();
    notify_recheck_push_subscribers(queue);
    recheck_epoch
}

/// Notify every registered recheck-push subscriber that new work arrived. Uses
/// a non-blocking send so a slow or dead subscriber cannot stall the pusher.
/// Disconnected subscribers (whose receiver was dropped without unsubscribing)
/// are removed in the same pass, keeping the subscriber list bounded.
fn notify_recheck_push_subscribers(queue: &RecheckQueue) {
    let mut subscribers = queue.push_subscribers.lock().unwrap();
    subscribers.retain(|sender| match sender.try_send(()) {
        Ok(()) => true,
        Err(crossbeam::channel::TrySendError::Full(())) => true,
        Err(crossbeam::channel::TrySendError::Disconnected(())) => false,
    });
}

/// Subscribe to recheck pushes. The returned receiver fires once per push to
/// the recheck stream (extra pushes while the receiver is full are silently
/// dropped — the receiver only needs to learn that *some* push happened, not
/// how many). When the receiver is dropped the subscriber is removed lazily
/// the next time a push happens, so callers do not need an explicit
/// unsubscribe step.
pub fn subscribe_recheck_pushes(queue: &RecheckQueue) -> Receiver<()> {
    let (tx, rx) = crossbeam::channel::bounded(1);
    queue.push_subscribers.lock().unwrap().push(tx);
    rx
}

fn push_recheck_msg(queue: &RecheckQueue, files: RecheckFiles) -> u64 {
    push_recheck_msg_with_metadata(queue, None, files)
}

pub fn push_files_to_recheck(queue: &RecheckQueue, changed_files: BTreeSet<String>) -> u64 {
    push_recheck_msg_marking_pending(
        queue,
        None,
        RecheckFiles::ChangedFiles(changed_files, false),
    )
}

pub fn push_files_to_recheck_with_metadata(
    queue: &RecheckQueue,
    metadata: Option<FileWatcherMetadata>,
    changed_files: BTreeSet<String>,
) -> u64 {
    push_recheck_msg_marking_pending(
        queue,
        metadata,
        RecheckFiles::ChangedFiles(changed_files, false),
    )
}

pub fn push_files_to_prioritize(queue: &RecheckQueue, changed_files: BTreeSet<String>) -> u64 {
    push_recheck_msg(queue, RecheckFiles::ChangedFiles(changed_files, true))
}

pub fn push_files_to_force_focused_and_recheck(
    queue: &RecheckQueue,
    files: BTreeSet<String>,
) -> u64 {
    push_recheck_msg_marking_pending(
        queue,
        None,
        RecheckFiles::FilesToForceFocusedAndRecheck {
            files,
            skip_incompatible: false,
        },
    )
}

pub fn push_global_find_ref_request<Request, FindRefsFound>(
    queue: &RecheckQueue,
    def_locs: Vec<Loc>,
    request: Request,
    client_id: crate::lsp_prot::ClientId,
    references_to_lsp_response: Box<
        dyn FnOnce(Result<FindRefsFound, String>) -> crate::lsp_prot::ResponseWithMetadata + Send,
    >,
) -> u64
where
    Request: Send + 'static,
    FindRefsFound: Send + 'static,
{
    let references_to_lsp_response = Box::new(move |find_ref_results: Box<dyn Any + Send>| {
        let find_ref_results = *find_ref_results
            .downcast::<Result<FindRefsFound, String>>()
            .expect("find ref result type should match request");
        references_to_lsp_response(find_ref_results)
    });
    push_recheck_msg_marking_pending(
        queue,
        None,
        RecheckFiles::GlobalFindRef {
            def_locs,
            find_ref_command: FindRefCommand {
                request: Box::new(request),
                client_id,
                references_to_lsp_response,
            },
        },
    )
}

/// Triggers a recheck of `files`.
/// Call this immediately after a lazy init, where `files` are the files
/// changed since mergebase.
pub fn push_lazy_init(
    queue: &RecheckQueue,
    metadata: Option<FileWatcherMetadata>,
    files: BTreeSet<String>,
) -> u64 {
    push_recheck_msg_marking_pending(
        queue,
        metadata,
        RecheckFiles::FilesToForceFocusedAndRecheck {
            files,
            skip_incompatible: true,
        },
    )
}

pub fn push_dependencies_to_prioritize(
    queue: &RecheckQueue,
    dependencies: FlowOrdSet<FileKey>,
) -> u64 {
    push_recheck_msg(queue, RecheckFiles::DependenciesToPrioritize(dependencies))
}

pub fn push_after_reinit(
    queue: &RecheckQueue,
    files_to_prioritize: Option<FlowOrdSet<FileKey>>,
    files_to_recheck: Option<FlowOrdSet<FileKey>>,
    files_to_force: Option<CheckedSet>,
) -> u64 {
    let files_to_prioritize = files_to_prioritize.unwrap_or_default();
    let files_to_recheck = files_to_recheck.unwrap_or_default();
    let files_to_force = files_to_force.unwrap_or_else(CheckedSet::empty);
    push_recheck_msg(
        queue,
        RecheckFiles::FilesToReinit {
            files_to_prioritize,
            files_to_recheck,
            files_to_force,
        },
    )
}

pub struct RecheckWorkload {
    pub recheck_epoch: u64,
    pub files_to_prioritize: FlowOrdSet<FileKey>,
    pub files_to_recheck: FlowOrdSet<FileKey>,
    pub files_to_force: CheckedSet,
    pub find_ref_command: Option<FindRefCommand>,
    pub metadata: FileWatcherMetadata,
    pub incompatible_lib_change: Option<IncompatibleLibChange>,
}

#[derive(Clone, Dupe)]
pub struct IncompatibleLibChange {
    pub changed_declaration_files: FlowOrdSet<FileKey>,
}

pub enum Updates {
    NormalUpdates(FlowOrdSet<FileKey>),
    RequiredFullCheckReinit {
        updates: FlowOrdSet<FileKey>,
        change: IncompatibleLibChange,
    },
}

fn empty_recheck_workload() -> RecheckWorkload {
    RecheckWorkload {
        recheck_epoch: 0,
        files_to_prioritize: FlowOrdSet::new(),
        files_to_recheck: FlowOrdSet::new(),
        files_to_force: CheckedSet::empty(),
        find_ref_command: None,
        metadata: empty_file_watcher_metadata(),
        incompatible_lib_change: None,
    }
}

fn with_recheck_acc<T>(queue: &RecheckQueue, f: impl FnOnce(&mut RecheckWorkload) -> T) -> T {
    let mut guard = queue.acc.lock().unwrap();
    if guard.is_none() {
        *guard = Some(empty_recheck_workload());
    }
    f(guard.as_mut().unwrap())
}

/// Updates `workload` while leaving it unchanged when there is nothing to do.
fn update(
    workload: &mut RecheckWorkload,
    files_to_prioritize: Option<&FlowOrdSet<FileKey>>,
    files_to_recheck: Option<&FlowOrdSet<FileKey>>,
    files_to_force: Option<&CheckedSet>,
    find_ref_command: Option<FindRefCommand>,
    metadata: Option<&FileWatcherMetadata>,
) -> bool {
    let mut changed = false;
    if let Some(new_files) = files_to_prioritize {
        let before = workload.files_to_prioritize.len();
        for f in new_files {
            workload.files_to_prioritize.insert(f.dupe());
        }
        if workload.files_to_prioritize.len() != before {
            changed = true;
        }
    }
    if let Some(new_files) = files_to_recheck {
        let before = workload.files_to_recheck.len();
        for f in new_files {
            workload.files_to_recheck.insert(f.dupe());
        }
        if workload.files_to_recheck.len() != before {
            changed = true;
        }
    }
    if let Some(new_force) = files_to_force {
        let before = workload.files_to_force.cardinal();
        workload.files_to_force.union(new_force.dupe());
        if workload.files_to_force.cardinal() != before {
            changed = true;
        }
    }
    if find_ref_command.is_some() {
        workload.find_ref_command = find_ref_command;
        changed = true;
    }
    if let Some(new_metadata) = metadata {
        let merged = merge_file_watcher_metadata(new_metadata, &workload.metadata);
        if workload.metadata != merged {
            workload.metadata = merged;
            changed = true;
        }
    }
    changed
}

fn update_to_require_reinit(workload: &mut RecheckWorkload, change: IncompatibleLibChange) -> bool {
    match &mut workload.incompatible_lib_change {
        None => {
            workload.incompatible_lib_change = Some(change);
            true
        }
        Some(existing) => {
            let previous_declaration_file_count = existing.changed_declaration_files.len();
            for file in change.changed_declaration_files {
                existing.changed_declaration_files.insert(file);
            }
            existing.changed_declaration_files.len() != previous_declaration_file_count
        }
    }
}

pub struct WorkloadChanges {
    pub num_files_to_prioritize: usize,
    pub num_files_to_recheck: usize,
    pub num_files_to_force: usize,
}

/// Processes the messages currently in the recheck stream and returns whether
/// the resulting workload changed.
///
/// The recheck stream gives us files as a set of strings. `process_updates`
/// filters out entries we do not care about and exits on incompatible changes.
///
/// `get_forced` returns the currently forced files. If the recheck stream asks
/// us to focus `foo.js` but it is already focused, then we can ignore it.
pub fn recheck_fetch(
    queue: &RecheckQueue,
    process_updates: &dyn Fn(bool, &BTreeSet<String>) -> Updates,
    get_forced: &dyn Fn() -> CheckedSet,
) -> bool {
    let msgs: Vec<RecheckMsg> = {
        let mut stream = queue.stream.lock().unwrap();
        std::mem::take(&mut *stream)
    };
    let mut changed = false;
    with_recheck_acc(queue, |workload| {
        for msg in msgs {
            let RecheckMsg {
                recheck_epoch,
                files,
                file_watcher_metadata,
            } = msg;
            let msg_changed = match files {
                RecheckFiles::ChangedFiles(changed_files, urgent) => {
                    match process_updates(false, &changed_files) {
                        Updates::NormalUpdates(updates) => {
                            if urgent {
                                update(workload, Some(&updates), None, None, None, None)
                            } else {
                                update(workload, None, Some(&updates), None, None, None)
                            }
                        }
                        Updates::RequiredFullCheckReinit { updates, change } => {
                            let w_changed = if urgent {
                                update(workload, Some(&updates), None, None, None, None)
                            } else {
                                update(workload, None, Some(&updates), None, None, None)
                            };
                            let r_changed = update_to_require_reinit(workload, change);
                            w_changed || r_changed
                        }
                    }
                }
                RecheckFiles::FilesToForceFocusedAndRecheck {
                    files,
                    skip_incompatible,
                } => match process_updates(skip_incompatible, &files) {
                    Updates::NormalUpdates(updates) => {
                        let focused = {
                            let forced = get_forced();
                            let forced_focused = forced.focused().dupe();
                            FlowOrdSet::from(
                                updates
                                    .dupe()
                                    .into_inner()
                                    .relative_complement(forced_focused.into_inner()),
                            )
                        };
                        let mut files_to_force = CheckedSet::empty();
                        files_to_force.add(Some(focused), None, None);
                        update(
                            workload,
                            None,
                            Some(&updates),
                            Some(&files_to_force),
                            None,
                            None,
                        )
                    }
                    Updates::RequiredFullCheckReinit { updates, change } => {
                        let focused = {
                            let forced = get_forced();
                            let forced_focused = forced.focused().dupe();
                            FlowOrdSet::from(
                                updates
                                    .dupe()
                                    .into_inner()
                                    .relative_complement(forced_focused.into_inner()),
                            )
                        };
                        let mut files_to_force = CheckedSet::empty();
                        files_to_force.add(Some(focused), None, None);
                        let w_changed = update(
                            workload,
                            None,
                            Some(&updates),
                            Some(&files_to_force),
                            None,
                            None,
                        );
                        let r_changed = update_to_require_reinit(workload, change);
                        w_changed || r_changed
                    }
                },
                RecheckFiles::GlobalFindRef {
                    def_locs,
                    find_ref_command,
                } => {
                    let files_to_recheck: FlowOrdSet<FileKey> = def_locs
                        .iter()
                        .filter_map(|loc| loc.source())
                        .cloned()
                        .collect();
                    update(
                        workload,
                        None,
                        Some(&files_to_recheck),
                        None,
                        Some(find_ref_command),
                        None,
                    )
                }
                RecheckFiles::DependenciesToPrioritize(dependencies) => {
                    let mut files_to_force = CheckedSet::empty();
                    files_to_force.add(None, None, Some(dependencies));
                    files_to_force.diff(&get_forced());
                    update(workload, None, None, Some(&files_to_force), None, None)
                }
                RecheckFiles::FilesToReinit {
                    files_to_prioritize,
                    files_to_recheck,
                    files_to_force,
                } => {
                    update(
                        workload,
                        Some(&files_to_prioritize),
                        Some(&files_to_recheck),
                        Some(&files_to_force),
                        None,
                        None,
                    );
                    false
                }
            };
            let metadata_changed = match &file_watcher_metadata {
                None => false,
                Some(metadata) => update(workload, None, None, None, None, Some(metadata)),
            };
            let epoch_changed = recheck_epoch > workload.recheck_epoch;
            workload.recheck_epoch = workload.recheck_epoch.max(recheck_epoch);
            changed = changed || msg_changed || metadata_changed || epoch_changed;
        }
    });
    changed
}

pub fn requeue_workload(queue: &RecheckQueue, workload: RecheckWorkload) {
    flow_hh_logger::info!(
        "Re-queueing force-check of {} files and recheck of {} files ({} dependencies)",
        workload.files_to_force.cardinal(),
        workload
            .files_to_recheck
            .dupe()
            .union(workload.files_to_prioritize.dupe())
            .len(),
        workload.files_to_prioritize.len(),
    );
    with_recheck_acc(queue, |prev| {
        prev.recheck_epoch = prev.recheck_epoch.max(workload.recheck_epoch);
        for f in &workload.files_to_prioritize {
            prev.files_to_prioritize.insert(f.dupe());
        }
        for f in &workload.files_to_recheck {
            prev.files_to_recheck.insert(f.dupe());
        }
        prev.files_to_force.union(workload.files_to_force);
        prev.find_ref_command = workload.find_ref_command.or(prev.find_ref_command.take());
        prev.metadata = merge_file_watcher_metadata(&prev.metadata, &workload.metadata);
        if prev.incompatible_lib_change.is_some() || workload.incompatible_lib_change.is_some() {
            flow_hh_logger::info!(
                "Previous recheck requires restart: {}; new workload requires restart: {}",
                prev.incompatible_lib_change.is_some(),
                workload.incompatible_lib_change.is_some(),
            );
        }
        if let Some(change) = workload.incompatible_lib_change {
            update_to_require_reinit(prev, change);
        }
    });
}

pub fn get_and_clear_recheck_workload(
    queue: &RecheckQueue,
    process_updates: &dyn Fn(bool, &BTreeSet<String>) -> Updates,
    get_forced: &dyn Fn() -> CheckedSet,
) -> RecheckWorkload {
    recheck_fetch(queue, process_updates, get_forced);
    let mut guard = queue.acc.lock().unwrap();
    let workload = guard.take().unwrap_or_else(empty_recheck_workload);
    *guard = Some(empty_recheck_workload());
    workload
}

/// Blocks until a recheck message arrives.
fn wait_for_recheck(queue: &RecheckQueue) {
    queue.notify.1.recv().unwrap();
}

fn recheck_workload_counts(queue: &RecheckQueue) -> (usize, usize, usize) {
    with_recheck_acc(queue, |w| {
        (
            w.files_to_prioritize.len(),
            w.files_to_recheck.len(),
            w.files_to_force.cardinal(),
        )
    })
}

fn fetch_recheck_changes(
    queue: &RecheckQueue,
    process_updates: &dyn Fn(bool, &BTreeSet<String>) -> Updates,
    get_forced: &dyn Fn() -> CheckedSet,
) -> Option<WorkloadChanges> {
    let before_len = recheck_workload_counts(queue);
    if recheck_fetch(queue, process_updates, get_forced) {
        let after_len = recheck_workload_counts(queue);
        Some(WorkloadChanges {
            num_files_to_prioritize: after_len.0 - before_len.0,
            num_files_to_recheck: after_len.1 - before_len.1,
            num_files_to_force: after_len.2 - before_len.2,
        })
    } else {
        None
    }
}

pub fn wait_for_updates_for_recheck(
    queue: &RecheckQueue,
    process_updates: &dyn Fn(bool, &BTreeSet<String>) -> Updates,
    get_forced: &dyn Fn() -> CheckedSet,
    stop: Option<&Receiver<()>>,
) -> Option<WorkloadChanges> {
    let recheck_pushes = stop.map(|_| subscribe_recheck_pushes(queue));
    loop {
        if let Some(changes) = fetch_recheck_changes(queue, process_updates, get_forced) {
            return Some(changes);
        }
        match stop {
            Some(stop) => {
                let recheck_pushes = recheck_pushes
                    .as_ref()
                    .expect("recheck push subscription should exist while stop is present");
                crossbeam::channel::select! {
                    recv(recheck_pushes) -> result => {
                        if result.is_err() {
                            return None;
                        }
                    }
                    recv(stop) -> _ => return None,
                }
            }
            None => wait_for_recheck(queue),
        }
    }
}

/// Block until a recheck message arrives without blocking the Tokio runtime.
pub async fn wait_for_recheck_async(queue: &RecheckQueue) {
    let notified = queue.notify.1.clone();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let waiter = std::thread::Builder::new()
        .name("wait_for_recheck".to_string())
        .spawn(move || {
            notified.recv().unwrap();
            match sender.send(()) {
                Ok(()) => {}
                // The serve loop stopped awaiting (e.g. server shutdown).
                Err(()) => {}
            }
        })
        .expect("failed to spawn wait_for_recheck thread");
    match receiver.await {
        Ok(()) => {}
        Err(err) => panic!("wait_for_recheck task failed: {}", err),
    }
    if let Err(err) = waiter.join() {
        std::panic::resume_unwind(err);
    }
}
