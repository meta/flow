/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! A map overlay: a shared immutable `base` with a `delta` of per-key changes
//! layered on top.
//!
//! `base` is shared via `Arc`, so cloning an `OverlayMap` copies only the
//! (typically small) `delta`. Reads consult `delta` first and fall through to
//! `base`; a removal of a key present in `base` is recorded as a `None`
//! tombstone so the base entry is hidden without mutating the shared base.

use std::collections::BTreeMap;
use std::collections::btree_map;
use std::iter::Peekable;
use std::sync::Arc;

use dupe::Dupe;
use parking_lot::ArcRwLockReadGuard;
use parking_lot::RawRwLockRecursive;
use parking_lot::RecursiveRwLock;

/// A `BTreeMap`-like map with a shared base and an owned delta of changes.
#[derive(Clone)]
pub struct OverlayMap<K: Ord + Clone, V: Clone, B = Arc<BTreeMap<K, V>>>
where
    B: OverlayMapBase<K, V>,
{
    base: B,
    /// `Some(v)` overrides/inserts `v`; `None` is a tombstone hiding the base entry.
    delta: BTreeMap<K, Option<V>>,
}

pub trait OverlayMapBase<K: Ord + Clone, V: Clone>: Clone {
    fn map(&self) -> &BTreeMap<K, V>;
}

pub struct OverlayMapDelta<K, V> {
    delta: BTreeMap<K, Option<V>>,
}

impl<K, V> OverlayMapDelta<K, V> {
    pub fn into_values(self) -> impl Iterator<Item = (K, V)> {
        self.delta
            .into_iter()
            .filter_map(|(key, entry)| entry.map(|value| (key, value)))
    }
}

pub fn apply_delta_to_base_map<K: Ord, V>(base: &mut BTreeMap<K, V>, delta: OverlayMapDelta<K, V>) {
    for (key, entry) in delta.delta {
        match entry {
            Some(value) => {
                base.insert(key, value);
            }
            None => {
                base.remove(&key);
            }
        }
    }
}

impl<K: Ord + Clone, V: Clone> OverlayMapBase<K, V> for Arc<BTreeMap<K, V>> {
    fn map(&self) -> &BTreeMap<K, V> {
        self
    }
}

pub type EnvCell<T> = RecursiveRwLock<T>;
pub type EnvCellReadGuard<T> = ArcRwLockReadGuard<RawRwLockRecursive, T>;

pub struct EnvCellMapBase<T, K: Ord + Clone, V: Clone> {
    snapshot: EnvCellReadGuard<T>,
    map: fn(&T) -> &BTreeMap<K, V>,
}

impl<T, K: Ord + Clone, V: Clone> Clone for EnvCellMapBase<T, K, V> {
    fn clone(&self) -> Self {
        let cell = ArcRwLockReadGuard::rwlock(&self.snapshot).dupe();
        Self {
            snapshot: cell.read_arc(),
            map: self.map,
        }
    }
}

impl<T, K: Ord + Clone, V: Clone> EnvCellMapBase<T, K, V> {
    pub fn new(cell: Arc<EnvCell<T>>, map: fn(&T) -> &BTreeMap<K, V>) -> Self {
        Self {
            snapshot: cell.read_arc(),
            map,
        }
    }

    pub fn cell(&self) -> &Arc<EnvCell<T>> {
        ArcRwLockReadGuard::rwlock(&self.snapshot)
    }
}

impl<K: Ord + Clone, V: Clone> EnvCellMapBase<BTreeMap<K, V>, K, V> {
    pub fn identity(cell: Arc<EnvCell<BTreeMap<K, V>>>) -> Self {
        fn identity<K: Ord + Clone, V: Clone>(map: &BTreeMap<K, V>) -> &BTreeMap<K, V> {
            map
        }

        Self::new(cell, identity)
    }
}

impl<T, K: Ord + Clone, V: Clone> OverlayMapBase<K, V> for EnvCellMapBase<T, K, V> {
    fn map(&self) -> &BTreeMap<K, V> {
        (self.map)(&self.snapshot)
    }
}

impl<K: Ord + Clone, V: Clone> OverlayMap<K, V> {
    pub fn new() -> Self {
        Self {
            base: Arc::new(BTreeMap::new()),
            delta: BTreeMap::new(),
        }
    }

    pub fn from_base_map(base: BTreeMap<K, V>) -> Self {
        Self {
            base: Arc::new(base),
            delta: BTreeMap::new(),
        }
    }
}

impl<K: Ord + Clone, V: Clone, B: OverlayMapBase<K, V>> OverlayMap<K, V, B> {
    pub fn with_base(base: B) -> Self {
        Self {
            base,
            delta: BTreeMap::new(),
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        match self.delta.get(key) {
            Some(entry) => entry.as_ref(),
            None => self.base.map().get(key),
        }
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    pub fn insert(&mut self, key: K, value: V) {
        self.delta.insert(key, Some(value));
    }

    pub fn remove(&mut self, key: &K) {
        if self.base.map().contains_key(key) {
            self.delta.insert(key.clone(), None);
        } else {
            self.delta.remove(key);
        }
    }

    pub fn insert_if_absent(&mut self, key: K, value: V) {
        if !self.contains_key(&key) {
            self.insert(key, value);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }

    pub fn len(&self) -> usize {
        let base = self.base.map();
        let mut len = base.len();
        for (key, entry) in &self.delta {
            match (entry, base.contains_key(key)) {
                (Some(_), false) => len += 1, // newly inserted key
                (None, true) => len -= 1,     // tombstone of a base key
                _ => {}                       // override, or tombstone with no base entry
            }
        }
        len
    }

    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter {
            base: self.base.map().iter().peekable(),
            delta: self.delta.iter().peekable(),
        }
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, v)| v)
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(k, _)| k)
    }

    /// Clone the visible contents into a fresh `BTreeMap` without consuming `self`.
    pub fn to_btree_map(&self) -> BTreeMap<K, V> {
        self.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    pub fn into_delta(self) -> OverlayMapDelta<K, V> {
        OverlayMapDelta { delta: self.delta }
    }

    pub fn into_delta_values(self) -> impl Iterator<Item = (K, V)> {
        self.into_delta().into_values()
    }
}

