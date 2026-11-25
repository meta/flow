/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Disjoint sets of checked files (files which we not only parse but also merge)
//!
//! Lazy mode:
//!   Focused files - files which the user cares about.
//!   Dependent files - files which directly or transitively depend on focused files
//!   Dependency files - files on which Focused or Dependent files  directly or transitively depend
//!
//! Non-lazy mode:
//!   Focused files - Every checked file
//!   Dependent files - Empty
//!   Dependency files - Empty

use dupe::Dupe;
use flow_data_structure_wrapper::ord_set::DiffItem;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_parser::file_key::FileKey;

// A file foo.js can be focused, a dependent, and a dependency all at the same time. However, in
// CheckedSet, we just keep track of its most important role. A focused file is more important
// than a dependent file, which is more important than a dependency file
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Focused,
    Dependent,
    Dependency,
}

#[derive(Debug, Clone)]
pub struct CheckedSet {
    focused: FlowOrdSet<FileKey>,
    dependents: FlowOrdSet<FileKey>,
    dependencies: FlowOrdSet<FileKey>,
}

impl Dupe for CheckedSet {}

/// Computes target - to_remove (set difference) using size-optimized strategy.
/// Mutates target in place to remove all elements in to_remove.
fn set_diff(target: &mut FlowOrdSet<FileKey>, to_remove: &FlowOrdSet<FileKey>) {
    if to_remove.is_empty() {
        return;
    }
    if target.is_empty() {
        return;
    }

    if to_remove.len() * 10 > target.len() * 9 {
        // Strategy 1: to_remove is huge (>90% of target), build new set from survivors

        let mut set = FlowOrdSet::new();
        // OrdSet::diff is optimized to avoid visiting shared trees in between target and to_remove.
        for v in target.diff(to_remove).filter_map(|diff_item| {
            if let DiffItem::Remove(v) = diff_item {
                Some(v)
            } else {
                None
            }
        }) {
            set.insert(v.dupe());
        }
        *target = set;
    } else {
        // Otherwise, let's just use the regular set differences
        let inner = std::mem::replace(target, FlowOrdSet::new()).into_inner();
        *target = inner
            .relative_complement(to_remove.dupe().into_inner()) // clone needed: FlowOrdSet.into_inner() consumes self
            .into();
    }
}

