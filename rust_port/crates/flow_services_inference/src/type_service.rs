/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crossbeam::channel;
use dupe::Dupe;
use dupe::IterDupedExt;
use flow_aloc::ALoc;
use flow_common::files;
use flow_common::files::FileOptions;
use flow_common::flow_version;
use flow_common::options::Options;
use flow_common::options::SavedStateFetcher;
use flow_common_errors::error_utils::ConcreteLocPrintableErrorSet;
use flow_common_errors::error_utils::PrintableError;
use flow_common_modulename::Modulename;
use flow_common_semver::semver;
use flow_common_tarjan::topsort;
use flow_common_transaction as side_effect_transaction;
use flow_common_utils::checked_set::CheckedSet;
use flow_common_utils::graph::Graph;
use flow_common_utils::graph::GraphLike;
use flow_config::FlowConfig;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_data_structure_wrapper::overlay_map::EnvCell;
use flow_data_structure_wrapper::overlay_map::OverlayMap;
use flow_data_structure_wrapper::overlay_map::OverlayMapBase;
use flow_data_structure_wrapper::overlay_map::apply_delta_to_base_map;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_heap::heap_state::CommittedHeap;
use flow_heap::parsing_heaps::ActiveTransaction;
use flow_heap::parsing_heaps::Transaction;
use flow_parser::ast::Program;
use flow_parser::dts_file_kind::DtsFileKind;
use flow_parser::file_key::FileKey;
use flow_parser::file_key::has_dts_ext;
use flow_parser::loc::Loc;
use flow_parser::loc_sig::LocSig;
use flow_parsing::parsing_service;
use flow_saved_state::FetchResult;
use flow_saved_state::SavedStateEnvData;
use flow_server_env::collated_errors::CollatedErrors;
use flow_server_env::collated_errors::OverlayCollatedErrors;
use flow_server_env::dependency_info::DependencyInfo;
use flow_server_env::dependency_info::OverlayDependencyInfo;
use flow_server_env::error_collator;
use flow_server_env::monitor_rpc;
use flow_server_env::persistent_connection::PersistentConnection;
use flow_server_env::server_env;
use flow_server_env::server_env::Env;
use flow_server_env::server_env::EnvRef;
use flow_server_env::server_env::EnvTransaction;
use flow_server_env::server_env::Errors;
use flow_server_env::server_env::OverlayCoverage;
use flow_server_env::server_env::OverlayDuplicateProviders;
use flow_server_env::server_env::OverlayErrorMap;
use flow_server_env::server_env::OverlayErrors;
use flow_server_env::server_env::env_cell;
use flow_server_env::server_env::overlay_coverage;
use flow_server_env::server_orchestrator::CommittedRecheck;
use flow_server_env::server_orchestrator::ServerOrchestratorHandle;
use flow_server_env::server_status;
use flow_server_files::server_files_js;
use flow_services_export::export_index::ExportIndex;
use flow_services_export::export_search::ExportSearch;
use flow_services_get_def::get_def_types::DefInfo;
use flow_services_module::PackageIncompatibleReturn;
use flow_services_references::find_refs_types::FindRefsFound;
use flow_typing_context::MasterContext;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::InternalError;
use flow_typing_errors::error_suppressions::ErrorSuppressions;
use flow_typing_errors::flow_error;
use flow_typing_errors::flow_error::ErrorSet;
use flow_utils_concurrency::thread_pool::ThreadPool;
use flow_utils_concurrency::worker_cancel;
use vec1::Vec1;

use crate::dep_service;
use crate::inference_utils;
use crate::init;
use crate::job_utils;
use crate::merge_service;
use crate::pure_dep_graph_operations;
use crate::recheck_stats;

#[derive(Debug)]
pub enum RecheckError {
    TooSlow,
    Canceled(Vec<FileKey>),
}

/// Whether an `ensure_parsed` announces its progress on the server status line.
///
/// Only work the status machine already counts as a typecheck may: it accepts a parsing-progress
/// event solely while the server is typechecking, and a command executor sits in `Free`, where the
/// same event is an illegal transition that panics a `debug_assertions` build.
#[derive(Clone, Copy, Debug)]
pub enum ParseProgress {
    Report,
    Silent,
}

/// A completed recheck whose heap and environment changes have not yet been published.
#[must_use = "a prepared recheck must be committed by the recheck thread"]
pub struct PreparedRecheck {
    env_transaction: EnvTransaction,
    heap_transaction: ActiveTransaction,
    /// Set when the recheck reinitialised from a saved state, whose dump is a whole heap and so
    /// was built as a heap of its own. The heap in force is untouched until the orchestrator
    /// adopts this one, which is why abandoning the recheck costs the server nothing.
    loaded_heap: Option<Arc<CommittedHeap>>,
}

impl PreparedRecheck {
    pub fn commit(self) -> CommittedRecheck {
        CommittedRecheck {
            env: self
                .env_transaction
                .commit_with_transaction(self.heap_transaction),
            heap: self.loaded_heap,
        }
    }
}

fn prepare_recheck(
    env_transaction: EnvTransaction,
    heap_transaction: ActiveTransaction,
    loaded_heap: Option<Arc<CommittedHeap>>,
) -> PreparedRecheck {
    PreparedRecheck {
        env_transaction,
        heap_transaction,
        loaded_heap,
    }
}

fn check_recheck_canceled() -> Result<(), RecheckError> {
    worker_cancel::check_should_cancel().map_err(|_| RecheckError::Canceled(Vec::new()))
}

fn with_memory_timer<T>(options: &Options, timer: &str, f: impl FnOnce() -> T) -> T {
    flow_profiling::memory_utils::with_memory_timer(options.profile && !options.quiet, timer, f)
}

fn with_timer<T>(options: &Options, timer: &str, f: impl FnOnce() -> T) -> T {
    match flow_profiling::profiling_js::current() {
        Some(profiling) => profiling.with_timer(options.profile && !options.quiet, timer, f),
        None => f(),
    }
}

pub(crate) fn clear_errors(
    files: &FlowOrdSet<FileKey>,
    mut errors: OverlayErrors,
) -> OverlayErrors {
    let OverlayErrors {
        ref mut local_errors,
        ref mut duplicate_providers,
        ref mut merge_errors,
        ref mut warnings,
        ref mut suppressions,
        ..
    } = errors;
    for file in files {
        local_errors.remove(file);
        merge_errors.remove(file);
        warnings.remove(file);
        suppressions.remove(file);
    }
    let duplicate_provider_keys: Vec<FlowSmolStr> = duplicate_providers
        .iter()
        .filter(|(_, (leader, others))| {
            files.contains(leader) || others.iter().any(|other| files.contains(other))
        })
        .map(|(key, _)| key.dupe())
        .collect();
    for key in duplicate_provider_keys {
        duplicate_providers.remove(&key);
    }
    errors
}

pub(crate) fn filter_errors(files: &FlowOrdSet<FileKey>, errors: &OverlayErrors) -> Errors {
    let OverlayErrors {
        local_errors,
        duplicate_providers,
        merge_errors,
        warnings,
        suppressions,
        ..
    } = errors;
    let local_errors: BTreeMap<FileKey, ErrorSet> = local_errors
        .iter()
        .filter(|(file, _)| files.contains(file))
        .map(|(k, v)| (k.dupe(), v.dupe()))
        .collect();
    let merge_errors: BTreeMap<FileKey, ErrorSet> = merge_errors
        .iter()
        .filter(|(file, _)| files.contains(file))
        .map(|(k, v)| (k.dupe(), v.dupe()))
        .collect();
    let warnings: BTreeMap<FileKey, ErrorSet> = warnings
        .iter()
        .filter(|(file, _)| files.contains(file))
        .map(|(k, v)| (k.dupe(), v.dupe()))
        .collect();
    let suppressions = suppressions.filter_by_file(files);
    Errors {
        local_errors,
        duplicate_providers: duplicate_providers
            .iter()
            .map(|(key, value)| (key.dupe(), value.clone()))
            .collect(),
        merge_errors,
        warnings,
        suppressions,
    }
}

/// A per-file error-set map, abstracting over the plain `BTreeMap` used for
/// transient maps and the `OverlayMap` used for the long-lived `Errors` fields.
trait ErrSetMap {
    fn get_errset(&self, file: &FileKey) -> Option<&ErrorSet>;
    fn insert_errset(&mut self, file: FileKey, errset: ErrorSet);
}

impl ErrSetMap for BTreeMap<FileKey, ErrorSet> {
    fn get_errset(&self, file: &FileKey) -> Option<&ErrorSet> {
        self.get(file)
    }
    fn insert_errset(&mut self, file: FileKey, errset: ErrorSet) {
        self.insert(file, errset);
    }
}

impl<B: OverlayMapBase<FileKey, ErrorSet>> ErrSetMap for OverlayMap<FileKey, ErrorSet, B> {
    fn get_errset(&self, file: &FileKey) -> Option<&ErrorSet> {
        self.get(file)
    }
    fn insert_errset(&mut self, file: FileKey, errset: ErrorSet) {
        self.insert(file, errset);
    }
}

fn update_errset<M: ErrSetMap>(map: &mut M, file: FileKey, errset: ErrorSet) {
    if !errset.is_empty() {
        let errset = match map.get_errset(&file) {
            Some(prev_errset) => prev_errset.union(&errset),
            None => errset,
        };
        map.insert_errset(file, errset);
    }
}

fn merge_error_maps<M: ErrSetMap>(mut left: M, right: BTreeMap<FileKey, ErrorSet>) -> M {
    for (file, errset) in right {
        let errset = match left.get_errset(&file) {
            Some(prev_errset) => prev_errset.union(&errset),
            None => errset,
        };
        left.insert_errset(file, errset);
    }
    left
}

fn collate_parse_results(parse_results: parsing_service::ParseResults) -> CollatedParseResults {
    let parsing_service::ParseResults {
        parsed,
        unparsed,
        changed,
        failed: (failed, errors),
        unchanged,
        not_found,
        package_json,
        dirty_modules,
        all_unordered_libs,
        dts_file_kinds,
        ts_global_augmentation_files,
    } = parse_results;
    assert!(changed.is_empty());
    let local_errors =
        failed
            .iter()
            .zip(errors.iter())
            .fold(BTreeMap::new(), |mut acc, (file, error)| {
                let errset = match error {
                    parsing_service::ParseFailure::UncaughtException(exn) => {
                        inference_utils::set_of_parse_exception(
                            file.dupe(),
                            FlowSmolStr::from(exn.as_str()),
                        )
                    }
                    parsing_service::ParseFailure::ParseError(parse_error) => {
                        inference_utils::set_of_parse_error(file.dupe(), parse_error.clone())
                    }
                    parsing_service::ParseFailure::DocblockErrors(errs) => {
                        inference_utils::set_of_docblock_errors(file.dupe(), errs)
                    }
                };
                update_errset(&mut acc, file.dupe(), errset);
                acc
            });
    let failed_set: FlowOrdSet<FileKey> = failed.into_iter().collect();
    let unparsed = unparsed.union(failed_set);
    CollatedParseResults {
        parsed,
        unparsed,
        unchanged,
        not_found,
        dirty_modules,
        local_errors,
        package_json,
        all_unordered_libs,
        dts_file_kinds,
        ts_global_augmentation_files,
    }
}

struct CollatedParseResults {
    parsed: FlowOrdSet<FileKey>,
    unparsed: FlowOrdSet<FileKey>,
    unchanged: FlowOrdSet<FileKey>,
    not_found: FlowOrdSet<FileKey>,
    dirty_modules: BTreeSet<Modulename>,
    local_errors: BTreeMap<FileKey, ErrorSet>,
    package_json: (
        Vec<FileKey>,
        Vec<Option<(Loc, flow_parser::parse_error::ParseError)>>,
    ),
    all_unordered_libs: BTreeSet<FlowSmolStr>,
    dts_file_kinds: BTreeMap<FileKey, DtsFileKind>,
    ts_global_augmentation_files: FlowOrdSet<FileKey>,
}

impl CollatedParseResults {
    fn discovered_global_libdefs(&self) -> FlowOrdSet<FileKey> {
        self.dts_file_kinds
            .iter()
            .filter(|(file, kind)| !file.is_lib_file() && **kind == DtsFileKind::GlobalLibdef)
            .map(|(file, _)| file.dupe())
            .collect()
    }

    fn discovered_global_libdefs_state(&self) -> Arc<BTreeSet<FileKey>> {
        Arc::new(self.discovered_global_libdefs().into_iter().collect())
    }
}

fn parse(
    pool: &ThreadPool,
    transaction: &Arc<Transaction>,
    options: &Arc<Options>,
    parse_next: parsing_service::Next,
) -> CollatedParseResults {
    with_memory_timer(options, "Parsing", || {
        let results =
            parsing_service::parse_with_defaults(pool, transaction, options, &[], parse_next);
        collate_parse_results(results)
    })
}

fn reparse(
    pool: &ThreadPool,
    transaction: &Arc<Transaction>,
    options: &Arc<Options>,
    def_info: &DefInfo,
    modified: parsing_service::Next,
) -> CollatedParseResults {
    with_memory_timer(options, "Parsing", || {
        let locs_to_dirtify = flow_services_get_def::get_def_utils::all_locs_of_def_info(def_info);
        let results = parsing_service::reparse_with_defaults(
            pool,
            transaction,
            options,
            &locs_to_dirtify,
            modified,
        );
        collate_parse_results(results)
    })
}

trait DuplicateProvidersMap {
    fn is_empty(&self) -> bool;
    fn remove_provider(&mut self, key: &FlowSmolStr);
    fn insert_provider_if_absent(&mut self, key: FlowSmolStr, value: (FileKey, Vec1<FileKey>));
}

impl DuplicateProvidersMap for BTreeMap<FlowSmolStr, (FileKey, Vec1<FileKey>)> {
    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn remove_provider(&mut self, key: &FlowSmolStr) {
        self.remove(key);
    }

    fn insert_provider_if_absent(&mut self, key: FlowSmolStr, value: (FileKey, Vec1<FileKey>)) {
        self.entry(key).or_insert(value);
    }
}

impl DuplicateProvidersMap for OverlayDuplicateProviders {
    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn remove_provider(&mut self, key: &FlowSmolStr) {
        self.remove(key);
    }

    fn insert_provider_if_absent(&mut self, key: FlowSmolStr, value: (FileKey, Vec1<FileKey>)) {
        self.insert_if_absent(key, value);
    }
}

fn commit_modules<M: DuplicateProvidersMap>(
    pool: &ThreadPool,
    options: &Arc<Options>,
    transaction: &Arc<Transaction>,
    mut duplicate_providers: M,
    dirty_modules: flow_common_modulename::ModulenameSet,
) -> (flow_common_modulename::ModulenameSet, M) {
    with_memory_timer(options, "CommitModules", || {
        if !duplicate_providers.is_empty() {
            for m in dirty_modules.iter() {
                match m {
                    Modulename::Haste(m) => {
                        duplicate_providers.remove_provider(m.module_name());
                    }
                    Modulename::Filename(_) => {}
                }
            }
        }
        let (changed_modules, new_duplicate_providers) =
            flow_services_module::commit_modules(pool, options, transaction, dirty_modules);
        for (key, value) in new_duplicate_providers {
            duplicate_providers.insert_provider_if_absent(key, value);
        }
        (changed_modules, duplicate_providers)
    })
}

fn resolve_requires_impl(
    pool: &ThreadPool,
    transaction: &Arc<Transaction>,
    options: &Arc<Options>,
    parsed: &FlowOrdSet<FileKey>,
    cancelable: bool,
) -> Result<(), RecheckError> {
    with_memory_timer(options, "ResolveRequires", || {
        let node_modules_containers =
            Arc::new(files::node_modules_containers.read().unwrap().clone());
        if cancelable {
            check_recheck_canceled()?;
        }
        let parsed_files: Vec<FileKey> = parsed.iter().map(|f| f.dupe()).collect();
        let next = flow_utils_concurrency::map_reduce::make_next(
            pool.num_workers(),
            None::<fn(i32, i32, i32)>,
            None,
            parsed_files,
        );
        let options_clone = options.dupe();
        let transaction_clone = transaction.dupe();
        flow_utils_concurrency::map_reduce::iter(pool, next, move |batch| {
            for file in batch {
                if cancelable && worker_cancel::should_cancel() {
                    return;
                }
                let Ok(()) = flow_services_module::add_parsed_resolved_requires(
                    &options_clone,
                    &transaction_clone,
                    &node_modules_containers,
                    &file,
                ) else {
                    continue;
                };
            }
        });
        if cancelable {
            check_recheck_canceled()?;
        }
        Ok(())
    })
}

fn resolve_requires(
    pool: &ThreadPool,
    transaction: &Arc<Transaction>,
    options: &Arc<Options>,
    parsed: &FlowOrdSet<FileKey>,
) {
    resolve_requires_impl(pool, transaction, options, parsed, false)
        .expect("non-recheck resolve requires should not be canceled");
}

fn resolve_requires_for_recheck(
    pool: &ThreadPool,
    transaction: &Arc<Transaction>,
    options: &Arc<Options>,
    parsed: &FlowOrdSet<FileKey>,
) -> Result<(), RecheckError> {
    resolve_requires_impl(pool, transaction, options, parsed, true)
}

fn error_set_of_internal_error(
    file: FileKey,
    (loc, internal_error): (ALoc, InternalError),
) -> ErrorSet {
    let msg = ErrorMessage::EInternal(Box::new((loc, internal_error)));
    let err = flow_error::error_of_msg(file, msg);
    ErrorSet::singleton(err)
}

pub struct MergeResult {
    pub suppressions: ErrorSuppressions,
    pub skipped_count: usize,
    pub sig_new_or_changed: FlowOrdSet<FileKey>,
    pub top_cycle: Option<(FileKey, usize)>,
    pub calc_deps_time: std::time::Duration,
    pub time_to_merge: std::time::Duration,
}

