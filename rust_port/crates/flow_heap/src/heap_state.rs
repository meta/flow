/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Heap base maps and lock-free transaction overlays.

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use arc_swap::ArcSwap;
use dupe::Dupe;
use flow_common::flow_import_specifier::FlowImportSpecifier;
use flow_common_modulename::HasteModuleInfo;
use flow_common_modulename::Modulename;
use flow_heap_serialization::ReaderCache;
use flow_parser::file_key::FileKey;
use flow_parser::loc::Loc;
use flow_parser_utils::package_json::PackageJson;
use flow_type_sig::packed_type_sig::Module as TypeSigModule;
use flow_utils_concurrency::lockfree_overlay_map::CommitMap;
use flow_utils_concurrency::lockfree_overlay_map::DrainedOverlayMap;
use flow_utils_concurrency::lockfree_overlay_map::DrainedSetOverlay;
use flow_utils_concurrency::lockfree_overlay_map::LockfreeOverlayMap;
use flow_utils_concurrency::lockfree_overlay_map::LockfreeSetOverlay;
use flow_utils_concurrency::lockfree_overlay_map::OverlayMapCommitValue;
use flow_utils_concurrency::lockfree_overlay_map::OverlayValue;
use flow_utils_concurrency::lockfree_overlay_map::SetOp;
use flow_utils_concurrency::lockfree_overlay_map::SetOwnerOps;
use flow_utils_concurrency::lockfree_overlay_map::commit_map_insert_hashed;
use flow_utils_concurrency::lockfree_overlay_map::commit_map_remove_hashed;
use flow_utils_concurrency::lockfree_overlay_map::commit_map_with_capacity;
use parking_lot::ArcRwLockReadGuard;
use parking_lot::Mutex;
use parking_lot::RawRwLockRecursive;
use parking_lot::RecursiveRwLock;
use rayon::join;
use rayon::prelude::*;

use crate::haste_module::HasteModule;
use crate::parse::FileEntry;
use crate::parse::Parse;
use crate::resolved_requires::Dependency;
use crate::resolved_requires::DependencyTarget;
use crate::resolved_requires::ResolvedRequires;
use crate::transaction::GcState;

/// A handle on whichever heap is in force.
///
/// The state is behind an [`ArcSwap`] because a bulk load replaces the whole heap rather than
/// adding to it: the replacement is built as a heap of its own — see
/// [`CommittedHeap::from_heap_dump`] — and installed here by [`CommittedHeap::swap_in`], while
/// everything that holds an `Arc<CommittedHeap>` keeps pointing at this same handle. Transactions
/// take the state by `Arc`, so one that is already open goes on reading the state it opened on.
pub struct CommittedHeap {
    pub(crate) state: ArcSwap<RecursiveRwLock<CommittedHeapState>>,
}

pub(crate) struct CommittedHeapState {
    pub(crate) data: CommittedHeapData,
    pub(crate) reader_cache: ReaderCache,
    pub(crate) gc_state: Mutex<GcState>,
    pub(crate) active_transactions: AtomicUsize,
}

impl CommittedHeapState {
    pub(crate) fn new(data: CommittedHeapData) -> Self {
        Self {
            data,
            reader_cache: ReaderCache::new(),
            gc_state: Mutex::new(GcState::default()),
            active_transactions: AtomicUsize::new(0),
        }
    }
}

pub(crate) struct CommittedHeapData {
    pub(crate) files: CommitMap<FileKey, FileEntry>,
    pub(crate) haste_modules: CommitMap<HasteModuleInfo, HasteModule>,
    pub(crate) file_dependents: HashMap<FileKey, Arc<Vec<FileKey>>>,
    pub(crate) haste_dependents: HashMap<HasteModuleInfo, Arc<Vec<FileKey>>>,
    pub(crate) haste_provider_candidates: HashMap<HasteModuleInfo, Arc<Vec<FileKey>>>,
}

impl CommittedHeapData {
    pub(crate) fn with_capacity(files: usize, haste_modules: usize) -> Self {
        Self {
            files: commit_map_with_capacity(files),
            haste_modules: commit_map_with_capacity(haste_modules),
            file_dependents: HashMap::new(),
            haste_dependents: HashMap::new(),
            haste_provider_candidates: HashMap::new(),
        }
    }

    pub(crate) fn is_fully_empty(&self) -> bool {
        self.files.is_empty()
            && self.haste_modules.is_empty()
            && self.file_dependents.is_empty()
            && self.haste_dependents.is_empty()
            && self.haste_provider_candidates.is_empty()
    }
}

impl CommittedHeap {
    pub fn new() -> Self {
        Self::with_capacity(0, 0)
    }

