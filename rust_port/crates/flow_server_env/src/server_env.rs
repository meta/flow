/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use dupe::Dupe;
use flow_common::files;
use flow_common::options::Options;
use flow_common_utils::checked_set::CheckedSet;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_data_structure_wrapper::overlay_map::EnvCell;
use flow_data_structure_wrapper::overlay_map::EnvCellMapBase;
pub use flow_data_structure_wrapper::overlay_map::EnvCellReadGuard;
use flow_data_structure_wrapper::overlay_map::OverlayMap;
use flow_data_structure_wrapper::overlay_map::apply_delta_to_base_map;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_heap::parsing_heaps::ActiveTransaction;
use flow_parser::file_key::FileKey;
use flow_services_coverage::FileCoverage;
use flow_services_export::export_search::ExportSearch;
use flow_typing_context::MasterContext;
use flow_typing_errors::error_suppressions::ErrorSuppressions;
use flow_typing_errors::flow_error::ErrorSet;
use flow_utils_concurrency::thread_pool::ThreadPool;
use vec1::Vec1;

use crate::collated_errors::CollatedErrors;
use crate::collated_errors::OverlayCollatedErrors;
use crate::dependency_info::DependencyInfo;
use crate::dependency_info::OverlayDependencyInfo;
use crate::persistent_connection::PersistentConnection;
use crate::server_prot::response::LazyStats;

// The "static" environment, initialized first and then unchanged.

pub struct Genv {
    pub options: Arc<Options>,
    pub workers: Option<ThreadPool>,
}

// The environment constantly maintained by the server.

// Do not change these to contain `Loc`s. Because these errors are stored between rechecks, it is
// critical that they contain `ALoc`s, so that we can update the concrete locations when we
// render the errors, without having to retypecheck the files that generated those errors.
#[derive(Clone)]
pub struct Errors {
    /// errors are stored in a map from file path to error set, so that the errors
    /// from checking particular files can be cleared during recheck.
    pub local_errors: BTreeMap<FileKey, ErrorSet>,
    /// duplicate providers found during commit_modules are stored separately so
    /// they can be cleared easily
    pub duplicate_providers: BTreeMap<FlowSmolStr, (FileKey, Vec1<FileKey>)>,
    /// errors encountered during merge have to be stored separately so
    /// dependencies can be cleared during merge.
    pub merge_errors: BTreeMap<FileKey, ErrorSet>,
    /// warnings are stored in a map from file path to error set, so that the warnings
    /// from checking particular files can be cleared during recheck.
    pub warnings: BTreeMap<FileKey, ErrorSet>,
    /// error suppressions in the code
    pub suppressions: ErrorSuppressions,
}

impl Errors {
    pub fn empty() -> Self {
        Self {
            local_errors: BTreeMap::new(),
            duplicate_providers: BTreeMap::new(),
            merge_errors: BTreeMap::new(),
            warnings: BTreeMap::new(),
            suppressions: ErrorSuppressions::empty(),
        }
    }
}

impl Default for Errors {
    fn default() -> Self {
        Self::empty()
    }
}

pub type OverlayErrorMap = OverlayMap<FileKey, ErrorSet, EnvCellMapBase<Errors, FileKey, ErrorSet>>;
pub type OverlayDuplicateProviders = OverlayMap<
    FlowSmolStr,
    (FileKey, Vec1<FileKey>),
    EnvCellMapBase<Errors, FlowSmolStr, (FileKey, Vec1<FileKey>)>,
>;

#[derive(Clone)]
pub struct OverlayErrors {
    pub base_cell: Arc<EnvCell<Errors>>,
    pub local_errors: OverlayErrorMap,
    pub duplicate_providers: OverlayDuplicateProviders,
    pub merge_errors: OverlayErrorMap,
    pub warnings: OverlayErrorMap,
    pub suppressions: ErrorSuppressions,
}

