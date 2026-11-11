/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use dupe::Dupe;
use flow_common::files::LibDir;
use flow_common::options::Options;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_heap::heap_state::CommittedHeap;
use flow_heap::parsing_heaps::ActiveTransaction;
use flow_parser::ast;
use flow_parser::file_key::FileKey;
use flow_parser::loc::Loc;
use flow_server_env::dependency_info::DependencyInfo;
use flow_server_env::server_env::Env as ServerEnv;
use flow_server_env::server_env::EnvCellReadGuard;
use flow_server_env::server_env::EnvTransaction;
use flow_server_env::server_env::Genv;
use flow_services_inference::merge_service::Checker;
use flow_typing_context::Metadata;
use flow_utils_concurrency::map_reduce::Bucket;
use flow_utils_concurrency::thread_pool::ThreadPool;

use super::codemod_context;
use super::codemod_printer;
use super::codemod_report;

fn files_default_lib_dir(options: &Options) -> Option<LibDir> {
    options.file_options.default_lib_dir.clone()
}

fn include_dependencies_and_dependents(
    options: &Options,
    input: flow_common_utils::checked_set::CheckedSet,
    unchanged_checked: flow_common_utils::checked_set::CheckedSet,
    all_dependent_files: flow_data_structure_wrapper::ord_set::FlowOrdSet<FileKey>,
    implementation_dependency_graph: &flow_common_utils::graph::Graph<FileKey>,
    sig_dependency_graph: &flow_common_utils::graph::Graph<FileKey>,
) -> (
    flow_common_utils::checked_set::CheckedSet,
    flow_common_utils::checked_set::CheckedSet,
    Vec<vec1::Vec1<FileKey>>,
    flow_data_structure_wrapper::ord_set::FlowOrdSet<FileKey>,
) {
    flow_services_inference::type_service::include_dependencies_and_dependents(
        options,
        input,
        unchanged_checked,
        all_dependent_files,
        implementation_dependency_graph,
        sig_dependency_graph,
    )
}

fn checked_set_all(
    to_merge: flow_common_utils::checked_set::CheckedSet,
) -> flow_data_structure_wrapper::ord_set::FlowOrdSet<FileKey> {
    to_merge.all()
}

fn checked_set_of_focused_list(files: Vec<FileKey>) -> flow_common_utils::checked_set::CheckedSet {
    flow_common_utils::checked_set::CheckedSet::of_focused_list(files)
}

fn types_js_calc_deps(
    options: &Options,
    components: Vec<vec1::Vec1<FileKey>>,
    roots: &flow_data_structure_wrapper::ord_set::FlowOrdSet<FileKey>,
) -> Vec<vec1::Vec1<FileKey>> {
    flow_services_inference::type_service::calc_deps(options, components, roots)
}

#[allow(dead_code)]
fn options_root(options: &Options) -> &std::path::Path {
    &options.root
}

fn diff_heaps_remove_batch(roots: &BTreeSet<FileKey>) {
    super::diff_heaps::remove_batch(roots);
}

fn profiling_start(label: &str, _should_print_summary: bool) {
    tracing::info!("Starting profiling: {}", label);
}

fn with_memory_timer<T>(options: &Options, timer: &str, f: impl FnOnce() -> T) -> T {
    flow_profiling::memory_utils::with_memory_timer(options.profile && !options.quiet, timer, f)
}

async fn with_timer_async<T>(
    options: &Options,
    timer: &str,
    future: impl std::future::Future<Output = T>,
) -> T {
    match flow_profiling::profiling_js::current() {
        Some(profiling) => {
            profiling
                .with_timer_async(options.profile && !options.quiet, timer, future)
                .await
        }
        None => future.await,
    }
}

#[allow(dead_code)]
fn state_reader_create() {}

fn next_of_filename_set(
    workers: &Option<ThreadPool>,
    filename_set: &BTreeSet<FileKey>,
) -> flow_parsing::parsing_service::Next {
    let files: Vec<FileKey> = filename_set.iter().cloned().collect();
    let num_workers = workers.as_ref().map_or(1, |w| w.num_workers());
    let batch_size = std::cmp::max(1, files.len() / std::cmp::max(1, num_workers));
    let state = Mutex::new((files, 0usize));
    Box::new(move || {
        let mut guard = state.lock().unwrap();
        let (ref files, ref mut index) = *guard;
        if *index >= files.len() {
            return None;
        }
        let end = std::cmp::min(*index + batch_size, files.len());
        let batch = files[*index..end].to_vec();
        *index = end;
        Some(batch)
    })
}

fn log_input_files(fileset: &BTreeSet<FileKey>) {
    let files_str: Vec<String> = fileset.iter().map(|f| f.to_absolute()).collect();
    flow_hh_logger::debug!(
        "Running codemod on {} files:\n{}",
        fileset.len(),
        files_str.join("\n")
    );
}

pub fn get_target_filename_set(
    options: &std::sync::Arc<flow_common::files::FileOptions>,
    all: bool,
    filename_set: BTreeSet<FileKey>,
) -> BTreeSet<FileKey> {
    filename_set
        .into_iter()
        .filter(|f| {
            let s = f.to_absolute();
            let is_valid_path = flow_common::files::is_valid_path(options, &s);
            let (is_ignored, _) = flow_common::files::is_ignored(options, &s);
            let is_lib = flow_common::files::is_configured_lib_file(options, &s);
            is_valid_path && (all || !is_ignored) && !is_lib
        })
        .collect()
}

pub fn extract_flowlibs_or_exit(options: &Options) {
    let default_lib_dir = files_default_lib_dir(options);
    match default_lib_dir {
        Some(libdir) => {
            let flowlib_libdir = match libdir {
                LibDir::Prelude(path) => flow_flowlib::LibDir::Prelude(path),
                LibDir::Flowlib(path) => flow_flowlib::LibDir::Flowlib(path),
                LibDir::FlowlibWithLibDomDts(path) => {
                    flow_flowlib::LibDir::FlowlibWithLibDomDts(path)
                }
                LibDir::Tslib(path) => flow_flowlib::LibDir::Tslib(path),
            };
            let extract_result =
                std::panic::catch_unwind(|| flow_flowlib::extract(&flowlib_libdir));
            match extract_result {
                Ok(()) => {}
                Err(_err) => {
                    let libdir_str = flow_flowlib::path_of_libdir(&flowlib_libdir)
                        .display()
                        .to_string();
                    let msg = format!(
                        "Could not extract flowlib files into {}: extract failed",
                        libdir_str
                    );
                    eprintln!("{}", msg);
                    flow_common_exit_status::exit(
                        flow_common_exit_status::FlowExitStatus::CouldNotExtractFlowlibs,
                    )
                }
            }
        }
        None => {}
    }
}

pub type UnitResult<A> = Result<
    A,
    (
        flow_aloc::ALoc,
        flow_typing_errors::error_message::InternalError,
    ),
>;

pub type ResultList<A> = Vec<(FileKey, UnitResult<Option<A>>)>;

pub type AbstractVisitor<A, Ctx> =
    Box<dyn Fn(&Options, &ast::Program<Loc, Loc>, Ctx) -> A + Send + Sync>;

pub trait SimpleTypedRunnerConfig {
    type Accumulator: Send + Sync + 'static;

    fn reporter() -> codemod_report::CodemodReport<Self::Accumulator>;
    fn expand_roots(
        _env: &ServerEnv,
        _options: &Options,
        roots: BTreeSet<FileKey>,
    ) -> BTreeSet<FileKey>;
    fn check_options(options: Options) -> Options;
    fn visit(
        options: &Options,
        ast: &ast::Program<Loc, Loc>,
        cctx: &mut codemod_context::typed::TypedCodemodContext<'_>,
    ) -> Self::Accumulator;
}