    pub fn with_capacity(files: usize, haste_modules: usize) -> Self {
        Self::with_data(CommittedHeapData::with_capacity(files, haste_modules))
    }

    pub(crate) fn with_data(data: CommittedHeapData) -> Self {
        Self {
            state: ArcSwap::from_pointee(RecursiveRwLock::new(CommittedHeapState::new(data))),
        }
    }

    /// Swaps `loaded` in: from here this handle is the heap `loaded` built.
    ///
    /// Publishing stores a pointer. It takes no lock and waits for no reader, because a
    /// transaction opened before the swap holds the state it opened on by `Arc` — see
    /// `ActiveTransaction::new` — and goes on reading it consistently. The state being replaced
    /// dies with the last transaction on it, not here.
    ///
    /// Only the server orchestrator calls this. Which heap the server runs on is part of the
    /// committed state it owns, established once when it adopts the heap init built and again
    /// whenever it commits a recheck that reinitialised from a saved state. A heap built anywhere
    /// else stays a heap of its own until the orchestrator adopts it.
    pub fn swap_in(&self, loaded: Arc<CommittedHeap>) {
        self.state.store(loaded.state.load_full());
    }

    pub fn apply_overlay(&self, overlay: &HeapOverlay) {
        let state = self.state.load_full();
        let mut state = state.write();
        let CommittedHeapData {
            files,
            haste_modules,
            file_dependents,
            haste_dependents,
            haste_provider_candidates,
        } = &mut state.data;
        join(
            || {
                join(
                    || apply_overlay_map(files, &overlay.files),
                    || apply_overlay_map(haste_modules, &overlay.haste_modules),
                );
            },
            || {
                join(
                    || apply_set_overlay(file_dependents, &overlay.file_dependents),
                    || {
                        join(
                            || apply_set_overlay(haste_dependents, &overlay.haste_dependents),
                            || {
                                apply_set_overlay(
                                    haste_provider_candidates,
                                    &overlay.haste_provider_candidates,
                                )
                            },
                        );
                    },
                );
            },
        );
    }

    pub(crate) fn apply_overlay_draining(&self, overlay: &mut HeapOverlay) {
        let deltas = overlay.take_commit_deltas();
        self.apply_commit_deltas(deltas);
    }

    fn apply_commit_deltas(&self, deltas: HeapOverlayCommitDeltas) {
        let state = self.state.load_full();
        let mut state = state.write();
        apply_commit_deltas_to_data(&mut state.data, deltas);
    }

    pub(crate) fn read_arc_recursive(&self) -> CommittedHeapReadGuard {
        self.state.load().read_arc()
    }

    pub(crate) fn apply_commit_deltas_to_both(
        &self,
        other: &CommittedHeap,
        overlay: &mut HeapOverlay,
    ) {
        let deltas = overlay.take_commit_deltas();
        let HeapOverlayCommitDeltas {
            files,
            haste_modules,
            file_dependents,
            haste_dependents,
            haste_provider_candidates,
        } = deltas;
        let committed = self.state.load_full();
        let other = other.state.load_full();
        let mut committed = committed.write();
        let mut other = other.write();
        let CommittedHeapData {
            files: committed_files,
            haste_modules: committed_haste_modules,
            file_dependents: committed_file_dependents,
            haste_dependents: committed_haste_dependents,
            haste_provider_candidates: committed_haste_provider_candidates,
        } = &mut committed.data;
        let CommittedHeapData {
            files: other_files,
            haste_modules: other_haste_modules,
            file_dependents: other_file_dependents,
            haste_dependents: other_haste_dependents,
            haste_provider_candidates: other_haste_provider_candidates,
        } = &mut other.data;
        join(
            move || {
                join(
                    move || apply_overlay_map_to_both_draining(committed_files, other_files, files),
                    move || {
                        apply_overlay_map_to_both_draining(
                            committed_haste_modules,
                            other_haste_modules,
                            haste_modules,
                        )
                    },
                );
            },
            move || {
                join(
                    move || {
                        apply_set_overlay_to_both_draining(
                            committed_file_dependents,
                            other_file_dependents,
                            file_dependents,
                        )
                    },
                    move || {
                        join(
                            move || {
                                apply_set_overlay_to_both_draining(
                                    committed_haste_dependents,
                                    other_haste_dependents,
                                    haste_dependents,
                                )
                            },
                            move || {
                                apply_set_overlay_to_both_draining(
                                    committed_haste_provider_candidates,
                                    other_haste_provider_candidates,
                                    haste_provider_candidates,
                                )
                            },
                        );
                    },
                );
            },
        );
    }