fn calc_deps_impl(
    options: &Options,
    components: Vec<Vec1<FileKey>>,
    to_merge: &FlowOrdSet<FileKey>,
    cancelable: bool,
) -> Result<Vec<Vec1<FileKey>>, RecheckError> {
    with_memory_timer(options, "CalcDeps", || {
        let mut acc = Vec::new();
        for component in components {
            if cancelable {
                check_recheck_canceled()?;
            }
            if component.iter().any(|f| to_merge.contains(f)) {
                acc.push(component);
            }
        }
        Ok(acc)
    })
}

pub fn calc_deps(
    options: &Options,
    components: Vec<Vec1<FileKey>>,
    to_merge: &FlowOrdSet<FileKey>,
) -> Vec<Vec1<FileKey>> {
    calc_deps_impl(options, components, to_merge, false)
        .expect("non-recheck dependency calculation should not be canceled")
}

fn calc_deps_for_recheck(
    options: &Options,
    components: Vec<Vec1<FileKey>>,
    to_merge: &FlowOrdSet<FileKey>,
) -> Result<Vec<Vec1<FileKey>>, RecheckError> {
    calc_deps_impl(options, components, to_merge, true)
}

fn include_dependencies_and_dependents_impl<IG, SG>(
    options: &Options,
    input: CheckedSet,
    unchanged_checked: CheckedSet,
    all_dependent_files: FlowOrdSet<FileKey>,
    implementation_dependency_graph: &IG,
    sig_dependency_graph: &SG,
    cancelable: bool,
) -> Result<
    (
        CheckedSet,
        CheckedSet,
        Vec<Vec1<FileKey>>,
        FlowOrdSet<FileKey>,
    ),
    RecheckError,
>
where
    IG: GraphLike<FileKey> + ?Sized,
    SG: GraphLike<FileKey> + ?Sized,
{
    with_memory_timer(options, "PruneDeps", || {
        if cancelable {
            check_recheck_canceled()?;
        }
        let mut to_check = input.dupe();
        to_check.add(None, Some(all_dependent_files.dupe()), None);

        let to_check_all = to_check.dupe().all();

        let preliminary_dependencies = pure_dep_graph_operations::calc_direct_dependencies(
            implementation_dependency_graph,
            &to_check_all,
        );

        let components = topsort(
            preliminary_dependencies.iter().map(|f| f.dupe()),
            sig_dependency_graph,
        );

        let mut dependencies = FlowOrdSet::new();
        for component in &components {
            if cancelable {
                check_recheck_canceled()?;
            }
            let all_in_unchanged_checked = component
                .iter()
                .all(|filename| unchanged_checked.mem(filename));

            if !all_in_unchanged_checked {
                for file in component {
                    dependencies.insert(file.dupe());
                }
            }
        }

        let mut definitely_to_merge = input.dupe();
        definitely_to_merge.add(None, None, Some(dependencies.dupe()));

        let mut to_merge = definitely_to_merge.dupe();
        to_merge.add(None, Some(all_dependent_files), None);

        let recheck_set = definitely_to_merge.all();

        Ok((to_merge, to_check, components, recheck_set))
    })
}

pub fn include_dependencies_and_dependents<IG, SG>(
    options: &Options,
    input: CheckedSet,
    unchanged_checked: CheckedSet,
    all_dependent_files: FlowOrdSet<FileKey>,
    implementation_dependency_graph: &IG,
    sig_dependency_graph: &SG,
) -> (
    CheckedSet,
    CheckedSet,
    Vec<Vec1<FileKey>>,
    FlowOrdSet<FileKey>,
)
where
    IG: GraphLike<FileKey> + ?Sized,
    SG: GraphLike<FileKey> + ?Sized,
{
    include_dependencies_and_dependents_impl(
        options,
        input,
        unchanged_checked,
        all_dependent_files,
        implementation_dependency_graph,
        sig_dependency_graph,
        false,
    )
    .expect("non-recheck dependency pruning should not be canceled")
}

fn include_dependencies_and_dependents_for_recheck<IG, SG>(
    options: &Options,
    input: CheckedSet,
    unchanged_checked: CheckedSet,
    all_dependent_files: FlowOrdSet<FileKey>,
    implementation_dependency_graph: &IG,
    sig_dependency_graph: &SG,
) -> Result<
    (
        CheckedSet,
        CheckedSet,
        Vec<Vec1<FileKey>>,
        FlowOrdSet<FileKey>,
    ),
    RecheckError,
>
where
    IG: GraphLike<FileKey> + ?Sized,
    SG: GraphLike<FileKey> + ?Sized,
{
    include_dependencies_and_dependents_impl(
        options,
        input,
        unchanged_checked,
        all_dependent_files,
        implementation_dependency_graph,
        sig_dependency_graph,
        true,
    )
}

fn update_first_internal_error(
    first_internal_error: Option<String>,
    (loc, internal_error): &(ALoc, InternalError),
) -> Option<String> {
    match first_internal_error {
        Some(_) => first_internal_error,
        None => Some(format!(
            "{}\n{}",
            loc.debug_to_string(true),
            flow_typing_errors::error_message::string_of_internal_error(internal_error),
        )),
    }
}

fn add_internal_error<M: ErrSetMap>(errors: &mut M, file: FileKey, err: (ALoc, InternalError)) {
    let new_errors = error_set_of_internal_error(file.dupe(), err);
    update_errset(errors, file, new_errors);
}

fn update_merge_results(
    mut acc: ErrorSuppressions,
    result: Option<(ErrorSuppressions, f64)>,
) -> ErrorSuppressions {
    match result {
        None => acc,
        Some((suppressions, _duration)) => {
            acc.update_suppressions(suppressions);
            acc
        }
    }
}

fn update_slow_files(
    acc: (i32, f64, Option<FileKey>),
    file: FileKey,
    check_time: f64,
) -> (i32, f64, Option<FileKey>) {
    if check_time > 1.0 {
        let (num_slow_files, slowest_time, slowest_file) = acc;
        let (slowest_time, slowest_file) = if check_time > slowest_time {
            (check_time, Some(file))
        } else {
            (slowest_time, slowest_file)
        };
        (num_slow_files + 1, slowest_time, slowest_file)
    } else {
        acc
    }
}

type FindRefResults = Result<FindRefsFound, String>;

type CheckResult = Result<
    Option<(
        ErrorSet,
        ErrorSet,
        ErrorSuppressions,
        flow_services_coverage::FileCoverage,
        Result<Vec<flow_services_references::find_refs_types::SingleRef>, String>,
        f64,
    )>,
    (ALoc, InternalError),
>;

type CheckAcc = (
    (
        OverlayErrorMap,
        OverlayErrorMap,
        ErrorSuppressions,
        OverlayCoverage,
        FindRefResults,
        Option<String>,
    ),
    (i32, f64, Option<FileKey>),
);

fn update_check_results(mut acc: CheckAcc, (file, result): (FileKey, CheckResult)) -> CheckAcc {
    let (
        (
            ref mut errors,
            ref mut warnings,
            ref mut suppressions,
            ref mut coverage,
            ref mut find_ref_results,
            ref mut first_internal_error,
        ),
        ref mut slow_files,
    ) = acc;
    match result {
        Ok(None) => {}
        Ok(Some((
            new_errors,
            new_warnings,
            new_suppressions,
            new_coverage,
            new_find_ref_results,
            check_time,
        ))) => {
            errors.remove(&file);
            warnings.remove(&file);
            suppressions.remove(&file);
            coverage.remove(&file);
            update_errset(errors, file.dupe(), new_errors);
            update_errset(warnings, file.dupe(), new_warnings);
            suppressions.update_suppressions(new_suppressions);
            coverage.insert(file.dupe(), new_coverage);
            *find_ref_results = match (
                std::mem::replace(find_ref_results, Ok(Vec::new())),
                new_find_ref_results,
            ) {
                (Ok(mut a), Ok(b)) => {
                    a.extend(b);
                    Ok(a)
                }
                (Err(a), Ok(_)) => Err(a),
                (Ok(_), Err(b)) => Err(b),
                (Err(a), Err(_)) => Err(a),
            };
            *slow_files = update_slow_files(std::mem::take(slow_files), file, check_time);
        }
        Err(e) => {
            errors.remove(&file);
            warnings.remove(&file);
            suppressions.remove(&file);
            coverage.remove(&file);
            *first_internal_error = update_first_internal_error(first_internal_error.take(), &e);
            add_internal_error(errors, file, e);
            *find_ref_results = Err(String::new());
        }
    }
    acc
}

fn run_merge_service(
    pool: &ThreadPool,
    transaction: &Arc<Transaction>,
    options: &Arc<Options>,
    for_find_all_refs: bool,
    sig_dependency_graph: &(impl GraphLike<FileKey> + ?Sized),
    components: Vec<Vec1<FileKey>>,
    recheck_set: &FlowOrdSet<FileKey>,
    suppressions: ErrorSuppressions,
) -> Result<(ErrorSuppressions, usize, FlowOrdSet<FileKey>), RecheckError> {
    with_memory_timer(options, "Merge", || {
        check_recheck_canceled()?;
        let (results, sig_opts_data) = merge_service::merge(
            pool,
            transaction,
            options,
            for_find_all_refs,
            sig_dependency_graph,
            components,
            recheck_set,
        )
        .map_err(|_| RecheckError::Canceled(Vec::new()))?;

        let suppressions = results.into_iter().fold(suppressions, update_merge_results);

        check_recheck_canceled()?;
        Ok((
            suppressions,
            sig_opts_data.skipped_count,
            sig_opts_data.sig_new_or_changed,
        ))
    })
}

fn merge(
    pool: &ThreadPool,
    transaction: &Arc<Transaction>,
    options: &Arc<Options>,
    for_find_all_refs: bool,
    to_merge: &CheckedSet,
    components: Vec<Vec1<FileKey>>,
    recheck_set: &FlowOrdSet<FileKey>,
    sig_dependency_graph: &(impl GraphLike<FileKey> + ?Sized),
    suppressions: ErrorSuppressions,
) -> Result<MergeResult, RecheckError> {
    if !options.quiet {
        flow_hh_logger::info!("to_merge: {}", to_merge.debug_counts_to_string());
        flow_hh_logger::info!("Calculating dependencies");
    }
    monitor_rpc::status_update(server_status::Event::CalculatingDependenciesProgress);
    let files_to_merge = to_merge.dupe().all();
    let calc_deps_start = Instant::now();
    let components = calc_deps_for_recheck(options, components, &files_to_merge)?;
    let calc_deps_time = calc_deps_start.elapsed();
    check_recheck_canceled()?;

    if !options.quiet {
        flow_hh_logger::info!("Merging");
    }
    let merge_start = Instant::now();

    let top_cycle = components
        .iter()
        .filter(|c: &&Vec1<FileKey>| c.len() > 1)
        .max_by_key(|c: &&Vec1<FileKey>| c.len())
        .map(|c: &Vec1<FileKey>| (c.first().dupe(), c.len()));

    let (suppressions, skipped_count, sig_new_or_changed) = run_merge_service(
        pool,
        transaction,
        options,
        for_find_all_refs,
        sig_dependency_graph,
        components,
        recheck_set,
        suppressions,
    )?;

    if options.profile && !options.quiet {
        with_memory_timer(options, "PrintGCStats", || {});
    }

    if !options.quiet {
        flow_hh_logger::info!("Merging Done");
    }
    let time_to_merge = merge_start.elapsed();

    Ok(MergeResult {
        suppressions,
        skipped_count,
        sig_new_or_changed,
        top_cycle,
        calc_deps_time,
        time_to_merge,
    })
}

mod check_files {
    use super::*;

    pub fn check_files(
        options: Arc<Options>,
        pool: &ThreadPool,
        transaction: &Arc<Transaction>,
        all_unordered_libs: Arc<BTreeSet<FlowSmolStr>>,
        find_ref_request: &flow_services_references::find_refs_types::Request,
        errors: OverlayErrors,
        updated_suppressions: ErrorSuppressions,
        coverage: OverlayCoverage,
        to_check: CheckedSet,
        dirty_direct_dependents: FlowOrdSet<FileKey>,
        sig_new_or_changed: FlowOrdSet<FileKey>,
        implementation_dependency_graph: &(impl GraphLike<FileKey> + ?Sized),
        master_cx: Arc<MasterContext>,
    ) -> Result<
        (
            OverlayErrors,
            OverlayCoverage,
            FindRefResults,
            f64,
            usize,
            Option<String>,
            i32,
            Option<String>,
        ),
        RecheckError,
    > {
        let options_ref = options.dupe();
        let quiet = options.quiet;
        with_memory_timer(&options_ref, "Check", || {
            if !quiet {
                flow_hh_logger::info!("Check prep");
                flow_hh_logger::info!("new or changed signatures: {}", sig_new_or_changed.len());
            }
            let focused_to_check = to_check.focused();
            let merged_dependents = to_check.dependents();
            let mut skipped_count = 0;
            let dependents_to_check = merged_dependents
                .iter()
                .filter_map(|file| {
                    let keep = dirty_direct_dependents.contains(file)
                        || implementation_dependency_graph
                            .find_opt(file)
                            .is_some_and(|deps| {
                                deps.iter().any(|f| sig_new_or_changed.contains(f))
                            });
                    if keep {
                        Some(file.dupe())
                    } else {
                        skipped_count += 1;
                        None
                    }
                })
                .collect::<Vec<_>>();
            let message = format!(
                "Check will skip {} of {} files",
                skipped_count,
                focused_to_check.len() + merged_dependents.len()
            );
            flow_hh_logger::info!("{}", message);
            let mut files = focused_to_check.dupe();
            for file in dependents_to_check {
                files.insert(file);
            }
            let intermediate_result_callback: Arc<dyn Fn(&[_]) + Send + Sync> = Arc::new(|_| {});
            if !quiet {
                flow_hh_logger::info!("Checking files");
            }

            let check_start_time = Instant::now();
            let max_size = options.max_files_checked_per_worker as usize;
            let num_workers = pool.num_workers();
            let (next, mk_next_merge, files_completed) = job_utils::mk_next(
                intermediate_result_callback,
                quiet,
                max_size,
                num_workers,
                files.iter().map(|f| f.dupe()).collect(),
            );
            type WorkerState = merge_service::Checker;
            thread_local! {
                static WORKER_CHECK: RefCell<Option<WorkerState>> = const { RefCell::new(None) };
            }
            let mk_check: Arc<dyn Fn() -> WorkerState + Send + Sync> = {
                let transaction = transaction.dupe();
                let options = options.dupe();
                let all_unordered_libs = all_unordered_libs.dupe();
                let find_ref_request = find_ref_request.clone();
                Arc::new(move || {
                    merge_service::mk_check(
                        transaction.dupe(),
                        options.dupe(),
                        all_unordered_libs.dupe(),
                        master_cx.as_ref(),
                        find_ref_request.clone(),
                    )
                })
            };
            let num_workers = pool.num_workers();
            let (deque_slots, stealers) = {
                let deques: Vec<crossbeam::deque::Worker<FileKey>> = (0..num_workers)
                    .map(|_| crossbeam::deque::Worker::new_fifo())
                    .collect();
                let stealers: Vec<crossbeam::deque::Stealer<FileKey>> =
                    deques.iter().map(|d| d.stealer()).collect();
                let slots: Vec<Option<crossbeam::deque::Worker<FileKey>>> =
                    deques.into_iter().map(Some).collect();
                (
                    Arc::new(flow_utils_concurrency::lock::Mutex::new(slots)),
                    Arc::new(stealers),
                )
            };
            thread_local! {
                static WORKER_DEQUE: RefCell<Option<crossbeam::deque::Worker<FileKey>>> =
                    const { RefCell::new(None) };
            }

            let deque_slots_for_job = deque_slots.dupe();
            let mk_check_for_steal = mk_check.dupe();
            let stealers_for_steal = stealers.dupe();

            type StealItem = (FileKey, CheckResult);
            #[derive(Debug)]
            struct StealAcc(
                Result<Vec<StealItem>, flow_utils_concurrency::worker_cancel::WorkerCanceled>,
            );
            impl Default for StealAcc {
                fn default() -> Self {
                    StealAcc(Ok(Vec::new()))
                }
            }

            let options_for_job = options.dupe();
            let ret = flow_utils_concurrency::map_reduce::call_with_stealing(
                pool,
                next,
                move |acc: &mut StealAcc, batch| {
                    if acc.0.is_err() {
                        return;
                    }
                    WORKER_CHECK.with(|cell: &RefCell<Option<WorkerState>>| {
                        let mut opt = cell.borrow_mut();
                        if opt.is_none() {
                            *opt = Some(mk_check());
                        }
                        let checker = opt.as_mut().unwrap();
                        checker.clear_cache();

                        WORKER_DEQUE.with(|deque_cell| {
                            let mut deque_opt = deque_cell.borrow_mut();
                            if deque_opt.is_none() {
                                let mut slots = deque_slots_for_job.lock();
                                *deque_opt = slots.iter_mut().find_map(|s| s.take());
                            }
                            let deque = deque_opt.as_ref().unwrap();

                            for file in batch {
                                deque.push(file);
                            }

                            match job_utils::mk_job_stealing(
                                &mut |file| checker.check(file, |_, r| r),
                                &options_for_job,
                                deque,
                            ) {
                                Ok(results) => {
                                    let acc_vec = acc.0.as_mut().unwrap();
                                    mk_next_merge(acc_vec, results);
                                }
                                Err(c) => {
                                    acc.0 = Err(c);
                                }
                            }
                        });
                        checker.clear_cache();
                    });
                },
                |a: &mut StealAcc, b: StealAcc| match (&mut a.0, b.0) {
                    (Err(_), _) => {}
                    (a_slot @ Ok(_), Err(c)) => {
                        *a_slot = Err(c);
                    }
                    (Ok(a_vec), Ok(b_vec)) => {
                        a_vec.extend(b_vec);
                    }
                },
                move |acc: &mut StealAcc| -> bool {
                    if acc.0.is_err() {
                        return false;
                    }
                    for stealer in stealers_for_steal.iter() {
                        if let crossbeam::deque::Steal::Success(file) = stealer.steal() {
                            WORKER_CHECK.with(|cell: &RefCell<Option<WorkerState>>| {
                                let mut opt = cell.borrow_mut();
                                if opt.is_none() {
                                    *opt = Some(mk_check_for_steal());
                                }
                                let checker = opt.as_mut().unwrap();
                                match checker.check(file.dupe(), |_, r| r) {
                                    merge_service::CheckJobOutcome::Ok(r) => {
                                        acc.0.as_mut().unwrap().push((file, Ok(r)));
                                    }
                                    merge_service::CheckJobOutcome::PerFileError(e) => {
                                        acc.0.as_mut().unwrap().push((file, Err(e)));
                                    }
                                    merge_service::CheckJobOutcome::Canceled(c) => {
                                        acc.0 = Err(c);
                                    }
                                }
                            });
                            files_completed.fetch_add(1, std::sync::atomic::Ordering::Release);
                            return true;
                        }
                    }
                    false
                },
            );
            pool.broadcast(move |_| {
                WORKER_CHECK.with(|cell| {
                    *cell.borrow_mut() = None;
                });
                WORKER_DEQUE.with(|cell| {
                    *cell.borrow_mut() = None;
                });
            });
            let ret_vec = match ret.0 {
                Ok(v) => v,
                Err(_) => return Err(RecheckError::Canceled(Vec::new())),
            };
            let OverlayErrors {
                merge_errors,
                warnings,
                local_errors,
                duplicate_providers,
                suppressions: _,
                base_cell,
            } = errors;
            let (
                (
                    merge_errors,
                    warnings,
                    suppressions,
                    coverage,
                    find_ref_results,
                    first_internal_error,
                ),
                slow_files,
            ) = ret_vec.into_iter().fold(
                (
                    (
                        merge_errors,
                        warnings,
                        updated_suppressions,
                        coverage,
                        Ok(Vec::new()),
                        None,
                    ),
                    (0, 0.0, None),
                ),
                update_check_results,
            );
            let (num_slow_files, _slowest_time, slowest_file) = slow_files;
            let time_to_check_merged = check_start_time.elapsed().as_secs_f64();

            if !quiet {
                flow_hh_logger::info!("Checking Done");
            }
            let errors = OverlayErrors {
                base_cell,
                local_errors,
                duplicate_providers,
                merge_errors,
                warnings,
                suppressions,
            };
            Ok((
                errors,
                coverage,
                find_ref_results,
                time_to_check_merged,
                skipped_count,
                slowest_file.map(|file| file.as_str().to_owned()),
                num_slow_files,
                first_internal_error.map(|msg| format!("First check internal error:\n{}", msg)),
            ))
        })
    }
}