pub trait UntypedRunnerConfig {
    type Accumulator;

    fn reporter() -> codemod_report::CodemodReport<Self::Accumulator>;
    fn visit(
        options: &Options,
        ast: &ast::Program<Loc, Loc>,
        cctx: codemod_context::untyped::UntypedCodemodContext,
    ) -> Self::Accumulator;
}

// Runner init function which is called after Types_js.init but before any of the jobs.
// This is useful to set up or populate shared memory for the jobs.
pub trait UntypedFlowInitRunnerConfig {
    type Accumulator;

    fn init(reader: &Arc<flow_heap::parsing_heaps::Transaction>);
    fn reporter() -> codemod_report::CodemodReport<Self::Accumulator>;
    fn visit(
        options: &Options,
        ast: &ast::Program<Loc, Loc>,
        cctx: codemod_context::untyped_flow_init::UntypedFlowInitCodemodContext,
    ) -> Self::Accumulator;
}

pub trait StepRunner {
    type Env;
    type Accumulator;

    fn reporter() -> codemod_report::CodemodReport<Self::Accumulator>;
    fn init_run(
        genv: &Genv,
        heap: &Arc<CommittedHeap>,
        roots: BTreeSet<FileKey>,
    ) -> impl std::future::Future<
        Output = Result<
            ((), (Self::Env, ResultList<Self::Accumulator>)),
            flow_utils_concurrency::worker_cancel::WorkerCanceled,
        >,
    >;
    fn recheck_run(
        genv: &Genv,
        heap: &Arc<CommittedHeap>,
        env: Self::Env,
        iteration: i32,
        roots: BTreeSet<FileKey>,
    ) -> impl std::future::Future<
        Output = Result<
            ((), (Self::Env, ResultList<Self::Accumulator>)),
            flow_utils_concurrency::worker_cancel::WorkerCanceled,
        >,
    >;
    fn digest(results: ResultList<Self::Accumulator>) -> (Vec<FileKey>, Self::Accumulator);
}

pub trait Runnable {
    /// Codemods drive their own loop with no orchestrator, so the heap is owned by the caller and
    /// lent for the run.
    fn run(
        genv: &Genv,
        heap: &Arc<CommittedHeap>,
        write: bool,
        repeat: bool,
        roots: BTreeSet<FileKey>,
    ) -> impl std::future::Future<Output = ()>;
}

async fn merge_targets<'a>(
    env: &'a ServerEnv,
    options: &Options,
    _profiling: &(),
    _get_dependent_files: impl Fn(
        &flow_common_utils::graph::Graph<FileKey>,
        &flow_common_utils::graph::Graph<FileKey>,
        &BTreeSet<FileKey>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = BTreeSet<FileKey>>>,
    >,
    roots: &BTreeSet<FileKey>,
) -> (
    EnvCellReadGuard<DependencyInfo>,
    Vec<vec1::Vec1<FileKey>>,
    flow_data_structure_wrapper::ord_set::FlowOrdSet<FileKey>,
    flow_common_utils::checked_set::CheckedSet,
) {
    let dependency_info = env.dependency_info();
    let sig_dependency_graph = dependency_info.sig_dependency_graph();
    let implementation_dependency_graph = dependency_info.implementation_dependency_graph();
    flow_hh_logger::info!("Calculating dependencies");
    let all_dependent_files: BTreeSet<FileKey> =
        _get_dependent_files(sig_dependency_graph, implementation_dependency_graph, roots).await;
    let (to_merge, to_check, _components, _recheck_set) = include_dependencies_and_dependents(
        options,
        checked_set_of_focused_list(roots.iter().cloned().collect()),
        flow_common_utils::checked_set::CheckedSet::empty(),
        flow_data_structure_wrapper::ord_set::FlowOrdSet::from_iter(all_dependent_files),
        implementation_dependency_graph,
        sig_dependency_graph,
    );
    let roots = checked_set_all(to_merge);
    let components = types_js_calc_deps(options, _components, &roots);
    (dependency_info, components, roots, to_check)
}

fn merge_job(
    _mutator: &(),
    options: &Options,
    for_find_all_refs: bool,
    reader: &flow_heap::parsing_heaps::Transaction,
    component: vec1::Vec1<FileKey>,
) -> (bool, ()) {
    let leader = component.first();
    let rest = &component[1..];
    let diff = match reader.typed_component(leader, rest) {
        None => false,
        Some(typed_component) => {
            let (hash, merge_hashes) = flow_services_inference::merge_service::sig_hash(
                for_find_all_refs,
                &options.root,
                reader,
                &typed_component,
            );
            flow_heap::parsing_heaps::merge_context_mutator::add_merge_on_diff(
                reader,
                for_find_all_refs,
                &typed_component,
                hash,
                merge_hashes,
            )
        }
    };
    (diff, ())
}

#[allow(unreachable_code)]
fn post_check<A>(
    _visit: &dyn Fn(
        &Options,
        &ast::Program<Loc, Loc>,
        &mut codemod_context::typed::TypedCodemodContext<'_>,
    ) -> A,
    _iteration: i32,
    _reader: &Arc<flow_heap::parsing_heaps::Transaction>,
    _options: &Options,
    _metadata: &Metadata,
    _file: &FileKey,
    checker: &mut Checker,
) -> Result<UnitResult<Option<((), A)>>, flow_utils_concurrency::worker_cancel::WorkerCanceled> {
    let outcome = checker.check(
        _file.clone(),
        |(cx, type_sig, file_sig, typed_ast), (_, _, error_suppressions, _, _, _)| {
            let ast = _reader.get_ast_unsafe(_file);
            let docblock = _reader.get_docblock_unsafe(_file);
            let mut ccx = codemod_context::typed::TypedCodemodContext {
                file: _file.clone(),
                type_sig,
                file_sig,
                metadata: _metadata.clone(),
                options: _options.clone(),
                cx,
                error_suppressions,
                typed_ast,
                docblock: (*docblock).clone(),
                iteration: _iteration,
                reader: _reader.clone(),
            };
            _visit(_options, &ast, &mut ccx)
        },
    );
    Ok(match outcome {
        flow_services_inference::merge_service::CheckJobOutcome::Ok(None) => Ok(None),
        flow_services_inference::merge_service::CheckJobOutcome::Ok(Some(result)) => {
            Ok(Some(((), result)))
        }
        flow_services_inference::merge_service::CheckJobOutcome::PerFileError(e) => Err(e),
        flow_services_inference::merge_service::CheckJobOutcome::Canceled(c) => return Err(c),
    })
}

struct CheckJobResults<A> {
    finished: ResultList<A>,
    unfinished: Vec<FileKey>,
    canceled: Option<flow_utils_concurrency::worker_cancel::WorkerCanceled>,
}

impl<A> Default for CheckJobResults<A> {
    fn default() -> Self {
        Self {
            finished: Vec::new(),
            unfinished: Vec::new(),
            canceled: None,
        }
    }
}

fn check_one_file<A>(
    check: &mut dyn FnMut(
        FileKey,
    ) -> Result<
        UnitResult<Option<((), A)>>,
        flow_utils_concurrency::worker_cancel::WorkerCanceled,
    >,
    file: FileKey,
) -> Result<(FileKey, UnitResult<Option<A>>), flow_utils_concurrency::worker_cancel::WorkerCanceled>
{
    let check_result = match check(file.clone())? {
        Ok(Some(((), acc))) => Ok(Some(acc)),
        Ok(None) => Ok(None),
        Err(err) => Err(err),
    };
    Ok((file, check_result))
}