impl CheckedSet {
    pub fn empty() -> Self {
        Self {
            focused: FlowOrdSet::new(),
            dependents: FlowOrdSet::new(),
            dependencies: FlowOrdSet::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.focused.is_empty() && self.dependents.is_empty() && self.dependencies.is_empty()
    }

    pub fn of_focused_list(focused: Vec<FileKey>) -> Self {
        let mut focused_set = FlowOrdSet::new();
        for key in focused {
            focused_set.insert(key);
        }
        Self {
            focused: focused_set,
            dependents: FlowOrdSet::new(),
            dependencies: FlowOrdSet::new(),
        }
    }

    pub fn cardinal(&self) -> usize {
        self.focused.len() + self.dependents.len() + self.dependencies.len()
    }

    pub fn focused_cardinal(&self) -> usize {
        self.focused.len()
    }

    pub fn dependents_cardinal(&self) -> usize {
        self.dependents.len()
    }

    pub fn dependencies_cardinal(&self) -> usize {
        self.dependencies.len()
    }

    pub fn mem(&self, key: &FileKey) -> bool {
        self.focused.contains(key)
            || self.dependents.contains(key)
            || self.dependencies.contains(key)
    }

    pub fn add(
        &mut self,
        focused: Option<FlowOrdSet<FileKey>>,
        dependents: Option<FlowOrdSet<FileKey>>,
        dependencies: Option<FlowOrdSet<FileKey>>,
    ) {
        if let Some(mut set) = focused {
            // Optimize: extend the smaller set into the larger one
            if self.focused.len() < set.len() {
                std::mem::swap(&mut self.focused, &mut set);
            }
            for key in set {
                self.focused.insert(key);
            }
        }
        if let Some(mut set) = dependents {
            if self.dependents.len() < set.len() {
                std::mem::swap(&mut self.dependents, &mut set);
            }
            for key in set {
                self.dependents.insert(key);
            }
        }
        if let Some(mut set) = dependencies {
            if self.dependencies.len() < set.len() {
                std::mem::swap(&mut self.dependencies, &mut set);
            }
            for key in set {
                self.dependencies.insert(key);
            }
        }

        // Ensure disjointness using size-optimized diff
        if !self.focused.is_empty() {
            set_diff(&mut self.dependents, &self.focused);
            set_diff(&mut self.dependencies, &self.focused);
        }
        if !self.dependents.is_empty() {
            set_diff(&mut self.dependencies, &self.dependents);
        }
    }

    pub fn remove(&mut self, to_remove: &FlowOrdSet<FileKey>) {
        for k in to_remove {
            self.focused.remove(k);
            self.dependents.remove(k);
            self.dependencies.remove(k);
        }
    }

    pub fn union(&mut self, other: Self) {
        self.add(
            Some(other.focused),
            Some(other.dependents),
            Some(other.dependencies),
        );
    }

    /// [diff a b] removes from [a] every key which exists in [b] and which has an equal or higher
    /// kind in [b] than it does in [a], where Focused > Dependent > Dependency. So
    ///
    /// diff
    ///   { A: Focused, B: Focused,   C: Dependency, D: Dependent }
    ///   { A: Focused, B: Dependent, C: Dependent}
    /// = { B: Focused, D: Dependent }
    pub fn diff(&mut self, other: &Self) {
        for k in &other.focused {
            self.focused.remove(k);
        }
        for k in &other.focused {
            self.dependents.remove(k);
        }
        for k in &other.dependents {
            self.dependents.remove(k);
        }
        for k in &other.focused {
            self.dependencies.remove(k);
        }
        for k in &other.dependents {
            self.dependencies.remove(k);
        }
        for k in &other.dependencies {
            self.dependencies.remove(k);
        }
    }

    pub fn filter<F>(&self, f: F) -> Self
    where
        F: Fn(&FileKey, Kind) -> bool,
    {
        let mut new_focused = FlowOrdSet::new();
        let mut new_dependents = FlowOrdSet::new();
        let mut new_dependencies = FlowOrdSet::new();
        for k in self.focused.iter().filter(|key| f(key, Kind::Focused)) {
            new_focused.insert(k.dupe());
        }
        for k in self.dependents.iter().filter(|key| f(key, Kind::Dependent)) {
            new_dependents.insert(k.dupe());
        }
        for k in self
            .dependencies
            .iter()
            .filter(|key| f(key, Kind::Dependency))
        {
            new_dependencies.insert(k.dupe());
        }
        Self {
            focused: new_focused,
            dependents: new_dependents,
            dependencies: new_dependencies,
        }
    }

    // Gives you a FilenameSet of all the checked files
    pub fn all(self) -> FlowOrdSet<FileKey> {
        self.focused
            .into_inner()
            .union(self.dependents.into_inner())
            .union(self.dependencies.into_inner())
            .into()
    }

    pub fn is_focused(kind: Kind) -> bool {
        kind == Kind::Focused
    }

    pub fn is_dependent(kind: Kind) -> bool {
        kind == Kind::Dependent
    }

    pub fn is_dependency(kind: Kind) -> bool {
        kind == Kind::Dependency
    }

    // Gives you a FilenameSet of all the focused files
    pub fn focused(&self) -> &FlowOrdSet<FileKey> {
        &self.focused
    }

    // Gives you a FilenameSet of all the dependent files
    pub fn dependents(&self) -> &FlowOrdSet<FileKey> {
        &self.dependents
    }

    // Gives you a FilenameSet of all the dependency files
    pub fn dependencies(&self) -> &FlowOrdSet<FileKey> {
        &self.dependencies
    }

    pub fn mem_focused(&self, x: &FileKey) -> bool {
        self.focused.contains(x)
    }

    pub fn mem_dependent(&self, x: &FileKey) -> bool {
        self.dependents.contains(x)
    }

    pub fn mem_dependency(&self, x: &FileKey) -> bool {
        self.dependencies.contains(x)
    }

    // This is O(n) in the size of the checked set. Because checked sets are typically very large, this
    // operation should be avoided in production code.
    pub fn debug_equal(&self, other: &CheckedSet) -> bool {
        self.focused == other.focused
            && self.dependents == other.dependents
            && self.dependencies == other.dependencies
    }

    // Helper function for debugging
    pub fn debug_to_string(&self, limit: Option<usize>) -> String {
        fn string_of_set(set: &FlowOrdSet<FileKey>, limit: Option<usize>) -> String {
            let files: Vec<String> = set.iter().map(|f| format!("\"{}\"", f.as_str())).collect();
            let files = match limit {
                None => files,
                Some(n) => crate::list_utils::first_upto_n(
                    n,
                    |t| Some(format!("[shown {}/{}]", n, t)),
                    files,
                ),
            };
            files.join("\n")
        }

        format!(
            "Focused:\n{}\nDependents:\n{}\nDependencies:\n{}",
            string_of_set(&self.focused, limit),
            string_of_set(&self.dependents, limit),
            string_of_set(&self.dependencies, limit)
        )
    }

    pub fn debug_counts_to_string(&self) -> String {
        format!(
            "Focused: {}, Dependents: {}, Dependencies: {}",
            self.focused_cardinal(),
            self.dependents_cardinal(),
            self.dependencies_cardinal()
        )
    }
}