impl OverlayErrors {
    pub fn new(base_cell: Arc<EnvCell<Errors>>) -> Self {
        let suppressions = base_cell.read().suppressions.clone();
        Self {
            base_cell: base_cell.dupe(),
            local_errors: OverlayMap::with_base(EnvCellMapBase::new(base_cell.dupe(), |errors| {
                &errors.local_errors
            })),
            duplicate_providers: OverlayMap::with_base(EnvCellMapBase::new(
                base_cell.dupe(),
                |errors| &errors.duplicate_providers,
            )),
            merge_errors: OverlayMap::with_base(EnvCellMapBase::new(base_cell.dupe(), |errors| {
                &errors.merge_errors
            })),
            warnings: OverlayMap::with_base(EnvCellMapBase::new(base_cell, |errors| {
                &errors.warnings
            })),
            suppressions,
        }
    }

    pub fn from_errors(errors: Errors) -> Self {
        Self::new(env_cell(errors))
    }

    pub fn commit(self) {
        let OverlayErrors {
            base_cell,
            local_errors,
            duplicate_providers,
            merge_errors,
            warnings,
            suppressions,
        } = self;
        let local_errors = local_errors.into_delta();
        let duplicate_providers = duplicate_providers.into_delta();
        let merge_errors = merge_errors.into_delta();
        let warnings = warnings.into_delta();

        let mut errors = base_cell.write();
        apply_delta_to_base_map(&mut errors.local_errors, local_errors);
        apply_delta_to_base_map(&mut errors.duplicate_providers, duplicate_providers);
        apply_delta_to_base_map(&mut errors.merge_errors, merge_errors);
        apply_delta_to_base_map(&mut errors.warnings, warnings);
        errors.suppressions = suppressions;
    }
}

pub type Coverage = BTreeMap<FileKey, FileCoverage>;
pub type OverlayCoverage =
    OverlayMap<FileKey, FileCoverage, EnvCellMapBase<Coverage, FileKey, FileCoverage>>;

#[derive(Clone, Dupe)]
pub struct GlobalLibFiles {
    /// Configured library paths encountered during parsing.
    configured: Arc<BTreeSet<FlowSmolStr>>,
    /// Source files classified as global TypeScript declaration libraries.
    discovered: Arc<BTreeSet<FileKey>>,
    /// Source modules containing TypeScript `declare global` blocks.
    global_augmentations: FlowOrdSet<FileKey>,
}

impl GlobalLibFiles {
    pub fn new(
        configured: Arc<BTreeSet<FlowSmolStr>>,
        discovered: Arc<BTreeSet<FileKey>>,
        global_augmentations: FlowOrdSet<FileKey>,
    ) -> Self {
        Self {
            configured,
            discovered,
            global_augmentations,
        }
    }

    pub fn configured(&self) -> Arc<BTreeSet<FlowSmolStr>> {
        self.configured.dupe()
    }

    pub fn discovered(&self) -> &Arc<BTreeSet<FileKey>> {
        &self.discovered
    }

    pub fn global_augmentations(&self) -> &FlowOrdSet<FileKey> {
        &self.global_augmentations
    }

    pub fn contains(&self, file: &FileKey) -> bool {
        files::is_lib_file(&self.configured, file) || self.discovered.contains(file)
    }
}

pub struct Env {
    /// All the files that we at least parse (includes libs).
    pub files: FlowOrdSet<FileKey>,
    pub dependency_info: Arc<EnvCell<DependencyInfo>>,
    /// All the current files we typecheck.
    pub checked_files: CheckedSet,
    /// package.json files
    pub package_json_files: FlowOrdSet<FileKey>,
    /// The configured global lib files, in their merge order.
    pub ordered_libs: Arc<Vec<FlowSmolStr>>,
    pub global_lib_files: GlobalLibFiles,
    /// The files which didn't parse (skipped or errored)
    pub unparsed: FlowOrdSet<FileKey>,
    pub errors: Arc<EnvCell<Errors>>,
    pub coverage: Arc<EnvCell<Coverage>>,
    pub collated_errors: Arc<EnvCell<CollatedErrors>>,
    pub connections: PersistentConnection,
    /// None means auto-imports are not enabled
    pub exports: Option<ExportSearch>,
    pub master_cx: Arc<MasterContext>,
}

pub type EnvRef = Arc<Env>;