fn mk_next_for_check<A>(
    max_size: usize,
    num_workers: usize,
    files: Vec<FileKey>,
    canceled: Arc<AtomicBool>,
) -> (
    impl flow_utils_concurrency::map_reduce::Next<FileKey>,
    impl Fn(&mut CheckJobResults<A>, CheckJobResults<A>) + Send + Sync,
) {
    let total_count = files.len();
    let todo = Arc::new(Mutex::new((VecDeque::from(files), total_count)));
    let files_completed = Arc::new(AtomicUsize::new(0));
    let num_workers = num_workers.max(1);

    let todo_for_next = todo.clone();
    let files_completed_for_next = files_completed.clone();
    let canceled_for_next = canceled.clone();
    let next = move || {
        if canceled_for_next.load(Ordering::Acquire) {
            return Bucket::Done;
        }

        let mut todo = todo_for_next.lock().unwrap();
        let (ref mut remaining_files, ref mut remaining_count) = *todo;
        let bucket_size = if *remaining_count >= max_size * num_workers {
            max_size
        } else {
            (*remaining_count / num_workers) + 1
        };
        let split_at = bucket_size.min(remaining_files.len());
        let bucket: Vec<FileKey> = remaining_files.drain(..split_at).collect();
        let bucket_size = bucket.len();
        *remaining_count -= bucket_size;

        if bucket_size == 0 {
            if files_completed_for_next.load(Ordering::Acquire) >= total_count {
                Bucket::Done
            } else {
                Bucket::Wait
            }
        } else {
            Bucket::Job(bucket)
        }
    };

    let todo_for_merge = todo.clone();
    let files_completed_for_merge = files_completed.clone();
    let merge = move |acc: &mut CheckJobResults<A>, mut batch: CheckJobResults<A>| {
        files_completed_for_merge.fetch_add(batch.finished.len(), Ordering::Release);

        if !batch.unfinished.is_empty() {
            let mut todo = todo_for_merge.lock().unwrap();
            let (ref mut remaining_files, ref mut remaining_count) = *todo;
            *remaining_count += batch.unfinished.len();
            for file in batch.unfinished.drain(..) {
                remaining_files.push_front(file);
            }
        }

        acc.finished.append(&mut batch.finished);
        if acc.canceled.is_none() {
            acc.canceled = batch.canceled;
        }
    };

    (next, merge)
}

fn check_all_files_sequential<A>(
    checker: &mut Checker,
    check: &dyn Fn(
        &mut Checker,
        FileKey,
    ) -> Result<
        UnitResult<Option<((), A)>>,
        flow_utils_concurrency::worker_cancel::WorkerCanceled,
    >,
    options: &Options,
    mut files: Vec<FileKey>,
) -> Result<ResultList<A>, flow_utils_concurrency::worker_cancel::WorkerCanceled> {
    let mut result = Vec::new();
    while !files.is_empty() {
        let files_len = files.len();
        checker.clear_cache();
        let job_result = flow_services_inference::job_utils::mk_job(
            &mut |file| check(checker, file),
            options,
            files,
        );
        checker.clear_cache();
        let (mut job_results, unfinished) = job_result?;
        result.append(&mut job_results);

        if unfinished.is_empty() {
            break;
        }

        if unfinished.len() == files_len {
            let mut unfinished = unfinished.into_iter();
            let file = unfinished
                .next()
                .expect("unfinished files should be nonempty");
            checker.clear_cache();
            result.push(check_one_file(&mut |file| check(checker, file), file)?);
            checker.clear_cache();
            files = unfinished.collect();
        } else {
            files = unfinished;
        }
    }
    Ok(result)
}

// `wrap` runs one file through the (reused) checker and applies the per-file
// `visit`/`post_check` around it, given the thread's `Metadata`.
fn check_all_files<A, Wrap>(
    workers: &Option<ThreadPool>,
    options: &Options,
    files: Vec<FileKey>,
    reader: &Arc<flow_heap::parsing_heaps::Transaction>,
    master_cx: &flow_typing_context::MasterContext,
    all_unordered_libs: Arc<BTreeSet<FlowSmolStr>>,
    wrap: Wrap,
) -> Result<ResultList<A>, flow_utils_concurrency::worker_cancel::WorkerCanceled>
where
    A: Send + Sync + 'static,
    Wrap: Fn(
            &mut Checker,
            &Metadata,
            FileKey,
        ) -> Result<
            UnitResult<Option<((), A)>>,
            flow_utils_concurrency::worker_cancel::WorkerCanceled,
        > + Send
        + Sync,
{
    // Build the per-file check machinery (check closure + cache + metadata) ONCE
    // per worker thread and reuse it across batches, mirroring
    // `type_service::check_files`. Building it per job (the previous behavior)
    // re-ran `mk_check_file` hundreds of times and accumulated the forced
    // dep-module export tables in process memory, roughly doubling glean's peak
    // RSS vs `full-check`/`focus-check` on the same fileset. `Metadata` holds an
    // `Rc` (not `Sync`), so it is created per thread and kept in the state.
    type InnerState = (Checker, Metadata);
    let mk_inner = || -> InnerState {
        let checker = flow_services_inference::merge_service::mk_check(
            reader.clone(),
            Arc::new(options.clone()),
            all_unordered_libs.dupe(),
            master_cx,
            flow_services_references::find_refs_types::empty_request(),
        );
        let metadata =
            flow_typing_context::mk_context_metadata(options, master_cx.global_libdefs());
        (checker, metadata)
    };

    let Some(workers) = workers else {
        let (mut inner, metadata) = mk_inner();
        return check_all_files_sequential(
            &mut inner,
            &|checker, file| wrap(checker, &metadata, file),
            options,
            files,
        );
    };

    thread_local! {
        static WORKER_CHECK: std::cell::RefCell<Option<InnerState>> =
            const { std::cell::RefCell::new(None) };
    }

    let canceled = Arc::new(AtomicBool::new(false));
    let num_workers = workers.num_workers();
    let max_size = options.max_files_checked_per_worker as usize;
    let (next, merge) = mk_next_for_check(max_size, num_workers, files, canceled.clone());
    let job = |acc: &mut CheckJobResults<A>, batch: Vec<FileKey>| {
        let (mut inner, metadata) = WORKER_CHECK
            .with(|cell| cell.borrow_mut().take())
            .unwrap_or_else(mk_inner);
        let batch_len = batch.len();
        inner.clear_cache();
        let job_result = flow_services_inference::job_utils::mk_job(
            &mut |file| wrap(&mut inner, &metadata, file),
            options,
            batch,
        );
        inner.clear_cache();
        match job_result {
            Ok((mut finished, unfinished)) => {
                acc.finished.append(&mut finished);
                if unfinished.len() == batch_len {
                    let mut unfinished = unfinished.into_iter();
                    let file = unfinished
                        .next()
                        .expect("unfinished files should be nonempty");
                    inner.clear_cache();
                    match check_one_file(&mut |file| wrap(&mut inner, &metadata, file), file) {
                        Ok(result) => acc.finished.push(result),
                        Err(err) => {
                            canceled.store(true, Ordering::Release);
                            acc.canceled = Some(err);
                        }
                    }
                    inner.clear_cache();
                    acc.unfinished.extend(unfinished);
                } else {
                    acc.unfinished = unfinished;
                }
            }
            Err(err) => {
                canceled.store(true, Ordering::Release);
                acc.canceled = Some(err);
            }
        }
        WORKER_CHECK.with(|cell| *cell.borrow_mut() = Some((inner, metadata)));
    };
    let result = flow_utils_concurrency::map_reduce::call(workers, next, job, merge);
    workers.broadcast(|_| {
        WORKER_CHECK.with(|cell| *cell.borrow_mut() = None);
    });
    match result.canceled {
        Some(err) => Err(err),
        None => Ok(result.finished),
    }
}

