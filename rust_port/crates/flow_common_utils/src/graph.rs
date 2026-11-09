/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// A directional graph which maintains back edges to facilitate fast traversals in either direction.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_data_structure_wrapper::overlay_map::EnvCell;
use flow_data_structure_wrapper::overlay_map::EnvCellReadGuard;

#[derive(serde::Serialize, serde::Deserialize)]
struct Node<K: Ord> {
    forward: BTreeSet<K>,
    // These edges are mutable *only* for efficiency during construction. Once the graph is
    // constructed these should never be mutated.
    backward: BTreeSet<K>,
}

fn empty_node<K: Ord>() -> Node<K> {
    Node {
        forward: BTreeSet::new(),
        backward: BTreeSet::new(),
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Graph<K: Eq + Ord + Hash + Dupe> {
    nodes: BTreeMap<K, Node<K>>,
}

pub struct GraphDelta<K: Eq + Ord + Hash + Dupe> {
    nodes: BTreeMap<K, Option<Node<K>>>,
}

pub trait GraphLike<K: Eq + Ord + Hash + Dupe> {
    fn find_opt(&self, elt: &K) -> Option<&BTreeSet<K>>;

    fn find_backward_opt(&self, elt: &K) -> Option<&BTreeSet<K>>;

    fn fold<F, A>(&self, f: F, init: A) -> A
    where
        F: FnMut(&K, &BTreeSet<K>, A) -> A;

    fn to_map(&self) -> BTreeMap<K, BTreeSet<K>>;

    fn find(&self, elt: &K) -> &BTreeSet<K> {
        self.find_opt(elt)
            .unwrap_or_else(|| panic!("Node not found in graph"))
    }

    fn find_backward(&self, elt: &K) -> &BTreeSet<K> {
        self.find_backward_opt(elt)
            .unwrap_or_else(|| panic!("Node not found in graph"))
    }
}

impl<K: Eq + Ord + Hash + Dupe> Graph<K> {
    pub fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
        }
    }

    pub fn of_map(map: BTreeMap<K, BTreeSet<K>>) -> Self {
        // Gather the backward edges by hash: finding the node of every edge in an ordered map
        // compares the edge with each key on its path, which dominates on large graphs. The keys of
        // `map` come in order, so each list of backward edges comes out sorted.
        let mut backward_edges: HashMap<K, Vec<K>> = HashMap::new();
        for (key, forward_edges) in &map {
            for edge in forward_edges {
                backward_edges
                    .entry(edge.dupe())
                    .or_default()
                    .push(key.dupe());
            }
        }

        let mut nodes: Vec<(K, Node<K>)> = Vec::with_capacity(map.len());
        for (key, forward) in map {
            let backward = backward_edges
                .remove(&key)
                .map_or_else(BTreeSet::new, BTreeSet::from_iter);
            nodes.push((key, Node { forward, backward }));
        }
        // Nodes that are only the target of edges
        nodes.extend(backward_edges.into_iter().map(|(key, backward)| {
            (
                key,
                Node {
                    forward: BTreeSet::new(),
                    backward: BTreeSet::from_iter(backward),
                },
            )
        }));

        Self {
            nodes: BTreeMap::from_iter(nodes),
        }
    }

    pub fn update_from_map(
        mut self,
        map: BTreeMap<K, BTreeSet<K>>,
        to_remove: &BTreeSet<K>,
    ) -> Self {
        // First, make changes as needed based on `map`. This includes updating dependency edges and
        // adding entirely new nodes.
        for (key, new_forward_edges) in map {
            // Take ownership of previous forward edges to avoid cloning
            let previous_forward_edges = self
                .nodes
                .get_mut(&key)
                .map(|n| std::mem::take(&mut n.forward))
                .unwrap_or_default();

            // For each new forward edge not in previous, add a backward edge
            for edge in &new_forward_edges {
                if !previous_forward_edges.contains(edge) {
                    self.nodes
                        .entry(edge.dupe())
                        .or_insert_with(empty_node)
                        .backward
                        .insert(key.dupe());
                }
            }

            // For each previous forward edge not in new, remove its backward edge
            for edge in &previous_forward_edges {
                if !new_forward_edges.contains(edge) {
                    if let Some(node) = self.nodes.get_mut(edge) {
                        node.backward.remove(&key);
                    }
                }
            }

            // Ensure the source node exists and set forward edges (move, not clone)
            self.nodes.entry(key).or_insert_with(empty_node).forward = new_forward_edges;
        }

        // Now, remove nodes as needed based on `to_remove`. This requires fixing up both
        // forward edges and backward edges which point to the entries to remove, as well as removing
        // the entries themselves.
        for key_to_remove in to_remove {
            // In practice we sometimes get asked to remove nodes that aren't present to begin
            // with. That's a bit weird, but let's just tolerate that by doing nothing.
            if let Some(node) = self.nodes.remove(key_to_remove) {
                // Remove forward dependency edges that refer to this key
                for backward_key in &node.backward {
                    if let Some(backward_node) = self.nodes.get_mut(backward_key) {
                        backward_node.forward.remove(key_to_remove);
                    }
                }

                // Remove backward dependency edges that refer to this key
                for forward_key in &node.forward {
                    if let Some(forward_node) = self.nodes.get_mut(forward_key) {
                        forward_node.backward.remove(key_to_remove);
                    }
                }
            }
        }

        self
    }

    pub fn apply_delta(&mut self, delta: GraphDelta<K>) {
        for (key, value) in delta.nodes {
            match value {
                Some(node) => {
                    self.nodes.insert(key, node);
                }
                None => {
                    self.nodes.remove(&key);
                }
            }
        }
    }

    pub fn to_backward_map(&self) -> BTreeMap<K, BTreeSet<K>> {
        self.nodes
            .iter()
            .map(|(k, node)| (k.dupe(), node.backward.iter().duped().collect()))
            .collect()
    }

    #[allow(dead_code)]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn from_indexed_edges(
        keys: &[K],
        graph_keys: Vec<u32>,
        forward: Vec<Vec<u32>>,
        backward: Vec<Vec<u32>>,
    ) -> Self {
        let mut nodes = BTreeMap::new();
        for (i, key_index) in graph_keys.into_iter().enumerate() {
            let Some(key) = keys.get(key_index as usize) else {
                continue;
            };
            let forward = forward
                .get(i)
                .into_iter()
                .flat_map(|edges| edges.iter())
                .filter_map(|idx| keys.get(*idx as usize))
                .duped()
                .collect::<BTreeSet<_>>();
            let backward = backward
                .get(i)
                .into_iter()
                .flat_map(|edges| edges.iter())
                .filter_map(|idx| keys.get(*idx as usize))
                .duped()
                .collect::<BTreeSet<_>>();
            nodes.insert(key.dupe(), Node { forward, backward });
        }
        Self { nodes }
    }

    pub fn find(&self, elt: &K) -> &BTreeSet<K> {
        GraphLike::find(self, elt)
    }

    pub fn find_opt(&self, elt: &K) -> Option<&BTreeSet<K>> {
        GraphLike::find_opt(self, elt)
    }

    pub fn find_backward(&self, elt: &K) -> &BTreeSet<K> {
        GraphLike::find_backward(self, elt)
    }

    pub fn find_backward_opt(&self, elt: &K) -> Option<&BTreeSet<K>> {
        GraphLike::find_backward_opt(self, elt)
    }

    pub fn fold<F, A>(&self, f: F, init: A) -> A
    where
        F: FnMut(&K, &BTreeSet<K>, A) -> A,
    {
        GraphLike::fold(self, f, init)
    }

    pub fn to_map(&self) -> BTreeMap<K, BTreeSet<K>> {
        GraphLike::to_map(self)
    }

    pub fn map<F>(&self, mut f: F) -> Self
    where
        F: FnMut(&K) -> K,
    {
        let mut new_nodes = BTreeMap::new();

        for (elt, node) in &self.nodes {
            let new_elt = f(elt);
            let new_forward: BTreeSet<K> = node.forward.iter().map(&mut f).collect();
            let new_backward: BTreeSet<K> = node.backward.iter().map(&mut f).collect();

            if new_nodes.contains_key(&new_elt) {
                panic!("Duplicate keys created by function passed to Graph.map");
            }

            new_nodes.insert(
                new_elt,
                Node {
                    forward: new_forward,
                    backward: new_backward,
                },
            );
        }

        Self { nodes: new_nodes }
    }
}