impl<K: Ord + Clone, V: Clone> Default for OverlayMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Ord + Clone, V: Clone> From<BTreeMap<K, V>> for OverlayMap<K, V> {
    fn from(base: BTreeMap<K, V>) -> Self {
        Self::from_base_map(base)
    }
}

impl<K: Ord + Clone, V: Clone> FromIterator<(K, V)> for OverlayMap<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        Self::from_base_map(iter.into_iter().collect())
    }
}

impl<'a, K: Ord + Clone, V: Clone, B: OverlayMapBase<K, V>> IntoIterator
    for &'a OverlayMap<K, V, B>
{
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Sorted merge of `base` and `delta`, with `delta` overriding `base` on equal
/// keys and tombstones (`delta` values of `None`) skipped. Yields keys in the
/// same order as the equivalent `BTreeMap`.
pub struct Iter<'a, K, V> {
    base: Peekable<btree_map::Iter<'a, K, V>>,
    delta: Peekable<btree_map::Iter<'a, K, Option<V>>>,
}

impl<'a, K: Ord, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match (self.base.peek(), self.delta.peek()) {
                (Some((base_key, _)), Some((delta_key, _))) => match base_key.cmp(delta_key) {
                    std::cmp::Ordering::Less => return self.base.next(),
                    std::cmp::Ordering::Greater => {
                        let (delta_key, delta_value) =
                            self.delta.next().expect("peeked delta entry");
                        if let Some(value) = delta_value {
                            return Some((delta_key, value));
                        }
                    }
                    std::cmp::Ordering::Equal => {
                        self.base.next();
                        let (delta_key, delta_value) =
                            self.delta.next().expect("peeked delta entry");
                        if let Some(value) = delta_value {
                            return Some((delta_key, value));
                        }
                    }
                },
                (Some(_), None) => return self.base.next(),
                (None, Some(_)) => {
                    let (delta_key, delta_value) = self.delta.next().expect("peeked delta entry");
                    if let Some(value) = delta_value {
                        return Some((delta_key, value));
                    }
                }
                (None, None) => return None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> OverlayMap<i32, i32> {
        OverlayMap::from_base_map(BTreeMap::from([(1, 10), (3, 30), (5, 50)]))
    }

    #[test]
    fn read_falls_through_to_base() {
        let map = base();
        assert_eq!(map.get(&1), Some(&10));
        assert_eq!(map.get(&3), Some(&30));
        assert_eq!(map.get(&2), None);
        assert_eq!(map.len(), 3);
    }

    #[test]
    fn delta_overrides_and_inserts() {
        let mut map = base();
        map.insert(3, 99); // override base
        map.insert(4, 40); // new key
        assert_eq!(map.get(&3), Some(&99));
        assert_eq!(map.get(&4), Some(&40));
        assert_eq!(map.get(&1), Some(&10));
        assert_eq!(map.len(), 4);
    }

    #[test]
    fn tombstone_hides_base_key() {
        let mut map = base();
        map.remove(&3);
        assert_eq!(map.get(&3), None);
        assert!(!map.contains_key(&3));
        assert_eq!(map.len(), 2);
        // Removing a key absent from base is a no-op.
        map.remove(&999);
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn iteration_matches_btreemap_order() {
        let mut map = base();
        map.insert(4, 40);
        map.insert(1, 11);
        map.remove(&3);
        let got: Vec<(i32, i32)> = map.iter().map(|(k, v)| (*k, *v)).collect();
        // Expected: base {1,3,5} with 1->11, 4 added, 3 tombstoned => sorted 1,4,5.
        assert_eq!(got, vec![(1, 11), (4, 40), (5, 50)]);
        let iterated: Vec<(i32, i32)> = map.iter().map(|(k, v)| (*k, *v)).collect();
        assert_eq!(iterated, got);
    }

    #[test]
    fn clone_then_mutate_does_not_disturb_original() {
        let original = base();
        let mut copy = original.clone();
        copy.insert(3, 99);
        copy.remove(&1);
        // The original keeps sharing the same base and its own delta.
        assert_eq!(original.get(&3), Some(&30));
        assert_eq!(original.get(&1), Some(&10));
        assert_eq!(copy.get(&3), Some(&99));
        assert_eq!(copy.get(&1), None);
    }

    #[test]
    fn delta_applies_to_external_base_map() {
        let mut map = base();
        map.insert(4, 40);
        map.remove(&5);
        let mut base = BTreeMap::from([(1, 10), (3, 30), (5, 50)]);
        let delta = map.into_delta();

        apply_delta_to_base_map(&mut base, delta);

        assert_eq!(base, BTreeMap::from([(1, 10), (3, 30), (4, 40)]));
    }

    #[test]
    fn empty_base_delta_values_iterates_insertions() {
        let mut map = OverlayMap::new();
        map.insert(2, 20);
        map.insert(1, 10);
        let values: Vec<_> = map.into_delta_values().collect();

        assert_eq!(values, vec![(1, 10), (2, 20)]);
    }
}
