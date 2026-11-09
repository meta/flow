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
use flow_common_utils::graph::EnvOverlayGraph;
use flow_common_utils::graph::Graph;
use flow_data_structure_wrapper::overlay_map::EnvCell;
use flow_parser::file_key::FileKey;
use flow_utils_concurrency::thread_pool::ThreadPool;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct DependencyInfo {
    sig_dependency_graph: Graph<FileKey>,
    implementation_dependency_graph: Graph<FileKey>,
}

pub struct PartialDependencyGraph {
    pub(crate) inner: BTreeMap<FileKey, (BTreeSet<FileKey>, BTreeSet<FileKey>)>,
}

impl PartialDependencyGraph {
    pub fn new() -> Self {
        Self {
            inner: BTreeMap::new(),
        }
    }

    pub fn from_map(map: BTreeMap<FileKey, (BTreeSet<FileKey>, BTreeSet<FileKey>)>) -> Self {
        Self { inner: map }
    }

    pub fn insert(
        &mut self,
        key: FileKey,
        value: (BTreeSet<FileKey>, BTreeSet<FileKey>),
    ) -> Option<(BTreeSet<FileKey>, BTreeSet<FileKey>)> {
        self.inner.insert(key, value)
    }

    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (&FileKey, &(BTreeSet<FileKey>, BTreeSet<FileKey>))> {
        self.inner.iter()
    }

    pub fn into_inner(self) -> BTreeMap<FileKey, (BTreeSet<FileKey>, BTreeSet<FileKey>)> {
        self.inner
    }
}

fn split_dependency_maps(
    map: BTreeMap<FileKey, (BTreeSet<FileKey>, BTreeSet<FileKey>)>,
) -> (
    BTreeMap<FileKey, BTreeSet<FileKey>>,
    BTreeMap<FileKey, BTreeSet<FileKey>>,
) {
    // Collecting the sorted entries builds each map in one pass, instead of searching it for the
    // place of every file.
    let (sig_entries, implementation_entries): (Vec<_>, Vec<_>) = map
        .into_iter()
        .map(|(file, (sig_deps, impl_deps))| ((file.dupe(), sig_deps), (file, impl_deps)))
        .unzip();
    (
        BTreeMap::from_iter(sig_entries),
        BTreeMap::from_iter(implementation_entries),
    )
}

impl DependencyInfo {
    pub fn from_graphs(
        sig_dependency_graph: Graph<FileKey>,
        implementation_dependency_graph: Graph<FileKey>,
    ) -> Self {
        Self {
            sig_dependency_graph,
            implementation_dependency_graph,
        }
    }

    pub fn empty() -> Self {
        Self {
            sig_dependency_graph: Graph::new(),
            implementation_dependency_graph: Graph::new(),
        }
    }

    pub fn of_map(pool: &ThreadPool, map: PartialDependencyGraph) -> Self {
        let _ = pool;
        let (sig_dependency_map, implementation_dependency_map) = split_dependency_maps(map.inner);
        let sig_dependency_graph = Graph::of_map(sig_dependency_map);
        let implementation_dependency_graph = Graph::of_map(implementation_dependency_map);
        Self {
            sig_dependency_graph,
            implementation_dependency_graph,
        }
    }

    pub fn update(
        old_dep_info: DependencyInfo,
        partial_dep_graph: PartialDependencyGraph,
        to_remove: BTreeSet<FileKey>,
    ) -> Self {
        let DependencyInfo {
            sig_dependency_graph: old_sig_graph,
            implementation_dependency_graph: old_impl_graph,
        } = old_dep_info;
        let (updated_sig_map, updated_impl_map) = split_dependency_maps(partial_dep_graph.inner);
        Self {
            sig_dependency_graph: old_sig_graph.update_from_map(updated_sig_map, &to_remove),
            implementation_dependency_graph: old_impl_graph
                .update_from_map(updated_impl_map, &to_remove),
        }
    }

    pub fn implementation_dependency_graph(&self) -> &Graph<FileKey> {
        &self.implementation_dependency_graph
    }

    pub fn sig_dependency_graph(&self) -> &Graph<FileKey> {
        &self.sig_dependency_graph
    }

    pub fn debug_to_string(&self) -> String {
        let DependencyInfo {
            sig_dependency_graph,
            implementation_dependency_graph,
        } = self;
        let sig_map = sig_dependency_graph.to_map();
        let impl_map = implementation_dependency_graph.to_map();
        format!(
            "Sig dependency graph:\n{:?}\nImplementation dependency graph:\n{:?}",
            sig_map, impl_map
        )
    }
}

pub struct OverlayDependencyInfo {
    base_cell: Arc<EnvCell<DependencyInfo>>,
    sig_dependency_graph: EnvOverlayGraph<DependencyInfo, FileKey>,
    implementation_dependency_graph: EnvOverlayGraph<DependencyInfo, FileKey>,
}

impl OverlayDependencyInfo {
    pub fn new(base_cell: Arc<EnvCell<DependencyInfo>>) -> Self {
        Self {
            base_cell: base_cell.dupe(),
            sig_dependency_graph: EnvOverlayGraph::new(
                base_cell.dupe(),
                DependencyInfo::sig_dependency_graph,
            ),
            implementation_dependency_graph: EnvOverlayGraph::new(
                base_cell,
                DependencyInfo::implementation_dependency_graph,
            ),
        }
    }

    pub fn update(
        &mut self,
        partial_dep_graph: PartialDependencyGraph,
        to_remove: BTreeSet<FileKey>,
    ) {
        let (updated_sig_map, updated_impl_map) = split_dependency_maps(partial_dep_graph.inner);
        self.sig_dependency_graph
            .update_from_map(updated_sig_map, &to_remove);
        self.implementation_dependency_graph
            .update_from_map(updated_impl_map, &to_remove);
    }

    pub fn implementation_dependency_graph(&self) -> &EnvOverlayGraph<DependencyInfo, FileKey> {
        &self.implementation_dependency_graph
    }

    pub fn sig_dependency_graph(&self) -> &EnvOverlayGraph<DependencyInfo, FileKey> {
        &self.sig_dependency_graph
    }

    pub fn commit(self) {
        let Self {
            base_cell,
            sig_dependency_graph,
            implementation_dependency_graph,
        } = self;
        let sig_dependency_graph = sig_dependency_graph.into_delta();
        let implementation_dependency_graph_delta = implementation_dependency_graph.into_delta();

        let mut dependency_info = base_cell.write();
        dependency_info
            .sig_dependency_graph
            .apply_delta(sig_dependency_graph);
        dependency_info
            .implementation_dependency_graph
            .apply_delta(implementation_dependency_graph_delta);
    }
}
