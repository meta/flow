/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

use flow_common_vcs::git;
use flow_common_vcs::hg;
use flow_common_vcs::vcs::Vcs;
use flow_common_vcs::vcs::{self};
use flow_common_vcs::vcs_utils;
use flow_server_env::monitor_prot;
use flow_server_env::monitor_prot::FileWatcherMetadata;
use flow_tokio_runtime::handle;

use crate::flow_server_monitor_options;
use crate::status_stream;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitReason {
    WatcherStopped,
    WatcherDied,
    WatcherMissedChanges,
}

pub struct ChangesCondition {
    changed: AtomicBool,
    notify: tokio::sync::Notify,
}

impl ChangesCondition {
    fn new() -> Self {
        Self {
            changed: AtomicBool::new(false),
            notify: tokio::sync::Notify::new(),
        }
    }

    fn notify_changed(&self) {
        self.changed.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    async fn wait(&self) {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.changed.swap(false, Ordering::AcqRel) {
                return;
            }
            notified.await;
        }
    }
}

// TODO - Push-based API for dfind. While the FileWatcher API is push based, dfind is faking it
// by polling every seconds.
//
// If we decide we care about this, we could Build DfindServerLwt which can listen for events and
// write messages at the same time (we don't want to block the file watching if the writes block)
// OCaml's `FileWatcher.watcher` is a class with `val mutable` fields and
// per-instance `Lwt_mutex` for internal serialization. Rust expresses the same
// shape with `&self` methods plus interior mutability inside each impl. This
// avoids requiring callers to hold an outer `tokio::sync::Mutex<AnyWatcher>`,
// which would deadlock the polling loop against the command loop (the polling
// loop sleeps inside `wait_for_changed_files`; the command loop needs the
// watcher on every client request).
pub trait Watcher: Send + Sync {
    fn name(&self) -> &str;
    fn debug(&self) -> bool;
    fn start_init(&self);
    fn wait_for_init(
        &self,
        timeout: Option<f64>,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send;
    fn get_and_clear_changed_files(
        &self,
    ) -> impl std::future::Future<Output = (BTreeSet<String>, Option<FileWatcherMetadata>, bool)> + Send;
    fn wait_for_changed_files(&self) -> impl std::future::Future<Output = ()> + Send;
    fn stop(&self) -> impl std::future::Future<Output = ()> + Send;
    fn waitpid(&self) -> impl std::future::Future<Output = ExitReason> + Send;
    fn getpid(&self) -> Option<i32>;
}

pub struct Dummy;

impl Dummy {
    pub fn new() -> Self {
        Dummy
    }
}

impl Watcher for Dummy {
    fn name(&self) -> &str {
        "dummy"
    }
    fn debug(&self) -> bool {
        false
    }
    fn start_init(&self) {}
    async fn wait_for_init(&self, _timeout: Option<f64>) -> Result<(), String> {
        Ok(())
    }
    async fn get_and_clear_changed_files(
        &self,
    ) -> (BTreeSet<String>, Option<FileWatcherMetadata>, bool) {
        (BTreeSet::new(), None, false)
    }
    async fn wait_for_changed_files(&self) {}
    async fn stop(&self) {}
    async fn waitpid(&self) -> ExitReason {
        std::future::pending().await
    }
    fn getpid(&self) -> Option<i32> {
        None
    }
}
pub async fn changes_since_mergebase(
    mergebase_with: &str,
    watch_paths: &[PathBuf],
) -> BTreeSet<String> {
    fn fold_relative_paths(
        mut acc: BTreeSet<String>,
        root: &std::path::Path,
        paths: &[String],
    ) -> BTreeSet<String> {
        for change in paths {
            let path = root.join(change).to_string_lossy().to_string();
            acc.insert(path);
        }
        acc
    }

    async fn files_changed_since_mergebase_with(
        vcs: Vcs,
        root: &std::path::Path,
        mergebase_with: &str,
    ) -> Result<(String, Vec<String>), vcs_utils::ErrorStatus> {
        let root_str = root.to_string_lossy().to_string();
        match vcs {
            Vcs::Git => {
                git::files_changed_since_mergebase_with(Some(&root_str), mergebase_with).await
            }
            Vcs::Hg => {
                hg::files_changed_since_mergebase_with(Some(&root_str), mergebase_with).await
            }
        }
    }

    async fn fold_files_changed_since_mergebase_with(
        acc: BTreeSet<String>,
        vcs: Vcs,
        root: &std::path::Path,
        mergebase_with: &str,
    ) -> BTreeSet<String> {
        let vcs_name = vcs::name(vcs);
        match files_changed_since_mergebase_with(vcs, root, mergebase_with).await {
            Err(_) => {
                flow_hh_logger::error!(
                    "Not checking changes since mergebase! {} failed to determine the initial mergebase.",
                    vcs_name
                );
                acc
            }
            Ok((mergebase, changes)) => {
                flow_hh_logger::info!(
                    "{} reports the initial mergebase as {:?}, and {} changes",
                    vcs_name,
                    mergebase,
                    changes.len()
                );
                fold_relative_paths(acc, root, &changes)
            }
        }
    }

    async fn helper(
        mergebase_with: &str,
        mut seen_roots: BTreeSet<String>,
        mut acc: BTreeSet<String>,
        paths: &[PathBuf],
    ) -> BTreeSet<String> {
        for path in paths {
            match vcs::find_root(None, path) {
                Some((vcs, root)) => {
                    let root_str = root.to_string_lossy().to_string();
                    if seen_roots.contains(&root_str) {
                    } else {
                        seen_roots.insert(root_str);
                        acc = fold_files_changed_since_mergebase_with(
                            acc,
                            vcs,
                            &root,
                            mergebase_with,
                        )
                        .await;
                    }
                }
                None => {}
            }
        }
        acc
    }

    helper(
        mergebase_with,
        BTreeSet::new(),
        BTreeSet::new(),
        watch_paths,
    )
    .await
}

/// Query VCS for the current mergebase hash.
pub async fn query_mergebase(
    root: &std::path::Path,
    mergebase_with: &str,
) -> Result<String, String> {
    match vcs::find_root(None, root) {
        None => Err("no VCS root found".to_string()),
        Some((vcs, vcs_root)) => {
            let cwd = vcs_root.to_string_lossy().to_string();
            let result = match vcs {
                Vcs::Hg => hg::merge_base(Some(&cwd), ".", mergebase_with).await,
                Vcs::Git => git::merge_base(Some(&cwd), mergebase_with, "HEAD").await,
            };
            match result {
                Ok(hash) => Ok(hash),
                Err(vcs_utils::ErrorStatus::NotInstalled { path }) => {
                    Err(format!("VCS not installed at {}", path))
                }
                Err(vcs_utils::ErrorStatus::Errored(msg)) => Err(msg),
            }
        }
    }
}

pub mod watchman_file_watcher {
    use super::*;

    pub struct EnvShared {
        pub files: BTreeSet<String>,
        pub metadata: FileWatcherMetadata,
        pub is_initial: bool,
    }

    pub struct Env {
        pub instance: Arc<tokio::sync::Mutex<Option<flow_watchman::Env>>>,
        pub shared: Arc<std::sync::Mutex<EnvShared>>,
        pub listening_thread: Arc<tokio::sync::Mutex<Option<tokio::task::JoinHandle<ExitReason>>>>,
        pub changes_condition: Arc<ChangesCondition>,
        pub init_settings: flow_watchman::InitSettings,
    }

    pub mod watchman_listen_loop {
        use super::*;

        pub fn should_pause() -> bool {
            true
        }

