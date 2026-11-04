/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use serde::Deserialize;

use crate::vcs::FileChange;
use crate::vcs_utils::Env;
pub use crate::vcs_utils::ErrorStatus;
use crate::vcs_utils::exec;
use crate::vcs_utils::split_null_terminated_lines;

async fn hg(cwd: Option<&str>, args: &[&str]) -> Result<String, ErrorStatus> {
    let env = Env::Extend(vec![
        ("NOSCMLOG".to_string(), "1".to_string()),
        ("HGPLAIN".to_string(), "1".to_string()),
    ]);
    exec(Some(env), cwd, "hg", args).await
}

pub async fn merge_base(cwd: Option<&str>, a: &str, b: &str) -> Result<String, ErrorStatus> {
    let revset = format!("ancestor({},{})", a, b);
    match hg(cwd, &["log", "-T", "{node}", "-r", &revset]).await {
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
    let revset = format!("ancestor({},{})", a, b);
    match hg(cwd, &["log", "-T", "{node} {date}", "-r", &revset]).await {
        Ok(stdout) => {
            let parts: Vec<&str> = stdout.split(' ').collect();
            match parts.as_slice() {
                [hash, timestamp] => match timestamp.parse::<f64>() {
                    Ok(ts) => Ok((hash.to_string(), ts as i64)),
                    Err(_) => Err(ErrorStatus::Errored("Malformed log response".to_string())),
                },
                _ => Err(ErrorStatus::Errored("Malformed log response".to_string())),
            }
        }
        Err(err) => Err(err),
    }
}

pub async fn files_changed_since(
    cwd: Option<&str>,
    hash: &str,
) -> Result<Vec<String>, ErrorStatus> {
    match hg(cwd, &["status", "--print0", "-n", "--rev", hash]).await {
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
    match merge_base(cwd, ".", commit).await {
        Err(err) => Err(err),
        Ok(mergebase) => match files_changed_since(cwd, &mergebase).await {
            Ok(changes) => Ok((mergebase, changes)),
            Err(err) => Err(err),
        },
    }
}

/// Files changed between `base` and `rev`, or the working copy when `rev` is `None`.
pub async fn files_changed_between(
    cwd: Option<&str>,
    base: &str,
    rev: Option<&str>,
) -> Result<Vec<(FileChange, String)>, ErrorStatus> {
    let mut args = vec![
        "status",
        "--print0",
        "--modified",
        "--added",
        "--removed",
        "--deleted",
        "--rev",
        base,
    ];
    if let Some(rev) = rev {
        args.extend(["--rev", rev]);
    }
    let stdout = hg(cwd, &args).await?;
    split_null_terminated_lines(&stdout)
        .into_iter()
        .map(|line| match line.split_once(' ') {
            Some(("M", path)) => Ok((FileChange::Modified, path.to_string())),
            Some(("A", path)) => Ok((FileChange::Added, path.to_string())),
            Some(("R" | "!", path)) => Ok((FileChange::Removed, path.to_string())),
            _ => Err(ErrorStatus::Errored(format!(
                "Malformed status line {:?}",
                line
            ))),
        })
        .collect()
}

#[derive(Deserialize)]
struct CatEntry {
    abspath: String,
    data: String,
}

/// Contents of the files at the repository-relative `paths` in `rev`, as `(path, contents)`.
pub async fn cat_files(
    cwd: Option<&str>,
    rev: &str,
    paths: &[&str],
) -> Result<Vec<(String, String)>, ErrorStatus> {
    let patterns: Vec<String> = paths.iter().map(|path| format!("path:{}", path)).collect();
    let mut args = vec!["cat", "--rev", rev, "--template", "json"];
    args.extend(patterns.iter().map(String::as_str));
    let stdout = hg(cwd, &args).await?;
    let entries: Vec<CatEntry> = serde_json::from_str(&stdout)
        .map_err(|err| ErrorStatus::Errored(format!("Malformed cat output: {}", err)))?;
    Ok(entries
        .into_iter()
        .map(|entry| (entry.abspath, entry.data))
        .collect())
}