impl<K: Eq + Ord + Hash + Dupe> GraphLike<K> for Graph<K> {
    fn find_opt(&self, elt: &K) -> Option<&BTreeSet<K>> {
        self.nodes.get(elt).map(|node| &node.forward)
    }

    fn find_backward_opt(&self, elt: &K) -> Option<&BTreeSet<K>> {
        self.nodes.get(elt).map(|node| &node.backward)
    }

    fn fold<F, A>(&self, mut f: F, init: A) -> A
    where
        F: FnMut(&K, &BTreeSet<K>, A) -> A,
    {
        self.nodes
            .iter()
            .fold(init, |acc, (key, node)| f(key, &node.forward, acc))
    }

    fn to_map(&self) -> BTreeMap<K, BTreeSet<K>> {
        self.nodes
            .iter()
            .map(|(k, node)| (k.dupe(), node.forward.iter().duped().collect()))
            .collect()
    }
}

impl<K: Eq + Ord + Hash + Dupe> Default for Graph<K> {
    fn default() -> Self {
        Self::new()
    }
}

struct GraphCellBase<T, K: Eq + Ord + Hash + Dupe> {
    snapshot: EnvCellReadGuard<T>,
    graph: fn(&T) -> &Graph<K>,
}

impl<T, K: Eq + Ord + Hash + Dupe> GraphCellBase<T, K> {
    fn new(cell: Arc<EnvCell<T>>, graph: fn(&T) -> &Graph<K>) -> Self {
        Self {
            snapshot: cell.read_arc(),
            graph,
        }
    }
}