pub trait TypedRunnerWithPrepassConfig {
    type Accumulator;
    type PrepassState;
    type PrepassResult;

    fn reporter() -> codemod_report::CodemodReport<Self::Accumulator>;
    fn expand_roots(
        _env: &ServerEnv,
        _options: &Options,
        roots: BTreeSet<FileKey>,
    ) -> BTreeSet<FileKey>;
    fn prepass_init() -> Self::PrepassState;
    fn mod_prepass_options(options: Options) -> Options;
    fn check_options(options: Options) -> Options;
    fn include_dependents_in_prepass() -> bool;
    fn prepass_run(
        cx: flow_typing_context::Context<'_>,
        state: &Self::PrepassState,
        file: FileKey,
        file_options: &Arc<flow_common::files::FileOptions>,
        reader: &Arc<flow_heap::parsing_heaps::Transaction>,
        file_sig: &Arc<flow_parser_utils::file_sig::FileSig>,
        typed_ast: &ast::Program<flow_aloc::ALoc, (flow_aloc::ALoc, flow_typing_type::type_::Type)>,
    ) -> Self::PrepassResult;
    fn store_precheck_result(results: BTreeMap<FileKey, UnitResult<Self::PrepassResult>>);
    fn visit(
        options: &Options,
        ast: &ast::Program<Loc, Loc>,
        cctx: &mut codemod_context::typed::TypedCodemodContext<'_>,
    ) -> Self::Accumulator;
}

pub trait TypedRunnerConfig {
    type Accumulator;

    fn reporter() -> codemod_report::CodemodReport<Self::Accumulator>;
    fn expand_roots(
        _env: &ServerEnv,
        _options: &Options,
        roots: BTreeSet<FileKey>,
    ) -> BTreeSet<FileKey>;
    fn merge_and_check(
        env: &ServerEnv,
        workers: &Option<ThreadPool>,
        options: &Options,
        profiling: &(),
        roots: BTreeSet<FileKey>,
        iteration: i32,
        transaction: &Arc<flow_heap::parsing_heaps::Transaction>,
    ) -> impl std::future::Future<
        Output = Result<
            ResultList<Self::Accumulator>,
            flow_utils_concurrency::worker_cancel::WorkerCanceled,
        >,
    >;
}

// Checks the codebase and applies C, providing it with the inference context.
pub struct SimpleTypedRunner<C: SimpleTypedRunnerConfig> {
    _phantom: std::marker::PhantomData<C>,
}

impl<C: SimpleTypedRunnerConfig> TypedRunnerConfig for SimpleTypedRunner<C> {
    type Accumulator = C::Accumulator;

    fn reporter() -> codemod_report::CodemodReport<Self::Accumulator> {
        C::reporter()
    }

    fn expand_roots(
        _env: &ServerEnv,
        _options: &Options,
        roots: BTreeSet<FileKey>,
    ) -> BTreeSet<FileKey> {
        C::expand_roots(_env, _options, roots)
    }

    #[allow(unreachable_code)]
    async fn merge_and_check(
        _env: &ServerEnv,
        _workers: &Option<ThreadPool>,
        _options: &Options,
        _profiling: &(),
        _roots: BTreeSet<FileKey>,
        _iteration: i32,
        _transaction: &Arc<flow_heap::parsing_heaps::Transaction>,
    ) -> Result<ResultList<Self::Accumulator>, flow_utils_concurrency::worker_cancel::WorkerCanceled>
    {
        let _should_print = _options.profile;
        let reader = _transaction.clone();
        let get_dependent_files = |_: &flow_common_utils::graph::Graph<FileKey>,
                                   _: &flow_common_utils::graph::Graph<FileKey>,
                                   _: &BTreeSet<FileKey>|
         -> std::pin::Pin<
            Box<dyn std::future::Future<Output = BTreeSet<FileKey>>>,
        > { Box::pin(async { BTreeSet::new() }) };
        let (_dependency_info, _components, files_to_merge, _) =
            merge_targets(_env, _options, &(), get_dependent_files, &_roots).await;
        if let Some(pool) = _workers {
            flow_services_inference::type_service::ensure_parsed_or_trigger_recheck(
                None,
                pool,
                &reader,
                &Arc::new(_options.clone()),
                files_to_merge.clone().into_iter().collect(),
                flow_services_inference::type_service::ParseProgress::Report,
            )
            .expect("ensure_parsed_or_trigger_recheck failed");
        }
        let mutator = ();
        flow_hh_logger::info!("Merging {} files", files_to_merge.len());
        if let Some(pool) = _workers {
            let _merge_results = with_memory_timer(_options, "Merge", || {
                flow_services_inference::merge_service::merge_runner(
                    pool,
                    &reader,
                    _options,
                    false, // for_find_all_refs
                    _dependency_info.sig_dependency_graph(),
                    _components,
                    &files_to_merge,
                    move |_transaction: &flow_heap::parsing_heaps::Transaction,
                          _opts: &Options,
                          _for_find_all_refs: bool,
                          _component: vec1::Vec1<FileKey>| {
                        Ok(merge_job(
                            &mutator,
                            _opts,
                            _for_find_all_refs,
                            _transaction,
                            _component,
                        ))
                    },
                )
            })?;
        }
        flow_hh_logger::info!("Merging done.");
        flow_hh_logger::info!("Checking {} files", _roots.len());
        let options = C::check_options(_options.clone());
        let visit_fn: &(
             dyn Fn(
            &Options,
            &ast::Program<Loc, Loc>,
            &mut codemod_context::typed::TypedCodemodContext<'_>,
        ) -> Self::Accumulator
                 + Sync
         ) = &C::visit;
        let files: Vec<FileKey> = _roots.iter().cloned().collect();
        let wrap = |inner: &mut Checker, metadata: &Metadata, file: FileKey| {
            post_check(
                visit_fn, _iteration, &reader, &options, metadata, &file, inner,
            )
        };
        let result = with_memory_timer(&options, "Check", || {
            check_all_files(
                _workers,
                &options,
                files,
                &reader,
                &_env.master_cx,
                _env.configured_libs(),
                wrap,
            )
        })?;
        flow_hh_logger::info!("Done");
        Ok(result)
    }
}

pub struct SimpleTypedTwoPassRunner<C: SimpleTypedRunnerConfig> {
    _phantom: std::marker::PhantomData<C>,
}

impl<C: SimpleTypedRunnerConfig> TypedRunnerConfig for SimpleTypedTwoPassRunner<C> {
    type Accumulator = (C::Accumulator, BTreeSet<FileKey>);

    fn reporter() -> codemod_report::CodemodReport<Self::Accumulator> {
        let c_reporter = C::reporter();
        let report = match c_reporter.report {
            codemod_report::Reporter::StringReporter(f) => {
                codemod_report::Reporter::StringReporter(Box::new(
                    move |opts: &codemod_report::ReporterOptions,
                          (acc, _): &(C::Accumulator, BTreeSet<FileKey>)| {
                        f(opts, acc)
                    },
                ))
            }
            codemod_report::Reporter::UnitReporter(f) => {
                codemod_report::Reporter::UnitReporter(Box::new(
                    move |opts: &codemod_report::ReporterOptions,
                          (acc, _): &(C::Accumulator, BTreeSet<FileKey>)| {
                        f(opts, acc)
                    },
                ))
            }
        };
        let c_combine = c_reporter.combine;
        let combine: Box<
            dyn Fn(Self::Accumulator, Self::Accumulator) -> Self::Accumulator + Send + Sync,
        > = Box::new(move |(acc1, deps1), (acc2, deps2)| {
            ((c_combine)(acc1, acc2), &deps1 | &deps2)
        });
        let empty = (c_reporter.empty, BTreeSet::new());
        codemod_report::CodemodReport {
            report,
            combine,
            empty,
        }
    }