        pub fn extract_hg_update_metadata(
            metadata: Option<&serde_json::Value>,
        ) -> (String, String) {
            match metadata {
                None => ("<UNKNOWN>".to_string(), "<UNKNOWN REV>".to_string()),
                Some(metadata) => {
                    let distance = match metadata.get("distance") {
                        Some(serde_json::Value::String(s)) => s.clone(),
                        Some(serde_json::Value::Number(n)) => n.to_string(),
                        _ => "<UNKNOWN>".to_string(),
                    };
                    let rev = match metadata.get("rev") {
                        Some(serde_json::Value::String(s)) => s.clone(),
                        _ => "<UNKNOWN REV>".to_string(),
                    };
                    (distance, rev)
                }
            }
        }

        pub fn log_state_enter(name: &str, metadata: Option<&serde_json::Value>) {
            let data = metadata.map(|m| m.to_string()).unwrap_or_default();
            flow_event_logger::file_watcher_event_started(name, &data);
        }

        pub fn log_state_leave(name: &str, metadata: Option<&serde_json::Value>) {
            let data = metadata.map(|m| m.to_string()).unwrap_or_default();
            flow_event_logger::file_watcher_event_finished(name, &data);
        }

        pub fn broadcast(env: &Env) {
            let shared = env.shared.lock().unwrap();
            if !shared.files.is_empty() || shared.metadata.missed_changes {
                env.changes_condition.notify_changed();
            }
        }

        pub async fn main(env: &Env) -> Result<(), flow_watchman::Failure> {
            let instance = {
                let mut guard = env.instance.lock().await;
                guard.take().expect("watchman instance missing")
            };
            let (instance, pushed_changes) = match flow_watchman::get_changes(instance).await {
                Ok((instance, pushed_changes)) => (instance, pushed_changes),
                Err(flow_watchman::Failure::Dead) => return Err(flow_watchman::Failure::Dead),
                Err(flow_watchman::Failure::Restarted) => {
                    status_stream::file_watcher_deferred("Watchman restart".to_string());
                    flow_hh_logger::error!(
                        "Watchman get_changes returned Restarted but env was consumed; treating as Dead"
                    );
                    return Err(flow_watchman::Failure::Dead);
                }
            };
            *env.instance.lock().await = Some(instance);
            match pushed_changes {
                flow_watchman::PushedChanges::FilesChanged {
                    changes,
                    changed_mergebase,
                } => {
                    // this event tells us all the files that changed. if changed_mergebase,
                    // then some of these changes are upstream files. we could avoid rechecking
                    // them if we re-init. we signal this by setting changed_mergebase.
                    {
                        let mut shared = env.shared.lock().unwrap();
                        shared.files.extend(changes);
                        let metadata = FileWatcherMetadata {
                            changed_mergebase,
                            missed_changes: false,
                        };
                        shared.metadata =
                            flow_monitor_rpc::monitor_prot::merge_file_watcher_metadata(
                                &shared.metadata,
                                &metadata,
                            );
                    }
                    broadcast(env);
                    Ok(())
                }
                flow_watchman::PushedChanges::MissedChanges {
                    prev_mergebase,
                    mergebase,
                    changes_since_mergebase,
                } => {
                    // When watchman restarts, we miss any filesystem changes that might happen while it's
                    // down. Likewise, if so many files change that Watchman's underlying file watchers
                    // can't keep up, it acts like it restarted. To re-synchronize, we need to recheck
                    // all of the files that could have changed while it wasn't watching:
                    //
                    // 1) a file that was previously unchanged is now changed
                    // 2) a changed file changed again
                    // 3) a previously changed file was reverted
                    // 4) the mergebase changed, changing some committed files
                    //
                    // Since watchman told us the changes since mergebase, we know about (1) and (2).
                    // We handle (3) by setting [missed_changes = true], which triggers a recheck of
                    // all focused (i.e. previously changed) files. But we can't incrementally
                    // handle (4): watchman can't tell us all the files that changed between the two
                    // mergebase commits (`hg` can, but it's not worth implementing this).
                    let changed_mergebase = prev_mergebase != mergebase;
                    if changed_mergebase {
                        flow_hh_logger::info!(
                            "Watchman missed changes, and the mergebase changed from {:?} to {:?}.",
                            prev_mergebase,
                            mergebase
                        );
                    } else {
                        flow_hh_logger::info!(
                            "Watchman missed changes, but the mergebase didn't change."
                        );
                    }
                    flow_hh_logger::info!(
                        "Watchman reports {} files have changed since the mergebase",
                        changes_since_mergebase.len()
                    );
                    {
                        let mut shared = env.shared.lock().unwrap();
                        shared.files.extend(changes_since_mergebase);
                        let metadata = FileWatcherMetadata {
                            changed_mergebase: Some(changed_mergebase),
                            missed_changes: true,
                        };
                        shared.metadata =
                            flow_monitor_rpc::monitor_prot::merge_file_watcher_metadata(
                                &shared.metadata,
                                &metadata,
                            );
                    }
                    broadcast(env);
                    Ok(())
                }
                flow_watchman::PushedChanges::StateEnter(name, metadata) => {
                    if name == "hg.update" {
                        let (distance, rev) = extract_hg_update_metadata(metadata.as_ref());
                        log_state_enter(&name, metadata.as_ref());
                        flow_hh_logger::info!(
                            "Watchman reports an hg.update just started. Moving {} revs from {}",
                            distance,
                            rev
                        );
                    } else if env.init_settings.defer_states.iter().any(|s| s == &name) {
                        log_state_enter(&name, metadata.as_ref());
                        flow_hh_logger::info!(
                            "Watchman reports {} just started. Filesystem notifications are paused.",
                            name
                        );
                        status_stream::file_watcher_deferred(name);
                    }
                    Ok(())
                }
                flow_watchman::PushedChanges::StateLeave(name, metadata) => {
                    if name == "hg.update" {
                        let (distance, rev) = extract_hg_update_metadata(metadata.as_ref());
                        log_state_leave(&name, metadata.as_ref());
                        flow_hh_logger::info!(
                            "Watchman reports an hg.update just finished. Moved {} revs to {}",
                            distance,
                            rev
                        );
                    } else if env.init_settings.defer_states.iter().any(|s| s == &name) {
                        log_state_leave(&name, metadata.as_ref());
                        flow_hh_logger::info!(
                            "Watchman reports {} ended. Filesystem notifications resumed.",
                            name
                        );
                        status_stream::file_watcher_ready();
                    }
                    Ok(())
                }
            }
        }

        pub fn catch(failure: &flow_watchman::Failure) -> flow_watchman::Failure {
            match failure {
                flow_watchman::Failure::Dead | flow_watchman::Failure::Restarted => {
                    flow_hh_logger::error!("Watchman unavailable. Exiting...");
                    failure.clone()
                }
            }
        }
    }

    pub async fn listen(env: Arc<Env>) -> ExitReason {
        loop {
            match watchman_listen_loop::main(&env).await {
                Ok(()) => {}
                Err(failure) => {
                    let failure = watchman_listen_loop::catch(&failure);
                    return match failure {
                        flow_watchman::Failure::Dead => ExitReason::WatcherDied,
                        flow_watchman::Failure::Restarted => ExitReason::WatcherMissedChanges,
                    };
                }
            }
        }
    }

    pub struct Watchman {
        // Wrapped behind interior mutability so methods can take `&self`,
        // matching OCaml's class-based shape with implicit field mutation.
        env: std::sync::Mutex<Option<Arc<Env>>>,
        init_thread: std::sync::Mutex<
            Option<tokio::task::JoinHandle<Result<(flow_watchman::Env, BTreeSet<String>), String>>>,
        >,
        init_settings: std::sync::Mutex<Option<flow_watchman::InitSettings>>,
        mergebase_with: String,
        server_options: flow_common::options::Options,
        watchman_options: flow_server_monitor_options::WatchmanOptions,
    }

