/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use dupe::Dupe;

/// How a `dump-types --for-tool` dump lays out the types it prints. It travels only with
/// `--for-tool`, so none of its settings can be set without it.
#[derive(
    Debug,
    Clone,
    Copy,
    Dupe,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum ForToolLayout {
    /// Each type as a tree, cut off below `depth` levels.
    Tree {
        /// How far to expand a type before printing `<recursion>`.
        depth: i32,
        /// When set, a serialized subtree of more than this many bytes moves into a
        /// shared `$defs` table and every occurrence becomes `{"$ref": id}`.
        dedup_threshold: Option<usize>,
    },
    /// Every type value once, as an entry of a `$defs` table that refers to the values it contains
    /// by `{"$ref": id}`, with no depth limit, so a recursive type is a cycle of refs.
    Graph {
        /// How many entries to print before giving up on the rest.
        node_cap: usize,
    },
}

impl ForToolLayout {
    pub const DEFAULT_GRAPH_NODE_CAP: usize = 1_000_000;
}