    fn expand_roots(
        _env: &ServerEnv,
        _options: &Options,
        roots: BTreeSet<FileKey>,
    ) -> BTreeSet<FileKey> {
        C::expand_roots(_env, _options, roots)
    }

    #[allow(unreachable_code)]
    async fn merge_and_check(
        _env: &ServerEnv,
        _workers: &Option<ThreadPool>,
        _options: &Options,
        _profiling: &(),
        _roots: BTreeSet<FileKey>,
        _iteration: i32,
        _transaction: &Arc<flow_heap::parsing_heaps::Transaction>,
    ) -> Result<ResultList<Self::Accumulator>, flow_utils_concurrency::worker_cancel::WorkerCanceled>
    {
        let _should_print = _options.profile;
        let reader = _transaction.clone();
        let get_dependent_files = |_: &flow_common_utils::graph::Graph<FileKey>,
                                   _: &flow_common_utils::graph::Graph<FileKey>,
                                   _: &BTreeSet<FileKey>|
         -> std::pin::Pin<
            Box<dyn std::future::Future<Output = BTreeSet<FileKey>>>,
        > { Box::pin(async { BTreeSet::new() }) };
        let (_dependency_info, _components, files_to_merge, _) =
            merge_targets(_env, _options, &(), get_dependent_files, &_roots).await;
        if let Some(pool) = _workers {
            flow_services_inference::type_service::ensure_parsed_or_trigger_recheck(
                None,
                pool,
                &reader,
                &Arc::new(_options.clone()),
                files_to_merge.clone().into_iter().collect(),
                flow_services_inference::type_service::ParseProgress::Report,
            )
            .expect("ensure_parsed_or_trigger_recheck failed");
        }
        let mutator = ();
        flow_hh_logger::info!("Merging {} files", files_to_merge.len());
        if let Some(pool) = _workers {
            let _merge_results = with_memory_timer(_options, "Merge", || {
                flow_services_inference::merge_service::merge_runner(
                    pool,
                    &reader,
                    _options,
                    false,
                    _dependency_info.sig_dependency_graph(),
                    _components,
                    &files_to_merge,
                    move |_sm: &flow_heap::parsing_heaps::Transaction,
                          _o: &Options,
                          _f: bool,
                          _c: vec1::Vec1<FileKey>| {
                        Ok(merge_job(&mutator, _o, _f, _sm, _c))
                    },
                )
            })?;
        }
        flow_hh_logger::info!("Merging done.");
        flow_hh_logger::info!("Checking {} files", _roots.len());
        let options = C::check_options(_options.clone());
        let visit =
            |options: &Options,
             ast: &ast::Program<Loc, Loc>,
             ctx: &mut codemod_context::typed::TypedCodemodContext<'_>| {
                let acc = C::visit(options, ast, ctx);
                let files = ctx.cx.reachable_deps().clone();
                (acc, files)
            };
        let files: Vec<FileKey> = _roots.iter().cloned().collect();
        let wrap = |inner: &mut Checker, metadata: &Metadata, file: FileKey| {
            post_check(
                &visit, _iteration, &reader, &options, metadata, &file, inner,
            )
        };
        let initial_run_result = with_memory_timer(&options, "Check", || {
            check_all_files(
                _workers,
                &options,
                files,
                &reader,
                &_env.master_cx,
                _env.configured_libs(),
                wrap,
            )
        })?;
        flow_hh_logger::info!("Initial run done");
        let second_run_roots: BTreeSet<FileKey> = initial_run_result.iter().fold(
            BTreeSet::<FileKey>::new(),
            |mut acc: BTreeSet<FileKey>, item| {
                let (_fk, r) = item;
                match r {
                    Ok(Some((_acc_inner, files))) => {
                        for f in files.difference(&_roots) {
                            acc.insert(f.clone());
                        }
                    }
                    _ => {}
                }
                acc
            },
        );
        let get_dependent_files2 = |_: &flow_common_utils::graph::Graph<FileKey>,
                                    _: &flow_common_utils::graph::Graph<FileKey>,
                                    _: &BTreeSet<FileKey>|
         -> std::pin::Pin<
            Box<dyn std::future::Future<Output = BTreeSet<FileKey>>>,
        > { Box::pin(async { BTreeSet::new() }) };
        let (_dependency_info2, _components2, files_to_merge2, _) =
            merge_targets(_env, _options, &(), get_dependent_files2, &second_run_roots).await;
        if let Some(pool) = _workers {
            flow_services_inference::type_service::ensure_parsed_or_trigger_recheck(
                None,
                pool,
                &reader,
                &Arc::new(_options.clone()),
                files_to_merge2.clone().into_iter().collect(),
                flow_services_inference::type_service::ParseProgress::Report,
            )
            .expect("ensure_parsed_or_trigger_recheck failed");
        }
        let mutator2 = ();
        flow_hh_logger::info!("Merging {} files", files_to_merge2.len());
        if let Some(pool) = _workers {
            let _merge_results = with_memory_timer(_options, "Merge", || {
                flow_services_inference::merge_service::merge_runner(
                    pool,
                    &reader,
                    _options,
                    false,
                    _dependency_info2.sig_dependency_graph(),
                    _components2,
                    &files_to_merge2,
                    move |_sm: &flow_heap::parsing_heaps::Transaction,
                          _o: &Options,
                          _f: bool,
                          _c: vec1::Vec1<FileKey>| {
                        Ok(merge_job(&mutator2, _o, _f, _sm, _c))
                    },
                )
            })?;
        }
        flow_hh_logger::info!("Merging done.");
        let files2: Vec<FileKey> = second_run_roots.iter().cloned().collect();
        let wrap2 = |inner: &mut Checker, metadata: &Metadata, file: FileKey| {
            post_check(
                &visit, _iteration, &reader, &options, metadata, &file, inner,
            )
        };
        let job_results2 = with_memory_timer(&options, "Check", || {
            check_all_files(
                _workers,
                &options,
                files2,
                &reader,
                &_env.master_cx,
                _env.configured_libs(),
                wrap2,
            )
        })?;
        let mut result: ResultList<Self::Accumulator> = initial_run_result;
        result.extend(job_results2);
        flow_hh_logger::info!("Pruned-deps run done");
        Ok(result)
    }
}

pub struct TypedRunnerWithPrepass<C: TypedRunnerWithPrepassConfig> {
    _phantom: std::marker::PhantomData<C>,
}

impl<C: TypedRunnerWithPrepassConfig> TypedRunnerConfig for TypedRunnerWithPrepass<C> {
    type Accumulator = C::Accumulator;

    fn reporter() -> codemod_report::CodemodReport<Self::Accumulator> {
        C::reporter()
    }

    fn expand_roots(
        _env: &ServerEnv,
        _options: &Options,
        roots: BTreeSet<FileKey>,
    ) -> BTreeSet<FileKey> {
        C::expand_roots(_env, _options, roots)
    }