    impl Watchman {
        pub fn new(
            mergebase_with: String,
            server_options: flow_common::options::Options,
            watchman_options: flow_server_monitor_options::WatchmanOptions,
        ) -> Self {
            Watchman {
                env: std::sync::Mutex::new(None),
                init_thread: std::sync::Mutex::new(None),
                init_settings: std::sync::Mutex::new(None),
                mergebase_with,
                server_options,
                watchman_options,
            }
        }

        fn get_env(&self) -> Arc<Env> {
            match self.env.lock().unwrap().as_ref() {
                None => panic!("Watchman was not initialized"),
                Some(env) => env.clone(),
            }
        }

        pub fn get_env_for_waitpid(&self) -> Arc<Env> {
            self.get_env()
        }

        fn log_watch_spec(&self) {
            let file_options = &self.server_options.file_options;
            let extensions =
                flow_server_file_watcher_spec::file_watcher_spec::get_suffixes(file_options);
            let file_names = flow_server_file_watcher_spec::file_watcher_spec::get_file_names(
                &self.server_options,
            );
            let include_dirs =
                flow_server_file_watcher_spec::file_watcher_spec::get_include_dirs_absolute(
                    &self.server_options,
                );
            flow_hh_logger::info!(
                "Watchman connection established with watch_spec: extensions=[{}], file_names=[{}], include_dirs=[{}], exclude_dirs=[{}]",
                extensions.join(", "),
                file_names.join(", "),
                include_dirs.join(", "),
                flow_server_file_watcher_spec::file_watcher_spec::EXCLUDE_DIRS.join(", ")
            );
        }
    }

    impl Watcher for Watchman {
        fn name(&self) -> &str {
            "watchman"
        }

        fn debug(&self) -> bool {
            self.watchman_options.debug
        }

        fn start_init(&self) {
            let flow_server_monitor_options::WatchmanOptions {
                debug,
                defer_states,
                sync_timeout,
            } = &self.watchman_options;
            let _file_options = &self.server_options.file_options;
            let watchman_expression_terms =
                flow_server_watchman_expression_terms::make(&self.server_options);
            let file_name_terms = Some(flow_server_watchman_expression_terms::file_name_terms(
                &self.server_options,
            ));
            let should_track_mergebase = self.server_options.lazy_mode;
            let settings = flow_watchman::InitSettings {
                debug_logging: *debug,
                defer_states: defer_states.clone(),
                expression_terms: watchman_expression_terms,
                file_name_terms,
                mergebase_with: self.mergebase_with.clone(),
                roots: flow_common::files::watched_paths(&self.server_options.file_options),
                should_track_mergebase,
                // Defer updates during `hg.update` and defer_states
                subscribe_mode: flow_watchman::SubscribeMode::DeferChanges,
                subscription_prefix: "flow_watcher".to_string(),
                sync_timeout: sync_timeout.map(|t| t as i64),
            };
            *self.init_settings.lock().unwrap() = Some(settings.clone());
            *self.init_thread.lock().unwrap() =
                Some(handle().spawn(async move { flow_watchman::init(settings).await }));
        }

        async fn wait_for_init(&self, timeout: Option<f64>) -> Result<(), String> {
            let init_thread = self
                .init_thread
                .lock()
                .unwrap()
                .take()
                .expect("init_thread should be set");
            let init_settings = self
                .init_settings
                .lock()
                .unwrap()
                .clone()
                .expect("init_settings should be set");
            let go = async move {
                match init_thread.await {
                    Ok(Ok((watchman, files))) => Ok((watchman, files)),
                    Ok(Err(msg)) => Err(msg),
                    Err(join_err) => {
                        let msg = format!("Failed to initialize watchman: {}", join_err);
                        flow_event_logger::watchman_uncaught_failure(&msg);
                        Err(msg)
                    }
                }
            };
            let result = match timeout {
                Some(timeout) => {
                    match tokio::time::timeout(Duration::from_secs_f64(timeout), go).await {
                        Ok(r) => r,
                        Err(_) => {
                            Err("Failed to initialize watchman: Watchman timed out".to_string())
                        }
                    }
                }
                None => go.await,
            };
            match result {
                Ok((watchman, files)) => {
                    if self.watchman_options.debug {
                        self.log_watch_spec();
                    }
                    let shared = Arc::new(std::sync::Mutex::new(EnvShared {
                        files,
                        metadata: monitor_prot::empty_file_watcher_metadata(),
                        is_initial: true,
                    }));
                    let new_env = Arc::new(Env {
                        instance: Arc::new(tokio::sync::Mutex::new(Some(watchman))),
                        shared,
                        listening_thread: Arc::new(tokio::sync::Mutex::new(None)),
                        changes_condition: Arc::new(ChangesCondition::new()),
                        init_settings,
                    });
                    let listen_env = new_env.clone();
                    let listening = handle().spawn(async move { listen(listen_env).await });
                    *new_env.listening_thread.lock().await = Some(listening);
                    *self.env.lock().unwrap() = Some(new_env);
                    Ok(())
                }
                Err(msg) => Err(msg),
            }
        }

        async fn get_and_clear_changed_files(
            &self,
        ) -> (BTreeSet<String>, Option<FileWatcherMetadata>, bool) {
            // Should we throw away metadata even if files is empty? glevi thinks that's fine, since we
            // probably don't care about hg updates or mergebase changing if no files were affected
            let env = self.get_env();
            let mut shared = env.shared.lock().unwrap();
            let ret = (
                std::mem::take(&mut shared.files),
                Some(shared.metadata.clone()),
                shared.is_initial,
            );
            shared.metadata = monitor_prot::empty_file_watcher_metadata();
            shared.is_initial = false;
            ret
        }

        async fn wait_for_changed_files(&self) {
            let env = self.get_env();
            env.changes_condition.wait().await;
        }

        async fn stop(&self) {
            // Flow doesn't own the watchman process, so it's not Flow's job to stop the watchman
            // process. What we can do, though, is stop listening to the messages
            let env = self.get_env();
            flow_hh_logger::info!("Canceling Watchman listening thread & closing connection");
            if let Some(handle) = env.listening_thread.lock().await.take() {
                handle.abort();
            }
            if let Some(instance) = env.instance.lock().await.as_ref() {
                flow_watchman::close(instance).await;
            }
        }

        async fn waitpid(&self) -> ExitReason {
            // If watchman dies, we can start it back up again and use clockspec to make sure we didn't
            // miss anything. So from the point of view of the FileWatcher abstraction, watchman never
            // dies and this method can just wait forever.
            //
            // However it's possible that something Really Really Bad might happen to watchman. If
            // the watchman listening thread itself dies, then we need to tell the monitor that this
            // file watcher is dead.
            let env = self.get_env().clone();
            // waitpid should return a thread that resolves when the listening_thread resolves. So why
            // don't we just return the listening_thread?
            //
            // It's because we need to return a cancelable thread. The listening_thread will be fulfilled
            // with [Watcher_stopped] when it is canceled, rather than rejected with [Lwt.Canceled] like
            // normal. That is the wrong behavior.
            //
            // So how do we wrap the listening_thread in a cancelable thread? By running it
            // asynchronously, having it signal when it resolves, and waiting for the signal
            let handle_opt = env.listening_thread.lock().await.take();
            match handle_opt {
                Some(handle) => match handle.await {
                    Ok(reason) => reason,
                    Err(_join_err) => ExitReason::WatcherStopped,
                },
                None => ExitReason::WatcherStopped,
            }
        }

