/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ffi::OsStr;
use std::path::Path;

use dupe::IterDupedExt;
use flow_common::files;
use flow_common::options::Options;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_heap::parsing_heaps::Transaction;
use flow_parser::file_key::FileKey;
use flow_parser::file_key::FileKeyInner;
use flow_utils_concurrency::map_reduce;
use flow_utils_concurrency::thread_pool::ThreadPool;

pub(crate) fn is_package_json(file: &FileKey) -> bool {
    matches!(file.inner(), FileKeyInner::JsonFile(_))
        && Path::new(file.as_str()).file_name() == Some(OsStr::new("package.json"))
}

/// Returns the package.json paths that files recorded reading, that are not heap files, and that
/// now lead through a symlink to a package.json whose package changed in this recheck: a link
/// created since those files were resolved. Call it after the recheck has parsed `updates`. Also
/// records the node_modules directories on those paths.
pub(crate) fn package_json_paths_through_links(
    pool: &ThreadPool,
    transaction: &Transaction,
    options: &Options,
    updates: &FlowOrdSet<FileKey>,
) -> Vec<FileKey> {
    let access = transaction.latest_heap_reader();
    let reader = access.reader();
    let changed_packages: Vec<String> = updates
        .iter()
        .filter(|file| {
            is_package_json(file) && {
                let package = reader.get_package_info(file);
                package.is_some() && package != reader.get_package_info_committed(file)
            }
        })
        .map(FileKey::to_absolute)
        .collect();
    if changed_packages.is_empty() {
        return Vec::new();
    }

    let candidates: Vec<FileKey> = reader
        .committed_file_dependencies()
        .filter(|file| is_package_json(file) && reader.get_parse(file).is_none())
        .duped()
        .collect();
    let chunk_size = 1.max(candidates.len() / pool.num_workers() + 1);
    let chunks: Vec<Vec<FileKey>> = candidates
        .chunks(chunk_size)
        .map(<[FileKey]>::to_vec)
        .collect();
    let found: Vec<FileKey> = map_reduce::fold(
        pool,
        chunks,
        |acc: &mut Vec<FileKey>, chunk| {
            // Most recorded paths do not exist: a `stat` rules them out before canonicalizing,
            // which costs one system call per component.
            acc.extend(
                chunk
                    .iter()
                    .filter(|file| {
                        let path = file.to_absolute();
                        std::fs::metadata(&path).is_ok()
                            && files::cached_canonicalize(Path::new(&path)).is_ok_and(|real| {
                                changed_packages
                                    .iter()
                                    .any(|package| Path::new(package) == real)
                            })
                    })
                    .duped(),
            )
        },
        |acc, other| acc.extend(other),
    );

    files::add_node_modules_containers(
        &options.file_options,
        &options.root,
        found.iter().map(FileKey::to_absolute),
    );
    found
}