    #[allow(unreachable_code)]
    async fn merge_and_check(
        _env: &ServerEnv,
        _workers: &Option<ThreadPool>,
        _options: &Options,
        _profiling: &(),
        _roots: BTreeSet<FileKey>,
        _iteration: i32,
        _transaction: &Arc<flow_heap::parsing_heaps::Transaction>,
    ) -> Result<ResultList<Self::Accumulator>, flow_utils_concurrency::worker_cancel::WorkerCanceled>
    {
        let _should_print = _options.profile;
        let reader = _transaction.clone();
        let get_dependent_files = |sig_dep: &flow_common_utils::graph::Graph<FileKey>,
                                   impl_dep: &flow_common_utils::graph::Graph<FileKey>,
                                   roots: &BTreeSet<FileKey>|
         -> std::pin::Pin<
            Box<dyn std::future::Future<Output = BTreeSet<FileKey>>>,
        > {
            if C::include_dependents_in_prepass() {
                let roots_set: flow_data_structure_wrapper::ord_set::FlowOrdSet<FileKey> =
                    roots.iter().cloned().collect();
                let result =
                    flow_services_inference::pure_dep_graph_operations::calc_all_dependents(
                        sig_dep, impl_dep, &roots_set,
                    );
                let btree_result: BTreeSet<FileKey> = result.into_iter().collect();
                Box::pin(async move { btree_result })
            } else {
                Box::pin(async { BTreeSet::new() })
            }
        };
        let (_dependency_info, _components, files_to_merge, _files_to_check) =
            merge_targets(_env, _options, &(), get_dependent_files, &_roots).await;
        if let Some(pool) = _workers {
            flow_services_inference::type_service::ensure_parsed_or_trigger_recheck(
                None,
                pool,
                &reader,
                &Arc::new(_options.clone()),
                files_to_merge.clone().into_iter().collect(),
                flow_services_inference::type_service::ParseProgress::Report,
            )
            .expect("ensure_parsed_or_trigger_recheck failed");
        }
        let mutator = ();
        flow_hh_logger::info!("Merging {} files", files_to_merge.len());
        if let Some(pool) = _workers {
            let _merge_results = flow_services_inference::merge_service::merge_runner(
                pool,
                &reader,
                _options,
                false,
                _dependency_info.sig_dependency_graph(),
                _components,
                &files_to_merge,
                move |_sm: &flow_heap::parsing_heaps::Transaction,
                      _o: &Options,
                      _f: bool,
                      _c: vec1::Vec1<FileKey>| {
                    Ok(merge_job(&mutator, _o, _f, _sm, _c))
                },
            )?;
        }
        flow_hh_logger::info!("Merging done.");
        let files_to_check = checked_set_all(_files_to_check);
        flow_hh_logger::info!("Pre-Checking {} files", files_to_check.len());
        let precheck_result: BTreeMap<FileKey, UnitResult<C::PrepassResult>> = {
            let state = C::prepass_init();
            let options = C::mod_prepass_options(_options.clone());
            let master_cx = &_env.master_cx;
            let mut checker = flow_services_inference::merge_service::mk_check(
                reader.clone(),
                Arc::new(options.clone()),
                _env.configured_libs(),
                master_cx,
                flow_services_references::find_refs_types::empty_request(),
            );
            let mut acc: BTreeMap<FileKey, UnitResult<C::PrepassResult>> = BTreeMap::new();
            let files_to_check_list: Vec<FileKey> = files_to_check.iter().cloned().collect();
            for file in files_to_check_list {
                match checker.check(file.clone(), |(cx, _type_sig, file_sig, typed_ast), _| {
                    C::prepass_run(
                        cx,
                        &state,
                        file.clone(),
                        &options.file_options,
                        &reader,
                        &file_sig,
                        &typed_ast,
                    )
                }) {
                    flow_services_inference::merge_service::CheckJobOutcome::Ok(None) => {}
                    flow_services_inference::merge_service::CheckJobOutcome::Ok(Some(result)) => {
                        acc.insert(file, Ok(result));
                    }
                    flow_services_inference::merge_service::CheckJobOutcome::PerFileError(e) => {
                        acc.insert(file, Err(e));
                    }
                    flow_services_inference::merge_service::CheckJobOutcome::Canceled(c) => {
                        return Err(c);
                    }
                }
            }
            acc
        };
        flow_hh_logger::info!("Pre-checking Done");
        flow_hh_logger::info!("Storing pre-checking results");
        C::store_precheck_result(precheck_result);
        flow_hh_logger::info!("Storing pre-checking results Done");
        flow_hh_logger::info!("Checking+Codemodding {} files", _roots.len());
        let options = C::check_options(_options.clone());
        let metadata =
            flow_typing_context::mk_context_metadata(&options, _env.master_cx.global_libdefs());
        let visit_fn: &(
             dyn Fn(
            &Options,
            &ast::Program<Loc, Loc>,
            &mut codemod_context::typed::TypedCodemodContext<'_>,
        ) -> Self::Accumulator
                 + Sync
         ) = &C::visit;
        let mut inner = flow_services_inference::merge_service::mk_check(
            reader.clone(),
            Arc::new(options.clone()),
            _env.configured_libs(),
            &_env.master_cx,
            flow_services_references::find_refs_types::empty_request(),
        );
        let check_fn = |inner: &mut Checker, file: FileKey| {
            post_check(
                visit_fn, _iteration, &reader, &options, &metadata, &file, inner,
            )
        };
        let files: Vec<FileKey> = _roots.iter().cloned().collect();
        let result = check_all_files_sequential(&mut inner, &check_fn, &options, files)?;
        flow_hh_logger::info!("Checking+Codemodding Done");
        Ok(result)
    }
}

pub struct TypedRunner<TRC: TypedRunnerConfig> {
    _phantom: std::marker::PhantomData<TRC>,
}