        fn getpid(&self) -> Option<i32> {
            None
        }
    }
}

pub use watchman_file_watcher::Watchman;

pub mod edenfs_file_watcher {
    use super::*;

    pub struct EnvShared {
        pub files: BTreeSet<String>,
        pub metadata: FileWatcherMetadata,
        pub is_initial: bool,
        pub mergebase: Option<String>,
    }

    pub struct Env {
        pub instance: Arc<tokio::sync::Mutex<flow_edenfs_watcher::Instance>>,
        pub notification_fd: std::os::raw::c_int,
        pub shared: Arc<std::sync::Mutex<EnvShared>>,
        pub listening_thread: Arc<tokio::sync::Mutex<Option<tokio::task::JoinHandle<ExitReason>>>>,
        pub changes_condition: Arc<ChangesCondition>,
        pub root: PathBuf,
        pub mergebase_with: String,
    }

    /// Query VCS for the current mergebase and compare with the stored value.
    /// Updates [env.mergebase] if it changed. Returns [Some bool] or [None] on error.
    async fn check_mergebase(env: &Env) -> Option<bool> {
        match query_mergebase(&env.root, &env.mergebase_with).await {
            Err(_) => {
                flow_hh_logger::warn!("EdenFS: failed to query mergebase");
                None
            }
            Ok(new_mergebase) => {
                let mut shared = env.shared.lock().unwrap();
                match shared.mergebase.clone() {
                    None => {
                        shared.mergebase = Some(new_mergebase);
                        None
                    }
                    Some(old_mergebase) => {
                        let changed = new_mergebase != old_mergebase;
                        if changed {
                            flow_hh_logger::info!(
                                "EdenFS: mergebase changed from {:?} to {:?}",
                                old_mergebase,
                                new_mergebase
                            );
                            shared.mergebase = Some(new_mergebase);
                        } else {
                            flow_hh_logger::debug!(
                                "EdenFS: mergebase unchanged at {:?}",
                                new_mergebase
                            );
                        }
                        Some(changed)
                    }
                }
            }
        }
    }

    /// Convert EdenFS watcher changes to a set of file paths and metadata updates.
    /// Note: The paths from EdenFS watcher are already absolute (the Rust code joins
    /// them with root_absolute), so we just add them directly without concatenating
    /// with root again.
    fn convert_changes(
        changes_list: &[flow_edenfs_watcher::Changes],
    ) -> (BTreeSet<String>, FileWatcherMetadata) {
        fn add_files(mut files: BTreeSet<String>, paths: &[String]) -> BTreeSet<String> {
            for path in paths {
                // Paths from EdenFS watcher are already absolute
                files.insert(path.clone());
            }
            files
        }
        let mut files: BTreeSet<String> = BTreeSet::new();
        let mut metadata = monitor_prot::empty_file_watcher_metadata();
        for change in changes_list {
            match change {
                flow_edenfs_watcher::Changes::FileChanges(paths) => {
                    files = add_files(files, paths);
                }
                flow_edenfs_watcher::Changes::CommitTransition {
                    from_commit,
                    to_commit,
                    file_changes,
                } => {
                    flow_hh_logger::info!(
                        "EdenFS watcher reports commit transition from {} to {} with {} changed files",
                        from_commit,
                        to_commit,
                        file_changes.len()
                    );
                    files = add_files(files, file_changes);
                    metadata = flow_monitor_rpc::monitor_prot::merge_file_watcher_metadata(
                        &metadata,
                        &FileWatcherMetadata {
                            changed_mergebase: Some(true),
                            missed_changes: false,
                        },
                    );
                }
                flow_edenfs_watcher::Changes::StateEnter(_)
                | flow_edenfs_watcher::Changes::StateLeave(_) => {}
                flow_edenfs_watcher::Changes::CommitDistanceExceeded { .. } => {
                    // Handled inline in [main] below — needs env.mergebase to compute
                    // changed_mergebase against the Rust-supplied authoritative value
                    // without a second VCS shellout.
                }
            }
        }
        (files, metadata)
    }

    pub mod edenfs_listen_loop {
        use super::*;

        pub fn should_pause() -> bool {
            true
        }

        // [hg.transaction] is in [edenfs_defer_states] for correctness (we defer
        // file notifications during it), but Watchman's [Defer_changes] mode
        // coalesces it into [hg.update] and never surfaces it as a separate state
        // event. Skip the scuba log on EdenFS to keep [FILE_WATCHER_EVENT_*]
        // volume comparable to Watchman.
        fn should_log_state_to_scuba(name: &str) -> bool {
            name != "hg.transaction"
        }

        pub fn log_state_enter(name: &str) {
            if should_log_state_to_scuba(name) {
                flow_event_logger::file_watcher_event_started(name, "");
            }
        }

        pub fn log_state_leave(name: &str) {
            if should_log_state_to_scuba(name) {
                flow_event_logger::file_watcher_event_finished(name, "");
            }
        }

        pub fn broadcast(env: &Env) {
            let shared = env.shared.lock().unwrap();
            if !shared.files.is_empty() || shared.metadata.missed_changes {
                env.changes_condition.notify_changed();
            }
        }

        pub fn handle_state_changes(changes_list: &[flow_edenfs_watcher::Changes]) {
            for change in changes_list {
                match change {
                    flow_edenfs_watcher::Changes::StateEnter(name) => {
                        log_state_enter(name);
                        flow_hh_logger::info!(
                            "EdenFS reports {} just started. Filesystem notifications are paused.",
                            name
                        );
                        status_stream::file_watcher_deferred(name.clone());
                    }
                    flow_edenfs_watcher::Changes::StateLeave(name) => {
                        log_state_leave(name);
                        flow_hh_logger::info!(
                            "EdenFS reports {} ended. Filesystem notifications resumed.",
                            name
                        );
                        status_stream::file_watcher_ready();
                    }
                    flow_edenfs_watcher::Changes::FileChanges(_)
                    | flow_edenfs_watcher::Changes::CommitTransition { .. }
                    | flow_edenfs_watcher::Changes::CommitDistanceExceeded { .. } => {}
                }
            }
        }