    pub fn apply_overlay_to_both(&self, other: &CommittedHeap, overlay: &HeapOverlay) {
        let committed = self.state.load_full();
        let other = other.state.load_full();
        let mut committed = committed.write();
        let mut other = other.write();
        let CommittedHeapData {
            files,
            haste_modules,
            file_dependents,
            haste_dependents,
            haste_provider_candidates,
        } = &mut committed.data;
        let CommittedHeapData {
            files: other_files,
            haste_modules: other_haste_modules,
            file_dependents: other_file_dependents,
            haste_dependents: other_haste_dependents,
            haste_provider_candidates: other_haste_provider_candidates,
        } = &mut other.data;
        join(
            || {
                join(
                    || apply_overlay_map_to_both(files, other_files, &overlay.files),
                    || {
                        apply_overlay_map_to_both(
                            haste_modules,
                            other_haste_modules,
                            &overlay.haste_modules,
                        )
                    },
                );
            },
            || {
                join(
                    || {
                        apply_set_overlay_to_both(
                            file_dependents,
                            other_file_dependents,
                            &overlay.file_dependents,
                        )
                    },
                    || {
                        join(
                            || {
                                apply_set_overlay_to_both(
                                    haste_dependents,
                                    other_haste_dependents,
                                    &overlay.haste_dependents,
                                )
                            },
                            || {
                                apply_set_overlay_to_both(
                                    haste_provider_candidates,
                                    other_haste_provider_candidates,
                                    &overlay.haste_provider_candidates,
                                )
                            },
                        );
                    },
                );
            },
        );
    }

    pub fn apply_overlay_to_both_draining(&self, other: &CommittedHeap, overlay: &mut HeapOverlay) {
        self.apply_commit_deltas_to_both(other, overlay);
    }
}