trait GraphBase<K: Eq + Ord + Hash + Dupe> {
    fn map(&self) -> &BTreeMap<K, Node<K>>;
}

impl<K: Eq + Ord + Hash + Dupe> GraphBase<K> for Arc<BTreeMap<K, Node<K>>> {
    fn map(&self) -> &BTreeMap<K, Node<K>> {
        self
    }
}

impl<T, K: Eq + Ord + Hash + Dupe> GraphBase<K> for GraphCellBase<T, K> {
    fn map(&self) -> &BTreeMap<K, Node<K>> {
        &(self.graph)(&self.snapshot).nodes
    }
}

struct GraphOverlay<K, B>
where
    K: Eq + Ord + Hash + Dupe,
    B: GraphBase<K>,
{
    base: B,
    delta: BTreeMap<K, Option<Node<K>>>,
}

impl<K, B> GraphOverlay<K, B>
where
    K: Eq + Ord + Hash + Dupe,
    B: GraphBase<K>,
{
    fn with_base(base: B) -> Self {
        Self {
            base,
            delta: BTreeMap::new(),
        }
    }

    fn node_opt(&self, key: &K) -> Option<&Node<K>> {
        match self.delta.get(key) {
            Some(Some(node)) => Some(node),
            Some(None) => None,
            None => self.base.map().get(key),
        }
    }

    fn copy_node(node: &Node<K>) -> Node<K> {
        Node {
            forward: node.forward.iter().duped().collect(),
            backward: node.backward.iter().duped().collect(),
        }
    }

    fn materialize_node(&self, key: &K) -> Node<K> {
        self.node_opt(key)
            .map(Self::copy_node)
            .unwrap_or_else(empty_node)
    }

    fn update_from_map(&mut self, map: BTreeMap<K, BTreeSet<K>>, to_remove: &BTreeSet<K>) {
        for (key, new_forward_edges) in map {
            let added_edges: Vec<K> = match self.node_opt(&key) {
                Some(node) => new_forward_edges
                    .iter()
                    .filter(|edge| !node.forward.contains(*edge))
                    .duped()
                    .collect(),
                None => new_forward_edges.iter().duped().collect(),
            };
            let removed_edges: Vec<K> = self
                .node_opt(&key)
                .map(|node| {
                    node.forward
                        .iter()
                        .filter(|edge| !new_forward_edges.contains(*edge))
                        .duped()
                        .collect()
                })
                .unwrap_or_default();

            for edge in added_edges {
                let mut node = self.materialize_node(&edge);
                node.backward.insert(key.dupe());
                self.delta.insert(edge, Some(node));
            }

            for edge in removed_edges {
                if self.node_opt(&edge).is_some() {
                    let mut node = self.materialize_node(&edge);
                    node.backward.remove(&key);
                    self.delta.insert(edge, Some(node));
                }
            }

            let mut node = self.materialize_node(&key);
            node.forward = new_forward_edges;
            self.delta.insert(key, Some(node));
        }

        for key_to_remove in to_remove {
            let Some(node) = self.node_opt(key_to_remove) else {
                continue;
            };
            let backward_keys: Vec<K> = node.backward.iter().duped().collect();
            let forward_keys: Vec<K> = node.forward.iter().duped().collect();

            self.delta.insert(key_to_remove.dupe(), None);

            for backward_key in backward_keys {
                if self.node_opt(&backward_key).is_some() {
                    let mut backward_node = self.materialize_node(&backward_key);
                    backward_node.forward.remove(key_to_remove);
                    self.delta.insert(backward_key, Some(backward_node));
                }
            }

            for forward_key in forward_keys {
                if self.node_opt(&forward_key).is_some() {
                    let mut forward_node = self.materialize_node(&forward_key);
                    forward_node.backward.remove(key_to_remove);
                    self.delta.insert(forward_key, Some(forward_node));
                }
            }
        }
    }

    fn commit_to(self, base: &mut Graph<K>) {
        base.apply_delta(self.into_delta());
    }

    fn into_delta(self) -> GraphDelta<K> {
        GraphDelta { nodes: self.delta }
    }
}