        /// Wait for the notification fd to become readable, then get changes.
        /// Uses the persistent Lwt_unix.file_descr created once during init, so
        /// that Lwt's internal state for this fd stays consistent across calls.
        pub async fn get_changes_async_lwt(
            env: &Env,
        ) -> Result<
            (
                Vec<flow_edenfs_watcher::Changes>,
                flow_edenfs_watcher::Clock,
                Option<serde_json::Value>,
            ),
            flow_edenfs_watcher::EdenfsWatcherError,
        > {
            #[cfg(unix)]
            let fd = env.notification_fd;
            #[cfg(unix)]
            let wait_result = {
                // SAFETY: `fd` is the EdenFS notification fd owned by the watcher
                // instance held in `env.instance`, which outlives this call, so the
                // fd stays open and refers to the same file description for the
                // lifetime of the temporary `AsyncFd` below (dropped before
                // returning). A concurrently destroyed instance surfaces as EBADF,
                // which is handled as shutdown below.
                let registration = unsafe {
                    tokio::io::unix::AsyncFd::register_with_interest(
                        fd,
                        tokio::io::Interest::READABLE,
                    )
                };
                match registration.map_err(|e| e.into_parts().1) {
                    Ok(async_fd) => match async_fd.readable().await {
                        Ok(mut guard) => {
                            guard.clear_ready();
                            Ok(())
                        }
                        Err(e) if e.raw_os_error() == Some(libc::EBADF) => {
                            Err(flow_edenfs_watcher::EdenfsWatcherError::EdenfsWatcherError(
                                "Notification fd closed".to_string(),
                            ))
                        }
                        Err(e) => Err(flow_edenfs_watcher::EdenfsWatcherError::EdenfsWatcherError(
                            format!("AsyncFd readable error: {}", e),
                        )),
                    },
                    Err(e) if e.raw_os_error() == Some(libc::EBADF) => {
                        Err(flow_edenfs_watcher::EdenfsWatcherError::EdenfsWatcherError(
                            "Notification fd closed".to_string(),
                        ))
                    }
                    Err(e) => Err(flow_edenfs_watcher::EdenfsWatcherError::EdenfsWatcherError(
                        format!("AsyncFd registration error: {}", e),
                    )),
                }
            };
            #[cfg(windows)]
            let wait_result: Result<(), flow_edenfs_watcher::EdenfsWatcherError> = {
                // On Windows the notification handle is an AF_INET/SOCK_STREAM socket
                // created by `filedescriptor::socketpair` and owned by the EdenFS instance
                // (it is also drained by `get_changes_async`). We must only poll it, never
                // take ownership, so we wait with `filedescriptor::poll` (WSAPoll) on a
                // blocking thread rather than wrapping it in a `tokio::net::TcpStream`,
                // which would close the socket on drop. The finite timeout lets this task
                // be cancelled (e.g. on watcher stop) between iterations.
                let fd = env.notification_fd as filedescriptor::SocketDescriptor;
                loop {
                    let poll_outcome = tokio::task::spawn_blocking(move || {
                        let mut pollfds = [filedescriptor::pollfd {
                            fd,
                            events: filedescriptor::POLLIN,
                            revents: 0,
                        }];
                        filedescriptor::poll(&mut pollfds, Some(Duration::from_millis(250)))
                            .map(|ready| (ready, pollfds[0].revents))
                    })
                    .await;
                    match poll_outcome {
                        // Timed out with nothing ready: loop so an aborted task can unwind.
                        Ok(Ok((0, _))) => continue,
                        Ok(Ok((_, revents))) => {
                            if revents & filedescriptor::POLLIN != 0 {
                                break Ok(());
                            }
                            break Err(
                                flow_edenfs_watcher::EdenfsWatcherError::EdenfsWatcherError(
                                    format!("Notification socket closed (revents={})", revents),
                                ),
                            );
                        }
                        Ok(Err(err)) => {
                            break Err(
                                flow_edenfs_watcher::EdenfsWatcherError::EdenfsWatcherError(
                                    format!("Notification socket poll failed: {:?}", err),
                                ),
                            );
                        }
                        Err(join_err) => {
                            break Err(
                                flow_edenfs_watcher::EdenfsWatcherError::EdenfsWatcherError(
                                    format!("Notification poll task failed: {}", join_err),
                                ),
                            );
                        }
                    }
                }
            };
            #[cfg(not(any(unix, windows)))]
            let wait_result: Result<(), flow_edenfs_watcher::EdenfsWatcherError> =
                Err(flow_edenfs_watcher::EdenfsWatcherError::EdenfsWatcherError(
                    "EdenFS notification fd polling is not supported on this platform".to_string(),
                ));
            match wait_result {
                Ok(()) => {
                    // `get_changes_async` drives the instance's own tokio runtime via
                    // `block_on`. This listen loop runs inside the shared
                    // flow-tokio-runtime on every platform (the rust_port monitor always
                    // has a runtime entered), so the drain must run on a blocking thread
                    // to avoid "Cannot start a runtime from within a runtime". Only the
                    // readiness wait above is platform-specific (AsyncFd vs WSAPoll); the
                    // drain itself is shared.
                    let instance = env.instance.clone();
                    match tokio::task::spawn_blocking(move || {
                        let instance = instance.blocking_lock();
                        flow_edenfs_watcher::get_changes_async(&instance)
                    })
                    .await
                    {
                        Ok(result) => result,
                        Err(join_err) => {
                            Err(flow_edenfs_watcher::EdenfsWatcherError::EdenfsWatcherError(
                                format!("get_changes task failed: {}", join_err),
                            ))
                        }
                    }
                }
                Err(err) => Err(err),
            }
        }

        pub async fn main(env: &Env) -> Result<(), flow_edenfs_watcher::EdenfsWatcherError> {
            match get_changes_async_lwt(env).await {
                Err(flow_edenfs_watcher::EdenfsWatcherError::LostChanges(msg)) => {
                    flow_hh_logger::error!("EdenFS watcher lost changes: {}", msg);
                    flow_event_logger::edenfs_watcher_lost_changes(&msg, &msg);
                    // Query the VCS for the current mergebase so the rechecker can trigger
                    // reinit if it changed during the lost window. Without this, the
                    // rechecker defaults [None] to [false] and skips reinit, leaving Flow
                    // with a stale snapshot of committed code.
                    let changed_mergebase = check_mergebase(env).await;
                    {
                        let mut shared = env.shared.lock().unwrap();
                        shared.metadata =
                            flow_monitor_rpc::monitor_prot::merge_file_watcher_metadata(
                                &shared.metadata,
                                &FileWatcherMetadata {
                                    changed_mergebase,
                                    missed_changes: true,
                                },
                            );
                    }
                    broadcast(env);
                    Ok(())
                }
                Err(_) if flow_edenfs_watcher::is_instance_destroyed() => {
                    // The instance was destroyed by the exit hook during shutdown.
                    // This is expected and not an error — treat it as a clean stop.
                    flow_hh_logger::info!(
                        "EdenFS watcher instance was destroyed (shutdown in progress)"
                    );
                    Err(flow_edenfs_watcher::EdenfsWatcherError::EdenfsWatcherError(
                        "Canceled".to_string(),
                    ))
                }
                Err(err) => {
                    let msg = format!("{:?}", err);
                    flow_event_logger::edenfs_watcher_error(&msg, "");
                    Err(err)
                }
                Ok((changes_list, _clock, _telemetry)) => {
                    handle_state_changes(&changes_list);
                    let (new_files, mut new_metadata) = convert_changes(&changes_list);
                    // If a commit transition occurred, check if mergebase actually changed.
                    // Run this BEFORE applying CommitDistanceExceeded below, so
                    // check_mergebase compares against the still-stale env.mergebase.
                    if let Some(true) = new_metadata.changed_mergebase {
                        flow_hh_logger::debug!(
                            "EdenFS: CommitTransition reported changed_mergebase=true; verifying with VCS"
                        );
                        let actual_changed = check_mergebase(env).await;
                        let changed_mergebase = match actual_changed {
                            Some(changed) => {
                                if !changed {
                                    flow_hh_logger::info!(
                                        "EdenFS: overriding changed_mergebase from true to false (mergebase did not actually change)"
                                    );
                                }
                                Some(changed)
                            }
                            None => new_metadata.changed_mergebase,
                        };
                        new_metadata.changed_mergebase = changed_mergebase;
                    }
                    {
                        let mut shared = env.shared.lock().unwrap();
                        shared.files.extend(new_files);
                        shared.metadata =
                            flow_monitor_rpc::monitor_prot::merge_file_watcher_metadata(
                                &shared.metadata,
                                &new_metadata,
                            );
                    }
                    // Handle CommitDistanceExceeded inline using the authoritative
                    // mergebase Rust shipped. Mirrors the Watchman [Missed_changes]
                    // branch: compute changed_mergebase by comparing against the cached
                    // env.mergebase, update the cache, and union the changes — no second
                    // VCS shellout.
                    for change in &changes_list {
                        if let flow_edenfs_watcher::Changes::CommitDistanceExceeded {
                            from_commit,
                            to_commit,
                            mergebase,
                            changes_since_mergebase,
                        } = change
                        {
                            let mut shared = env.shared.lock().unwrap();
                            let changed_mergebase = match &shared.mergebase {
                                Some(prev) => prev != mergebase,
                                None => true,
                            };
                            flow_hh_logger::info!(
                                "EdenFS watcher reports commit distance exceeded from {} to {} (mergebase={}{}, {} changes since mergebase)",
                                from_commit,
                                to_commit,
                                mergebase,
                                if changed_mergebase {
                                    ", changed"
                                } else {
                                    ", unchanged"
                                },
                                changes_since_mergebase.len()
                            );
                            // Don't cache an empty mergebase — that would
                            // falsely flip changed_mergebase on the next
                            // event. Reinit still fires on this event via the
                            // metadata below, which is what we want when
                            // sapling couldn't resolve the mergebase.
                            if !mergebase.is_empty() {
                                shared.mergebase = Some(mergebase.clone());
                            }
                            for p in changes_since_mergebase {
                                shared.files.insert(p.clone());
                            }
                            shared.metadata =
                                flow_monitor_rpc::monitor_prot::merge_file_watcher_metadata(
                                    &shared.metadata,
                                    &FileWatcherMetadata {
                                        changed_mergebase: Some(changed_mergebase),
                                        missed_changes: true,
                                    },
                                );
                        }
                    }
                    broadcast(env);
                    Ok(())
                }
            }
        }

