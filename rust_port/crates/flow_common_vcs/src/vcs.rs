/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vcs {
    Hg,
    Git,
}

/// How a file differs between two states of a repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChange {
    Added,
    Modified,
    Removed,
}

pub fn find_root(recursion_limit: Option<usize>, dir: &Path) -> Option<(Vcs, PathBuf)> {
    let recursion_limit = recursion_limit.unwrap_or(100);
    fn walk(recursion_limit: usize, dir: &Path) -> Option<(Vcs, PathBuf)> {
        let parent_dir = dir.parent()?;
        if dir.join(".hg").exists() {
            Some((Vcs::Hg, dir.to_path_buf()))
        } else if dir.join(".git").exists() {
            Some((Vcs::Git, dir.to_path_buf()))
        } else if recursion_limit == 0 {
            None
        } else {
            walk(recursion_limit - 1, parent_dir)
        }
    }
    walk(recursion_limit, dir)
}

pub fn find(recursion_limit: Option<usize>, dir: &Path) -> Option<Vcs> {
    find_root(recursion_limit, dir).map(|(t, _)| t)
}

pub fn name(vcs: Vcs) -> &'static str {
    match vcs {
        Vcs::Git => "Git",
        Vcs::Hg => "Mercurial",
    }
}