impl<K, B> GraphLike<K> for GraphOverlay<K, B>
where
    K: Eq + Ord + Hash + Dupe,
    B: GraphBase<K>,
{
    fn find_opt(&self, elt: &K) -> Option<&BTreeSet<K>> {
        self.node_opt(elt).map(|node| &node.forward)
    }

    fn find_backward_opt(&self, elt: &K) -> Option<&BTreeSet<K>> {
        self.node_opt(elt).map(|node| &node.backward)
    }

    fn fold<F, A>(&self, mut f: F, init: A) -> A
    where
        F: FnMut(&K, &BTreeSet<K>, A) -> A,
    {
        let mut base = self.base.map().iter().peekable();
        let mut delta = self.delta.iter().peekable();
        let mut acc = init;

        loop {
            match (base.peek(), delta.peek()) {
                (Some((base_key, _)), Some((delta_key, _))) => match (*base_key).cmp(*delta_key) {
                    std::cmp::Ordering::Less => {
                        let (key, node) = base.next().expect("base entry should exist");
                        acc = f(key, &node.forward, acc);
                    }
                    std::cmp::Ordering::Equal => {
                        base.next();
                        let (key, node) = delta.next().expect("delta entry should exist");
                        if let Some(node) = node {
                            acc = f(key, &node.forward, acc);
                        }
                    }
                    std::cmp::Ordering::Greater => {
                        let (key, node) = delta.next().expect("delta entry should exist");
                        if let Some(node) = node {
                            acc = f(key, &node.forward, acc);
                        }
                    }
                },
                (Some(_), None) => {
                    let (key, node) = base.next().expect("base entry should exist");
                    acc = f(key, &node.forward, acc);
                }
                (None, Some(_)) => {
                    let (key, node) = delta.next().expect("delta entry should exist");
                    if let Some(node) = node {
                        acc = f(key, &node.forward, acc);
                    }
                }
                (None, None) => break,
            }
        }

        acc
    }

    fn to_map(&self) -> BTreeMap<K, BTreeSet<K>> {
        self.fold(
            |key, edges, mut acc| {
                acc.insert(key.dupe(), edges.iter().duped().collect());
                acc
            },
            BTreeMap::new(),
        )
    }
}