impl Env {
    /// Report only focused + dependents as "checked" files. Dependencies are only merged
    /// (signatures computed) but not type-checked, so they shouldn't count toward the
    /// user-visible "checking N files" number.
    pub fn lazy_stats(&self, options: &Options) -> LazyStats {
        let checked_files = (self.checked_files.focused_cardinal()
            + self.checked_files.dependents_cardinal()) as i32;
        LazyStats {
            lazy_mode: options.lazy_mode,
            checked_files,
            total_files: self.files.len() as i32,
        }
    }

    pub fn configured_libs(&self) -> Arc<BTreeSet<FlowSmolStr>> {
        self.global_lib_files.configured()
    }

    pub fn discovered_global_libdefs(&self) -> &BTreeSet<FileKey> {
        self.global_lib_files.discovered()
    }

    pub fn global_augmentation_files(&self) -> &FlowOrdSet<FileKey> {
        self.global_lib_files.global_augmentations()
    }

    /// Returns whether a file contributes declarations to the global library scope.
    pub fn is_lib_file(&self, file: &FileKey) -> bool {
        self.global_lib_files.contains(file)
    }

    pub fn dependency_info(&self) -> EnvCellReadGuard<DependencyInfo> {
        self.dependency_info.read_arc()
    }

    pub fn errors(&self) -> EnvCellReadGuard<Errors> {
        self.errors.read_arc()
    }

    pub fn coverage(&self) -> EnvCellReadGuard<Coverage> {
        self.coverage.read_arc()
    }

    pub fn collated_errors(&self) -> EnvCellReadGuard<CollatedErrors> {
        self.collated_errors.read_arc()
    }
}

pub struct EnvTransaction {
    env: EnvRef,
    files: Option<FlowOrdSet<FileKey>>,
    dependency_info: Option<OverlayDependencyInfo>,
    checked_files: Option<CheckedSet>,
    package_json_files: Option<FlowOrdSet<FileKey>>,
    ordered_libs: Option<Vec<FlowSmolStr>>,
    global_lib_files: Option<GlobalLibFiles>,
    unparsed: Option<FlowOrdSet<FileKey>>,
    errors: Option<OverlayErrors>,
    coverage: Option<OverlayCoverage>,
    collated_errors: Option<OverlayCollatedErrors>,
    connections: Option<PersistentConnection>,
    exports: Option<Option<ExportSearch>>,
    master_cx: Option<Arc<MasterContext>>,
}

impl EnvTransaction {
    pub fn new(env: EnvRef) -> Self {
        Self {
            env,
            files: None,
            dependency_info: None,
            checked_files: None,
            package_json_files: None,
            ordered_libs: None,
            global_lib_files: None,
            unparsed: None,
            errors: None,
            coverage: None,
            collated_errors: None,
            connections: None,
            exports: None,
            master_cx: None,
        }
    }

    pub fn files(&self) -> &FlowOrdSet<FileKey> {
        self.files.as_ref().unwrap_or(&self.env.files)
    }

    pub fn dependency_info(&mut self) -> &OverlayDependencyInfo {
        if self.dependency_info.is_none() {
            self.dependency_info =
                Some(OverlayDependencyInfo::new(self.env.dependency_info.dupe()));
        }
        self.dependency_info
            .as_ref()
            .expect("dependency info initialized")
    }

    pub fn dependency_info_mut(&mut self) -> &mut OverlayDependencyInfo {
        if self.dependency_info.is_none() {
            self.dependency_info =
                Some(OverlayDependencyInfo::new(self.env.dependency_info.dupe()));
        }
        self.dependency_info
            .as_mut()
            .expect("dependency info initialized")
    }

    pub fn checked_files(&self) -> &CheckedSet {
        self.checked_files
            .as_ref()
            .unwrap_or(&self.env.checked_files)
    }

    pub fn package_json_files(&self) -> &FlowOrdSet<FileKey> {
        self.package_json_files
            .as_ref()
            .unwrap_or(&self.env.package_json_files)
    }

    pub fn ordered_libs(&self) -> &[FlowSmolStr] {
        self.ordered_libs
            .as_ref()
            .map_or(self.env.ordered_libs.as_slice(), |libs| libs.as_slice())
    }

