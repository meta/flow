/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! A wrapper around im::OrdMap that implements Dupe for cheap cloning.
//!
//! im::OrdMap is an immutable ordered map that uses structural sharing for efficient
//! cloning. This wrapper adds the Dupe trait, allowing consistent use of `.dupe()`
//! instead of `.clone()` throughout the Flow codebase.

use std::ops::Deref;
use std::ops::DerefMut;

use dupe::Dupe;
pub use im::ordmap::ConsumingIter;
pub use im::ordmap::DiffItem;
pub use im::ordmap::Iter;

/// A wrapper around im::OrdMap that implements Dupe for cheap cloning.
///
/// This type provides the same functionality as im::OrdMap but also implements
/// the Dupe trait, making it consistent with other types in the Flow codebase
/// that use `.dupe()` for cheap copies.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct FlowOrdMap<K: Ord + Clone, V: Clone>(im::OrdMap<K, V>);

impl<K: Ord + Clone, V: Ord + Clone> PartialOrd for FlowOrdMap<K, V> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<K: Ord + Clone, V: Ord + Clone> Ord for FlowOrdMap<K, V> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.iter().cmp(other.0.iter())
    }
}

impl<K: Ord + Clone, V: Clone> Default for FlowOrdMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Ord + Clone, V: Clone> Dupe for FlowOrdMap<K, V> {}

impl<K: Ord + Clone, V: Clone> FlowOrdMap<K, V> {
    pub fn new() -> Self {
        FlowOrdMap(im::OrdMap::new())
    }

    pub fn into_inner(self) -> im::OrdMap<K, V> {
        self.0
    }

    pub fn as_inner(&self) -> &im::OrdMap<K, V> {
        &self.0
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

impl<K: Ord + Clone, V: Clone> Deref for FlowOrdMap<K, V> {
    type Target = im::OrdMap<K, V>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<K: Ord + Clone, V: Clone> DerefMut for FlowOrdMap<K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<K: Ord + Clone, V: Clone> From<im::OrdMap<K, V>> for FlowOrdMap<K, V> {
    fn from(m: im::OrdMap<K, V>) -> Self {
        FlowOrdMap(m)
    }
}

impl<K: Ord + Clone, V: Clone> From<FlowOrdMap<K, V>> for im::OrdMap<K, V> {
    fn from(m: FlowOrdMap<K, V>) -> Self {
        m.0
    }
}

impl<K: Ord + Clone, V: Clone> FromIterator<(K, V)> for FlowOrdMap<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        FlowOrdMap(im::OrdMap::from_iter(iter))
    }
}

impl<K: Ord + Clone, V: Clone> IntoIterator for FlowOrdMap<K, V> {
    type Item = (K, V);
    type IntoIter = im::ordmap::ConsumingIter<(K, V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, K: Ord + Clone, V: Clone> IntoIterator for &'a FlowOrdMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = im::ordmap::Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

pub type OrdMapConsumingIter<A> = im::ordmap::ConsumingIter<A>;

pub type OrdMapIter<'a, K, V> = im::ordmap::Iter<'a, K, V>;