pub struct OverlayGraph<K: Eq + Ord + Hash + Dupe> {
    inner: GraphOverlay<K, Arc<BTreeMap<K, Node<K>>>>,
}

impl<K: Eq + Ord + Hash + Dupe> OverlayGraph<K> {
    pub fn from_graph(graph: Graph<K>) -> Self {
        Self {
            inner: GraphOverlay::with_base(Arc::new(graph.nodes)),
        }
    }

    pub fn update_from_map(&mut self, map: BTreeMap<K, BTreeSet<K>>, to_remove: &BTreeSet<K>) {
        self.inner.update_from_map(map, to_remove);
    }

    pub fn commit_to(self, base: &mut Graph<K>) {
        self.inner.commit_to(base);
    }

    pub fn into_delta(self) -> GraphDelta<K> {
        self.inner.into_delta()
    }

    pub fn to_backward_map(&self) -> BTreeMap<K, BTreeSet<K>> {
        let mut base = self.inner.base.map().iter().peekable();
        let mut delta = self.inner.delta.iter().peekable();
        let mut acc = BTreeMap::new();

        loop {
            match (base.peek(), delta.peek()) {
                (Some((base_key, _)), Some((delta_key, _))) => match (*base_key).cmp(*delta_key) {
                    std::cmp::Ordering::Less => {
                        let (key, node) = base.next().expect("base entry should exist");
                        acc.insert(key.dupe(), node.backward.iter().duped().collect());
                    }
                    std::cmp::Ordering::Equal => {
                        base.next();
                        let (key, node) = delta.next().expect("delta entry should exist");
                        if let Some(node) = node {
                            acc.insert(key.dupe(), node.backward.iter().duped().collect());
                        }
                    }
                    std::cmp::Ordering::Greater => {
                        let (key, node) = delta.next().expect("delta entry should exist");
                        if let Some(node) = node {
                            acc.insert(key.dupe(), node.backward.iter().duped().collect());
                        }
                    }
                },
                (Some(_), None) => {
                    let (key, node) = base.next().expect("base entry should exist");
                    acc.insert(key.dupe(), node.backward.iter().duped().collect());
                }
                (None, Some(_)) => {
                    let (key, node) = delta.next().expect("delta entry should exist");
                    if let Some(node) = node {
                        acc.insert(key.dupe(), node.backward.iter().duped().collect());
                    }
                }
                (None, None) => break,
            }
        }

        acc
    }
}

impl<K: Eq + Ord + Hash + Dupe> GraphLike<K> for OverlayGraph<K> {
    fn find_opt(&self, elt: &K) -> Option<&BTreeSet<K>> {
        self.inner.find_opt(elt)
    }

    fn find_backward_opt(&self, elt: &K) -> Option<&BTreeSet<K>> {
        self.inner.find_backward_opt(elt)
    }

    fn fold<F, A>(&self, f: F, init: A) -> A
    where
        F: FnMut(&K, &BTreeSet<K>, A) -> A,
    {
        self.inner.fold(f, init)
    }