    pub fn global_lib_files(&self) -> &GlobalLibFiles {
        self.global_lib_files
            .as_ref()
            .unwrap_or(&self.env.global_lib_files)
    }

    pub fn configured_libs(&self) -> &BTreeSet<FlowSmolStr> {
        &self.global_lib_files().configured
    }

    pub fn discovered_global_libdefs(&self) -> &BTreeSet<FileKey> {
        self.global_lib_files().discovered()
    }

    pub fn global_augmentation_files(&self) -> &FlowOrdSet<FileKey> {
        self.global_lib_files().global_augmentations()
    }

    /// Returns whether a file contributes declarations to the global library scope.
    pub fn is_lib_file(&self, file: &FileKey) -> bool {
        self.global_lib_files().contains(file)
    }

    pub fn unparsed(&self) -> &FlowOrdSet<FileKey> {
        self.unparsed.as_ref().unwrap_or(&self.env.unparsed)
    }

    pub fn errors(&mut self) -> &OverlayErrors {
        if self.errors.is_none() {
            self.errors = Some(OverlayErrors::new(self.env.errors.dupe()));
        }
        self.errors.as_ref().expect("errors initialized")
    }

    pub fn coverage(&mut self) -> &OverlayCoverage {
        if self.coverage.is_none() {
            self.coverage = Some(overlay_coverage(self.env.coverage.dupe()));
        }
        self.coverage.as_ref().expect("coverage initialized")
    }

    pub fn collated_errors(&mut self) -> &OverlayCollatedErrors {
        if self.collated_errors.is_none() {
            self.collated_errors =
                Some(OverlayCollatedErrors::new(self.env.collated_errors.dupe()));
        }
        self.collated_errors
            .as_ref()
            .expect("collated errors initialized")
    }

    pub fn connections(&self) -> &PersistentConnection {
        self.connections.as_ref().unwrap_or(&self.env.connections)
    }

    pub fn exports(&self) -> Option<&ExportSearch> {
        self.exports.as_ref().unwrap_or(&self.env.exports).as_ref()
    }

    pub fn master_cx(&self) -> &Arc<MasterContext> {
        self.master_cx.as_ref().unwrap_or(&self.env.master_cx)
    }

    pub fn take_errors(&mut self) -> OverlayErrors {
        self.errors
            .take()
            .unwrap_or_else(|| OverlayErrors::new(self.env.errors.dupe()))
    }

    pub fn take_coverage(&mut self) -> OverlayCoverage {
        self.coverage
            .take()
            .unwrap_or_else(|| overlay_coverage(self.env.coverage.dupe()))
    }

    pub fn take_collated_errors(&mut self) -> OverlayCollatedErrors {
        self.collated_errors
            .take()
            .unwrap_or_else(|| OverlayCollatedErrors::new(self.env.collated_errors.dupe()))
    }

    pub fn take_dependency_info(&mut self) -> OverlayDependencyInfo {
        self.dependency_info
            .take()
            .unwrap_or_else(|| OverlayDependencyInfo::new(self.env.dependency_info.dupe()))
    }

    pub fn set_files(&mut self, files: FlowOrdSet<FileKey>) {
        self.files = Some(files);
    }

    pub fn set_checked_files(&mut self, checked_files: CheckedSet) {
        self.checked_files = Some(checked_files);
    }

    pub fn set_unparsed(&mut self, unparsed: FlowOrdSet<FileKey>) {
        self.unparsed = Some(unparsed);
    }

    pub fn set_package_json_files(&mut self, package_json_files: FlowOrdSet<FileKey>) {
        self.package_json_files = Some(package_json_files);
    }

    pub fn set_configured_global_libs(&mut self, configured: BTreeSet<FlowSmolStr>) {
        self.global_lib_files = Some(GlobalLibFiles::new(
            Arc::new(configured),
            self.global_lib_files().discovered().dupe(),
            self.global_lib_files().global_augmentations().dupe(),
        ));
    }

    pub fn set_errors(&mut self, errors: OverlayErrors) {
        self.errors = Some(errors);
    }

    pub fn set_coverage(&mut self, coverage: OverlayCoverage) {
        self.coverage = Some(coverage);
    }