        pub fn catch(
            err: &flow_edenfs_watcher::EdenfsWatcherError,
        ) -> flow_edenfs_watcher::EdenfsWatcherError {
            flow_hh_logger::error!("EdenFS watcher unavailable. Exiting...");
            err.clone()
        }
    }

    pub async fn listen(env: Arc<Env>) -> ExitReason {
        loop {
            match edenfs_listen_loop::main(&env).await {
                Ok(()) => {}
                Err(err) => {
                    let err = edenfs_listen_loop::catch(&err);
                    return match err {
                        flow_edenfs_watcher::EdenfsWatcherError::LostChanges(_) => {
                            ExitReason::WatcherMissedChanges
                        }
                        flow_edenfs_watcher::EdenfsWatcherError::EdenfsWatcherError(_)
                        | flow_edenfs_watcher::EdenfsWatcherError::NonEdenMount => {
                            ExitReason::WatcherDied
                        }
                    };
                }
            }
        }
    }

    pub struct EdenFS {
        // Wrapped behind interior mutability so methods can take `&self`,
        // matching OCaml's class-based shape with implicit field mutation.
        env: std::sync::Mutex<Option<Arc<Env>>>,
        init_thread: std::sync::Mutex<
            Option<
                tokio::task::JoinHandle<
                    Result<
                        (flow_edenfs_watcher::Instance, flow_edenfs_watcher::Clock),
                        flow_edenfs_watcher::EdenfsWatcherError,
                    >,
                >,
            >,
        >,
        mergebase_with: String,
        server_options: flow_common::options::Options,
        edenfs_options: flow_server_monitor_options::EdenfsOptions,
        root_path: PathBuf,
        watch_paths: Vec<PathBuf>,
    }

    impl EdenFS {
        pub fn new(
            mergebase_with: String,
            server_options: flow_common::options::Options,
            edenfs_options: flow_server_monitor_options::EdenfsOptions,
        ) -> Self {
            let _file_options = &server_options.file_options;
            let root_path = (*server_options.root).clone();
            let watch_paths = flow_common::files::watched_paths(&server_options.file_options);
            EdenFS {
                env: std::sync::Mutex::new(None),
                init_thread: std::sync::Mutex::new(None),
                mergebase_with,
                server_options,
                edenfs_options,
                root_path,
                watch_paths,
            }
        }

        fn get_env(&self) -> Arc<Env> {
            match self.env.lock().unwrap().as_ref() {
                None => panic!("EdenFS watcher was not initialized"),
                Some(env) => env.clone(),
            }
        }

        pub fn get_env_for_waitpid(&self) -> Arc<Env> {
            self.get_env()
        }

        fn log_watch_spec(&self) {
            // Log absolute paths for consistency with Watchman's log format.
            // Note: The actual watch_spec passed to Rust uses relative paths.
            let file_options = &self.server_options.file_options;
            let extensions =
                flow_server_file_watcher_spec::file_watcher_spec::get_suffixes(file_options);
            let file_names = flow_server_file_watcher_spec::file_watcher_spec::get_file_names(
                &self.server_options,
            );
            let include_dirs =
                flow_server_file_watcher_spec::file_watcher_spec::get_include_dirs_absolute(
                    &self.server_options,
                );
            flow_hh_logger::info!(
                "EdenFS connection established with watch_spec: extensions=[{}], file_names=[{}], include_dirs=[{}], exclude_dirs=[{}]",
                extensions.join(", "),
                file_names.join(", "),
                include_dirs.join(", "),
                flow_server_file_watcher_spec::file_watcher_spec::EXCLUDE_DIRS.join(", ")
            );
        }
    }

    impl Watcher for EdenFS {
        fn name(&self) -> &str {
            "edenfs"
        }

        fn debug(&self) -> bool {
            self.edenfs_options.edenfs_debug
        }

        fn start_init(&self) {
            let flow_server_monitor_options::EdenfsOptions {
                edenfs_debug,
                edenfs_timeout_secs,
                edenfs_throttle_time_ms,
                edenfs_defer_states,
                edenfs_max_commit_distance,
                edenfs_force_subprocess_mergebase,
                edenfs_watchman_fallback: _,
            } = &self.edenfs_options;
            let settings = flow_edenfs_watcher::Settings {
                root: self.root_path.clone(),
                watch_spec: flow_edenfs_watcher::watch_spec(&self.server_options),
                debug_logging: *edenfs_debug,
                timeout_secs: *edenfs_timeout_secs as isize,
                throttle_time_ms: *edenfs_throttle_time_ms as isize,
                report_telemetry: true,
                state_tracking: true,
                defer_states: edenfs_defer_states.clone(),
                max_commit_distance: *edenfs_max_commit_distance as isize,
                mergebase_with: self.mergebase_with.clone(),
                force_subprocess_mergebase: *edenfs_force_subprocess_mergebase,
            };
            // `flow_edenfs_watcher::init` is a synchronous, blocking call: it builds its
            // own multi-threaded tokio runtime and drives it with `block_on`. It must run
            // on a blocking pool thread, not an async worker thread of `flow-tokio-runtime`
            // — `block_on` panics with "Cannot start a runtime from within a runtime" when
            // invoked from a thread already driving a runtime. (`spawn_blocking` returns a
            // `JoinHandle` with the same output type as `spawn`, so `wait_for_init` is
            // unchanged.)
            *self.init_thread.lock().unwrap() =
                Some(handle().spawn_blocking(move || flow_edenfs_watcher::init(settings)));
        }