#[derive(Debug)]
pub(crate) struct UnexpectedFileChanges(Vec<FileKey>);

fn handle_unexpected_file_changes(
    orchestrator: Option<&ServerOrchestratorHandle>,
    changed_files: Vec<FileKey>,
) -> RecheckError {
    let filename_set: BTreeSet<String> = changed_files.iter().map(FileKey::to_absolute).collect();
    let file_count = filename_set.len();
    flow_hh_logger::info!(
        "Canceling recheck due to {} unexpected file changes",
        file_count
    );
    // `None` outside a server (codemods, `flow full-check`): there is no recheck loop to schedule
    // the files for, so cancelling is the whole story.
    if let Some(orchestrator) = orchestrator {
        flow_server_env::server_monitor_listener_state::push_files_to_prioritize(
            orchestrator.recheck(),
            filename_set,
        );
    }
    RecheckError::Canceled(changed_files)
}

fn ensure_parsed(
    pool: &ThreadPool,
    transaction: &Arc<Transaction>,
    options: &Arc<Options>,
    files: FlowOrdSet<FileKey>,
    progress: ParseProgress,
) -> Result<(), UnexpectedFileChanges> {
    with_memory_timer(options, "EnsureParsed", || {
        let parse_unexpected_skips = parsing_service::ensure_parsed(
            pool,
            transaction,
            options,
            files,
            move |total, start, _length| match progress {
                ParseProgress::Report => {
                    let finished = start;
                    monitor_rpc::status_update(server_status::Event::ParsingProgress(
                        server_status::Progress {
                            total: Some(total),
                            finished,
                        },
                    ));
                }
                ParseProgress::Silent => {}
            },
        );
        if parse_unexpected_skips.is_empty() {
            Ok(())
        } else {
            let skips: Vec<FileKey> = parse_unexpected_skips.into_iter().collect();
            Err(UnexpectedFileChanges(skips))
        }
    })
}

pub fn ensure_parsed_or_trigger_recheck(
    orchestrator: Option<&ServerOrchestratorHandle>,
    pool: &ThreadPool,
    transaction: &Arc<Transaction>,
    options: &Arc<Options>,
    files: FlowOrdSet<FileKey>,
    progress: ParseProgress,
) -> Result<(), RecheckError> {
    match ensure_parsed(pool, transaction, options, files, progress) {
        Ok(()) => Ok(()),
        Err(UnexpectedFileChanges(changed_files)) => {
            Err(handle_unexpected_file_changes(orchestrator, changed_files))
        }
    }
}

fn init_libs(
    options: &Arc<Options>,
    transaction: &Arc<Transaction>,
    all_unordered_libs: Arc<BTreeSet<FlowSmolStr>>,
    ordered_libs: Vec<init::OrderedLibInput>,
    local_errors: BTreeMap<FileKey, ErrorSet>,
    warnings: BTreeMap<FileKey, ErrorSet>,
    suppressions: ErrorSuppressions,
) -> (
    bool,
    BTreeMap<FileKey, ErrorSet>,
    BTreeMap<FileKey, ErrorSet>,
    ErrorSuppressions,
    flow_imports_exports::exports::Exports,
    Arc<MasterContext>,
) {
    with_memory_timer(options, "InitLibs", || {
        let init::InitResult {
            ok: libs_ok,
            errors: lib_errors,
            warnings: lib_warnings,
            suppressions: lib_suppressions,
            exports: lib_exports,
            master_cx,
        } = init::init(options, transaction, all_unordered_libs, ordered_libs);
        let local_errors = {
            let mut acc = lib_errors;
            for (file, errset) in local_errors {
                acc.entry(file).or_insert(errset);
            }
            acc
        };
        let warnings = {
            let mut acc = lib_warnings;
            for (file, errset) in warnings {
                acc.entry(file).or_insert(errset);
            }
            acc
        };
        let mut suppressions = suppressions;
        suppressions.update_suppressions(lib_suppressions);
        (
            libs_ok,
            local_errors,
            warnings,
            suppressions,
            lib_exports,
            master_cx,
        )
    })
}

fn filter_out_node_modules(options: &Options, files: &FlowOrdSet<FileKey>) -> FlowOrdSet<FileKey> {
    if options.node_modules_errors {
        return files.dupe();
    }
    let root = &options.root;
    let file_options = &options.file_options;

    let mut result = files.dupe();
    let is_in_node_modules = files::generate_is_within_node_modules_fn(root, file_options);
    for file in files {
        if is_in_node_modules(&file.to_absolute()) {
            result.remove(file);
        }
    }
    result
}

pub(crate) fn unfocused_files_to_infer(
    options: &Options,
    input_focused: &FlowOrdSet<FileKey>,
    input_dependencies: FlowOrdSet<FileKey>,
) -> CheckedSet {
    let focused = filter_out_node_modules(options, input_focused);
    let mut checked_set = CheckedSet::empty();
    checked_set.add(Some(focused), None, Some(input_dependencies));
    checked_set
}

pub(crate) fn files_to_infer(options: &Options, parsed: FlowOrdSet<FileKey>) -> CheckedSet {
    with_memory_timer(options, "FilesToInfer", || {
        unfocused_files_to_infer(options, &parsed, FlowOrdSet::new())
    })
}

pub(crate) fn restart_if_faster_than_recheck(
    options: &Options,
    checked_files: &CheckedSet,
    to_merge: &CheckedSet,
) -> Result<(), RecheckError> {
    let files_already_checked = checked_files.cardinal();
    let files_to_merge = to_merge.cardinal();
    let files_to_check = to_merge.focused_cardinal() + to_merge.dependents_cardinal();
    flow_hh_logger::info!(
        "We've already checked {} files. About to merge {} files and check {} files",
        files_already_checked,
        files_to_merge,
        files_to_check
    );
    let init_time = recheck_stats::get_init_time();
    let per_merge_file_time = recheck_stats::get_per_merge_file_time();
    let per_check_file_time = recheck_stats::get_per_check_file_time();
    let time_to_restart = init_time;
    let time_to_recheck = (per_merge_file_time * files_to_merge as f64)
        + (per_check_file_time * files_to_check as f64);
    let per_file_time = per_merge_file_time + per_check_file_time;
    let estimates = recheck_stats::Estimates {
        estimated_time_to_recheck: time_to_recheck,
        estimated_time_to_restart: time_to_restart,
        estimated_time_to_init: init_time,
        estimated_time_per_file: per_file_time,
        estimated_files_to_recheck: files_to_merge as i64,
        estimated_files_to_init: files_already_checked as i64,
    };
    flow_hh_logger::debug!(
        "Estimated restart time: {}s (init from saved state)",
        time_to_restart
    );
    flow_hh_logger::debug!(
        "Estimated recheck time: {}s * {} merge files + {}s * {} check files = {}s",
        per_merge_file_time,
        files_to_merge,
        per_check_file_time,
        files_to_check,
        time_to_recheck
    );
    flow_hh_logger::info!(
        "Estimating a recheck would take {:.2}s and a restart would take {:.2}s",
        time_to_recheck,
        time_to_restart
    );
    if time_to_restart < time_to_recheck {
        flow_hh_logger::info!("Recheck too slow: choosing to REINIT from saved state");
        recheck_stats::record_last_estimates(options, &estimates);
        Err(RecheckError::TooSlow)
    } else {
        flow_hh_logger::info!("Recheck fast enough: choosing to RECHECK incrementally");
        Ok(())
    }
}

pub struct DetermineWhatToRecheckResult {
    pub to_merge: CheckedSet,
    pub to_check: CheckedSet,
    pub components: Vec<Vec1<FileKey>>,
    pub recheck_set: FlowOrdSet<FileKey>,
    pub dependent_file_count: usize,
}

pub(crate) mod recheck {
    use super::*;

    pub struct RecheckResult {
        pub modified_count: usize,
        pub deleted_count: usize,
        pub dependent_file_count: usize,
        #[allow(dead_code)]
        pub to_merge: CheckedSet,
        pub to_check: CheckedSet,
        pub top_cycle: Option<(FileKey, usize)>,
        pub merge_skip_count: usize,
        pub check_skip_count: usize,
        pub slowest_file: Option<String>,
        pub num_slow_files: usize,
    }

    pub(crate) struct IntermediateValues {
        pub modified_count: usize,
        pub deleted_count: usize,
        pub dirty_direct_dependents: FlowOrdSet<FileKey>,
        pub errors: OverlayErrors,
        pub collated_errors: flow_server_env::collated_errors::OverlayCollatedErrors,
        pub freshparsed: CheckedSet,
        pub unchanged_checked: CheckedSet,
        pub unchanged_files_to_force: CheckedSet,
        pub unchanged_files_to_upgrade: CheckedSet,
    }

    pub(crate) fn recheck_parse_and_update_dependency_info(
        orchestrator: Option<&ServerOrchestratorHandle>,
        pool: &ThreadPool,
        transaction: &Arc<Transaction>,
        options: &Arc<Options>,
        updates: &CheckedSet,
        side_effects: &mut side_effect_transaction::Transaction,
        def_info: &DefInfo,
        files_to_force: CheckedSet,
        env: &mut EnvTransaction,
    ) -> Result<IntermediateValues, RecheckError> {
        check_recheck_canceled()?;
        let loc_of_aloc = |loc: &ALoc| -> Loc { transaction.loc_of_aloc(loc) };
        let transaction_for_ast = transaction.dupe();
        let get_ast = move |file: &FileKey| -> Option<Arc<Program<Loc, Loc>>> {
            transaction_for_ast.get_ast(file)
        };
        let errors = env.take_errors();
        let collated_errors = env.take_collated_errors();
        let mut files_to_force = files_to_force;
        files_to_force.diff(env.checked_files());

        flow_hh_logger::info!("Parsing");
        monitor_rpc::status_update(server_status::Event::ParsingProgress(
            server_status::Progress {
                total: None,
                finished: 0,
            },
        ));

        let modified_set = updates.dupe().all();
        let modified_files: Vec<FileKey> = modified_set.iter().map(|f| f.dupe()).collect();
        let modified_next: parsing_service::Next = parsing_service::next_of_filename_set(
            pool,
            modified_files,
            Some(|total: i32, start: i32, _length: i32| {
                let finished = start;
                monitor_rpc::status_update(server_status::Event::ParsingProgress(
                    server_status::Progress {
                        total: Some(total),
                        finished,
                    },
                ));
            }),
        );
        let CollatedParseResults {
            parsed: parsed_set,
            unparsed: unparsed_set,
            unchanged: unchanged_parse,
            not_found: deleted,
            dirty_modules,
            local_errors: new_local_errors,
            package_json: _,
            all_unordered_libs: discovered_libs,
            dts_file_kinds: _,
            ts_global_augmentation_files: _,
        } = reparse(pool, transaction, options, def_info, modified_next);
        check_recheck_canceled()?;

        let changed_files: Vec<FileKey> = modified_set
            .iter()
            .filter(|file| !unchanged_parse.contains(file))
            .map(|file| file.dupe())
            .collect();
        let transaction_for_reparse_commit = transaction.dupe();
        side_effect_transaction::add(
            side_effects,
            move || {
                flow_hh_logger::info!("Committing reparse");
                transaction_for_reparse_commit
                    .committed_heap()
                    .remove_reader_cache_batch(&changed_files);
            },
            || {
                flow_hh_logger::info!("Rolling back reparse");
            },
        );

        let new_or_changed = parsed_set.dupe().union(unparsed_set.dupe());
        let new_or_changed_or_deleted = new_or_changed.dupe().union(deleted.dupe());

        let mut all_unordered_libs = env.configured_libs().clone();
        for file in &new_or_changed_or_deleted {
            all_unordered_libs.remove(file.to_absolute().as_str());
        }
        all_unordered_libs.extend(discovered_libs);
        env.set_configured_global_libs(all_unordered_libs);

        let freshparsed = updates.filter(|file, _kind| parsed_set.contains(file));

        let OverlayErrors {
            base_cell,
            local_errors,
            duplicate_providers,
            merge_errors,
            warnings,
            suppressions,
        } = clear_errors(&new_or_changed_or_deleted, errors);

        let mut collated_errors = collated_errors;
        {
            collated_errors.clear_all(&new_or_changed_or_deleted);
            let unsuppressable_error_codes: BTreeSet<FlowSmolStr> =
                options.unsuppressable_error_codes.iter().duped().collect();
            error_collator::update_local_collated_errors(
                &loc_of_aloc,
                &get_ast,
                &|file| env.is_lib_file(file),
                &options.root,
                &options.file_options,
                options.node_modules_errors,
                &unsuppressable_error_codes,
                &suppressions,
                &new_local_errors,
                &mut collated_errors,
            );
        }

        let mut coverage = env.take_coverage();
        for file in &new_or_changed_or_deleted {
            coverage.remove(file);
        }

        let local_errors = merge_error_maps(local_errors, new_local_errors);

        let old_parsed = env.files().dupe();
        let old_parsed_count = old_parsed.len();
        let unchanged = FlowOrdSet::from(
            old_parsed
                .into_inner()
                .relative_complement(new_or_changed_or_deleted.into_inner()),
        );

        let deleted_count = deleted.len();
        let modified_count = new_or_changed.len();
        if deleted_count + modified_count > 0 {
            flow_hh_logger::info!(
                "recheck {} modified, {} deleted files",
                modified_count,
                deleted_count
            );
            let log_files = |files: &FlowOrdSet<FileKey>, msg: &str, n: usize| {
                flow_hh_logger::info!("{} files:", msg);
                for (i, f) in files.iter().enumerate() {
                    let i = i + 1;
                    let cap = 500;
                    if i <= cap {
                        flow_hh_logger::info!("{}/{}: {}", i, n, f.as_str());
                    } else if flow_hh_logger::level::passes_min_level(flow_hh_logger::Level::Debug)
                    {
                        flow_hh_logger::debug!("{}/{}: {}", i, n, f.as_str());
                    } else if i == cap + 1 {
                        flow_hh_logger::info!("...");
                    }
                }
            };
            if modified_count > 0 {
                log_files(&new_or_changed, "modified", modified_count);
            }
            if deleted_count > 0 {
                log_files(&deleted, "deleted", deleted_count);
            }
        }

        // old_parsed and unchanged sets are large and `cardinal` is O(n), so avoid
        // this call unless we are in debug mode.
        if flow_hh_logger::level::passes_min_level(flow_hh_logger::Level::Debug) {
            flow_hh_logger::debug!(
                "recheck: old = {}, del = {}, fresh = {}, unmod = {}",
                old_parsed_count,
                deleted.len(),
                freshparsed.cardinal(),
                unchanged.len(),
            );
        }

        let unchanged_files_to_upgrade = updates.filter(|file, kind| {
            !CheckedSet::is_dependency(kind)
                && unchanged_parse.contains(file)
                && env.checked_files().mem_dependency(file)
        });

        // TODO: Is OLD_M the same as NEW_M?

        let new_or_changed_or_deleted_for_remove = parsed_set
            .dupe()
            .union(unparsed_set.dupe())
            .union(deleted.dupe());
        let mut unchanged_checked = env.checked_files().dupe();
        unchanged_checked.remove(&new_or_changed_or_deleted_for_remove);

        let unchanged_files_to_force =
            files_to_force.filter(|file, _kind| unchanged.contains(file));

        monitor_rpc::status_update(server_status::Event::ResolvingDependenciesProgress);
        let dirty_modules: flow_common_modulename::ModulenameSet =
            dirty_modules.into_iter().collect();
        let (changed_modules, duplicate_providers) = commit_modules(
            pool,
            options,
            transaction,
            duplicate_providers,
            dirty_modules,
        );
        check_recheck_canceled()?;

        let unparsed_or_deleted: BTreeSet<FileKey> = {
            let mut set: BTreeSet<FileKey> = unparsed_set.iter().duped().collect();
            for f in &deleted {
                set.insert(f.dupe());
            }
            set
        };

        let dirty_direct_dependents: FlowOrdSet<FileKey> =
            with_memory_timer(options, "DirectDependentFiles", || {
                let dirty_direct_dependents_btree =
                    dep_service::calc_unchanged_dependents(transaction, None, changed_modules);
                dirty_direct_dependents_btree.into_iter().collect()
            });

        flow_hh_logger::info!("Re-resolving parsed and directly dependent files");
        let dirty_direct_dependents_set: FlowOrdSet<FileKey> = dirty_direct_dependents.dupe();
        ensure_parsed(
            pool,
            transaction,
            options,
            dirty_direct_dependents_set,
            ParseProgress::Report,
        )
        .map_err(|UnexpectedFileChanges(changed_files)| {
            handle_unexpected_file_changes(orchestrator, changed_files)
        })?;
        let parsed_set_for_resolve = parsed_set.dupe().union(dirty_direct_dependents.dupe());
        resolve_requires_for_recheck(pool, transaction, options, &parsed_set_for_resolve)?;
        check_recheck_canceled()?;

        flow_hh_logger::info!("Recalculating dependency graph");
        monitor_rpc::status_update(server_status::Event::CalculatingDependenciesProgress);
        let parsed = parsed_set.dupe().union(unchanged.dupe());
        with_memory_timer(options, "CalcDepsTypecheck", || {
            let files_to_update_dependency_info = parsed_set.union(dirty_direct_dependents.dupe());
            let partial_dependency_graph = dep_service::calc_partial_dependency_graph(
                pool,
                transaction,
                &files_to_update_dependency_info,
                &parsed,
            );
            env.dependency_info_mut()
                .update(partial_dependency_graph, unparsed_or_deleted);
        });
        check_recheck_canceled()?;

        let to_remove = parsed.dupe().union(deleted);
        let unparsed = FlowOrdSet::from(
            env.unparsed()
                .dupe()
                .into_inner()
                .relative_complement(to_remove.into_inner()),
        )
        .union(unparsed_set);

        env.set_files(parsed);
        env.set_unparsed(unparsed);
        env.set_coverage(coverage);

        let errors = OverlayErrors {
            base_cell,
            local_errors,
            duplicate_providers,
            merge_errors,
            warnings,
            suppressions,
        };

        let intermediate_values = IntermediateValues {
            modified_count,
            deleted_count,
            dirty_direct_dependents,
            errors,
            collated_errors,
            freshparsed,
            unchanged_checked,
            unchanged_files_to_force,
            unchanged_files_to_upgrade,
        };
        Ok(intermediate_values)
    }