impl Default for CommittedHeap {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct HeapOverlayCommitDeltas {
    files: DrainedOverlayMap<FileKey, FileEntry>,
    haste_modules: DrainedOverlayMap<HasteModuleInfo, HasteModule>,
    file_dependents: DrainedSetOverlay<FileKey, FileKey>,
    haste_dependents: DrainedSetOverlay<HasteModuleInfo, FileKey>,
    haste_provider_candidates: DrainedSetOverlay<HasteModuleInfo, FileKey>,
}

pub(crate) type CommittedHeapReadGuard = ArcRwLockReadGuard<RawRwLockRecursive, CommittedHeapState>;

fn apply_commit_deltas_to_data(committed: &mut CommittedHeapData, deltas: HeapOverlayCommitDeltas) {
    let HeapOverlayCommitDeltas {
        files,
        haste_modules,
        file_dependents,
        haste_dependents,
        haste_provider_candidates,
    } = deltas;
    let CommittedHeapData {
        files: committed_files,
        haste_modules: committed_haste_modules,
        file_dependents: committed_file_dependents,
        haste_dependents: committed_haste_dependents,
        haste_provider_candidates: committed_haste_provider_candidates,
    } = committed;
    join(
        move || {
            join(
                move || apply_overlay_map_draining(committed_files, files),
                move || apply_overlay_map_draining(committed_haste_modules, haste_modules),
            );
        },
        move || {
            join(
                move || apply_set_overlay_draining(committed_file_dependents, file_dependents),
                move || {
                    join(
                        move || {
                            apply_set_overlay_draining(committed_haste_dependents, haste_dependents)
                        },
                        move || {
                            apply_set_overlay_draining(
                                committed_haste_provider_candidates,
                                haste_provider_candidates,
                            )
                        },
                    );
                },
            );
        },
    );
}

fn apply_overlay_map<K, V>(committed: &mut CommitMap<K, V>, overlay: &LockfreeOverlayMap<K, V>)
where
    K: Eq + Hash + Dupe + Send + 'static,
    V: Dupe + Send + Sync + 'static,
{
    if committed.is_empty() {
        *committed = materialize_overlay_map(overlay);
        return;
    }
    for (key, value) in overlay.iter() {
        match value {
            OverlayValue::Present(value) => {
                committed.insert(key, value);
            }
            OverlayValue::Deleted => {
                committed.remove(&key);
            }
        }
    }
}

fn apply_set_overlay<K, V>(
    committed: &mut HashMap<K, Arc<Vec<V>>>,
    overlay: &LockfreeSetOverlay<K, V>,
) where
    K: Eq + Hash + Dupe + Send + 'static,
    V: Eq + Ord + Hash + Dupe + Send + Sync + 'static,
{
    if committed.is_empty() {
        *committed = materialize_set_overlay(overlay);
        return;
    }
    for (owner, value, op) in overlay.iter() {
        apply_set_op(committed, owner, value, op);
    }
}

fn apply_set_overlay_draining<K, V>(
    committed: &mut HashMap<K, Arc<Vec<V>>>,
    overlay: DrainedSetOverlay<K, V>,
) where
    K: Eq + Hash + Dupe + Send + 'static,
    V: Eq + Ord + Hash + Dupe + Send + Sync + 'static,
{
    if committed.is_empty() {
        *committed = materialize_set_overlay_draining(overlay);
        return;
    }
    for (owner, values) in overlay.into_owner_groups() {
        for (value, op) in values {
            apply_set_op(committed, owner.dupe(), value, op);
        }
    }
}

fn apply_overlay_map_draining<K, V>(
    committed: &mut CommitMap<K, V>,
    overlay: DrainedOverlayMap<K, V>,
) where
    K: Eq + Hash + Send + 'static,
    V: Send + 'static,
{
    if committed.is_empty() {
        *committed = materialize_overlay_map_draining(overlay);
        return;
    }
    for (hash, key, value) in overlay.into_entries() {
        match value {
            OverlayMapCommitValue::Present(value) => {
                commit_map_insert_hashed(committed, hash, key, value);
            }
            OverlayMapCommitValue::Deleted => {
                commit_map_remove_hashed(committed, hash, &key);
            }
        }
    }
}

fn apply_overlay_map_to_both<K, V>(
    committed: &mut CommitMap<K, V>,
    other: &mut CommitMap<K, V>,
    overlay: &LockfreeOverlayMap<K, V>,
) where
    K: Eq + Hash + Dupe + Send + 'static,
    V: Dupe + Send + Sync + 'static,
{
    if committed.is_empty() && other.is_empty() {
        let overlay = materialize_overlay_map(overlay);
        *committed = overlay
            .iter()
            .map(|(key, value)| (key.dupe(), value.dupe()))
            .collect();
        *other = overlay;
        return;
    }
    for (key, value) in overlay.iter() {
        match value {
            OverlayValue::Present(value) => {
                committed.insert(key.dupe(), value.dupe());
                other.insert(key, value);
            }
            OverlayValue::Deleted => {
                committed.remove(&key);
                other.remove(&key);
            }
        }
    }
}

fn materialize_overlay_map<K, V>(overlay: &LockfreeOverlayMap<K, V>) -> CommitMap<K, V>
where
    K: Eq + Hash + Dupe + Send + 'static,
    V: Dupe + Send + Sync + 'static,
{
    overlay
        .iter()
        .collect::<Vec<_>>()
        .into_par_iter()
        .fold(CommitMap::default, |mut map, (key, value)| {
            match value {
                OverlayValue::Present(value) => {
                    map.insert(key, value);
                }
                OverlayValue::Deleted => {}
            }
            map
        })
        .reduce(CommitMap::default, |mut left, right| {
            left.extend(right);
            left
        })
}

fn apply_overlay_map_to_both_draining<K, V>(
    committed: &mut CommitMap<K, V>,
    other: &mut CommitMap<K, V>,
    overlay: DrainedOverlayMap<K, V>,
) where
    K: Eq + Hash + Dupe + Send + 'static,
    V: Dupe + Send + 'static,
{
    for (hash, key, value) in overlay.into_entries() {
        match value {
            OverlayMapCommitValue::Present(value) => {
                commit_map_insert_hashed(committed, hash, key.dupe(), value.dupe());
                commit_map_insert_hashed(other, hash, key, value);
            }
            OverlayMapCommitValue::Deleted => {
                commit_map_remove_hashed(committed, hash, &key);
                commit_map_remove_hashed(other, hash, &key);
            }
        }
    }
}

fn apply_set_overlay_to_both<K, V>(
    committed: &mut HashMap<K, Arc<Vec<V>>>,
    other: &mut HashMap<K, Arc<Vec<V>>>,
    overlay: &LockfreeSetOverlay<K, V>,
) where
    K: Eq + Hash + Dupe + Send + 'static,
    V: Eq + Ord + Hash + Dupe + Send + Sync + 'static,
{
    if committed.is_empty() && other.is_empty() {
        let overlay = materialize_set_overlay(overlay);
        *committed = overlay.clone();
        *other = overlay;
        return;
    }
    for (owner, value, op) in overlay.iter() {
        apply_set_op(committed, owner.dupe(), value.dupe(), op);
        apply_set_op(other, owner, value, op);
    }
}

fn apply_set_overlay_to_both_draining<K, V>(
    committed: &mut HashMap<K, Arc<Vec<V>>>,
    other: &mut HashMap<K, Arc<Vec<V>>>,
    overlay: DrainedSetOverlay<K, V>,
) where
    K: Eq + Hash + Dupe + Send + 'static,
    V: Eq + Ord + Hash + Dupe + Send + Sync + 'static,
{
    if committed.is_empty() && other.is_empty() {
        let overlay = materialize_set_overlay_draining(overlay);
        *committed = overlay.clone();
        *other = overlay;
        return;
    }
    for (owner, values) in overlay.into_owner_groups() {
        for (value, op) in values {
            apply_set_op(committed, owner.dupe(), value.dupe(), op);
            apply_set_op(other, owner.dupe(), value, op);
        }
    }
}

fn materialize_set_overlay<K, V>(overlay: &LockfreeSetOverlay<K, V>) -> HashMap<K, Arc<Vec<V>>>
where
    K: Eq + Hash + Dupe + Send + 'static,
    V: Eq + Ord + Hash + Dupe + Send + Sync + 'static,
{
    overlay
        .iter_owner_groups_for_empty_base()
        .collect::<Vec<_>>()
        .into_par_iter()
        .fold(HashMap::new, |mut map, (owner, values)| {
            let set = set_from_owner_ops(values);
            if !set.is_empty() {
                map.insert(owner, Arc::new(set));
            }
            map
        })
        .reduce(HashMap::new, |mut left, right| {
            left.extend(right);
            left
        })
}

fn materialize_set_overlay_draining<K, V>(
    overlay: DrainedSetOverlay<K, V>,
) -> HashMap<K, Arc<Vec<V>>>
where
    K: Eq + Hash + Dupe + Send + 'static,
    V: Eq + Ord + Hash + Dupe + Send + Sync + 'static,
{
    let mut map = HashMap::with_capacity(overlay.owner_count());
    for (owner, values) in overlay.into_arc_owner_groups_for_empty_base() {
        if !values.is_empty() {
            map.insert(owner, values);
        }
    }
    map
}

fn materialize_overlay_map_draining<K, V>(overlay: DrainedOverlayMap<K, V>) -> CommitMap<K, V>
where
    K: Eq + Hash + Send + 'static,
    V: Send + 'static,
{
    let mut map = commit_map_with_capacity(overlay.len());
    for (hash, key, value) in overlay.into_entries() {
        match value {
            OverlayMapCommitValue::Present(value) => {
                commit_map_insert_hashed(&mut map, hash, key, value);
            }
            OverlayMapCommitValue::Deleted => {}
        }
    }
    map
}

fn set_from_owner_ops<V>(values: SetOwnerOps<V>) -> Vec<V> {
    match values {
        SetOwnerOps::Adds(values) => values,
        SetOwnerOps::Ops(values) => values
            .into_iter()
            .filter_map(|(value, op)| match op {
                SetOp::Add => Some(value),
                SetOp::Remove => None,
            })
            .collect(),
    }
}

fn apply_set_op<K, V>(committed: &mut HashMap<K, Arc<Vec<V>>>, owner: K, value: V, op: SetOp)
where
    K: Eq + Hash,
    V: Eq + Ord + Dupe,
{
    match op {
        SetOp::Add => {
            let values = mutable_set_values(committed.entry(owner).or_default());
            if !values.contains(&value) {
                values.push(value);
            }
        }
        SetOp::Remove => {
            let remove_owner = if let Some(values) = committed.get_mut(&owner) {
                let values = mutable_set_values(values);
                values.retain(|candidate| candidate != &value);
                values.is_empty()
            } else {
                false
            };
            if remove_owner {
                committed.remove(&owner);
            }
        }
    }
}

fn mutable_set_values<V>(values: &mut Arc<Vec<V>>) -> &mut Vec<V>
where
    V: Dupe,
{
    if Arc::get_mut(values).is_none() {
        *values = Arc::new(values.iter().map(Dupe::dupe).collect());
    }
    Arc::get_mut(values).expect("set values should be unique after clone")
}

#[derive(Debug, Default)]
pub struct HeapOverlay {
    files: LockfreeOverlayMap<FileKey, FileEntry>,
    haste_modules: LockfreeOverlayMap<HasteModuleInfo, HasteModule>,
    file_dependents: LockfreeSetOverlay<FileKey, FileKey>,
    haste_dependents: LockfreeSetOverlay<HasteModuleInfo, FileKey>,
    haste_provider_candidates: LockfreeSetOverlay<HasteModuleInfo, FileKey>,
}

impl HeapOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
            && self.haste_modules.is_empty()
            && self.file_dependents.is_empty()
            && self.haste_dependents.is_empty()
            && self.haste_provider_candidates.is_empty()
    }

    pub(crate) fn clear_latest_entries_parallel(&mut self) {
        join(
            || self.files.clear_latest_entries_parallel(),
            || self.haste_modules.clear_latest_entries_parallel(),
        );
    }

    pub fn changed_files(&self) -> impl Iterator<Item = FileKey> + '_ {
        self.files
            .keys()
            .map(Dupe::dupe)
            .chain(self.file_dependents.owner_keys())
    }

    pub fn changed_haste_modules(&self) -> impl Iterator<Item = HasteModuleInfo> + '_ {
        self.haste_modules
            .keys()
            .map(Dupe::dupe)
            .chain(self.haste_dependents.owner_keys())
            .chain(self.haste_provider_candidates.owner_keys())
    }

    fn take_commit_deltas(&mut self) -> HeapOverlayCommitDeltas {
        let (
            (files, haste_modules),
            (file_dependents, (haste_dependents, haste_provider_candidates)),
        ) = join(
            || {
                join(
                    || self.files.take_commit_entries(),
                    || self.haste_modules.take_commit_entries(),
                )
            },
            || {
                join(
                    || self.file_dependents.take_commit_owner_groups(),
                    || {
                        join(
                            || self.haste_dependents.take_commit_owner_groups(),
                            || self.haste_provider_candidates.take_commit_owner_groups(),
                        )
                    },
                )
            },
        );
        HeapOverlayCommitDeltas {
            files,
            haste_modules,
            file_dependents,
            haste_dependents,
            haste_provider_candidates,
        }
    }

    pub fn file_count_over(&self, base: &CommitMap<FileKey, FileEntry>) -> usize {
        overlay_count_over(base, &self.files)
    }

    pub fn haste_module_count_over(&self, base: &CommitMap<HasteModuleInfo, HasteModule>) -> usize {
        overlay_count_over(base, &self.haste_modules)
    }
}