        async fn wait_for_init(&self, timeout: Option<f64>) -> Result<(), String> {
            let init_thread = self
                .init_thread
                .lock()
                .unwrap()
                .take()
                .expect("init_thread should be set");
            let go = async move {
                match init_thread.await {
                    Ok(Ok((instance, _clock))) => Ok(instance),
                    Ok(Err(err)) => Err(format!("{:?}", err)),
                    Err(join_err) => {
                        let msg = format!("Failed to initialize EdenFS watcher: {}", join_err);
                        flow_event_logger::file_watcher_uncaught_failure(&msg);
                        Err(msg)
                    }
                }
            };
            let result = match timeout {
                Some(timeout) => {
                    match tokio::time::timeout(Duration::from_secs_f64(timeout), go).await {
                        Ok(r) => r,
                        Err(_) => Err("Failed to initialize EdenFS watcher: timed out".to_string()),
                    }
                }
                None => go.await,
            };
            match result {
                Ok(instance) => {
                    if self.edenfs_options.edenfs_debug {
                        self.log_watch_spec();
                    }
                    // Create the Lwt fd wrapper once and reuse it for the lifetime of the
                    // watcher. Recreating it on every poll call corrupts Lwt's internal
                    // state for the fd, causing wait_read to stop waking up.
                    let notification_fd = match flow_edenfs_watcher::get_notification_fd(&instance)
                    {
                        Ok(fd) => fd,
                        Err(err) => {
                            let msg = format!("{:?}", err);
                            return Err(format!("Failed to get EdenFS notification fd: {}", msg));
                        }
                    };
                    // Query VCS for the initial mergebase
                    let initial_mergebase =
                        match query_mergebase(&self.root_path, &self.mergebase_with).await {
                            Ok(hash) => {
                                flow_hh_logger::info!("EdenFS: initial mergebase is {:?}", hash);
                                Some(hash)
                            }
                            Err(_) => {
                                flow_hh_logger::warn!(
                                    "EdenFS: failed to determine initial mergebase"
                                );
                                None
                            }
                        };
                    let shared = Arc::new(std::sync::Mutex::new(EnvShared {
                        files: BTreeSet::new(),
                        metadata: monitor_prot::empty_file_watcher_metadata(),
                        is_initial: true,
                        mergebase: initial_mergebase,
                    }));
                    let new_env = Arc::new(Env {
                        instance: Arc::new(tokio::sync::Mutex::new(instance)),
                        notification_fd,
                        shared,
                        listening_thread: Arc::new(tokio::sync::Mutex::new(None)),
                        changes_condition: Arc::new(ChangesCondition::new()),
                        root: self.root_path.clone(),
                        mergebase_with: self.mergebase_with.clone(),
                    });
                    let listen_env = new_env.clone();
                    let listening = handle().spawn(async move { listen(listen_env).await });
                    *new_env.listening_thread.lock().await = Some(listening);
                    *self.env.lock().unwrap() = Some(new_env);
                    // For lazy mode, get initial files changed since mergebase.
                    // Unlike Watchman which returns initial files during init, we use the
                    // VCS-based approach like dfind to get files changed since mergebase.
                    if self.server_options.lazy_mode {
                        let changes =
                            changes_since_mergebase(&self.mergebase_with, &self.watch_paths).await;
                        let env = self.get_env();
                        let mut shared = env.shared.lock().unwrap();
                        shared.files.extend(changes);
                    }
                    Ok(())
                }
                Err(msg) => Err(msg),
            }
        }

        async fn get_and_clear_changed_files(
            &self,
        ) -> (BTreeSet<String>, Option<FileWatcherMetadata>, bool) {
            let env = self.get_env();
            let mut shared = env.shared.lock().unwrap();
            let ret = (
                std::mem::take(&mut shared.files),
                Some(shared.metadata.clone()),
                shared.is_initial,
            );
            shared.metadata = monitor_prot::empty_file_watcher_metadata();
            shared.is_initial = false;
            ret
        }

        async fn wait_for_changed_files(&self) {
            let env = self.get_env();
            env.changes_condition.wait().await;
        }

        async fn stop(&self) {
            let env = self.get_env();
            flow_hh_logger::info!("Canceling EdenFS listening thread");
            if let Some(handle) = env.listening_thread.lock().await.take() {
                handle.abort();
            }
            // The exit hook will call destroy_instance_ffi on the Rust instance,
            // which stops the worker thread and shuts down the Tokio runtime
            // before Folly's atexit handlers run.
        }

        async fn waitpid(&self) -> ExitReason {
            let env = self.get_env();
            let handle_opt = env.listening_thread.lock().await.take();
            match handle_opt {
                Some(handle) => match handle.await {
                    Ok(reason) => reason,
                    Err(_join_err) => ExitReason::WatcherStopped,
                },
                None => ExitReason::WatcherStopped,
            }
        }

        fn getpid(&self) -> Option<i32> {
            None
        }
    }
}

pub use edenfs_file_watcher::EdenFS;

pub mod dfind_file_watcher {
    use super::*;

    // dfind exposes a poll-based protocol: each `get_changes` call returns the
    // changes accumulated since the previous call (or empty), and never blocks
    // waiting for new ones. So unlike Watchman / EdenFS, the listen loop polls
    // on a fixed interval rather than blocking on a push.
    const POLL_INTERVAL: Duration = Duration::from_secs(1);

    pub struct EnvShared {
        pub files: BTreeSet<String>,
        pub is_initial: bool,
    }

    pub struct Env {
        pub instance: Arc<flow_dfind::Dfind>,
        pub shared: Arc<std::sync::Mutex<EnvShared>>,
        pub listening_thread: Arc<tokio::sync::Mutex<Option<tokio::task::JoinHandle<ExitReason>>>>,
        pub changes_condition: Arc<ChangesCondition>,
    }