    pub fn set_collated_errors(&mut self, collated_errors: OverlayCollatedErrors) {
        self.collated_errors = Some(collated_errors);
    }

    pub fn set_dependency_info(&mut self, dependency_info: OverlayDependencyInfo) {
        self.dependency_info = Some(dependency_info);
    }

    pub fn set_connections(&mut self, connections: PersistentConnection) {
        self.connections = Some(connections);
    }

    pub fn set_exports(&mut self, exports: Option<ExportSearch>) {
        self.exports = Some(exports);
    }

    pub fn commit(self) -> EnvRef {
        let EnvTransaction {
            env,
            files,
            dependency_info,
            checked_files,
            package_json_files,
            ordered_libs,
            global_lib_files,
            unparsed,
            errors,
            coverage,
            collated_errors,
            connections,
            exports,
            master_cx,
        } = self;
        let needs_new_env = files.is_some()
            || checked_files.is_some()
            || package_json_files.is_some()
            || ordered_libs.is_some()
            || global_lib_files.is_some()
            || unparsed.is_some()
            || connections.is_some()
            || exports.is_some()
            || master_cx.is_some();

        if let Some(dependency_info) = dependency_info {
            dependency_info.commit();
        }
        if let Some(errors) = errors {
            errors.commit();
        }
        if let Some(coverage) = coverage {
            let coverage = coverage.into_delta();
            let mut base = env.coverage.write();
            apply_delta_to_base_map(&mut base, coverage);
        }
        if let Some(collated_errors) = collated_errors {
            collated_errors.commit();
        }
        if needs_new_env {
            Arc::new(Env {
                files: files.unwrap_or_else(|| env.files.dupe()),
                dependency_info: env.dependency_info.dupe(),
                checked_files: checked_files.unwrap_or_else(|| env.checked_files.dupe()),
                package_json_files: package_json_files
                    .unwrap_or_else(|| env.package_json_files.dupe()),
                ordered_libs: ordered_libs
                    .map(Arc::new)
                    .unwrap_or_else(|| env.ordered_libs.dupe()),
                global_lib_files: global_lib_files.unwrap_or_else(|| env.global_lib_files.dupe()),
                unparsed: unparsed.unwrap_or_else(|| env.unparsed.dupe()),
                errors: env.errors.dupe(),
                coverage: env.coverage.dupe(),
                collated_errors: env.collated_errors.dupe(),
                connections: connections.unwrap_or_else(|| env.connections.dupe()),
                exports: exports.unwrap_or_else(|| env.exports.dupe()),
                master_cx: master_cx.unwrap_or_else(|| env.master_cx.dupe()),
            })
        } else {
            env
        }
    }

    /// Commits the heap and Env fields together at the consuming transaction boundary.
    pub fn commit_with_transaction(self, transaction: ActiveTransaction) -> EnvRef {
        transaction.commit();
        self.commit()
    }

    pub fn into_env(self) -> Env {
        let EnvTransaction {
            env,
            files,
            dependency_info,
            checked_files,
            package_json_files,
            ordered_libs,
            global_lib_files,
            unparsed,
            errors,
            coverage,
            collated_errors,
            connections,
            exports,
            master_cx,
        } = self;

        if let Some(dependency_info) = dependency_info {
            dependency_info.commit();
        }
        if let Some(errors) = errors {
            errors.commit();
        }
        if let Some(coverage) = coverage {
            let coverage = coverage.into_delta();
            let mut base = env.coverage.write();
            apply_delta_to_base_map(&mut base, coverage);
        }
        if let Some(collated_errors) = collated_errors {
            collated_errors.commit();
        }
        Env {
            files: files.unwrap_or_else(|| env.files.dupe()),
            dependency_info: env.dependency_info.dupe(),
            checked_files: checked_files.unwrap_or_else(|| env.checked_files.dupe()),
            package_json_files: package_json_files.unwrap_or_else(|| env.package_json_files.dupe()),
            ordered_libs: ordered_libs
                .map(Arc::new)
                .unwrap_or_else(|| env.ordered_libs.dupe()),
            global_lib_files: global_lib_files.unwrap_or_else(|| env.global_lib_files.dupe()),
            unparsed: unparsed.unwrap_or_else(|| env.unparsed.dupe()),
            errors: env.errors.dupe(),
            coverage: env.coverage.dupe(),
            collated_errors: env.collated_errors.dupe(),
            connections: connections.unwrap_or_else(|| env.connections.dupe()),
            exports: exports.unwrap_or_else(|| env.exports.dupe()),
            master_cx: master_cx.unwrap_or_else(|| env.master_cx.dupe()),
        }
    }

