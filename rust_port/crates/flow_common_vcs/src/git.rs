/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use crate::vcs::FileChange;
pub use crate::vcs_utils::ErrorStatus;
use crate::vcs_utils::exec;
use crate::vcs_utils::split_null_terminated_lines;

async fn git(cwd: Option<&str>, args: &[&str]) -> Result<String, ErrorStatus> {
    exec(None, cwd, "git", args).await
}

pub async fn merge_base(cwd: Option<&str>, a: &str, b: &str) -> Result<String, ErrorStatus> {
    match git(cwd, &["merge-base", a, b]).await {
        Ok(stdout) => {
            if stdout.len() >= 40 {
                Ok(stdout[..40].to_string())
            } else {
                Err(ErrorStatus::Errored("Malformed merge-base".to_string()))
            }
        }
        Err(err) => Err(err),
    }
}

pub async fn merge_base_and_timestamp(
    cwd: Option<&str>,
    a: &str,
    b: &str,
) -> Result<(String, i64), ErrorStatus> {
    match merge_base(cwd, a, b).await {
        Err(err) => Err(err),
        Ok(hash) => match git(cwd, &["log", "--format=%ct", "-n1", &hash]).await {
            Ok(stdout) => match stdout.trim().parse::<i64>() {
                Ok(timestamp) => Ok((hash, timestamp)),
                Err(_) => Err(ErrorStatus::Errored(format!(
                    "Failed to convert timestamp {:?}",
                    stdout
                ))),
            },
            Err(err) => Err(err),
        },
    }
}

pub async fn files_changed_since(
    cwd: Option<&str>,
    hash: &str,
) -> Result<Vec<String>, ErrorStatus> {
    match git(cwd, &["diff", "-z", "--name-only", hash]).await {
        Ok(stdout) => Ok(split_null_terminated_lines(&stdout)
            .into_iter()
            .map(|s| s.to_string())
            .collect()),
        Err(err) => Err(err),
    }
}

pub async fn files_changed_since_mergebase_with(
    cwd: Option<&str>,
    commit: &str,
) -> Result<(String, Vec<String>), ErrorStatus> {
    match merge_base(cwd, commit, "HEAD").await {
        Err(err) => Err(err),
        Ok(mergebase) => match files_changed_since(cwd, &mergebase).await {
            Ok(changes) => Ok((mergebase, changes)),
            Err(err) => Err(err),
        },
    }
}

/// Files changed between `base` and `rev`, or the working tree when `rev` is `None`.
pub async fn files_changed_between(
    cwd: Option<&str>,
    base: &str,
    rev: Option<&str>,
) -> Result<Vec<(FileChange, String)>, ErrorStatus> {
    let mut args = vec!["diff", "-z", "--name-status", "--no-renames", base];
    if let Some(rev) = rev {
        args.push(rev);
    }
    let stdout = git(cwd, &args).await?;
    split_null_terminated_lines(&stdout)
        .chunks(2)
        .map(|entry| match entry {
            ["M" | "T" | "U", path] => Ok((FileChange::Modified, path.to_string())),
            ["A", path] => Ok((FileChange::Added, path.to_string())),
            ["D", path] => Ok((FileChange::Removed, path.to_string())),
            _ => Err(ErrorStatus::Errored(format!(
                "Malformed diff entry {:?}",
                entry
            ))),
        })
        .collect()
}

/// Contents of the files at the repository-relative `paths` in `rev`, as `(path, contents)`.
pub async fn cat_files(
    cwd: Option<&str>,
    rev: &str,
    paths: &[&str],
) -> Result<Vec<(String, String)>, ErrorStatus> {
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let contents = git(cwd, &["cat-file", "blob", &format!("{}:{}", rev, path)]).await?;
        files.push((path.to_string(), contents));
    }
    Ok(files)
}