    /// Poll the dfind daemon, draining its pending changes into `shared.files`
    /// and signalling waiters. This mirrors the Watchman / EdenFS listen loops:
    /// it is spawned once during init and the command loop never aborts it, so
    /// the destructive drain always lands in `shared.files` before any waiter is
    /// woken. (Driving the drain from inside the abortable `wait_for_changed_files`
    /// instead — as a naive poll would — drops the drained batch whenever the
    /// command loop cancels the wait, silently losing the change.)
    pub async fn listen(env: Arc<Env>) -> ExitReason {
        loop {
            match flow_dfind::get_changes(&env.instance).await {
                Ok(new_files) => {
                    if !new_files.is_empty() {
                        {
                            let mut shared = env.shared.lock().unwrap();
                            shared.files.extend(new_files);
                        }
                        env.changes_condition.notify_changed();
                    }
                }
                // dfind only reports Stopped once its handle is gone, i.e. after
                // `stop` took the instance. Treat it as the daemon going away.
                Err(flow_dfind::Error::Stopped) => {
                    flow_hh_logger::debug!("Connection to dfind broke");
                    return ExitReason::WatcherDied;
                }
                Err(err) => {
                    flow_hh_logger::debug!("dfind get_changes failed: {}", err);
                }
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    pub struct DFind {
        // Built in `wait_for_init` once the daemon is ready; holds the listen
        // loop and the shared buffer it feeds.
        env: std::sync::Mutex<Option<Arc<Env>>>,
        // Set by `start_init`, consumed by `wait_for_init` when building `Env`.
        instance: std::sync::Mutex<Option<Arc<flow_dfind::Dfind>>>,
        watch_paths: std::sync::Mutex<Vec<PathBuf>>,
        mergebase_with: String,
        server_options: flow_common::options::Options,
    }

    impl DFind {
        pub fn new(mergebase_with: String, server_options: flow_common::options::Options) -> Self {
            DFind {
                env: std::sync::Mutex::new(None),
                instance: std::sync::Mutex::new(None),
                watch_paths: std::sync::Mutex::new(Vec::new()),
                mergebase_with,
                server_options,
            }
        }

        fn get_env(&self) -> Arc<Env> {
            match self.env.lock().unwrap().as_ref() {
                None => panic!("Dfind was not initialized"),
                Some(env) => env.clone(),
            }
        }
    }

    impl Watcher for DFind {
        fn name(&self) -> &str {
            "dfind"
        }

        fn debug(&self) -> bool {
            false
        }

        fn start_init(&self) {
            let file_options = &self.server_options.file_options;
            let watch_paths = flow_common::files::watched_paths(file_options);
            let log_file = {
                let flowconfig_name = &self.server_options.flowconfig_name;
                let tmp_dir = &self.server_options.temp_dir;
                let root = &self.server_options.root;
                flow_server_files::server_files_js::dfind_log_file(flowconfig_name, tmp_dir, root)
            };
            let init_args = flow_dfind::InitArgs {
                scuba_table: "flow_server_events".to_string(),
                roots: watch_paths.clone(),
            };
            let fds = flow_dfind::DaemonFds {
                log_file: PathBuf::from(log_file),
            };
            *self.watch_paths.lock().unwrap() = watch_paths;
            match flow_dfind::init(fds, init_args) {
                Ok(d) => {
                    *self.instance.lock().unwrap() = Some(Arc::new(d));
                }
                Err(err) => {
                    flow_hh_logger::error!("Failed to initialize dfind: {}", err);
                }
            }
        }

        async fn wait_for_init(&self, _timeout: Option<f64>) -> Result<(), String> {
            let instance = match self.instance.lock().unwrap().clone() {
                None => return Err("Failed to initialize dfind".to_string()),
                Some(d) => d,
            };
            flow_dfind::wait_until_ready(&instance).await;
            let mut initial_files = BTreeSet::new();
            if self.server_options.lazy_mode {
                let watch_paths = self.watch_paths.lock().unwrap().clone();
                let changes = changes_since_mergebase(&self.mergebase_with, &watch_paths).await;
                initial_files.extend(changes);
            }
            let shared = Arc::new(std::sync::Mutex::new(EnvShared {
                files: initial_files,
                is_initial: true,
            }));
            let new_env = Arc::new(Env {
                instance,
                shared,
                listening_thread: Arc::new(tokio::sync::Mutex::new(None)),
                changes_condition: Arc::new(ChangesCondition::new()),
            });
            let listen_env = new_env.clone();
            let listening = handle().spawn(async move { listen(listen_env).await });
            *new_env.listening_thread.lock().await = Some(listening);
            *self.env.lock().unwrap() = Some(new_env);
            Ok(())
        }

        async fn get_and_clear_changed_files(
            &self,
        ) -> (BTreeSet<String>, Option<FileWatcherMetadata>, bool) {
            let env = self.get_env();
            let mut shared = env.shared.lock().unwrap();
            let ret = (std::mem::take(&mut shared.files), None, shared.is_initial);
            shared.is_initial = false;
            ret
        }

        async fn wait_for_changed_files(&self) {
            let env = self.get_env();
            env.changes_condition.wait().await;
        }

        async fn stop(&self) {
            let env = self.get_env();
            flow_hh_logger::info!("Canceling dfind listening thread");
            if let Some(handle) = env.listening_thread.lock().await.take() {
                handle.abort();
            }
            flow_dfind::stop(&env.instance);
        }

        async fn waitpid(&self) -> ExitReason {
            std::future::pending().await
        }

        fn getpid(&self) -> Option<i32> {
            None
        }
    }
}

pub use dfind_file_watcher::DFind;

pub enum AnyWatcher {
    Dummy(Dummy),
    DFind(DFind),
    Watchman(Watchman),
    EdenFS(EdenFS),
}

impl AnyWatcher {
    pub fn name(&self) -> &str {
        match self {
            AnyWatcher::Dummy(w) => w.name(),
            AnyWatcher::DFind(w) => w.name(),
            AnyWatcher::Watchman(w) => w.name(),
            AnyWatcher::EdenFS(w) => w.name(),
        }
    }

    pub fn debug(&self) -> bool {
        match self {
            AnyWatcher::Dummy(w) => w.debug(),
            AnyWatcher::DFind(w) => w.debug(),
            AnyWatcher::Watchman(w) => w.debug(),
            AnyWatcher::EdenFS(w) => w.debug(),
        }
    }

    pub fn start_init(&self) {
        match self {
            AnyWatcher::Dummy(w) => w.start_init(),
            AnyWatcher::DFind(w) => w.start_init(),
            AnyWatcher::Watchman(w) => w.start_init(),
            AnyWatcher::EdenFS(w) => w.start_init(),
        }
    }

    pub async fn wait_for_init(&self, timeout: Option<f64>) -> Result<(), String> {
        match self {
            AnyWatcher::Dummy(w) => w.wait_for_init(timeout).await,
            AnyWatcher::DFind(w) => w.wait_for_init(timeout).await,
            AnyWatcher::Watchman(w) => w.wait_for_init(timeout).await,
            AnyWatcher::EdenFS(w) => w.wait_for_init(timeout).await,
        }
    }

    pub async fn get_and_clear_changed_files(
        &self,
    ) -> (BTreeSet<String>, Option<FileWatcherMetadata>, bool) {
        match self {
            AnyWatcher::Dummy(w) => w.get_and_clear_changed_files().await,
            AnyWatcher::DFind(w) => w.get_and_clear_changed_files().await,
            AnyWatcher::Watchman(w) => w.get_and_clear_changed_files().await,
            AnyWatcher::EdenFS(w) => w.get_and_clear_changed_files().await,
        }
    }

    pub async fn wait_for_changed_files(&self) {
        match self {
            AnyWatcher::Dummy(w) => w.wait_for_changed_files().await,
            AnyWatcher::DFind(w) => w.wait_for_changed_files().await,
            AnyWatcher::Watchman(w) => w.wait_for_changed_files().await,
            AnyWatcher::EdenFS(w) => w.wait_for_changed_files().await,
        }
    }

    pub async fn stop(&self) {
        match self {
            AnyWatcher::Dummy(w) => w.stop().await,
            AnyWatcher::DFind(w) => w.stop().await,
            AnyWatcher::Watchman(w) => w.stop().await,
            AnyWatcher::EdenFS(w) => w.stop().await,
        }
    }

    pub async fn waitpid(&self) -> ExitReason {
        match self {
            AnyWatcher::Dummy(w) => w.waitpid().await,
            AnyWatcher::DFind(w) => w.waitpid().await,
            AnyWatcher::Watchman(w) => w.waitpid().await,
            AnyWatcher::EdenFS(w) => w.waitpid().await,
        }
    }

    pub fn waitpid_owned(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ExitReason> + Send + 'static>> {
        match self {
            AnyWatcher::Dummy(_) => Box::pin(std::future::pending()),
            AnyWatcher::DFind(_) => Box::pin(std::future::pending()),
            AnyWatcher::Watchman(w) => {
                let env = w.get_env_for_waitpid();
                Box::pin(async move {
                    let handle_opt = env.listening_thread.lock().await.take();
                    match handle_opt {
                        Some(handle) => match handle.await {
                            Ok(reason) => reason,
                            Err(_join_err) => ExitReason::WatcherStopped,
                        },
                        None => ExitReason::WatcherStopped,
                    }
                })
            }
            AnyWatcher::EdenFS(w) => {
                let env = w.get_env_for_waitpid();
                Box::pin(async move {
                    let handle_opt = env.listening_thread.lock().await.take();
                    match handle_opt {
                        Some(handle) => match handle.await {
                            Ok(reason) => reason,
                            Err(_join_err) => ExitReason::WatcherStopped,
                        },
                        None => ExitReason::WatcherStopped,
                    }
                })
            }
        }
    }

    pub fn getpid(&self) -> Option<i32> {
        match self {
            AnyWatcher::Dummy(w) => w.getpid(),
            AnyWatcher::DFind(w) => w.getpid(),
            AnyWatcher::Watchman(w) => w.getpid(),
            AnyWatcher::EdenFS(w) => w.getpid(),
        }
    }
}