fn overlay_count_over<K, V>(base: &CommitMap<K, V>, overlay: &LockfreeOverlayMap<K, V>) -> usize
where
    K: Eq + Hash + Dupe + 'static,
    V: Dupe + 'static,
{
    overlay.count_over(base)
}

#[derive(Clone)]
pub struct HeapReader<'a> {
    committed: &'a CommittedHeapData,
    overlay: Option<&'a HeapOverlay>,
}

impl<'a> HeapReader<'a> {
    pub(crate) fn committed(committed: &'a CommittedHeapData) -> Self {
        Self {
            committed,
            overlay: None,
        }
    }

    pub(crate) fn transactional(
        committed: &'a CommittedHeapData,
        overlay: &'a HeapOverlay,
    ) -> Self {
        Self {
            committed,
            overlay: Some(overlay),
        }
    }

    fn overlay_file_entry(&self, file: &FileKey) -> Option<Option<FileEntry>> {
        self.overlay
            .as_ref()
            .and_then(|overlay| overlay.files.get(file))
            .map(|value| match value {
                OverlayValue::Present(entry) => Some(entry),
                OverlayValue::Deleted => None,
            })
    }

    fn overlay_haste_module(&self, info: &HasteModuleInfo) -> Option<Option<HasteModule>> {
        self.overlay
            .as_ref()
            .and_then(|overlay| overlay.haste_modules.get(info))
            .map(|value| match value {
                OverlayValue::Present(module) => Some(module),
                OverlayValue::Deleted => None,
            })
    }