impl<TRC: TypedRunnerConfig> StepRunner for TypedRunner<TRC>
where
    TRC::Accumulator: Clone,
{
    type Env = ServerEnv;
    type Accumulator = TRC::Accumulator;

    fn reporter() -> codemod_report::CodemodReport<Self::Accumulator> {
        TRC::reporter()
    }

    #[allow(unreachable_code)]
    async fn init_run(
        _genv: &Genv,
        heap: &Arc<CommittedHeap>,
        _roots: BTreeSet<FileKey>,
    ) -> Result<
        ((), (Self::Env, ResultList<Self::Accumulator>)),
        flow_utils_concurrency::worker_cancel::WorkerCanceled,
    > {
        let options = &*_genv.options;
        let workers = &_genv.workers;
        let should_print_summary = options.profile;
        let profiling = profiling_start("Codemod", should_print_summary);
        let (heap_transaction, transaction, options_arc) =
            with_memory_timer(options, "CodemodInit", || {
                extract_flowlibs_or_exit(options);
                let heap_transaction = ActiveTransaction::new(heap.clone());
                let transaction = heap_transaction.handle();
                let options_arc = Arc::new(options.clone());
                (heap_transaction, transaction, options_arc)
            });
        let pool = workers.as_ref().expect("workers required for init");
        let root = &options.root;
        let (env, _libs_ok) = flow_services_inference::type_service::init_from_scratch(
            &options_arc,
            pool,
            &transaction,
            root,
            None,
        );
        let roots = with_memory_timer(options, "CodemodTargets", || {
            let file_options = &options.file_options;
            let all = options.all;
            let roots = get_target_filename_set(file_options, all, _roots);
            let roots = TRC::expand_roots(&env, options, roots);
            let env_files: BTreeSet<FileKey> = env.files.iter().cloned().collect();
            let roots = roots.intersection(&env_files).cloned().collect();
            log_input_files(&roots);
            roots
        });
        let results =
            TRC::merge_and_check(&env, workers, options, &profiling, roots, 0, &transaction)
                .await?;
        with_memory_timer(options, "CodemodCommit", || {
            drop(transaction);
            heap_transaction.commit();
        });
        Ok(((), (env, results)))
    }

    #[allow(unreachable_code)]
    async fn recheck_run(
        _genv: &Genv,
        heap: &Arc<CommittedHeap>,
        env: Self::Env,
        _iteration: i32,
        _roots: BTreeSet<FileKey>,
    ) -> Result<
        ((), (Self::Env, ResultList<Self::Accumulator>)),
        flow_utils_concurrency::worker_cancel::WorkerCanceled,
    > {
        let options = &*_genv.options;
        let workers = &_genv.workers;
        let should_print_summary = options.profile;
        let profiling = profiling_start("Codemod", should_print_summary);
        diff_heaps_remove_batch(&_roots);
        let options_arc = Arc::new(options.clone());
        let pool = workers.as_ref().expect("workers required for recheck");
        let mut updates = flow_common_utils::checked_set::CheckedSet::empty();
        let focused_set: flow_data_structure_wrapper::ord_set::FlowOrdSet<FileKey> =
            _roots.iter().cloned().collect();
        updates.add(Some(focused_set), None, None);
        let find_ref_request = flow_services_references::find_refs_types::empty_request();
        let files_to_force = flow_common_utils::checked_set::CheckedSet::empty();
        let mut will_be_checked_files = flow_common_utils::checked_set::CheckedSet::empty();
        let recheck_result = flow_services_inference::type_service::recheck(
            None,
            pool,
            heap,
            &options_arc,
            &updates,
            &find_ref_request,
            files_to_force,
            None,
            None,  // changed_mergebase
            false, // missed_changes
            &mut will_be_checked_files,
            Arc::new(env),
        );
        let (_, _, _, prepared) = recheck_result.expect("recheck failed");
        let committed = prepared.commit();
        let env = committed.env;
        let transaction = ActiveTransaction::new(heap.clone());
        log_input_files(&_roots);
        let results = TRC::merge_and_check(
            &env,
            workers,
            options,
            &profiling,
            _roots,
            _iteration,
            &transaction.handle(),
        )
        .await?;
        let env = EnvTransaction::new(env).into_env();
        Ok(((), (env, results)))
    }

    #[allow(unreachable_code)]
    fn digest(results: ResultList<Self::Accumulator>) -> (Vec<FileKey>, Self::Accumulator) {
        let reporter = TRC::reporter();
        let empty = reporter.empty.clone();
        results.into_iter().fold(
            (Vec::new(), empty),
            |(mut acc_files, acc_result), (file_key, result)| match result {
                Ok(Some(ok)) => {
                    acc_files.push(file_key);
                    let combined = (reporter.combine)(acc_result, ok);
                    (acc_files, combined)
                }
                Ok(None) => (acc_files, acc_result),
                Err((aloc, err)) => {
                    eprintln!(
                        "{}: {}",
                        flow_common::reason::string_of_aloc(None, &aloc),
                        flow_typing_errors::error_message::string_of_internal_error(&err),
                    );
                    (acc_files, acc_result)
                }
            },
        )
    }
}

pub fn untyped_runner_job<A, Ctx>(
    mk_ccx: impl Fn(&FileKey) -> Ctx,
    visit: impl Fn(&ast::Program<Loc, Loc>, Ctx) -> A,
    abstract_reader: &flow_heap::parsing_heaps::Transaction,
    file_list: Vec<FileKey>,
) -> ResultList<A> {
    file_list.into_iter().fold(Vec::new(), |mut acc, file| {
        let ast = abstract_reader.get_ast_unsafe(&file);
        let result = visit(&ast, mk_ccx(&file));
        acc.push((file, Ok(Some(result))));
        acc
    })
}

pub fn untyped_digest<A>(
    reporter: &codemod_report::CodemodReport<A>,
    results: ResultList<A>,
) -> (Vec<FileKey>, A)
where
    A: Clone,
{
    let empty = reporter.empty.clone();
    results.into_iter().fold(
        (Vec::new(), empty),
        |(mut acc_files, acc_result), (file_key, result)| match result {
            Err(_) | Ok(None) => (acc_files, acc_result),
            Ok(Some(result)) => {
                acc_files.push(file_key);
                let combined = (reporter.combine)(result, acc_result);
                (acc_files, combined)
            }
        },
    )
}

pub struct UntypedRunner<C: UntypedRunnerConfig> {
    _phantom: std::marker::PhantomData<C>,
}

impl<C: UntypedRunnerConfig> UntypedRunner<C>
where
    C::Accumulator: Clone,
{
    #[allow(unreachable_code)]
    async fn run(
        _genv: &Genv,
        heap: &Arc<CommittedHeap>,
        _roots: BTreeSet<FileKey>,
    ) -> ((), ResultList<C::Accumulator>) {
        let options = &*_genv.options;
        let workers = &_genv.workers;
        let should_print_summary = options.profile;
        let _profiling = profiling_start("Codemod", should_print_summary);
        let file_options = &options.file_options;
        let all = options.all;
        let filename_set = get_target_filename_set(file_options, all, _roots);
        let next = next_of_filename_set(workers, &filename_set);
        let reader = ActiveTransaction::new(heap.clone());
        let reader_handle = reader.handle();
        let parse_results = flow_parsing::parsing_service::parse_with_defaults(
            workers.as_ref().unwrap(),
            &reader_handle,
            &Arc::new(options.clone()),
            &[],
            next,
        );
        let roots: BTreeSet<FileKey> = parse_results.parsed.into_iter().collect();
        log_input_files(&roots);
        let _next2 = next_of_filename_set(workers, &roots);
        let mk_ccx =
            |file: &FileKey| codemod_context::untyped::UntypedCodemodContext { file: file.clone() };
        let visit = |ast: &ast::Program<Loc, Loc>,
                     ccx: codemod_context::untyped::UntypedCodemodContext| {
            C::visit(options, ast, ccx)
        };
        let files: Vec<FileKey> = roots.into_iter().collect();
        let result: ResultList<C::Accumulator> = untyped_runner_job(mk_ccx, visit, &reader, files);
        ((), result)
    }
}

impl<C: UntypedRunnerConfig> StepRunner for UntypedRunner<C>
where
    C::Accumulator: Clone,
{
    type Env = ();
    type Accumulator = C::Accumulator;

    fn reporter() -> codemod_report::CodemodReport<Self::Accumulator> {
        C::reporter()
    }

    async fn init_run(
        genv: &Genv,
        heap: &Arc<CommittedHeap>,
        roots: BTreeSet<FileKey>,
    ) -> Result<
        ((), (Self::Env, ResultList<Self::Accumulator>)),
        flow_utils_concurrency::worker_cancel::WorkerCanceled,
    > {
        let ((), results) = Self::run(genv, heap, roots).await;
        Ok(((), ((), results)))
    }

    async fn recheck_run(
        genv: &Genv,
        heap: &Arc<CommittedHeap>,
        _env: Self::Env,
        _iteration: i32,
        roots: BTreeSet<FileKey>,
    ) -> Result<
        ((), (Self::Env, ResultList<Self::Accumulator>)),
        flow_utils_concurrency::worker_cancel::WorkerCanceled,
    > {
        let ((), results) = Self::run(genv, heap, roots).await;
        Ok(((), ((), results)))
    }

    fn digest(results: ResultList<Self::Accumulator>) -> (Vec<FileKey>, Self::Accumulator) {
        let reporter = C::reporter();
        untyped_digest(&reporter, results)
    }
}