    /// Commits the heap and consumes the remaining Env transaction into an owned Env.
    pub fn into_env_with_transaction(self, transaction: ActiveTransaction) -> Env {
        transaction.commit();
        self.into_env()
    }
}

pub fn env_cell<T>(value: T) -> Arc<EnvCell<T>> {
    Arc::new(EnvCell::new(value))
}

pub fn overlay_coverage(cell: Arc<EnvCell<Coverage>>) -> OverlayCoverage {
    OverlayMap::with_base(EnvCellMapBase::identity(cell))
}

pub(crate) fn with_connections(env: EnvRef, connections: PersistentConnection) -> EnvRef {
    let mut transaction = EnvTransaction::new(env);
    transaction.set_connections(connections);
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use flow_common_utils::graph::GraphLike;
    use flow_heap::heap_state::CommittedHeap;

    use super::*;
    use crate::dependency_info::PartialDependencyGraph;

    fn empty_errors() -> Errors {
        Errors {
            local_errors: BTreeMap::new(),
            duplicate_providers: BTreeMap::new(),
            merge_errors: BTreeMap::new(),
            warnings: BTreeMap::new(),
            suppressions: ErrorSuppressions::empty(),
        }
    }

    fn env_ref() -> EnvRef {
        Arc::new(Env {
            files: FlowOrdSet::new(),
            dependency_info: env_cell(DependencyInfo::empty()),
            checked_files: CheckedSet::empty(),
            package_json_files: FlowOrdSet::new(),
            ordered_libs: Arc::new(Vec::new()),
            global_lib_files: GlobalLibFiles::new(
                Arc::new(BTreeSet::new()),
                Arc::new(BTreeSet::new()),
                FlowOrdSet::new(),
            ),
            unparsed: FlowOrdSet::new(),
            errors: env_cell(empty_errors()),
            coverage: env_cell(BTreeMap::new()),
            collated_errors: env_cell(CollatedErrors::empty()),
            connections: PersistentConnection(Arc::new(Vec::new())),
            exports: None,
            master_cx: Arc::new(MasterContext::EmptyMasterContext),
        })
    }

    fn source_file(name: &str) -> FileKey {
        FileKey::source_file_of_absolute(&format!("/tmp/{name}.js"))
    }

    #[test]
    fn heap_changes_publish_only_at_transaction_commit() {
        let heap = Arc::new(CommittedHeap::default());
        let env = env_ref();
        let heap_transaction = ActiveTransaction::new(heap.dupe());
        let env_transaction = EnvTransaction::new(env.dupe());
        let file = source_file("file");

        heap_transaction.add_unparsed(file.dupe(), 42, None);

        assert!(
            heap_transaction
                .latest_heap_reader()
                .reader()
                .committed_file_entry(&file)
                .is_none()
        );
        assert!(heap_transaction.get_parse(&file).is_some());

        env_transaction.commit_with_transaction(heap_transaction);
        let reader = ActiveTransaction::new(heap);

        assert!(
            reader
                .latest_heap_reader()
                .reader()
                .committed_file_entry(&file)
                .is_some()
        );
        assert!(reader.get_parse_committed(&file).is_some());
    }

    fn partial_dependency_graph(
        file: FileKey,
        sig_deps: BTreeSet<FileKey>,
        impl_deps: BTreeSet<FileKey>,
    ) -> PartialDependencyGraph {
        PartialDependencyGraph::from_map(BTreeMap::from([(file, (sig_deps, impl_deps))]))
    }

    fn dependency_info_update() -> (FileKey, FileKey, FileKey, PartialDependencyGraph) {
        let file = source_file("a");
        let sig_dep = source_file("sig");
        let impl_dep = source_file("impl");
        let partial_dependency_graph = partial_dependency_graph(
            file.dupe(),
            BTreeSet::from([sig_dep.dupe()]),
            BTreeSet::from([impl_dep.dupe()]),
        );
        (file, sig_dep, impl_dep, partial_dependency_graph)
    }

    #[test]
    fn unchanged_transaction_preserves_base_env() {
        let env = env_ref();
        let transaction = EnvTransaction::new(env.dupe());

        let committed = transaction.into_env();

        assert_eq!(env.files.len(), committed.files.len());
        assert_eq!(env.connections.0, committed.connections.0);
    }

    #[test]
    fn updated_checked_files_do_not_mutate_base_env() {
        let env = env_ref();
        let file = FileKey::source_file_of_absolute("/tmp/a.js");
        let mut checked_files = CheckedSet::empty();
        checked_files.add(Some([file].into_iter().collect()), None, None);

        let mut transaction = EnvTransaction::new(env.dupe());
        transaction.set_checked_files(checked_files);
        let committed = transaction.into_env();

        assert_eq!(env.checked_files.focused_cardinal(), 0);
        assert_eq!(committed.checked_files.focused_cardinal(), 1);
    }

    #[test]
    fn connection_update_preserves_other_fields() {
        let env = env_ref();
        let file = FileKey::source_file_of_absolute("/tmp/a.js");
        let mut files = FlowOrdSet::new();
        files.insert(file);
        let mut transaction = EnvTransaction::new(env);
        transaction.set_files(files);
        let env = Arc::new(transaction.into_env());

        let committed = with_connections(env.dupe(), PersistentConnection(Arc::new(vec![42])));

        assert_eq!(env.connections.0.as_ref(), &Vec::<i32>::new());
        assert_eq!(committed.connections.0.as_ref(), &vec![42]);
        assert_eq!(committed.files.len(), 1);
    }

    #[test]
    fn dependency_info_update_is_visible_inside_transaction_only() {
        let env = env_ref();
        let (file, sig_dep, impl_dep, partial_dependency_graph) = dependency_info_update();
        let mut transaction = EnvTransaction::new(env.dupe());

        transaction
            .dependency_info_mut()
            .update(partial_dependency_graph, BTreeSet::new());

        let transaction_dependency_info = transaction.dependency_info();
        assert_eq!(
            transaction_dependency_info
                .sig_dependency_graph()
                .find_opt(&file),
            Some(&BTreeSet::from([sig_dep]))
        );
        assert_eq!(
            transaction_dependency_info
                .implementation_dependency_graph()
                .find_opt(&file),
            Some(&BTreeSet::from([impl_dep]))
        );
        assert!(
            env.dependency_info()
                .sig_dependency_graph()
                .find_opt(&file)
                .is_none()
        );
    }

    #[test]
    fn dropped_transaction_does_not_commit_dependency_info() {
        let env = env_ref();
        let (file, _sig_dep, _impl_dep, partial_dependency_graph) = dependency_info_update();

        {
            let mut transaction = EnvTransaction::new(env.dupe());
            transaction
                .dependency_info_mut()
                .update(partial_dependency_graph, BTreeSet::new());
        }

        assert!(
            env.dependency_info()
                .sig_dependency_graph()
                .find_opt(&file)
                .is_none()
        );
        assert!(
            env.dependency_info()
                .implementation_dependency_graph()
                .find_opt(&file)
                .is_none()
        );
    }

    #[test]
    fn commit_updates_dependency_info_graphs() {
        let env = env_ref();
        let (file, sig_dep, impl_dep, partial_dependency_graph) = dependency_info_update();
        let mut transaction = EnvTransaction::new(env.dupe());
        transaction
            .dependency_info_mut()
            .update(partial_dependency_graph, BTreeSet::new());

        let committed = transaction.commit();
        let dependency_info = committed.dependency_info();

        assert_eq!(
            dependency_info.sig_dependency_graph().find_opt(&file),
            Some(&BTreeSet::from([sig_dep]))
        );
        assert_eq!(
            dependency_info
                .implementation_dependency_graph()
                .find_opt(&file),
            Some(&BTreeSet::from([impl_dep]))
        );
        assert!(Arc::ptr_eq(&env, &committed));
    }
}