    pub fn file_entry(&self, file: &FileKey) -> Option<FileEntry> {
        match self.overlay_file_entry(file) {
            Some(entry) => entry,
            None => self.committed_file_entry(file),
        }
    }

    pub fn committed_file_entry(&self, file: &FileKey) -> Option<FileEntry> {
        self.committed.files.get(file).map(Dupe::dupe)
    }

    pub fn get_parse(&self, file: &FileKey) -> Option<Parse> {
        self.file_entry(file).and_then(|entry| entry.parse())
    }

    pub fn get_requires_unsafe(&self, file: &FileKey) -> Arc<[FlowImportSpecifier]> {
        match self.get_parse(file) {
            Some(Parse::Typed(typed)) => typed.requires.dupe(),
            _ => panic!("Typed parse not found for file: {}", file.as_str()),
        }
    }

    pub fn get_resolved_requires_unsafe(&self, file: &FileKey) -> ResolvedRequires {
        match self.get_parse(file) {
            Some(Parse::Typed(typed)) => typed.resolved_requires_unsafe(),
            _ => panic!("Typed parse not found for file: {}", file.as_str()),
        }
    }

    pub fn get_type_sig_unsafe(&self, file: &FileKey) -> Arc<TypeSigModule<Loc>> {
        match self.get_parse(file) {
            Some(Parse::Typed(typed)) => typed.type_sig_unsafe(file),
            _ => panic!("Typed parse not found for file: {}", file.as_str()),
        }
    }