    fn to_map(&self) -> BTreeMap<K, BTreeSet<K>> {
        self.inner.to_map()
    }
}

pub struct EnvOverlayGraph<T, K: Eq + Ord + Hash + Dupe> {
    inner: GraphOverlay<K, GraphCellBase<T, K>>,
}

impl<T, K: Eq + Ord + Hash + Dupe> EnvOverlayGraph<T, K> {
    pub fn new(cell: Arc<EnvCell<T>>, graph: fn(&T) -> &Graph<K>) -> Self {
        Self {
            inner: GraphOverlay::with_base(GraphCellBase::new(cell, graph)),
        }
    }

    pub fn update_from_map(&mut self, map: BTreeMap<K, BTreeSet<K>>, to_remove: &BTreeSet<K>) {
        self.inner.update_from_map(map, to_remove);
    }

    pub fn commit_to(self, base: &mut Graph<K>) {
        self.inner.commit_to(base);
    }

    pub fn into_delta(self) -> GraphDelta<K> {
        self.inner.into_delta()
    }
}

impl<T, K: Eq + Ord + Hash + Dupe> GraphLike<K> for EnvOverlayGraph<T, K> {
    fn find_opt(&self, elt: &K) -> Option<&BTreeSet<K>> {
        self.inner.find_opt(elt)
    }

    fn find_backward_opt(&self, elt: &K) -> Option<&BTreeSet<K>> {
        self.inner.find_backward_opt(elt)
    }

    fn fold<F, A>(&self, f: F, init: A) -> A
    where
        F: FnMut(&K, &BTreeSet<K>, A) -> A,
    {
        self.inner.fold(f, init)
    }