    pub(crate) fn determine_what_to_recheck<SG, IG>(
        options: &Options,
        sig_dependency_graph: &SG,
        implementation_dependency_graph: &IG,
        freshparsed: &CheckedSet,
        unchanged_checked: CheckedSet,
        unchanged_files_to_force: &CheckedSet,
        dirty_direct_dependents: &FlowOrdSet<FileKey>,
    ) -> Result<DetermineWhatToRecheckResult, RecheckError>
    where
        SG: GraphLike<FileKey> + ?Sized,
        IG: GraphLike<FileKey> + ?Sized,
    {
        check_recheck_canceled()?;
        let input_focused = freshparsed
            .focused()
            .dupe()
            .union(unchanged_files_to_force.focused().dupe());
        let input_dependencies = freshparsed
            .dependencies()
            .dupe()
            .union(unchanged_files_to_force.dependencies().dupe());
        let all_dependent_files = with_memory_timer(options, "AllDependentFiles", || {
            let roots = input_focused.dupe().union(dirty_direct_dependents.dupe());
            let all_dependent_files = pure_dep_graph_operations::calc_all_dependents(
                sig_dependency_graph,
                implementation_dependency_graph,
                &roots,
            );
            filter_out_node_modules(options, &all_dependent_files)
        });
        check_recheck_canceled()?;
        let input = unfocused_files_to_infer(options, &input_focused, input_dependencies);
        let (to_merge, to_check, components, recheck_set) =
            include_dependencies_and_dependents_for_recheck(
                options,
                input,
                unchanged_checked.dupe(),
                all_dependent_files.dupe(),
                implementation_dependency_graph,
                sig_dependency_graph,
            )?;
        let dependent_file_count = all_dependent_files.len();
        Ok(DetermineWhatToRecheckResult {
            to_merge,
            to_check,
            components,
            recheck_set,
            dependent_file_count,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn recheck_merge(
        orchestrator: Option<&ServerOrchestratorHandle>,
        pool: &ThreadPool,
        transaction: &Arc<Transaction>,
        options: &Arc<Options>,
        find_ref_request: &flow_services_references::find_refs_types::Request,
        for_find_all_refs: bool,
        will_be_checked_files: &mut CheckedSet,
        changed_mergebase: Option<bool>,
        intermediate_values: IntermediateValues,
        env: &mut EnvTransaction,
    ) -> Result<
        (
            RecheckResult,
            Box<dyn FnOnce()>,
            FindRefResults,
            Option<String>,
        ),
        RecheckError,
    > {
        let IntermediateValues {
            modified_count,
            deleted_count,
            dirty_direct_dependents,
            errors,
            collated_errors,
            freshparsed,
            unchanged_checked,
            unchanged_files_to_force,
            unchanged_files_to_upgrade,
        } = intermediate_values;
        let coverage_for_check = env.take_coverage();
        let all_unordered_libs = Arc::new(env.configured_libs().clone());
        let dependency_info = env.take_dependency_info();
        let result = (|| {
            let implementation_dependency_graph = dependency_info.implementation_dependency_graph();
            let sig_dependency_graph = dependency_info.sig_dependency_graph();
            flow_hh_logger::info!("Determining what to recheck...");
            monitor_rpc::status_update(server_status::Event::CalculatingDependentsStart);
            let mut unchanged_files_to_force = unchanged_files_to_force;
            unchanged_files_to_force.union(unchanged_files_to_upgrade.dupe());
            let DetermineWhatToRecheckResult {
                to_merge,
                to_check,
                components,
                recheck_set,
                dependent_file_count,
            } = determine_what_to_recheck(
                options,
                sig_dependency_graph,
                implementation_dependency_graph,
                &freshparsed,
                unchanged_checked.dupe(),
                &unchanged_files_to_force,
                &dirty_direct_dependents,
            )?;
            monitor_rpc::status_update(server_status::Event::CalculatingDependentsEnd);
            will_be_checked_files.union(to_merge.dupe());

            match changed_mergebase {
                Some(true) if options.lazy_mode && options.estimate_recheck_time => {
                    restart_if_faster_than_recheck(options, env.checked_files(), &to_merge)?;
                }
                _ => {}
            }
            check_recheck_canceled()?;
            ensure_parsed_or_trigger_recheck(
                orchestrator,
                pool,
                transaction,
                options,
                to_merge.dupe().all(),
                ParseProgress::Report,
            )?;
            check_recheck_canceled()?;
            if dependent_file_count > 0 {
                flow_hh_logger::info!("recheck {} dependent files:", dependent_file_count);
            }
            let MergeResult {
                suppressions: updated_suppressions,
                skipped_count: merge_skip_count,
                sig_new_or_changed,
                top_cycle,
                calc_deps_time: _,
                time_to_merge,
            } = merge(
                pool,
                transaction,
                options,
                for_find_all_refs,
                &to_merge,
                components,
                &recheck_set,
                sig_dependency_graph,
                errors.suppressions.clone(),
            )?;

            let exports = match env.exports() {
                None => None,
                Some(exports) => {
                    check_recheck_canceled()?;
                    flow_hh_logger::info!("Updating index");
                    let updated_exports = with_memory_timer(options, "Indexing", || {
                        let dirty_files: BTreeSet<FileKey> =
                            sig_new_or_changed.iter().duped().collect();
                        flow_services_export::export_service::update(
                            pool,
                            transaction,
                            &dirty_files,
                            exports,
                        )
                    });
                    flow_hh_logger::info!("Done updating index");
                    Some(updated_exports)
                }
            };
            check_recheck_canceled()?;

            let sig_new_or_changed =
                sig_new_or_changed.union(unchanged_files_to_upgrade.dupe().all());
            let (
                mut errors,
                coverage,
                find_ref_results,
                time_to_check_merged,
                check_skip_count,
                slowest_file,
                num_slow_files,
                check_internal_error,
            ) = check_files::check_files(
                options.dupe(),
                pool,
                transaction,
                all_unordered_libs,
                find_ref_request,
                errors,
                updated_suppressions,
                coverage_for_check,
                to_check.dupe(),
                dirty_direct_dependents,
                sig_new_or_changed,
                implementation_dependency_graph,
                env.master_cx().dupe(),
            )?;
            check_recheck_canceled()?;
            if let Some(ref err) = check_internal_error {
                flow_hh_logger::error!("{}", err);
            }

            // Deduplicate ETrivialRecursiveDefinition errors from cyclic
            // dependencies. In OCaml, the single-threaded check phase shares a
            // check cache so cycle errors are generated only once. In the Rust
            // port, multi-threaded checking means each thread has its own check
            // cache, so cycle members checked on different threads independently
            // detect the cycle and each generate an error.
            //
            // Fix: find files that both have ETrivialRecursiveDefinition errors
            // AND are mutually dependent (same cycle). Keep only the error from
            // the lexicographically smallest file in each cycle group.
            {
                use flow_typing_errors::error_message::ErrorMessage;
                let dep_graph = dependency_info.implementation_dependency_graph();
                let files_with_cycle_errors: BTreeSet<FileKey> = errors
                    .merge_errors
                    .iter()
                    .filter(|(_, errs)| {
                        errs.iter().any(|e| {
                            matches!(
                                e.msg_of_error(),
                                ErrorMessage::ETrivialRecursiveDefinition(..)
                            )
                        })
                    })
                    .map(|(f, _)| f.dupe())
                    .collect();
                if files_with_cycle_errors.len() > 1 {
                    let mut files_to_strip: BTreeSet<FileKey> = BTreeSet::new();
                    for file in &files_with_cycle_errors {
                        if let Some(deps) = dep_graph.find_opt(file) {
                            for dep in deps {
                                if files_with_cycle_errors.contains(dep)
                                    && file.as_str() > dep.as_str()
                                {
                                    files_to_strip.insert(file.dupe());
                                }
                            }
                        }
                    }
                    for file in &files_to_strip {
                        if let Some(errs) = errors.merge_errors.get(file) {
                            let filtered = errs.dupe().filter(|e| {
                                !matches!(
                                    e.msg_of_error(),
                                    ErrorMessage::ETrivialRecursiveDefinition(..)
                                )
                            });
                            errors.merge_errors.insert(file.dupe(), filtered);
                        }
                    }
                }
            }
            check_recheck_canceled()?;

            let options_for_record = options.dupe();
            let merge_time = time_to_merge.as_secs_f64();
            let check_time = time_to_check_merged;
            let merged_files = to_merge.cardinal() as i64;
            let checked_files =
                (to_check.focused_cardinal() + to_check.dependents_cardinal()) as i64;
            let record_recheck_time: Box<dyn FnOnce()> = Box::new(move || {
                recheck_stats::record_recheck_time(
                    &options_for_record,
                    merge_time,
                    check_time,
                    merged_files,
                    checked_files,
                );
            });
            let mut checked_files = unchanged_checked;
            checked_files.union(to_merge.dupe());
            flow_hh_logger::info!("Checked set: {}", checked_files.debug_counts_to_string());

            env.set_checked_files(checked_files);
            env.set_errors(errors);
            env.set_collated_errors(collated_errors);
            env.set_coverage(coverage);
            env.set_exports(exports);

            Ok((
                RecheckResult {
                    modified_count,
                    deleted_count,
                    dependent_file_count,
                    to_merge,
                    to_check,
                    top_cycle,
                    merge_skip_count,
                    check_skip_count,
                    slowest_file,
                    num_slow_files: num_slow_files as usize,
                },
                record_recheck_time,
                find_ref_results,
                check_internal_error,
            ))
        })();
        env.set_dependency_info(dependency_info);
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn full(
        orchestrator: Option<&ServerOrchestratorHandle>,
        pool: &ThreadPool,
        transaction: &Arc<Transaction>,
        options: &Arc<Options>,
        updates: &CheckedSet,
        side_effects: &mut side_effect_transaction::Transaction,
        find_ref_request: &flow_services_references::find_refs_types::Request,
        files_to_force: CheckedSet,
        changed_mergebase: Option<bool>,
        will_be_checked_files: &mut CheckedSet,
        env: &mut EnvTransaction,
    ) -> Result<
        (
            RecheckResult,
            Box<dyn FnOnce()>,
            FindRefResults,
            Option<String>,
        ),
        RecheckError,
    > {
        let def_info = &find_ref_request.def_info;
        let intermediate_values = recheck_parse_and_update_dependency_info(
            orchestrator,
            pool,
            transaction,
            options,
            updates,
            side_effects,
            def_info,
            files_to_force,
            env,
        )?;
        let for_find_all_refs = match def_info {
            DefInfo::VariableDefinition(_, _) | DefInfo::PropertyDefinition(_) => true,
            DefInfo::NoDefinition(_) => false,
        };
        recheck_merge(
            orchestrator,
            pool,
            transaction,
            options,
            find_ref_request,
            for_find_all_refs,
            will_be_checked_files,
            changed_mergebase,
            intermediate_values,
            env,
        )
    }

    pub(crate) fn parse_and_update_dependency_info(
        orchestrator: Option<&ServerOrchestratorHandle>,
        pool: &ThreadPool,
        transaction: &Arc<Transaction>,
        options: &Arc<Options>,
        updates: &CheckedSet,
        side_effects: &mut side_effect_transaction::Transaction,
        def_info: &DefInfo,
        files_to_force: CheckedSet,
        env: &mut EnvTransaction,
    ) -> Result<(), RecheckError> {
        let intermediate_values = recheck_parse_and_update_dependency_info(
            orchestrator,
            pool,
            transaction,
            options,
            updates,
            side_effects,
            def_info,
            files_to_force,
            env,
        )?;
        let IntermediateValues {
            errors,
            collated_errors,
            ..
        } = intermediate_values;
        env.set_errors(errors);
        env.set_collated_errors(collated_errors);
        Ok(())
    }
}

fn with_transaction<T>(
    name: &str,
    f: impl FnOnce(&mut side_effect_transaction::Transaction) -> T,
) -> T {
    side_effect_transaction::with_transaction_sync(name, f)
}

fn with_transaction_result<T, E>(
    name: &str,
    f: impl FnOnce(&mut side_effect_transaction::Transaction) -> Result<T, E>,
) -> Result<T, E> {
    side_effect_transaction::with_transaction_result_sync(name, f)
}

type RecheckImplResult = Result<
    (
        Box<dyn FnOnce(&serde_json::Value)>,
        RecheckStats,
        FindRefResults,
    ),
    RecheckError,
>;

#[allow(clippy::too_many_arguments)]
pub(crate) fn recheck_impl(
    orchestrator: Option<&ServerOrchestratorHandle>,
    pool: &ThreadPool,
    transaction: &Arc<Transaction>,
    options: &Arc<Options>,
    updates: &CheckedSet,
    find_ref_request: &flow_services_references::find_refs_types::Request,
    files_to_force: CheckedSet,
    changed_mergebase: Option<bool>,
    will_be_checked_files: &mut CheckedSet,
    env: &mut EnvTransaction,
) -> RecheckImplResult {
    check_recheck_canceled()?;
    let (stats, record_recheck_time, find_ref_results, first_internal_error) =
        match side_effect_transaction::with_transaction_result_sync("recheck", |side_effects| {
            recheck::full(
                orchestrator,
                pool,
                transaction,
                options,
                updates,
                side_effects,
                find_ref_request,
                files_to_force,
                changed_mergebase,
                will_be_checked_files,
                env,
            )
        }) {
            Ok(result) => result,
            Err(err) => {
                transaction.committed_heap().clear_reader_cache();
                return Err(err);
            }
        };

    if worker_cancel::should_cancel() {
        transaction.committed_heap().clear_reader_cache();
        return Err(RecheckError::Canceled(Vec::new()));
    }

    let recheck::RecheckResult {
        modified_count,
        deleted_count,
        dependent_file_count,
        to_merge,
        to_check,
        top_cycle,
        merge_skip_count,
        check_skip_count,
        slowest_file,
        num_slow_files,
    } = stats;

    if let Err(err) = check_recheck_canceled() {
        transaction.committed_heap().clear_reader_cache();
        return Err(err);
    }

    let collate_result = with_memory_timer(options, "CollateErrors", || {
        check_recheck_canceled()?;
        let focused_to_check = to_check.focused().dupe();
        let dependents_to_check = to_check.dependents().dupe();
        let files = FlowOrdSet::from(
            focused_to_check
                .into_inner()
                .union(dependents_to_check.into_inner()),
        );
        let (new_errors, all_suppressions) = {
            let errors = env.errors();
            (filter_errors(&files, errors), errors.suppressions.clone())
        };
        let mut collated_errors = env.take_collated_errors();
        collated_errors.clear_merge(&files);

        {
            let loc_of_aloc = |loc: &ALoc| -> Loc { transaction.loc_of_aloc(loc) };
            let transaction_for_ast = transaction.dupe();
            let get_ast = move |file: &FileKey| -> Option<Arc<Program<Loc, Loc>>> {
                transaction_for_ast.get_ast(file)
            };
            error_collator::update_collated_errors(
                &loc_of_aloc,
                &get_ast,
                &|file| env.is_lib_file(file),
                options,
                env.checked_files(),
                &all_suppressions,
                &new_errors,
                &mut collated_errors,
            );
        }
        let error_resolution_stat =
            error_collator::update_error_state_timestamps(&mut collated_errors);

        check_recheck_canceled()?;
        Ok((collated_errors, error_resolution_stat))
    });
    let (collated_errors, error_resolution_stat) = match collate_result {
        Ok(result) => result,
        Err(err) => {
            transaction.committed_heap().clear_reader_cache();
            return Err(err);
        }
    };

    let typing_errors_data = if options.log_per_error_typing_telemetry {
        let per_file_errors = error_collator::compute_per_file_errors(10, &collated_errors);
        let typing_errors_data = {
            let error_info_to_json =
                |error_collator::PerErrorInfo {
                     error_code,
                     line_agnostic_hash,
                     error_message,
                 }: error_collator::PerErrorInfo|
                 -> serde_json::Value {
                    let mut props = serde_json::Map::new();
                    props.insert("error_code".to_string(), error_code.into());
                    props.insert("line_agnostic_hash".to_string(), line_agnostic_hash.into());
                    let props = match error_message {
                        Some(msg) => {
                            props.insert("error_context".to_string(), msg.into());
                            props
                        }
                        None => props,
                    };
                    serde_json::Value::Object(props)
                };
            let mut total_count = 0;
            let by_file: serde_json::Map<String, serde_json::Value> = per_file_errors
                .into_iter()
                .map(|error_collator::PerFileErrors { file_path, errors }| {
                    total_count += errors.len();
                    (
                        file_path.to_string(),
                        serde_json::Value::Array(
                            errors.into_iter().map(&error_info_to_json).collect(),
                        ),
                    )
                })
                .collect();
            serde_json::json!({
                "total_count": total_count,
                "by_file": by_file,
            })
        };
        Some(typing_errors_data)
    } else {
        None
    };

    env.set_collated_errors(collated_errors);

    let log_recheck_event: Box<dyn FnOnce(&serde_json::Value)> = Box::new(move |profiling| {
        flow_event_logger::recheck(
            modified_count as i32,
            deleted_count as i32,
            to_merge.dependencies_cardinal() as i32,
            dependent_file_count as i32,
            merge_skip_count as i32,
            check_skip_count as i32,
            profiling,
            error_resolution_stat.time_to_resolve_all_type_errors,
            error_resolution_stat.time_to_resolve_all_type_errors_in_one_file,
            error_resolution_stat.time_to_resolve_all_subtyping_errors,
            error_resolution_stat.time_to_resolve_all_subtyping_errors_in_one_file,
            error_resolution_stat.rounds_to_resolve_all_type_errors,
            error_resolution_stat.rounds_to_resolve_all_type_errors_in_one_file,
            error_resolution_stat.rounds_to_resolve_all_subtyping_errors,
            error_resolution_stat.rounds_to_resolve_all_subtyping_errors_in_one_file,
            first_internal_error.as_deref(),
            slowest_file.as_deref(),
            Some(num_slow_files as i32),
            changed_mergebase,
        );
        if let Some(typing_errors_data) = typing_errors_data.as_ref() {
            flow_event_logger::log_typing_errors(typing_errors_data);
        }
        record_recheck_time();
    });

    let changed_file_count = modified_count + deleted_count;
    let recheck_stats = RecheckStats {
        dependent_file_count,
        changed_file_count,
        top_cycle,
    };

    Ok((log_recheck_event, recheck_stats, find_ref_results))
}

pub struct RecheckStats {
    pub dependent_file_count: usize,
    pub changed_file_count: usize,
    pub top_cycle: Option<(FileKey, usize)>,
}

pub fn make_next_files(
    root: &Path,
    file_options: Arc<FileOptions>,
    include_libdef: bool,
    symlink_map: Option<&files::SymlinkMap>,
    mut send_chunked: impl FnMut(Vec<FileKey>),
) {
    let file_opts_for_convert = file_options.dupe();
    files::make_next_files(
        root,
        None,
        false,
        false,
        file_options,
        include_libdef,
        symlink_map,
        |chunk: Vec<PathBuf>| {
            let files: Vec<FileKey> = chunk
                .into_iter()
                .map(|p| {
                    let path_str = p.to_string_lossy().into_owned();
                    files::filename_from_string(&file_opts_for_convert, &path_str)
                })
                .collect();
            send_chunked(files);
        },
    );
}

fn mk_env(
    files: FlowOrdSet<FileKey>,
    unparsed: FlowOrdSet<FileKey>,
    package_json_files: FlowOrdSet<FileKey>,
    dependency_info: DependencyInfo,
    ordered_libs: Vec<FlowSmolStr>,
    global_lib_files: server_env::GlobalLibFiles,
    errors: Errors,
    collated_errors: CollatedErrors,
    exports: Option<ExportSearch>,
    master_cx: Arc<MasterContext>,
    previous_connections: Option<&PersistentConnection>,
) -> Env {
    Env {
        files,
        unparsed,
        dependency_info: env_cell(dependency_info),
        checked_files: CheckedSet::empty(),
        package_json_files,
        ordered_libs: Arc::new(ordered_libs),
        global_lib_files,
        errors: env_cell(errors),
        coverage: env_cell(BTreeMap::new()),
        collated_errors: env_cell(collated_errors),
        connections: previous_connections
            .map(|c| c.dupe())
            .unwrap_or_else(flow_server_env::persistent_connection::empty),
        exports,
        master_cx,
    }
}

#[allow(dead_code)]
// Verify that the hash in shared memory matches what's on disk.
// Used to verify saved state.
fn verify_hash(transaction: &Arc<Transaction>, file_key: &FileKey) -> bool {
    let filename_string = file_key.as_str();
    let content = match std::fs::read_to_string(filename_string) {
        Ok(c) => c,
        Err(_) => return false,
    };
    parsing_service::does_content_match_file_hash(transaction, file_key, &content)
}

#[allow(dead_code)]
fn assert_valid_hashes(updates: &CheckedSet, invalid_hashes: Vec<FileKey>) {
    let invalid_hashes: Vec<FileKey> = invalid_hashes
        .into_iter()
        .filter(|file| !updates.mem(file))
        .collect();
    if invalid_hashes.is_empty() {
        flow_hh_logger::info!("Saved state verification succeeded");
    } else {
        let files_str: String = invalid_hashes
            .iter()
            .map(|f| f.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        eprintln!(
            "The following files do not match their hashes in saved state:\n{}",
            files_str
        );
        flow_hh_logger::error!("Saved state verification failed");
        flow_common_exit_status::exit(flow_common_exit_status::FlowExitStatus::InvalidSavedState);
    }
}

fn is_incompatible_package_json(
    options: &Options,
    transaction: &Transaction,
    want: &dyn Fn(&str) -> bool,
    sroot: &str,
    file_options: &FileOptions,
    f: &str,
) -> PackageIncompatibleReturn {
    let is_incompatible = |filename_str: &str| -> PackageIncompatibleReturn {
        let filename = FileKey::json_file_of_absolute(filename_str);
        match std::fs::read_to_string(filename_str).ok() {
            None => PackageIncompatibleReturn::Incompatible(
                flow_services_module::PackageIncompatibleReason::Unknown,
            ),
            Some(content) => {
                let result = parsing_service::parse_package_json_file(
                    options,
                    Ok(content.as_str()),
                    &filename,
                )
                .map_err(|_| ());
                let old_package = transaction
                    .get_package_info(&filename)
                    .map(|pkg| Ok((*pkg).clone()));
                flow_services_module::package_incompatible(&filename, old_package, result)
            }
        }
    };
    if (f.starts_with(sroot) || files::is_included(file_options, f))
        && Path::new(f)
            .file_name()
            .is_some_and(|name| name == "package.json")
        && want(f)
    {
        is_incompatible(f)
    } else {
        PackageIncompatibleReturn::Compatible
    }
}

fn get_updated_flowconfig(config_path: &str) -> Result<(FlowConfig, String), String> {
    flow_config::get(config_path)
        .map(|(config, _warnings, hash)| (config, hash))
        .map_err(|_| "Config changed in an incompatible way".to_string())
}

fn assert_compatible_flowconfig_version(config: &FlowConfig) -> Result<(), String> {
    let not_satisfied = |version_constraint: &str| -> bool {
        match semver::satisfies(Some(true), version_constraint, flow_version::version()) {
            Ok(result) => !result,
            Err(_) => true,
        }
    };
    match config.version.as_deref() {
        Some(version_constraint) if not_satisfied(version_constraint) => Err(format!(
            "Wrong version of Flow. The config specifies version {} but this is version {}",
            version_constraint,
            flow_version::version()
        )),
        _ => Ok(()),
    }
}

fn assert_compatible_flowconfig_change(options: &Options, config_path: &str) -> Result<(), String> {
    let old_hash = &options.flowconfig_hash;
    let (new_config, new_hash) = get_updated_flowconfig(config_path)?;
    if old_hash.as_str() == new_hash.as_str() {
        Ok(())
    } else {
        flow_hh_logger::error!(
            "Flowconfig hash changed from {:?} to {:?}",
            old_hash,
            new_hash
        );
        assert_compatible_flowconfig_version(&new_config)?;
        Err("Config changed in an incompatible way".to_string())
    }
}

fn filter_saved_state_updates(
    file_options: &FileOptions,
    sroot: &str,
    want: &dyn Fn(&str) -> bool,
    updates: &BTreeSet<String>,
) -> FlowOrdSet<FileKey> {
    let mut acc = FlowOrdSet::new();
    for f in updates {
        if files::is_flow_file(file_options, f)
            && ((file_options.implicitly_include_root && f.starts_with(sroot))
                || files::is_included(file_options, f))
            && want(f)
        {
            let filename = files::filename_from_string(file_options, f);
            acc.insert(filename);
        }
    }
    acc
}

/// This function tries to find out declaration changes that should invalidate the global scope.
/// Two additional tricky cases compared to baseline libdef change:
///
/// 1. previous global contributors stop contributing.
///    Handled in this function by comparing content
/// 2. previous non-global declaration files become global contributors.
///    Handled in this function by parsing with an ad-hoc transaction to figure out that it happened.
fn global_scope_changes(
    pool: &ThreadPool,
    committed_heap: &Arc<CommittedHeap>,
    options: &Arc<Options>,
    saved_discovered: &BTreeSet<FileKey>,
    saved_global_augmentations: &FlowOrdSet<FileKey>,
    updates: &FlowOrdSet<FileKey>,
) -> BTreeSet<FileKey> {
    let scratch = ActiveTransaction::new(committed_heap.dupe());
    let transaction = scratch.handle();
    let mut moved: BTreeSet<FileKey> = updates
        .iter()
        .filter(|file| {
            saved_discovered.contains(*file) || transaction.has_ts_global_augmentation(file)
        })
        .duped()
        .collect();
    moved.extend(
        saved_global_augmentations
            .iter()
            .filter(|file| parsing_service::did_content_change(&transaction, file))
            .duped(),
    );
    let unclassified: Vec<FileKey> = updates
        .iter()
        .filter(|file| {
            !moved.contains(*file)
                && !files::is_configured_lib_file(&options.file_options, file.as_str())
                && (has_dts_ext(file.as_str()) || files::has_ts_ext(file))
        })
        .duped()
        .collect();
    if !unclassified.is_empty() {
        let next: parsing_service::Next = {
            let mut files = Some(unclassified);
            Box::new(move || files.take())
        };
        let parsed = parse(pool, &transaction, options, next);
        moved.extend(parsed.discovered_global_libdefs());
        moved.extend(parsed.ts_global_augmentation_files);
    }
    moved
}

fn process_saved_state_updates(
    pool: &ThreadPool,
    committed_heap: &Arc<CommittedHeap>,
    options: &Arc<Options>,
    transaction: &Transaction,
    saved_discovered: &BTreeSet<FileKey>,
    saved_global_augmentations: &FlowOrdSet<FileKey>,
    updates: &BTreeSet<String>,
) -> Result<FlowOrdSet<FileKey>, String> {
    let file_options = &options.file_options;
    let root = &*options.root;
    let config_path = server_files_js::config_file(&options.flowconfig_name, root);
    if updates.contains(&config_path) {
        assert_compatible_flowconfig_change(options, &config_path)?;
    }

    let sroot = format!("{}{}", root.to_string_lossy(), std::path::MAIN_SEPARATOR);
    let want = |f: &str| -> bool { files::wanted(file_options, false, f) };
    for file in updates {
        match is_incompatible_package_json(options, transaction, &want, &sroot, file_options, file)
        {
            PackageIncompatibleReturn::Compatible => {}
            PackageIncompatibleReturn::Incompatible(reason) => {
                return Err(format!(
                    "Modified package: {} ({})\nPackages changed in an incompatible way",
                    file, reason
                ));
            }
        }
    }

    let flow_typed_path = files::get_flowtyped_path(root)
        .to_string_lossy()
        .to_string();
    let libs: BTreeSet<String> = updates
        .iter()
        .filter(|filename| {
            let is_lib = files::is_configured_lib_file(file_options, filename)
                || **filename == flow_typed_path;
            is_lib
                && parsing_service::did_content_change(transaction, &files::lib_file_key(filename))
        })
        .cloned()
        .collect();
    if !libs.is_empty() {
        let messages = libs
            .iter()
            .rev()
            .map(|f| format!("Modified lib file: {}", f))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!(
            "{}\nLib files changed in an incompatible way",
            messages
        ));
    }

    let filtered_updates = filter_saved_state_updates(file_options, &sroot, &want, updates);

    // Restoring over a moved global scope would put every file in the repo under a set of
    // globals the saved state was never built with, and nothing short of a whole-repo parse
    // can rebuild that set. Refuse the state instead and let the caller start from scratch.
    let global_scope_changes = global_scope_changes(
        pool,
        committed_heap,
        options,
        saved_discovered,
        saved_global_augmentations,
        &filtered_updates,
    );
    if !global_scope_changes.is_empty() {
        let messages = global_scope_changes
            .iter()
            .rev()
            .map(|file| format!("Modified global library definition: {}", file.as_str()))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!(
            "{}\nGlobal library definitions changed in an incompatible way",
            messages
        ));
    }

    Ok(filtered_updates)
}

fn saved_duplicate_providers_from_direct_state(
    duplicate_providers: BTreeMap<FlowSmolStr, (FileKey, Vec<FileKey>)>,
) -> BTreeMap<FlowSmolStr, (FileKey, Vec1<FileKey>)> {
    duplicate_providers
        .into_iter()
        .filter_map(|(key, (leader, others))| {
            Vec1::try_from_vec(others)
                .ok()
                .map(|others| (key, (leader, others)))
        })
        .collect()
}

fn clear_deleted_heaps(
    transaction: &Arc<Transaction>,
    env: Option<&Env>,
    parsed: &FlowOrdSet<FileKey>,
    unparsed: &FlowOrdSet<FileKey>,
    package_json_files: &FlowOrdSet<FileKey>,
) -> BTreeSet<Modulename> {
    let Some(env) = env else {
        return BTreeSet::new();
    };
    let old_files = env
        .files
        .dupe()
        .into_inner()
        .union(env.unparsed.dupe().into_inner())
        .union(env.package_json_files.dupe().into_inner());
    let current_files = parsed
        .dupe()
        .into_inner()
        .union(unparsed.dupe().into_inner())
        .union(package_json_files.dupe().into_inner());
    old_files
        .relative_complement(current_files)
        .iter()
        .fold(BTreeSet::new(), |mut acc, file| {
            let dirty =
                transaction.clear_file(file.dupe(), transaction.get_haste_module_info(file));
            acc.extend(dirty);
            acc
        })
}

#[allow(clippy::too_many_arguments)]
fn init_with_initial_state(
    pool: &ThreadPool,
    transaction: &Arc<Transaction>,
    options: &Arc<Options>,
    restore_dependency_info: impl FnOnce() -> Arc<EnvCell<DependencyInfo>>,
    saved_duplicate_providers: BTreeMap<FlowSmolStr, (FileKey, Vec1<FileKey>)>,
    saved_export_index: Option<ExportIndex>,
    saved_discovered_global_libdefs: BTreeSet<FileKey>,
    saved_global_augmentations: FlowOrdSet<FileKey>,
    env: Option<&Env>,
    parsed: FlowOrdSet<FileKey>,
    unparsed: FlowOrdSet<FileKey>,
    package_json_files: FlowOrdSet<FileKey>,
    dirty_modules: BTreeSet<Modulename>,
    local_errors: BTreeMap<FileKey, ErrorSet>,
) -> (Env, bool) {
    // We actually parse and typecheck the libraries, even though we're loading from saved state.
    // We'd need to check them anyway, as soon as any file is checked, since we don't track
    // dependents for libraries. And we don't really support incrementally checking libraries
    //
    // The order of libraries is significant. If two libraries define the same thing, the one
    // merged later wins. For this reason, the saved state stores the order in which the non-flowlib
    // libraries were merged. So all we need to guarantee here is:
    //
    // 1. The builtin libraries are merged first
    // 2. The non-builtin libraries are merged in the same order as before
    flow_hh_logger::info!("Loading libraries");
    monitor_rpc::status_update(server_status::Event::LoadLibrariesStart);
    let ordered_libs = files::ordered_and_unordered_lib_paths(&options.file_options);

    let additional_lib_files: Vec<FileKey> = ordered_libs
        .iter()
        .map(|name| files::lib_file_key(name))
        .chain(saved_discovered_global_libdefs)
        .chain(saved_global_augmentations.iter().duped())
        .collect();
    let reparsed_lib_files: FlowOrdSet<FileKey> = additional_lib_files.iter().duped().collect();
    let next: parsing_service::Next = {
        let mut files = Some(additional_lib_files);
        Box::new(move || files.take().filter(|files| !files.is_empty()))
    };

    let parse_results = parse(pool, transaction, options, next);
    let discovered_global_libdefs = parse_results.discovered_global_libdefs_state();
    let CollatedParseResults {
        parsed: additional_parsed,
        unparsed: additional_unparsed,
        unchanged: _unchanged,
        not_found: _not_found,
        dirty_modules: additional_dirty_modules,
        local_errors: additional_local_errors,
        package_json: _package_json,
        all_unordered_libs: discovered_libs,
        dts_file_kinds: _,
        ts_global_augmentation_files,
    } = parse_results;

    let all_unordered_libs_set = Arc::new(discovered_libs);
    let global_lib_files = server_env::GlobalLibFiles::new(
        all_unordered_libs_set.dupe(),
        discovered_global_libdefs.dupe(),
        ts_global_augmentation_files.dupe(),
    );

    let (libs_ok, local_errors, warnings, suppressions, lib_exports, master_cx) = init_libs(
        options,
        transaction,
        all_unordered_libs_set.dupe(),
        init::assemble_ordered_lib_inputs(
            &ordered_libs,
            &discovered_global_libdefs,
            &ts_global_augmentation_files,
        ),
        local_errors,
        BTreeMap::new(),
        ErrorSuppressions::empty(),
    );

    // The caller's sets may still place a library where it was before the parse above:
    // `reinit_full_check` passes the previous environment's, and a saved state lists discovered
    // globals among its parsed files. That parse decides where each library belongs now.
    let parsed = FlowOrdSet::from(
        parsed
            .into_inner()
            .relative_complement(reparsed_lib_files.dupe().into_inner())
            .union(additional_parsed.dupe().into_inner()),
    );
    let unparsed = FlowOrdSet::from(
        unparsed
            .into_inner()
            .relative_complement(reparsed_lib_files.into_inner())
            .union(additional_unparsed.into_inner()),
    );
    let dirty_modules: flow_common_modulename::ModulenameSet = dirty_modules
        .into_iter()
        .chain(additional_dirty_modules)
        .collect();
    let local_errors = merge_error_maps(local_errors, additional_local_errors);

    flow_hh_logger::info!("Resolving dependencies");
    monitor_rpc::status_update(server_status::Event::ResolvingDependenciesProgress);
    let (_changed_modules, duplicate_providers) = commit_modules(
        pool,
        options,
        transaction,
        saved_duplicate_providers,
        dirty_modules,
    );
    resolve_requires(pool, transaction, options, &additional_parsed);
    let dependency_info = restore_dependency_info();

    // The libdefs parsed above joined `parsed`, and so `env.files`, but the
    // graph we just restored predates them: a libdef added to a `[libs]`
    // directory since the saved state was written has no node at all. Every
    // other path that grows `env.files` grows the graph with it, so do the same here — lazy init goes
    // on to focus and merge exactly these files, and merging one the sig graph
    // has never heard of is fatal.
    let additional_parsed_typed: FlowOrdSet<FileKey> = additional_parsed
        .iter()
        .filter(|file| transaction.get_typed_parse(file).is_some())
        .duped()
        .collect();
    if !additional_parsed_typed.is_empty() {
        let partial_dependency_graph = dep_service::calc_partial_dependency_graph(
            pool,
            transaction,
            &additional_parsed_typed,
            &parsed,
        );
        let mut overlay = OverlayDependencyInfo::new(dependency_info.dupe());
        overlay.update(partial_dependency_graph, BTreeSet::new());
        overlay.commit();
    }

    let exports = if options.autoimports {
        match saved_export_index {
            Some(index) => {
                flow_hh_logger::info!("Indexing files (from saved state)");
                let exports = with_memory_timer(options, "Indexing", || {
                    flow_services_export::export_search::init(index)
                });
                flow_hh_logger::info!("Indexing files Done");
                Some(exports)
            }
            None => {
                flow_hh_logger::info!("Indexing files");
                let exports = with_memory_timer(options, "Indexing", || {
                    let parsed: BTreeSet<FileKey> = parsed.iter().duped().collect();
                    flow_services_export::export_service::init(
                        pool,
                        transaction,
                        &lib_exports,
                        &parsed,
                    )
                });
                flow_hh_logger::info!("Indexing files Done");
                Some(exports)
            }
        }
    } else {
        None
    };
    let mut collated_errors = OverlayCollatedErrors::from_collated_errors(CollatedErrors::empty());
    {
        let loc_of_aloc = |loc: &ALoc| -> Loc { transaction.loc_of_aloc(loc) };
        let transaction_for_ast = transaction.dupe();
        let get_ast = move |file: &FileKey| -> Option<Arc<Program<Loc, Loc>>> {
            transaction_for_ast.get_ast(file)
        };
        let unsuppressable_error_codes: BTreeSet<FlowSmolStr> =
            options.unsuppressable_error_codes.iter().duped().collect();
        error_collator::update_local_collated_errors(
            &loc_of_aloc,
            &get_ast,
            &|file| global_lib_files.contains(file),
            &options.root,
            &options.file_options,
            options.node_modules_errors,
            &unsuppressable_error_codes,
            &suppressions,
            &local_errors,
            &mut collated_errors,
        );
        error_collator::update_error_state_timestamps(&mut collated_errors);
    }
    let mut collated_errors_base = CollatedErrors::empty();
    collated_errors.apply_to_base(&mut collated_errors_base);
    let collated_errors = collated_errors_base;
    let errors = Errors {
        local_errors,
        duplicate_providers,
        merge_errors: BTreeMap::new(),
        warnings,
        suppressions,
    };
    let env = Env {
        files: parsed,
        dependency_info,
        checked_files: CheckedSet::empty(),
        package_json_files,
        ordered_libs: Arc::new(
            ordered_libs
                .into_iter()
                .map(|s| FlowSmolStr::from(s.as_str()))
                .collect(),
        ),
        global_lib_files,
        unparsed,
        errors: env_cell(errors),
        coverage: env_cell(BTreeMap::new()),
        collated_errors: env_cell(collated_errors),
        connections: env
            .map(|env| env.connections.dupe())
            .unwrap_or_else(flow_server_env::persistent_connection::empty),
        exports,
        master_cx,
    };
    (env, libs_ok)
}

pub fn init_from_saved_state(
    pool: &ThreadPool,
    transaction: &Arc<Transaction>,
    options: &Arc<Options>,
    saved_state: SavedStateEnvData,
    updates: &CheckedSet,
    env: Option<&Env>,
) -> (Env, bool) {
    let SavedStateEnvData {
        parsed_files,
        unparsed_files,
        package_json_files: saved_package_json_files,
        local_errors: saved_local_errors,
        node_modules_containers: saved_node_modules_containers,
        dependency_info,
        duplicate_providers,
        export_index,
        global_lib_files,
        ..
    } = saved_state;
    flow_hh_logger::info!("Processing saved state file sets");
    monitor_rpc::status_update(server_status::Event::RestoringHeapsStart);
    // The heap dump was bulk-loaded into a heap of its own during the load step, so all file data
    // is already in the heap this transaction is open on. The saved-state metadata contains only
    // file sets, so we just need to:
    // 1. Handle deleted files (files in saved state that no longer exist on disk)
    // 2. Compute dirty modules by querying the already-populated heap
    // 3. Optionally verify file hashes
    *files::node_modules_containers.write().unwrap() = saved_node_modules_containers;
    let parsed = parsed_files;
    let unparsed = unparsed_files;
    let package_json_files = saved_package_json_files;
    let dirty_modules =
        clear_deleted_heaps(transaction, env, &parsed, &unparsed, &package_json_files);
    let invalid_hashes = if options.saved_state_verify {
        let check_hash = |mut acc: Vec<FileKey>, file: &FileKey| {
            if !verify_hash(transaction, file) {
                acc.push(file.dupe());
            }
            acc
        };
        package_json_files.iter().fold(
            unparsed
                .iter()
                .fold(parsed.iter().fold(Vec::new(), check_hash), check_hash),
            check_hash,
        )
    } else {
        Vec::new()
    };
    if options.saved_state_verify {
        assert_valid_hashes(updates, invalid_hashes);
    }
    let local_errors = saved_local_errors;

    init_with_initial_state(
        pool,
        transaction,
        options,
        || env_cell(dependency_info),
        saved_duplicate_providers_from_direct_state(duplicate_providers),
        if options.saved_state_persist_export_index {
            export_index
        } else {
            None
        },
        global_lib_files.discovered,
        global_lib_files.global_augmentations,
        env,
        parsed,
        unparsed,
        package_json_files,
        dirty_modules,
        local_errors,
    )
}

pub fn handle_updates_since_saved_state(
    orchestrator: Option<&ServerOrchestratorHandle>,
    pool: &ThreadPool,
    mut transaction: ActiveTransaction,
    options: &Arc<Options>,
    libs_ok: bool,
    updates: &CheckedSet,
    env: Env,
) -> Result<(Env, ActiveTransaction), RecheckError> {
    let should_force_recheck = options.saved_state_force_recheck;
    // We know that all the files in updates have changed since the saved state was generated. We
    // have two ways to deal with them:
    if !options.lazy_mode || should_force_recheck {
        if updates.is_empty() || !libs_ok {
            // Don't recheck if the libs are not ok
            return Ok((env, transaction));
        }
        // In non-lazy mode, we return updates here. They will immediately be rechecked. Due to
        // fanout, this can be a huge recheck, but it's sound.
        //
        // We'll also hit this code path in lazy modes if the user has passed
        // --saved-state-force-recheck. These users want to force Flow to recheck all the files that
        // have changed since the saved state was generated
        let find_ref_request = flow_services_references::find_refs_types::empty_request();
        let mut updated_files = updates.dupe();
        let env = Arc::new(env);
        loop {
            let mut env_transaction = EnvTransaction::new(env.dupe());
            let mut will_be_checked_files = CheckedSet::empty();
            match recheck_impl(
                orchestrator,
                pool,
                &transaction.handle(),
                options,
                &updated_files,
                &find_ref_request,
                CheckedSet::empty(),
                None,
                &mut will_be_checked_files,
                &mut env_transaction,
            ) {
                Ok((_log_recheck_event, _summary_info, _find_ref_results)) => {
                    return Ok((env_transaction.into_env(), transaction));
                }
                Err(RecheckError::Canceled(changed_files)) if !changed_files.is_empty() => {
                    let heap = transaction.committed_heap();
                    heap.clear_reader_cache();
                    transaction = ActiveTransaction::new(heap);
                    let dependencies = changed_files.into_iter().collect();
                    updated_files.add(None, None, Some(dependencies));
                }
                Err(err) => return Err(err),
            }
        }
    } else {
        // In lazy mode, we try to avoid the fanout problem. All we really want to do in lazy mode
        // is to update the dependency graph and stuff like that. We don't actually want to merge
        // anything yet.
        let mut updated_files = updates.dupe();
        let env = Arc::new(env);
        loop {
            let mut env_transaction = EnvTransaction::new(env.dupe());
            match with_transaction_result("lazy init update deps", |side_effects| {
                recheck::parse_and_update_dependency_info(
                    orchestrator,
                    pool,
                    &transaction.handle(),
                    options,
                    &updated_files,
                    side_effects,
                    &DefInfo::NoDefinition(None),
                    CheckedSet::empty(),
                    &mut env_transaction,
                )
            }) {
                Ok(()) => {
                    return Ok((env_transaction.into_env(), transaction));
                }
                Err(RecheckError::Canceled(changed_files)) if !changed_files.is_empty() => {
                    let heap = transaction.committed_heap();
                    heap.clear_reader_cache();
                    transaction = ActiveTransaction::new(heap);
                    let dependencies = changed_files.into_iter().collect();
                    updated_files.add(None, None, Some(dependencies));
                }
                Err(err) => {
                    transaction.committed_heap().clear_reader_cache();
                    return Err(err);
                }
            }
        }
    }
}

/// Crawls the root and builds an [`Env`] from the files it finds. `previous_env` is `Some`
/// only when an environment is being rebuilt over a heap that a previous one already
/// populated: its inventory is what tells us which files have since disappeared, and its
/// persistent connections have to survive the rebuild.
pub fn init_from_scratch(
    options: &Arc<Options>,
    pool: &ThreadPool,
    transaction: &Arc<Transaction>,
    root: &Path,
    previous_env: Option<&Env>,
) -> (Env, bool /* libs_ok */) {
    with_transaction("init", |_transaction| {
        let ordered_libs = files::ordered_and_unordered_lib_paths(&options.file_options);
        let file_opts = options.file_options.dupe();
        let root_buf = root.to_path_buf();
        let symlink_transaction = transaction.dupe();

        let (sender, receiver) = channel::unbounded::<Vec<FileKey>>();
        let receiver = Arc::new(receiver);

        let handle = std::thread::spawn(move || {
            make_next_files(
                &root_buf,
                file_opts,
                true,
                Some(symlink_transaction.symlink_paths()),
                |files| {
                    sender.send(files).unwrap();
                },
            );
            symlink_transaction.symlink_paths().mark_complete();
            drop(sender);
        });
        flow_hh_logger::info!("Parsing");
        let receiver_for_next = receiver.dupe();
        let mut total = 0;
        let next: parsing_service::Next = Box::new(move || {
            let files = receiver_for_next.recv().ok()?;
            monitor_rpc::status_update(server_status::Event::ParsingProgress(
                server_status::Progress {
                    total: None,
                    finished: total,
                },
            ));
            total += files.len() as i32;
            Some(files)
        });

        let parse_results = parse(pool, transaction, options, next);
        let discovered_global_libdefs = parse_results.discovered_global_libdefs_state();
        let CollatedParseResults {
            parsed: parsed_set,
            unparsed: unparsed_set,
            unchanged,
            not_found: _not_found,
            dirty_modules,
            local_errors,
            package_json: (package_json_files_list, package_json_errors),
            all_unordered_libs: discovered_libs,
            dts_file_kinds: _,
            ts_global_augmentation_files,
        } = parse_results;
        handle.join().unwrap();

        let all_unordered_libs_set = Arc::new(discovered_libs);
        let global_lib_files = server_env::GlobalLibFiles::new(
            all_unordered_libs_set.dupe(),
            discovered_global_libdefs.dupe(),
            ts_global_augmentation_files.dupe(),
        );

        assert!(unchanged.is_empty());

        let warnings = BTreeMap::new();
        let package_errors = package_json_files_list
            .iter()
            .zip(package_json_errors.iter())
            .fold(
                BTreeMap::new(),
                |mut acc, (source_file, parse_error)| match parse_error {
                    None => acc,
                    Some((loc, err)) => {
                        let error_set = inference_utils::set_of_parse_error(
                            source_file.dupe(),
                            (loc.dupe(), err.clone()),
                        );
                        update_errset(&mut acc, source_file.dupe(), error_set);
                        acc
                    }
                },
            );
        let package_json_files = package_json_files_list
            .iter()
            .duped()
            .collect::<FlowOrdSet<_>>();
        let local_errors = merge_error_maps(package_errors, local_errors);

        // The crawl only reports files that exist, so anything the previous environment
        // knew about and this one didn't find is gone. Its heap entry has to go with it,
        // and the modules it used to provide have to be recommitted below.
        let dirty_modules_ordered: flow_common_modulename::ModulenameSet = dirty_modules
            .into_iter()
            .chain(clear_deleted_heaps(
                transaction,
                previous_env,
                &parsed_set,
                &unparsed_set,
                &package_json_files,
            ))
            .collect();

        flow_hh_logger::info!("Loading libraries");
        let (libs_ok, local_errors, warnings, suppressions, lib_exports, master_cx) = init_libs(
            options,
            transaction,
            all_unordered_libs_set.dupe(),
            init::assemble_ordered_lib_inputs(
                &ordered_libs,
                &discovered_global_libdefs,
                &ts_global_augmentation_files,
            ),
            local_errors,
            warnings,
            ErrorSuppressions::empty(),
        );

        flow_hh_logger::info!("Resolving dependencies");
        monitor_rpc::status_update(server_status::Event::ResolvingDependenciesProgress);
        let (_changed_modules, duplicate_providers) = commit_modules(
            pool,
            options,
            transaction,
            BTreeMap::new(),
            dirty_modules_ordered,
        );

        resolve_requires(pool, transaction, options, &parsed_set);

        monitor_rpc::status_update(server_status::Event::CalculatingDependenciesProgress);
        let dependency_info = with_memory_timer(options, "CalcDepsTypecheck", || {
            dep_service::calc_dependency_info(pool, transaction, &parsed_set)
        });

        let mut collated_errors =
            OverlayCollatedErrors::from_collated_errors(CollatedErrors::empty());
        {
            let loc_of_aloc = |loc: &ALoc| -> Loc { transaction.loc_of_aloc(loc) };
            let transaction_for_ast = transaction.dupe();
            let get_ast = move |file: &FileKey| -> Option<Arc<Program<Loc, Loc>>> {
                transaction_for_ast.get_ast(file)
            };
            let unsuppressable_error_codes: BTreeSet<FlowSmolStr> =
                options.unsuppressable_error_codes.iter().duped().collect();
            error_collator::update_local_collated_errors(
                &loc_of_aloc,
                &get_ast,
                &|file| global_lib_files.contains(file),
                &options.root,
                &options.file_options,
                options.node_modules_errors,
                &unsuppressable_error_codes,
                &suppressions,
                &local_errors,
                &mut collated_errors,
            );
            error_collator::update_error_state_timestamps(&mut collated_errors);
        }
        let mut collated_errors_base = CollatedErrors::empty();
        collated_errors.apply_to_base(&mut collated_errors_base);
        let collated_errors = collated_errors_base;

        let errors = Errors {
            local_errors,
            duplicate_providers,
            merge_errors: BTreeMap::new(),
            warnings,
            suppressions,
        };
        let exports = if options.autoimports {
            flow_hh_logger::info!("Indexing files");
            let exports = with_memory_timer(options, "Indexing", || {
                let parsed: BTreeSet<FileKey> = parsed_set.iter().duped().collect();
                flow_services_export::export_service::init(pool, transaction, &lib_exports, &parsed)
            });
            flow_hh_logger::info!("Indexing files Done");
            Some(exports)
        } else {
            None
        };
        let env = mk_env(
            parsed_set.dupe(),
            unparsed_set.dupe(),
            package_json_files,
            dependency_info,
            ordered_libs
                .into_iter()
                .map(|s| FlowSmolStr::from(s.as_str()))
                .collect(),
            global_lib_files,
            errors,
            collated_errors,
            exports,
            master_cx,
            previous_env.map(|env| &env.connections),
        );

        (env, libs_ok)
    })
}

#[allow(dead_code)]
pub fn exit_if_no_fallback(msg: Option<&str>, options: &Options) {
    if options.saved_state_no_fallback {
        if let Some(msg) = msg {
            eprintln!("Invalid_saved_state: {}", msg);
        }
        flow_common_exit_status::exit(flow_common_exit_status::FlowExitStatus::InvalidSavedState);
    }
}

// Loads the saved state and returns the transaction to continue under, along with the heap the
// saved state brought.
//
// A saved state's dump replaces the whole heap rather than adding to it, so it is built as a heap
// of its own and everything that follows — validating it, and the init or recheck that runs on top
// of it — happens there. The heap in force is left exactly as it was, whether the saved state turns
// out to be usable or not; making the loaded heap the one the server runs on is the orchestrator's
// call, not this function's. (Staging a whole heap through a transaction overlay instead would
// mean two serial passes over every entry and dependency edge.)
pub fn load_saved_state(
    pool: &ThreadPool,
    options: &Arc<Options>,
) -> Result<
    (
        ActiveTransaction,
        Arc<CommittedHeap>,
        SavedStateEnvData,
        CheckedSet,
    ),
    String,
> {
    // Does a best-effort job to load a saved state. If it fails, returns None
    let (fetch_profiling, fetch_result) =
        flow_profiling::profiling_js::with_profiling_sync("FetchSavedState", false, |_| {
            match options.saved_state_fetcher {
                SavedStateFetcher::DummyFetcher => {
                    flow_saved_state_fetcher::saved_state_dummy_fetcher::fetch(options)
                }
                SavedStateFetcher::LocalFetcher => {
                    flow_saved_state_fetcher::saved_state_local_fetcher::fetch(options)
                }
                SavedStateFetcher::ScmFetcher => {
                    flow_saved_state_fetcher::saved_state_scm_fetcher::fetch(options)
                }
                SavedStateFetcher::FbFetcher => {
                    #[cfg(fbcode_build)]
                    {
                        flow_facebook_saved_state::saved_state_fb_fetcher::fetch(options)
                    }
                    #[cfg(not(fbcode_build))]
                    {
                        flow_saved_state_fetcher::saved_state_dummy_fetcher::fetch(options)
                    }
                }
            }
        });
    if let Some(profiling) = flow_profiling::profiling_js::current() {
        profiling.merge(&fetch_profiling);
    };
    match fetch_result {
        FetchResult::No_saved_state => Err("No saved state available".to_string()),
        FetchResult::Saved_state_error(msg) => {
            exit_if_no_fallback(Some(&msg), options);
            Err(msg)
        }
        FetchResult::Saved_state {
            saved_state_filename,
            changed_files,
        } => {
            let (saved_state, loaded_heap) = match with_timer(options, "LoadSavedState", || {
                flow_saved_state::load(&saved_state_filename, options)
            }) {
                Ok(loaded) => loaded,
                Err(reason) => {
                    let msg = format!("Failed to load saved state: {}", reason);
                    exit_if_no_fallback(Some(&msg), options);
                    return Err(msg);
                }
            };
            // The saved state arrives as a heap of its own, so everything from here runs against
            // that heap, and nothing publishes it until the orchestrator adopts it.
            let transaction = ActiveTransaction::new(loaded_heap.dupe());
            let updates = match process_saved_state_updates(
                pool,
                &loaded_heap,
                options,
                &transaction,
                &saved_state.global_lib_files.discovered,
                &saved_state.global_lib_files.global_augmentations,
                &changed_files,
            ) {
                Ok(updates) => updates,
                Err(msg) => {
                    flow_hh_logger::error!(
                        "The saved state is no longer valid due to file changes: {}",
                        msg
                    );
                    exit_if_no_fallback(Some(&msg), options);
                    return Err(msg);
                }
            };
            flow_hh_logger::info!(
                "Saved state script reports {} files changed & we care about {} of them",
                changed_files.len(),
                updates.len()
            );
            let mut checked = CheckedSet::empty();
            checked.add(Some(updates), None, None);
            Ok((transaction, loaded_heap, saved_state, checked))
        }
    }
}

/// Builds the server's first committed state: the `Env`, and the heap it was built over.
///
/// The heap comes back rather than being installed. When a saved state supplies one, init runs
/// over that heap start to finish and `committed_heap` is never touched, so an init that gives up
/// leaves nothing behind to undo. Whoever owns the heap decides what to do with the result: for a
/// server that is [`flow_server_env::server_orchestrator::ServerOrchestrator::start`], and for
/// `flow full-check` it is the frame that made the heap in the first place.
pub fn init(
    orchestrator: Option<&ServerOrchestratorHandle>,
    options: &Arc<Options>,
    pool: &ThreadPool,
    committed_heap: &Arc<CommittedHeap>,
    focus_targets: Option<FlowOrdSet<FileKey>>,
) -> Result<(Env, Arc<CommittedHeap>, Option<String>), RecheckError> {
    let start_time = Instant::now();
    let loaded = load_saved_state(pool, options);
    // Init works on the heap the saved state brought, when there was one, so point memory sampling
    // at that heap: its timers should measure the heap being built, not the empty one the
    // orchestrator still has in force.
    let init_heap = match &loaded {
        Ok((transaction, ..)) => transaction.committed_heap(),
        Err(_) => committed_heap.dupe(),
    };
    flow_profiling::memory_utils::with_committed_heap(init_heap.dupe(), || {
        let (transaction, env, libs_ok) = match loaded {
            Ok((transaction, _loaded_heap, saved_state, updates)) => {
                // We loaded a saved state successfully! We are awesome!
                let (env, libs_ok) = init_from_saved_state(
                    pool,
                    &transaction.handle(),
                    options,
                    saved_state,
                    &updates,
                    None,
                );
                let (env, transaction) = handle_updates_since_saved_state(
                    orchestrator,
                    pool,
                    transaction,
                    options,
                    libs_ok,
                    &updates,
                    env,
                )?;
                (transaction, env, libs_ok)
            }
            Err(msg) => {
                // Either there is no saved state or we failed to load it for some reason
                flow_hh_logger::info!("Failed to load saved state: {}", msg);
                let transaction = ActiveTransaction::new(committed_heap.dupe());
                let (env, libs_ok) = init_from_scratch(
                    options,
                    pool,
                    &transaction.handle(),
                    options.root.as_path(),
                    None,
                );
                (transaction, env, libs_ok)
            }
        };
        let init_time = start_time.elapsed().as_secs_f64();
        recheck_stats::init(options, init_time, env.files.len() as i64);

        let (env, first_internal_error): (Env, Option<String>) = if !libs_ok {
            (env, None)
        } else if options.lazy_mode {
            match focus_targets {
                None => configured_libdef_check_for_lazy_init(
                    orchestrator,
                    options,
                    pool,
                    &transaction.handle(),
                    env,
                )?,
                Some(focus_targets) => focus_check_for_init(
                    orchestrator,
                    options,
                    pool,
                    &transaction.handle(),
                    focus_targets,
                    env,
                )?,
            }
        } else {
            full_check_for_init(orchestrator, options, pool, &transaction.handle(), env)?
        };

        let env = EnvTransaction::new(Arc::new(env)).into_env_with_transaction(transaction);
        Ok((env, init_heap, first_internal_error))
    })
}

#[allow(clippy::too_many_arguments)]
fn reinit(
    orchestrator: Option<&ServerOrchestratorHandle>,
    pool: &ThreadPool,
    options: &Arc<Options>,
    allow_fallback: bool,
    reason: &str,
    updates: &CheckedSet,
    files_to_force: CheckedSet,
    will_be_checked_files: &mut CheckedSet,
    env: EnvRef,
) -> Option<(
    Box<dyn FnOnce(&serde_json::Value)>,
    RecheckStats,
    FindRefResults,
    PreparedRecheck,
)> {
    let (transaction, loaded_heap, saved_state, updates_since_saved_state) =
        match load_saved_state(pool, options) {
            Ok(result) => result,
            Err(msg) => {
                // Either there is no saved state or we failed to load it for some reason
                flow_hh_logger::info!("Failed to load saved state: {}", msg);
                if allow_fallback {
                    return None;
                } else {
                    // TODO: fully re-initializing from scratch doesn't make sense. instead, we should
                    // recrawl to find the files that changed (since the file watcher can't tell us)
                    // and recheck all of that.
                    //
                    // for now, we exit and get restarted from scratch like we've done historically.
                    flow_common_exit_status::exit(
                        flow_common_exit_status::FlowExitStatus::FileWatcherMissedChanges,
                    );
                }
            }
        };
    // We loaded a saved state successfully! We are awesome!
    flow_hh_logger::info!("Reinitializing from saved state");
    let (env, _libs_ok) = init_from_saved_state(
        pool,
        &transaction.handle(),
        options,
        saved_state,
        &updates_since_saved_state,
        Some(env.as_ref()),
    );

    let updates_since_saved_state = if !options.lazy_mode || options.saved_state_force_recheck {
        updates_since_saved_state
    } else {
        // In lazy mode, we just want to merge the upstream changes since saved
        // state (as dependencies), so downgrade them.
        let mut downgraded = CheckedSet::empty();
        downgraded.add(None, None, Some(updates_since_saved_state.clone().all()));
        downgraded
    };

    // schedule a recheck of all changes since saved state -- both the upstream
    // changes between when the saved state was generated and master, and local
    // updates since master.
    let mut all_updates = updates_since_saved_state.clone();
    all_updates.union(updates.dupe());
    *will_be_checked_files = all_updates.clone();
    will_be_checked_files.union(files_to_force.dupe());
    if let Some(orchestrator) = orchestrator {
        flow_server_env::server_monitor_listener_state::push_after_reinit(
            orchestrator.recheck(),
            Some(all_updates.dependencies().dupe()),
            Some(all_updates.focused().dupe()),
            Some(files_to_force),
        );
    }

    let reason = reason.to_string();
    let log_recheck_event: Box<dyn FnOnce(&serde_json::Value)> = Box::new(move |profiling| {
        flow_event_logger::reinit(&reason, profiling);
    });
    let recheck_stats = RecheckStats {
        dependent_file_count: 0,
        changed_file_count: 0,
        top_cycle: None,
    };
    let prepared = prepare_recheck(
        EnvTransaction::new(Arc::new(env)),
        transaction,
        Some(loaded_heap),
    );
    Some((log_recheck_event, recheck_stats, Ok(vec![]), prepared))
}

fn reinit_full_check(
    orchestrator: Option<&ServerOrchestratorHandle>,
    pool: &ThreadPool,
    committed_heap: &Arc<CommittedHeap>,
    options: &Arc<Options>,
    updates: &CheckedSet,
    files_to_force: CheckedSet,
    changed_declaration_files: FlowOrdSet<FileKey>,
    will_be_checked_files: &mut CheckedSet,
    env: EnvRef,
) -> Result<
    (
        Box<dyn FnOnce(&serde_json::Value)>,
        RecheckStats,
        FindRefResults,
        PreparedRecheck,
    ),
    RecheckError,
> {
    let transaction = ActiveTransaction::new(committed_heap.dupe());
    transaction.committed_heap().clear_reader_cache();

    let (new_env, libs_ok, log_recheck_event): (Env, bool, Box<dyn FnOnce(&serde_json::Value)>) =
        if changed_declaration_files.is_empty() {
            flow_hh_logger::info!("Reiniting with a full check.");
            let (new_env, libs_ok) = with_transaction("partial-reinit", |_side_effects| {
                init_with_initial_state(
                    pool,
                    &transaction.handle(),
                    options,
                    || env.dependency_info.dupe(),
                    env.errors().duplicate_providers.clone(),
                    None,
                    env.discovered_global_libdefs().clone(),
                    env.global_augmentation_files().dupe(),
                    Some(env.as_ref()),
                    env.files.dupe(),
                    env.unparsed.dupe(),
                    env.package_json_files.dupe(),
                    BTreeSet::new(),
                    env.errors().local_errors.clone(),
                )
            });
            let log: Box<dyn FnOnce(&serde_json::Value)> = Box::new(|profiling| {
                flow_event_logger::reinit_full_check(profiling);
            });
            (new_env, libs_ok, log)
        } else {
            // Note from samzhou19815:
            //
            // This is a more difficult reinit case.
            //
            // When global libdefs change in a complex way (e.g. a d.ts file no longer is a global
            // libdef or become one), it's hard to recover and do a cheaper init_with_initial_state,
            // since figuring things out typically need to at least reparse these affected files.
            //
            // While it's conceivable that we can do this for just .d.ts classification change, it's
            // much harder once we need to support things like `declare global` and `declare module`
            // changes. These changes should not happen a lot.
            // **Therefore, let's just give up and do a full reinit.** ¯\_(ツ)_/¯

            let changed_file_count = changed_declaration_files.len();
            let old_global_count = env.discovered_global_libdefs().len();
            flow_hh_logger::info!(
                "Reiniting with a full check after {changed_file_count} TypeScript global declaration changes."
            );
            let (new_env, libs_ok) = init_from_scratch(
                options,
                pool,
                &transaction.handle(),
                options.root.as_path(),
                Some(env.as_ref()),
            );
            let new_global_count = new_env.discovered_global_libdefs().len();
            let inventory_file_count = new_env.files.len() + new_env.unparsed.len();
            let deleted_file_count = env
                .files
                .iter()
                .chain(env.unparsed.iter())
                .filter(|file| !new_env.files.contains(file) && !new_env.unparsed.contains(file))
                .count();
            flow_hh_logger::info!(
                "Global-lib recovery rebuilt {inventory_file_count} files ({deleted_file_count} gone) and changed discovered globals from {old_global_count} to {new_global_count}."
            );
            let log: Box<dyn FnOnce(&serde_json::Value)> = Box::new(move |profiling| {
                flow_event_logger::global_lib_reinit(
                    changed_file_count,
                    old_global_count,
                    new_global_count,
                    inventory_file_count,
                    deleted_file_count,
                    profiling,
                );
            });
            (new_env, libs_ok, log)
        };

    // A server that starts with libraries that fail checks nothing (see `init`), so do the same
    // until a library change makes them work again. Otherwise schedule a recheck of all changes
    // since saved state -- both the upstream changes between when the saved state was generated
    // and master, and local updates since master.
    if libs_ok {
        let mut all_checked_set = CheckedSet::empty();
        all_checked_set.add(Some(new_env.files.dupe()), None, None);
        all_checked_set.union(files_to_force);
        all_checked_set.union(updates.dupe());
        *will_be_checked_files = all_checked_set.dupe();

        if let Some(orchestrator) = orchestrator {
            flow_server_env::server_monitor_listener_state::push_after_reinit(
                orchestrator.recheck(),
                Some(all_checked_set.focused().dupe()),
                Some(all_checked_set.focused().dupe()),
                Some(all_checked_set),
            );
        }
    } else {
        *will_be_checked_files = CheckedSet::empty();
    }

    let recheck_stats = RecheckStats {
        dependent_file_count: 0,
        changed_file_count: 0,
        top_cycle: None,
    };
    let prepared = prepare_recheck(EnvTransaction::new(Arc::new(new_env)), transaction, None);
    Ok((log_recheck_event, recheck_stats, Ok(vec![]), prepared))
}

#[allow(clippy::too_many_arguments)]
pub fn recheck(
    orchestrator: Option<&ServerOrchestratorHandle>,
    pool: &ThreadPool,
    committed_heap: &Arc<CommittedHeap>,
    options: &Arc<Options>,
    updates: &CheckedSet,
    find_ref_request: &flow_services_references::find_refs_types::Request,
    files_to_force: CheckedSet,
    incompatible_lib_change: Option<
        flow_server_env::server_monitor_listener_state::IncompatibleLibChange,
    >,
    changed_mergebase: Option<bool>,
    missed_changes: bool,
    will_be_checked_files: &mut CheckedSet,
    env: EnvRef,
) -> Result<
    (
        Box<dyn FnOnce(&serde_json::Value)>,
        RecheckStats,
        FindRefResults,
        PreparedRecheck,
    ),
    RecheckError,
> {
    let did_change_mergebase = changed_mergebase.unwrap_or(false);
    if !did_change_mergebase {
        // When the mergebase did not change, the only reinit trigger is a local libdef
        // change. [Recheck_too_slow] cannot fire (it requires `changed_mergebase == Some(true)`),
        // and [missed_changes] without a mergebase change was already handled upstream
        // by adding focused files to [updates] (rechecker.ml).
        if let Some(change) = incompatible_lib_change {
            reinit_full_check(
                orchestrator,
                pool,
                committed_heap,
                options,
                updates,
                files_to_force,
                change.changed_declaration_files,
                will_be_checked_files,
                env,
            )
        } else {
            let heap_transaction = ActiveTransaction::new(committed_heap.dupe());
            let mut env_transaction = EnvTransaction::new(env);
            // Bind the result first: a `&heap_transaction.handle()` temporary in a
            // `match` scrutinee lives until the end of the match, and
            // `commit_with_transaction` requires every handle to be dropped.
            let recheck_result = recheck_impl(
                orchestrator,
                pool,
                &heap_transaction.handle(),
                options,
                updates,
                find_ref_request,
                files_to_force,
                changed_mergebase,
                will_be_checked_files,
                &mut env_transaction,
            );
            match recheck_result {
                Ok((log_recheck_event, recheck_stats, find_ref_results)) => {
                    let prepared = prepare_recheck(env_transaction, heap_transaction, None);
                    Ok((log_recheck_event, recheck_stats, find_ref_results, prepared))
                }
                Err(err) => Err(err),
            }
        }
    } else {
        let reinit_or_restart = |reason: &str,
                                 files_to_force: CheckedSet,
                                 will_be_checked_files: &mut CheckedSet,
                                 env: EnvRef| {
            if options.saved_state_restart_on_reinit {
                flow_hh_logger::info!("Reinit ({}): exiting for a clean restart", reason);
                flow_common_exit_status::exit(flow_common_exit_status::FlowExitStatus::Restart);
            }
            // allow_fallback is false, so reinit exits on error and always returns Some
            reinit(
                orchestrator,
                pool,
                options,
                false,
                reason,
                updates,
                files_to_force,
                will_be_checked_files,
                env,
            )
            .unwrap()
        };
        if let Some(change) = incompatible_lib_change.as_ref()
            && !change.changed_declaration_files.is_empty()
        {
            reinit_full_check(
                orchestrator,
                pool,
                committed_heap,
                options,
                updates,
                files_to_force,
                change.changed_declaration_files.dupe(),
                will_be_checked_files,
                env,
            )
        } else if let Some(change) = incompatible_lib_change {
            // The mergebase changed and a libdef changed. Try loading saved state first -
            // if the saved state was built after the libdef change, we only need to recheck
            // the delta files instead of all files. If saved state loading fails (e.g.
            // because the libdef also changed locally), fall back to reinit_full_check.
            flow_hh_logger::info!(
                "Libdef changed with mergebase change; trying saved-state reinit first"
            );
            match reinit(
                orchestrator,
                pool,
                options,
                true,
                "libdef_change_with_mergebase_change",
                updates,
                files_to_force.dupe(),
                will_be_checked_files,
                env.dupe(),
            ) {
                Some(result) => Ok(result),
                None => {
                    flow_hh_logger::info!(
                        "Saved-state reinit failed; falling back to reinit_full_check"
                    );
                    reinit_full_check(
                        orchestrator,
                        pool,
                        committed_heap,
                        options,
                        updates,
                        files_to_force,
                        change.changed_declaration_files,
                        will_be_checked_files,
                        env,
                    )
                }
            }
        } else if missed_changes {
            Ok(reinit_or_restart(
                "missed_changes",
                files_to_force,
                will_be_checked_files,
                env,
            ))
        } else {
            let env_for_reinit = env.dupe();
            let files_to_force_for_reinit = files_to_force.dupe();
            let heap_transaction = ActiveTransaction::new(committed_heap.dupe());
            let mut env_transaction = EnvTransaction::new(env);
            // Bind the result first: a `&heap_transaction.handle()` temporary in a
            // `match` scrutinee lives until the end of the match, and
            // `commit_with_transaction` requires every handle to be dropped.
            let recheck_result = recheck_impl(
                orchestrator,
                pool,
                &heap_transaction.handle(),
                options,
                updates,
                find_ref_request,
                files_to_force,
                changed_mergebase,
                will_be_checked_files,
                &mut env_transaction,
            );
            match recheck_result {
                Ok((log_recheck_event, recheck_stats, find_ref_results)) => {
                    let prepared = prepare_recheck(env_transaction, heap_transaction, None);
                    Ok((log_recheck_event, recheck_stats, find_ref_results, prepared))
                }
                Err(RecheckError::TooSlow) => {
                    drop(heap_transaction);
                    Ok(reinit_or_restart(
                        "recheck_too_slow",
                        files_to_force_for_reinit,
                        will_be_checked_files,
                        env_for_reinit,
                    ))
                }
                Err(RecheckError::Canceled(changed_files)) => {
                    Err(RecheckError::Canceled(changed_files))
                }
            }
        }
    }
}

pub fn check_files_for_init(
    orchestrator: Option<&ServerOrchestratorHandle>,
    options: &Arc<Options>,
    pool: &ThreadPool,
    transaction: &Arc<Transaction>,
    parsed: FlowOrdSet<FileKey>,
    message: &str,
    env: Env,
) -> Result<(Env, Option<String> /* check_internal_error */), RecheckError> {
    with_transaction_result(message, |_transaction| -> Result<_, RecheckError> {
        let Env {
            dependency_info,
            errors: env_errors,
            coverage: env_coverage,
            collated_errors: env_collated_errors,
            master_cx,
            files: env_files,
            checked_files: env_checked_files,
            package_json_files,
            ordered_libs,
            global_lib_files,
            unparsed,
            connections,
            exports,
        } = env;
        let env_errors_cell = env_errors.dupe();
        let env_coverage_cell = env_coverage.dupe();
        let env_collated_errors_cell = env_collated_errors.dupe();
        let dependency_info_cell = dependency_info.dupe();
        let dependency_info = dependency_info.read_arc();
        let env_errors = OverlayErrors::new(env_errors);
        let env_coverage = overlay_coverage(env_coverage);
        let env_collated_errors = OverlayCollatedErrors::new(env_collated_errors);

        let to_infer = files_to_infer(options, parsed);
        let implementation_dependency_graph = dependency_info.implementation_dependency_graph();
        let sig_dependency_graph = dependency_info.sig_dependency_graph();
        let (to_merge, to_check, components, recheck_set) = include_dependencies_and_dependents(
            options,
            to_infer,
            CheckedSet::empty(),
            FlowOrdSet::new(),
            implementation_dependency_graph,
            sig_dependency_graph,
        );
        ensure_parsed_or_trigger_recheck(
            orchestrator,
            pool,
            transaction,
            options,
            to_merge.dupe().all(),
            ParseProgress::Report,
        )?;
        let merge_result = merge(
            pool,
            transaction,
            options,
            false, // for_find_all_refs: init path doesn't use find-all-refs
            &to_merge,
            components,
            &recheck_set,
            sig_dependency_graph,
            env_errors.suppressions.clone(),
        )?;
        let (
            mut errors,
            coverage,
            _find_ref_results,
            _time_to_check_merged,
            _skipped,
            _slowest_file,
            _num_slow_files,
            check_internal_error,
        ) = check_files::check_files(
            options.dupe(),
            pool,
            transaction,
            global_lib_files.configured().dupe(),
            &flow_services_references::find_refs_types::empty_request(),
            env_errors,
            merge_result.suppressions.clone(),
            env_coverage,
            to_check.dupe(),
            FlowOrdSet::new(),
            merge_result.sig_new_or_changed.dupe(),
            implementation_dependency_graph,
            master_cx.dupe(),
        )?;
        if let Some(ref err) = check_internal_error {
            flow_hh_logger::error!("{}", err);
        }
        // Deduplicate ETrivialRecursiveDefinition errors from cyclic
        // dependencies. In OCaml, the single-threaded check phase shares a
        // check cache so cycle errors are generated only once. In the Rust
        // port, multi-threaded checking means each thread has its own check
        // cache, so cycle members checked on different threads independently
        // detect the cycle and each generate an error.
        //
        // Fix: find files that both have ETrivialRecursiveDefinition errors
        // AND are mutually dependent (same cycle). Keep only the error from
        // the lexicographically smallest file in each cycle group.
        {
            use flow_typing_errors::error_message::ErrorMessage;
            let dep_graph = dependency_info.implementation_dependency_graph();
            let files_with_cycle_errors: BTreeSet<FileKey> = errors
                .merge_errors
                .iter()
                .filter(|(_, errs)| {
                    errs.iter().any(|e| {
                        matches!(
                            e.msg_of_error(),
                            ErrorMessage::ETrivialRecursiveDefinition(..)
                        )
                    })
                })
                .map(|(f, _)| f.dupe())
                .collect();
            if files_with_cycle_errors.len() > 1 {
                let mut files_to_strip: BTreeSet<FileKey> = BTreeSet::new();
                for file in &files_with_cycle_errors {
                    if let Some(deps) = dep_graph.find_opt(file) {
                        for dep in deps {
                            if files_with_cycle_errors.contains(dep) && file.as_str() > dep.as_str()
                            {
                                files_to_strip.insert(file.dupe());
                            }
                        }
                    }
                }
                for file in &files_to_strip {
                    if let Some(errs) = errors.merge_errors.get(file) {
                        let filtered = errs.dupe().filter(|e| {
                            !matches!(
                                e.msg_of_error(),
                                ErrorMessage::ETrivialRecursiveDefinition(..)
                            )
                        });
                        errors.merge_errors.insert(file.dupe(), filtered);
                    }
                }
            }
        }

        // Update collated errors
        let mut collated_errors = env_collated_errors;
        with_timer(options, "CollateErrors", || {
            let loc_of_aloc = |loc: &ALoc| -> Loc { transaction.loc_of_aloc(loc) };
            let transaction_for_ast = transaction.dupe();
            let get_ast = move |file: &FileKey| -> Option<Arc<Program<Loc, Loc>>> {
                transaction_for_ast.get_ast(file)
            };
            error_collator::update_collated_overlay_errors(
                &loc_of_aloc,
                &get_ast,
                &|file| global_lib_files.contains(file),
                options,
                &to_merge,
                &merge_result.suppressions,
                &errors,
                &mut collated_errors,
            );
            error_collator::update_error_state_timestamps(&mut collated_errors);
        });
        errors.commit();
        let coverage = coverage.into_delta();
        {
            let mut base = env_coverage_cell.write();
            apply_delta_to_base_map(&mut base, coverage);
        }
        collated_errors.commit();
        let env = Env {
            files: env_files,
            dependency_info: dependency_info_cell,
            checked_files: {
                let mut checked_files = to_merge;
                checked_files.union(env_checked_files);
                checked_files
            },
            package_json_files,
            ordered_libs,
            global_lib_files,
            unparsed,
            errors: env_errors_cell,
            coverage: env_coverage_cell,
            collated_errors: env_collated_errors_cell,
            connections,
            exports,
            master_cx,
        };

        flow_hh_logger::info!(
            "Checked set: {}",
            env.checked_files.debug_counts_to_string()
        );

        Ok((env, check_internal_error))
    })
}

fn lazy_libdef_file_keys(
    configured_libs: &BTreeSet<FlowSmolStr>,
    discovered_globals: impl IntoIterator<Item = FileKey>,
) -> FlowOrdSet<FileKey> {
    configured_libs
        .iter()
        .map(|path| FileKey::lib_file_of_absolute(path))
        .chain(discovered_globals)
        .collect()
}

fn configured_libdef_check_for_lazy_init(
    orchestrator: Option<&ServerOrchestratorHandle>,
    options: &Arc<Options>,
    pool: &ThreadPool,
    transaction: &Arc<Transaction>,
    env: Env,
) -> Result<(Env, Option<String>), RecheckError> {
    let configured_libs = env.configured_libs();
    let parsed = lazy_libdef_file_keys(
        &configured_libs,
        env.discovered_global_libdefs().iter().duped(),
    );
    libdef_check_for_lazy_init(orchestrator, options, pool, transaction, parsed, env)
}

pub fn libdef_check_for_lazy_init(
    orchestrator: Option<&ServerOrchestratorHandle>,
    options: &Arc<Options>,
    pool: &ThreadPool,
    transaction: &Arc<Transaction>,
    parsed: FlowOrdSet<FileKey>,
    env: Env,
) -> Result<(Env, Option<String>), RecheckError> {
    check_files_for_init(
        orchestrator,
        options,
        pool,
        transaction,
        parsed,
        "lazy init check",
        env,
    )
}

pub fn focus_check_for_init(
    orchestrator: Option<&ServerOrchestratorHandle>,
    options: &Arc<Options>,
    pool: &ThreadPool,
    transaction: &Arc<Transaction>,
    focus_targets: FlowOrdSet<FileKey>,
    env: Env,
) -> Result<(Env, Option<String>), RecheckError> {
    let (env, first_internal_error) =
        configured_libdef_check_for_lazy_init(orchestrator, options, pool, transaction, env)?;
    let files_to_force = {
        let mut checked_set = CheckedSet::empty();
        checked_set.add(Some(focus_targets), None, None);
        checked_set
    };
    let mut will_be_checked_files = files_to_force.dupe();
    let mut env_transaction = EnvTransaction::new(Arc::new(env));
    let (_, _, _) = recheck_impl(
        orchestrator,
        pool,
        transaction,
        options,
        &CheckedSet::empty(),
        &flow_services_references::find_refs_types::empty_request(),
        files_to_force,
        None,
        &mut will_be_checked_files,
        &mut env_transaction,
    )?;
    Ok((env_transaction.into_env(), first_internal_error))
}

pub fn full_check_for_init(
    orchestrator: Option<&ServerOrchestratorHandle>,
    options: &Arc<Options>,
    pool: &ThreadPool,
    transaction: &Arc<Transaction>,
    env: Env,
) -> Result<(Env, Option<String>), RecheckError> {
    let parsed = env.files.dupe();
    check_files_for_init(
        orchestrator,
        options,
        pool,
        transaction,
        parsed,
        "full check",
        env,
    )
}

pub fn check_once(
    options: Arc<Options>,
    pool: &ThreadPool,
    committed_heap: &Arc<CommittedHeap>,
    root: &Path,
    focus_targets: Option<FlowOrdSet<FileKey>>,
) -> (
    ConcreteLocPrintableErrorSet,
    ConcreteLocPrintableErrorSet,
    Vec<(PrintableError<Loc>, BTreeSet<Loc>)>,
    Option<String>, /* lazy_msg */
) {
    let total_start = Instant::now();
    let transaction = ActiveTransaction::new(committed_heap.dupe());

    // OCaml full-check still routes through the dummy saved-state fetcher, whose
    // fetch function contributes a `FetchSavedState` child profile.
    with_timer(&options, "FetchSavedState", || {});

    let (env, libs_ok) = init_from_scratch(&options, pool, &transaction.handle(), root, None);
    let env = if libs_ok {
        let (env, _first_internal_error) = if options.lazy_mode {
            match focus_targets {
                None => configured_libdef_check_for_lazy_init(
                    None,
                    &options,
                    pool,
                    &transaction.handle(),
                    env,
                ),
                Some(focus_targets) => focus_check_for_init(
                    None,
                    &options,
                    pool,
                    &transaction.handle(),
                    focus_targets,
                    env,
                ),
            }
        } else {
            full_check_for_init(None, &options, pool, &transaction.handle(), env)
        }
        .expect("Unexpected file changes during full check");
        env
    } else {
        env
    };
    let env = EnvTransaction::new(Arc::new(env)).into_env_with_transaction(transaction);
    let transaction = ActiveTransaction::new(committed_heap.dupe());

    let total_time = total_start.elapsed();
    if !options.quiet && !options.profile {
        eprintln!("Total:              {:6.2}s", total_time.as_secs_f64());
    }

    let lazy_msg = if options.lazy_mode {
        let checked_files =
            (env.checked_files.focused_cardinal() + env.checked_files.dependents_cardinal()) as i32;
        let total_files = env.files.len() as i32;
        Some(format!("Checked {}/{} files.", checked_files, total_files))
    } else {
        None
    };

    let (errors, warnings, suppressed_errors) = error_collator::get(&env);
    let strip_root = if options.strip_root {
        Some(options.root.as_path())
    } else {
        None
    };
    let loc_of_aloc = |aloc: &ALoc| -> Loc { transaction.loc_of_aloc(aloc) };
    let get_ast = |file: &FileKey| -> Option<Arc<Program<Loc, Loc>>> { transaction.get_ast(file) };
    let suppressed_errors = if options.include_suppressions {
        suppressed_errors
            .into_iter()
            .map(|(e, loc_set)| {
                (
                    flow_typing_errors::intermediate_error::to_printable_error(
                        &loc_of_aloc,
                        get_ast,
                        strip_root,
                        e,
                    ),
                    loc_set,
                )
            })
            .collect()
    } else {
        vec![]
    };
    (errors, warnings, suppressed_errors, lazy_msg)
}

pub fn debug_determine_what_to_recheck(
    options: &Options,
    sig_dependency_graph: &Graph<FileKey>,
    implementation_dependency_graph: &Graph<FileKey>,
    freshparsed: &CheckedSet,
    unchanged_checked: CheckedSet,
    unchanged_files_to_force: &CheckedSet,
    dirty_direct_dependents: &FlowOrdSet<FileKey>,
) -> DetermineWhatToRecheckResult {
    recheck::determine_what_to_recheck(
        options,
        sig_dependency_graph,
        implementation_dependency_graph,
        freshparsed,
        unchanged_checked,
        unchanged_files_to_force,
        dirty_direct_dependents,
    )
    .expect("debug dependency calculation should not be canceled")
}

pub fn debug_include_dependencies_and_dependents(
    options: &Options,
    unchanged_checked: CheckedSet,
    input: CheckedSet,
    implementation_dependency_graph: &Graph<FileKey>,
    sig_dependency_graph: &Graph<FileKey>,
    all_dependent_files: FlowOrdSet<FileKey>,
) -> (
    CheckedSet,
    CheckedSet,
    Vec<Vec1<FileKey>>,
    FlowOrdSet<FileKey>,
) {
    include_dependencies_and_dependents(
        options,
        input,
        unchanged_checked,
        all_dependent_files,
        implementation_dependency_graph,
        sig_dependency_graph,
    )
}
