/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::future::Future;
use std::num::NonZeroUsize;
use std::sync::LazyLock;
use std::sync::OnceLock;

use tokio::runtime::Handle;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn build_runtime(worker_threads: Option<NonZeroUsize>) -> Runtime {
    #[cfg(not(target_os = "emscripten"))]
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    #[cfg(target_os = "emscripten")]
    let mut builder = tokio::runtime::Builder::new_current_thread();
    builder.enable_all().thread_name("flow-tokio-runtime");
    if let Some(worker_threads) = worker_threads {
        builder.worker_threads(worker_threads.get());
    }
    // Blocking-pool threads run parallelizable LSP workloads and deep `Env` drops, both of
    // which recurse as deeply as the type checker. Tokio's 2MiB default overflows on them.
    #[cfg(not(target_arch = "wasm32"))]
    builder.thread_stack_size(flow_utils_concurrency::thread_pool::DEFAULT_STACK_SIZE);
    builder
        .build()
        .expect("failed to create tokio runtime for Flow")
}

fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        let worker_threads = std::env::var("FLOW_MAX_WORKERS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .and_then(NonZeroUsize::new);
        build_runtime(worker_threads)
    })
}

/// Initializes Flow's shared Tokio runtime with an explicit worker count.
///
/// This must be called before the runtime is first used. Repeated calls are harmless; the first
/// initialization determines the worker count for the process.
pub fn init_worker_threads(worker_threads: NonZeroUsize) {
    RUNTIME.get_or_init(|| build_runtime(Some(worker_threads)));
}

static BLOCKING_POOL_PREWARMED: LazyLock<()> = LazyLock::new(|| {
    let runtime_handle = handle();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel::<()>();
    let task = runtime_handle.spawn_blocking(move || {
        started_tx
            .send(())
            .expect("flow tokio runtime prewarm signal should be received");
    });
    runtime_handle
        .block_on(started_rx)
        .expect("flow tokio runtime should prewarm");
    match runtime_handle.block_on(task) {
        Ok(()) => {}
        Err(err) if err.is_panic() => std::panic::resume_unwind(err.into_panic()),
        Err(err) => panic!("flow tokio runtime prewarm task failed: {}", err),
    }
});

/// Returns Flow's shared Tokio runtime handle.
pub fn handle() -> Handle {
    runtime().handle().clone()
}

pub fn prewarm_blocking_pool() {
    LazyLock::force(&BLOCKING_POOL_PREWARMED);
}

pub fn block_on<F: Future>(future: F) -> F::Output {
    handle().block_on(future)
}

/// Spawns on the current Tokio runtime when one is active, otherwise on Flow's shared runtime.
pub fn spawn(future: impl Future<Output = ()> + Send + 'static) {
    let runtime_handle = match Handle::try_current() {
        Ok(handle) => handle,
        Err(_) => handle(),
    };
    runtime_handle.spawn(future);
}

pub fn spawn_blocking<F, R>(f: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    handle().spawn_blocking(f)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::sync::mpsc;
    use std::time::Duration;

    use super::handle;
    use super::init_worker_threads;
    use super::spawn;

    #[test]
    fn spawn_runs_without_current_tokio_runtime() {
        init_worker_threads(NonZeroUsize::new(2).expect("2 is non-zero"));
        assert_eq!(handle().metrics().num_workers(), 2);

        let (tx, rx) = mpsc::channel();
        spawn(async move {
            tx.send(())
                .expect("test receiver should still be waiting for the task");
        });
        rx.recv_timeout(Duration::from_secs(5))
            .expect("shared runtime should drive spawned task");
    }
}