    pub fn get_package_info(&self, file: &FileKey) -> Option<Arc<PackageJson>> {
        match self.get_parse(file) {
            Some(Parse::Package(package)) => Some(package.package_info.dupe()),
            Some(Parse::Typed(_) | Parse::Untyped(_)) | None => None,
        }
    }

    pub fn is_typed_file(&self, file: &FileKey) -> bool {
        matches!(self.get_parse(file), Some(Parse::Typed(_)))
    }

    pub fn is_package_file(&self, file: &FileKey) -> bool {
        matches!(self.get_parse(file), Some(Parse::Package(_)))
    }

    pub fn get_parse_committed(&self, file: &FileKey) -> Option<Parse> {
        self.committed_file_entry(file)
            .and_then(|entry| entry.parse())
    }

    pub fn get_haste_info(&self, file: &FileKey) -> Option<HasteModuleInfo> {
        self.file_entry(file)
            .and_then(|entry| entry.get_haste_info())
    }

    pub fn get_haste_info_committed(&self, file: &FileKey) -> Option<HasteModuleInfo> {
        self.committed_file_entry(file)
            .and_then(|entry| entry.get_haste_info())
    }

    pub fn get_haste_module(&self, info: &HasteModuleInfo) -> Option<HasteModule> {
        match self.overlay_haste_module(info) {
            Some(module) => module,
            None => self.get_haste_module_committed(info),
        }
    }

    pub fn get_haste_module_committed(&self, info: &HasteModuleInfo) -> Option<HasteModule> {
        self.committed.haste_modules.get(info).map(Dupe::dupe)
    }

    pub fn get_provider(&self, dependency: &Dependency) -> Option<FileKey> {
        match dependency.target() {
            DependencyTarget::HasteModule(info) => self
                .get_haste_module(info)
                .and_then(|module| module.get_provider()),
            DependencyTarget::File(file) => self
                .file_entry(file)
                .and_then(|entry| self.file_provider_latest(file, &entry)),
        }
    }

    pub fn get_dependency(&self, modulename: &Modulename) -> Option<Dependency> {
        match modulename {
            Modulename::Haste(info) => self.get_haste_module(info).map(|m| m.dependency()),
            Modulename::Filename(file) => self.file_entry(file).map(|e| e.dependency()),
        }
    }

    pub fn get_provider_committed(&self, dependency: &Dependency) -> Option<FileKey> {
        match dependency.target() {
            DependencyTarget::HasteModule(info) => self
                .get_haste_module_committed(info)
                .and_then(|module| module.get_provider()),
            DependencyTarget::File(file) => self
                .committed_file_entry(file)
                .and_then(|entry| self.file_provider_committed(file, &entry)),
        }
    }

    fn file_provider_latest(&self, file: &FileKey, entry: &FileEntry) -> Option<FileKey> {
        if let Some(alternate) = entry.get_alternate_file()
            && self.get_parse(&alternate).is_some()
        {
            return Some(alternate);
        }
        entry.parse().map(|_| file.dupe())
    }

    fn file_provider_committed(&self, file: &FileKey, entry: &FileEntry) -> Option<FileKey> {
        if let Some(alternate) = entry.get_alternate_file()
            && self.get_parse_committed(&alternate).is_some()
        {
            return Some(alternate);
        }
        entry.parse().map(|_| file.dupe())
    }

    pub fn file_has_changed(&self, file: &FileKey) -> bool {
        self.overlay
            .as_ref()
            .is_some_and(|overlay| overlay.files.get(file).is_some())
    }

    pub fn dependents(&self, module: &Modulename) -> BTreeSet<FileKey> {
        match module {
            Modulename::Filename(file) => self.file_dependents(file),
            Modulename::Haste(info) => self.haste_dependents(info),
        }
    }

    pub fn haste_provider_candidates(&self, info: &HasteModuleInfo) -> BTreeSet<FileKey> {
        set_with_overlay(
            &self.committed.haste_provider_candidates,
            self.overlay
                .as_ref()
                .map(|overlay| &overlay.haste_provider_candidates),
            info,
        )
    }

    pub fn file_dependents(&self, file: &FileKey) -> BTreeSet<FileKey> {
        set_with_overlay(
            &self.committed.file_dependents,
            self.overlay
                .as_ref()
                .map(|overlay| &overlay.file_dependents),
            file,
        )
    }

    pub fn haste_dependents(&self, info: &HasteModuleInfo) -> BTreeSet<FileKey> {
        set_with_overlay(
            &self.committed.haste_dependents,
            self.overlay
                .as_ref()
                .map(|overlay| &overlay.haste_dependents),
            info,
        )
    }
}