pub struct UntypedFlowInitRunner<C: UntypedFlowInitRunnerConfig> {
    _phantom: std::marker::PhantomData<C>,
}

impl<C: UntypedFlowInitRunnerConfig> UntypedFlowInitRunner<C>
where
    C::Accumulator: Clone,
{
    #[allow(unreachable_code)]
    async fn run(
        _genv: &Genv,
        heap: &Arc<CommittedHeap>,
        _roots: BTreeSet<FileKey>,
    ) -> ((), ResultList<C::Accumulator>) {
        let options = &*_genv.options;
        let workers = &_genv.workers;
        let should_print_summary = options.profile;
        let _profiling = profiling_start("Codemod", should_print_summary);
        let mut options = options.clone();
        options.all = true;
        extract_flowlibs_or_exit(&options);
        let transaction = ActiveTransaction::new(heap.clone());
        let reader = transaction.handle();
        let options_arc = Arc::new(options.clone());
        let pool = workers.as_ref().expect("workers required for init");
        let root = &options.root;
        let (env, _libs_ok) = flow_services_inference::type_service::init_from_scratch(
            &options_arc,
            pool,
            &reader,
            root,
            None,
        );
        let file_options = &options.file_options;
        let all = options.all;
        let filename_set = get_target_filename_set(file_options, all, _roots);
        let env_files: BTreeSet<FileKey> = env.files.iter().cloned().collect();
        let filename_set: BTreeSet<FileKey> =
            filename_set.intersection(&env_files).cloned().collect();
        let _next = next_of_filename_set(workers, &filename_set);
        C::init(&reader);
        let reader_for_ccx = reader.clone();
        let mk_ccx =
            |file: &FileKey| codemod_context::untyped_flow_init::UntypedFlowInitCodemodContext {
                file: file.clone(),
                reader: reader_for_ccx.clone(),
            };
        let visit =
            |ast: &ast::Program<Loc, Loc>,
             ccx: codemod_context::untyped_flow_init::UntypedFlowInitCodemodContext| {
                C::visit(&options, ast, ccx)
            };
        let abstract_reader = reader.clone();
        log_input_files(&filename_set);
        let files: Vec<FileKey> = filename_set.into_iter().collect();
        let result: ResultList<C::Accumulator> =
            untyped_runner_job(mk_ccx, visit, &abstract_reader, files);
        ((), result)
    }
}

impl<C: UntypedFlowInitRunnerConfig> StepRunner for UntypedFlowInitRunner<C>
where
    C::Accumulator: Clone,
{
    type Env = ();
    type Accumulator = C::Accumulator;

    fn reporter() -> codemod_report::CodemodReport<Self::Accumulator> {
        C::reporter()
    }

    async fn init_run(
        genv: &Genv,
        heap: &Arc<CommittedHeap>,
        roots: BTreeSet<FileKey>,
    ) -> Result<
        ((), (Self::Env, ResultList<Self::Accumulator>)),
        flow_utils_concurrency::worker_cancel::WorkerCanceled,
    > {
        let ((), results) = Self::run(genv, heap, roots).await;
        Ok(((), ((), results)))
    }

    async fn recheck_run(
        genv: &Genv,
        heap: &Arc<CommittedHeap>,
        _env: Self::Env,
        _iteration: i32,
        roots: BTreeSet<FileKey>,
    ) -> Result<
        ((), (Self::Env, ResultList<Self::Accumulator>)),
        flow_utils_concurrency::worker_cancel::WorkerCanceled,
    > {
        let ((), results) = Self::run(genv, heap, roots).await;
        Ok(((), ((), results)))
    }

    fn digest(results: ResultList<Self::Accumulator>) -> (Vec<FileKey>, Self::Accumulator) {
        let reporter = C::reporter();
        untyped_digest(&reporter, results)
    }
}

const MAX_NUMBER_OF_ITERATIONS: i32 = 10;

fn header(iteration: i32, filenames: &BTreeSet<FileKey>) {
    // Use natural counting starting from 1.
    println!(">>> Iteration: {}", iteration + 1);
    println!(">>> Running codemod on {} files...", filenames.len());
}

pub struct RepeatRunner<SR: StepRunner> {
    _phantom: std::marker::PhantomData<SR>,
}

impl<SR: StepRunner> RepeatRunner<SR>
where
    SR::Accumulator: Clone,
{
    async fn post_run(
        _options: &Options,
        write: bool,
        results: ResultList<SR::Accumulator>,
    ) -> Option<BTreeSet<FileKey>> {
        let strip_root: Option<std::path::PathBuf> = if _options.strip_root {
            Some((*_options.root).clone())
        } else {
            None
        };

        let reporter_options = codemod_report::ReporterOptions {
            strip_root: strip_root.clone(),
        };

        let (files, report) = SR::digest(results);

        let changed_files = codemod_printer::print_asts(&strip_root, write, files).await;

        let sr_reporter = SR::reporter();
        codemod_printer::print_results(&reporter_options, &sr_reporter.report, &report);

        changed_files.map(|files| files.into_iter().collect())
    }

    async fn loop_run(
        genv: &Genv,
        heap: &Arc<CommittedHeap>,
        mut env: SR::Env,
        mut iteration: i32,
        options: &Options,
        write: bool,
        mut changed_files: Option<BTreeSet<FileKey>>,
    ) -> Result<(), flow_utils_concurrency::worker_cancel::WorkerCanceled> {
        loop {
            if iteration > MAX_NUMBER_OF_ITERATIONS {
                eprintln!(">>> Reached maximum number of iterations (10). Exiting...");
                return Ok(());
            }
            match changed_files {
                None => return Ok(()),
                Some(ref set) if set.is_empty() => return Ok(()),
                Some(roots) => {
                    header(iteration, &roots);
                    let (_, (new_env, results)) =
                        SR::recheck_run(genv, heap, env, iteration, roots).await?;
                    env = new_env;
                    changed_files = with_timer_async(
                        options,
                        "CodemodReport",
                        Self::post_run(options, write, results),
                    )
                    .await;
                    iteration += 1;
                }
            }
        }
    }
}

impl<SR: StepRunner> Runnable for RepeatRunner<SR>
where
    SR::Accumulator: Clone,
{
    async fn run(
        genv: &Genv,
        heap: &Arc<CommittedHeap>,
        write: bool,
        repeat: bool,
        roots: BTreeSet<FileKey>,
    ) {
        if repeat {
            header(0, &roots);
        }
        let (_prof, (env, results)) = match SR::init_run(genv, heap, roots).await {
            Ok(v) => v,
            Err(_) => {
                eprintln!(">>> Codemod canceled.");
                return;
            }
        };
        let options = &*genv.options;
        let changed_files = with_timer_async(
            options,
            "CodemodReport",
            Self::post_run(options, write, results),
        )
        .await;
        if repeat
            && Self::loop_run(genv, heap, env, 1, options, write, changed_files)
                .await
                .is_err()
        {
            eprintln!(">>> Codemod canceled.");
        }
    }
}

pub type MakeSimpleTypedRunner<C> = RepeatRunner<TypedRunner<SimpleTypedRunner<C>>>;

pub type MakeSimpleTypedTwoPassRunner<C> = RepeatRunner<TypedRunner<SimpleTypedTwoPassRunner<C>>>;

pub type MakeTypedRunnerWithPrepass<C> = RepeatRunner<TypedRunner<TypedRunnerWithPrepass<C>>>;

pub type MakeUntypedFlowInitRunner<C> = RepeatRunner<UntypedFlowInitRunner<C>>;

pub type MakeUntypedRunner<C> = RepeatRunner<UntypedRunner<C>>;