    fn to_map(&self) -> BTreeMap<K, BTreeSet<K>> {
        self.inner.to_map()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    type StringGraph = Graph<Arc<str>>;

    fn assert_ssets_equal(expected: &BTreeSet<Arc<str>>, actual: &BTreeSet<Arc<str>>) {
        assert_eq!(expected, actual);
    }

    fn assert_smaps_equal(
        expected: &BTreeMap<Arc<str>, BTreeSet<Arc<str>>>,
        actual: &BTreeMap<Arc<str>, BTreeSet<Arc<str>>>,
    ) {
        assert_eq!(expected, actual);
    }

    fn sset_of_list(items: &[&str]) -> BTreeSet<Arc<str>> {
        items.iter().map(|s| Arc::from(*s)).collect()
    }

    fn smap_of_list(pairs: &[(&str, &[&str])]) -> BTreeMap<Arc<str>, BTreeSet<Arc<str>>> {
        pairs
            .iter()
            .map(|(key, values)| (Arc::from(*key), sset_of_list(values)))
            .collect()
    }

    fn map() -> BTreeMap<Arc<str>, BTreeSet<Arc<str>>> {
        smap_of_list(&[("foo", &["bar", "baz"]), ("bar", &["baz"]), ("baz", &[])])
    }

    fn reverse_map() -> BTreeMap<Arc<str>, BTreeSet<Arc<str>>> {
        smap_of_list(&[("foo", &[]), ("bar", &["foo"]), ("baz", &["foo", "bar"])])
    }

    fn graph() -> StringGraph {
        StringGraph::of_map(map())
    }

    fn assert_overlay_update_matches_graph(
        update_pairs: &[(&str, &[&str])],
        to_remove_items: &[&str],
    ) {
        let to_remove = sset_of_list(to_remove_items);
        let expected = graph().update_from_map(smap_of_list(update_pairs), &to_remove);
        let mut overlay = OverlayGraph::from_graph(graph());

        overlay.update_from_map(smap_of_list(update_pairs), &to_remove);

        assert_smaps_equal(&expected.to_map(), &overlay.to_map());
        assert_smaps_equal(&expected.to_backward_map(), &overlay.to_backward_map());

        let mut committed = graph();
        overlay.commit_to(&mut committed);

        assert_smaps_equal(&expected.to_map(), &committed.to_map());
        assert_smaps_equal(&expected.to_backward_map(), &committed.to_backward_map());
    }

    #[test]
    fn basic_construction() {
        let graph = graph();
        let result_forward_map = graph.to_map();
        let result_backward_map = graph.to_backward_map();
        assert_smaps_equal(&map(), &result_forward_map);
        assert_smaps_equal(&reverse_map(), &result_backward_map);
    }

    #[test]
    fn find() {
        let graph = graph();
        let result = graph.find(&Arc::from("foo"));
        let expected_map = map();
        let expected = expected_map.get(&Arc::from("foo") as &Arc<str>).unwrap();
        assert_ssets_equal(expected, result);
    }

    #[test]
    fn find_opt() {
        let graph = graph();
        let result = graph.find_opt(&Arc::from("foo"));
        let result = result.unwrap();
        let expected_map = map();
        let expected = expected_map.get(&Arc::from("foo") as &Arc<str>).unwrap();
        assert_ssets_equal(expected, result);
    }

    #[test]
    fn find_opt_none() {
        let graph = graph();
        let result = graph.find_opt(&Arc::from("qux"));
        let expected: Option<&BTreeSet<Arc<str>>> = None;
        assert_eq!(expected, result);
    }

    #[test]
    fn find_backward() {
        let graph = graph();
        let result = graph.find_backward(&Arc::from("baz"));
        let expected_map = reverse_map();
        let expected = expected_map.get(&Arc::from("baz") as &Arc<str>).unwrap();
        assert_ssets_equal(expected, result);
    }

    #[test]
    fn find_backward_opt() {
        let graph = graph();
        let result = graph.find_backward_opt(&Arc::from("baz"));
        let result = result.unwrap();
        let expected_map = reverse_map();
        let expected = expected_map.get(&Arc::from("baz") as &Arc<str>).unwrap();
        assert_ssets_equal(expected, result);
    }

    #[test]
    fn find_backward_opt_none() {
        let graph = graph();
        let result = graph.find_backward_opt(&Arc::from("qux"));
        let expected: Option<&BTreeSet<Arc<str>>> = None;
        assert_eq!(expected, result);
    }

    #[test]
    fn add() {
        let graph = graph();
        let update_map = smap_of_list(&[("qux", &["foo", "baz"])]);
        let graph = graph.update_from_map(update_map, &BTreeSet::new());
        let result_forward_map = graph.to_map();
        let mut expected_forward_map = map();
        expected_forward_map.insert(Arc::from("qux"), sset_of_list(&["foo", "baz"]));
        let result_backward_map = graph.to_backward_map();
        let mut expected_backward_map = reverse_map();
        expected_backward_map.insert(Arc::from("foo"), sset_of_list(&["qux"]));
        expected_backward_map.insert(Arc::from("baz"), sset_of_list(&["qux", "foo", "bar"]));
        expected_backward_map.insert(Arc::from("qux"), BTreeSet::new());
        assert_smaps_equal(&expected_forward_map, &result_forward_map);
        assert_smaps_equal(&expected_backward_map, &result_backward_map);
    }

    #[test]
    fn remove() {
        let graph = graph();
        let mut to_remove = BTreeSet::new();
        to_remove.insert(Arc::from("bar"));
        let graph = graph.update_from_map(BTreeMap::new(), &to_remove);
        let result_forward_map = graph.to_map();
        let result_backward_map = graph.to_backward_map();
        let expected_forward_map = smap_of_list(&[("foo", &["baz"]), ("baz", &[])]);
        let expected_backward_map = smap_of_list(&[("foo", &[]), ("baz", &["foo"])]);
        assert_smaps_equal(&expected_forward_map, &result_forward_map);
        assert_smaps_equal(&expected_backward_map, &result_backward_map);
    }

    #[test]
    fn remove_all() {
        let graph = graph();
        let to_remove: BTreeSet<Arc<str>> = map().keys().duped().collect();
        let graph = graph.update_from_map(BTreeMap::new(), &to_remove);
        let result_forward_map = graph.to_map();
        let result_backward_map = graph.to_backward_map();
        let expected: BTreeMap<Arc<str>, BTreeSet<Arc<str>>> = BTreeMap::new();
        assert_smaps_equal(&expected, &result_forward_map);
        assert_smaps_equal(&expected, &result_backward_map);
    }

    #[test]
    fn remove_nonexistent() {
        let graph = graph();
        let mut to_remove = BTreeSet::new();
        to_remove.insert(Arc::from("fake"));
        let graph = graph.update_from_map(BTreeMap::new(), &to_remove);
        let result_forward_map = graph.to_map();
        let result_backward_map = graph.to_backward_map();
        assert_smaps_equal(&map(), &result_forward_map);
        assert_smaps_equal(&reverse_map(), &result_backward_map);
    }

    #[test]
    fn modify() {
        let graph = graph();
        let update_map = smap_of_list(&[("foo", &[])]);
        let graph = graph.update_from_map(update_map, &BTreeSet::new());
        let result_forward_map = graph.to_map();
        let result_backward_map = graph.to_backward_map();
        let mut expected_forward_map = map();
        expected_forward_map.insert(Arc::from("foo"), BTreeSet::new());
        let expected_backward_map = smap_of_list(&[("foo", &[]), ("bar", &[]), ("baz", &["bar"])]);
        assert_smaps_equal(&expected_forward_map, &result_forward_map);
        assert_smaps_equal(&expected_backward_map, &result_backward_map);
    }

    #[test]
    fn modify_complex() {
        let graph = graph();
        let update_map = smap_of_list(&[("foo", &["qux"]), ("qux", &["baz"])]);
        let mut to_remove = BTreeSet::new();
        to_remove.insert(Arc::from("bar"));
        let graph = graph.update_from_map(update_map, &to_remove);
        let result_forward_map = graph.to_map();
        let result_backward_map = graph.to_backward_map();
        let expected_forward_map =
            smap_of_list(&[("foo", &["qux"]), ("baz", &[]), ("qux", &["baz"])]);
        let expected_backward_map =
            smap_of_list(&[("foo", &[]), ("baz", &["qux"]), ("qux", &["foo"])]);
        assert_smaps_equal(&expected_forward_map, &result_forward_map);
        assert_smaps_equal(&expected_backward_map, &result_backward_map);
    }

    #[test]
    fn overlay_add_matches_graph_update() {
        assert_overlay_update_matches_graph(&[("qux", &["foo", "baz"])], &[]);
    }

    #[test]
    fn overlay_remove_matches_graph_update() {
        assert_overlay_update_matches_graph(&[], &["bar"]);
    }

    #[test]
    fn overlay_remove_nonexistent_matches_graph_update() {
        assert_overlay_update_matches_graph(&[], &["fake"]);
    }

    #[test]
    fn overlay_modify_matches_graph_update() {
        assert_overlay_update_matches_graph(&[("foo", &[])], &[]);
    }

    #[test]
    fn overlay_complex_update_matches_graph_update() {
        assert_overlay_update_matches_graph(&[("foo", &["qux"]), ("qux", &["baz"])], &["bar"]);
    }

    #[test]
    fn map_test() {
        let graph = graph();
        let mapped_graph = graph.map(|x| Arc::from(format!("foo_{}", x).as_str()));
        let result_forward_map = mapped_graph.to_map();
        let expected_forward_map = smap_of_list(&[
            ("foo_foo", &["foo_bar", "foo_baz"]),
            ("foo_bar", &["foo_baz"]),
            ("foo_baz", &[]),
        ]);
        assert_smaps_equal(&expected_forward_map, &result_forward_map);
    }
}