fn set_with_overlay<K, V>(
    committed: &HashMap<K, Arc<Vec<V>>>,
    overlay: Option<&LockfreeSetOverlay<K, V>>,
    owner: &K,
) -> BTreeSet<V>
where
    K: Eq + Hash + Dupe + 'static,
    V: Eq + Ord + Hash + Dupe + 'static,
{
    let mut set = committed
        .get(owner)
        .into_iter()
        .flat_map(|values| values.iter().map(Dupe::dupe))
        .collect::<BTreeSet<_>>();
    if let Some(overlay) = overlay {
        for (value, op) in overlay.iter_for_owner(owner) {
            match op {
                SetOp::Add => {
                    set.insert(value);
                }
                SetOp::Remove => {
                    set.remove(&value);
                }
            }
        }
    }
    set
}

#[derive(Clone)]
pub struct HeapWriter<'a> {
    reader: HeapReader<'a>,
    overlay: &'a HeapOverlay,
}

impl<'a> HeapWriter<'a> {
    pub(crate) fn new(committed: &'a CommittedHeapData, overlay: &'a HeapOverlay) -> Self {
        Self {
            reader: HeapReader::transactional(committed, overlay),
            overlay,
        }
    }

    pub fn reader(&self) -> &HeapReader<'a> {
        &self.reader
    }

    pub fn set_file_entry(&self, file: FileKey, entry: FileEntry) {
        self.overlay.files.insert(file, entry);
    }

    /// Derive a new entry from the current one without a window in which another
    /// writer's store can be lost. Every writer that changes part of an entry goes
    /// through here; `set_file_entry` is for writers that replace it outright.
    pub fn update_file_entry(&self, file: &FileKey, update: impl FnOnce(FileEntry) -> FileEntry) {
        let committed = self.reader.committed;
        self.overlay.files.update(
            file.dupe(),
            || committed.files.get(file).map(Dupe::dupe),
            |current| current.map(update),
        );
    }

    /// Same, for a writer that also creates the entry when the file is new.
    pub fn upsert_file_entry(
        &self,
        file: &FileKey,
        update: impl FnOnce(Option<FileEntry>) -> FileEntry,
    ) {
        let committed = self.reader.committed;
        self.overlay.files.update(
            file.dupe(),
            || committed.files.get(file).map(Dupe::dupe),
            |current| Some(update(current)),
        );
    }

    pub fn remove_file_entry(&self, file: FileKey) {
        self.overlay.files.remove(file);
    }

    pub fn set_haste_module(&self, info: HasteModuleInfo, module: HasteModule) {
        self.overlay.haste_modules.insert(info, module);
    }

    pub fn remove_haste_module(&self, info: HasteModuleInfo) {
        self.overlay.haste_modules.remove(info);
    }

    pub fn add_file_dependent(&self, owner: FileKey, dependent: FileKey) {
        self.overlay.file_dependents.add(owner, dependent);
    }

    pub fn remove_file_dependent(&self, owner: FileKey, dependent: FileKey) {
        self.set_file_dependent_op(owner, dependent, SetOp::Remove);
    }

    fn set_file_dependent_op(&self, owner: FileKey, dependent: FileKey, op: SetOp) {
        match op {
            SetOp::Add => self.overlay.file_dependents.add(owner, dependent),
            SetOp::Remove => self.overlay.file_dependents.remove(owner, dependent),
        }
    }

    pub fn add_haste_dependent(&self, owner: HasteModuleInfo, dependent: FileKey) {
        self.overlay.haste_dependents.add(owner, dependent);
    }

    pub fn remove_haste_dependent(&self, owner: HasteModuleInfo, dependent: FileKey) {
        self.set_haste_dependent_op(owner, dependent, SetOp::Remove);
    }

    fn set_haste_dependent_op(&self, owner: HasteModuleInfo, dependent: FileKey, op: SetOp) {
        match op {
            SetOp::Add => self.overlay.haste_dependents.add(owner, dependent),
            SetOp::Remove => self.overlay.haste_dependents.remove(owner, dependent),
        }
    }

    pub fn add_haste_provider_candidate(&self, owner: HasteModuleInfo, provider: FileKey) {
        self.overlay.haste_provider_candidates.add(owner, provider);
    }

    pub fn remove_haste_provider_candidate(&self, owner: HasteModuleInfo, provider: FileKey) {
        self.set_haste_provider_candidate_op(owner, provider, SetOp::Remove);
    }

    fn set_haste_provider_candidate_op(
        &self,
        owner: HasteModuleInfo,
        provider: FileKey,
        op: SetOp,
    ) {
        match op {
            SetOp::Add => self.overlay.haste_provider_candidates.add(owner, provider),
            SetOp::Remove => self
                .overlay
                .haste_provider_candidates
                .remove(owner, provider),
        }
    }
}
