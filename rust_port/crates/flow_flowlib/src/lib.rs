/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use flow_common::sys_utils;

mod flowlib_contents;
mod flowlib_with_lib_dom_d_ts_contents;
mod prelude_contents;
mod tslib_contents;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinLib {
    Flowlib,
    FlowlibWithLibDomDts,
    Prelude,
    Tslib,
}

fn hash(lib: BuiltinLib) -> String {
    match lib {
        BuiltinLib::Flowlib => flowlib_contents::HASH.to_string(),
        BuiltinLib::FlowlibWithLibDomDts => flowlib_with_lib_dom_d_ts_contents::HASH.to_string(),
        BuiltinLib::Prelude => prelude_contents::HASH.to_string(),
        BuiltinLib::Tslib => tslib_contents::HASH.to_string(),
    }
}

fn contents(lib: BuiltinLib) -> &'static [(&'static str, &'static str)] {
    match lib {
        BuiltinLib::Flowlib => flowlib_contents::CONTENTS.as_slice(),
        BuiltinLib::FlowlibWithLibDomDts => flowlib_with_lib_dom_d_ts_contents::CONTENTS.as_slice(),
        BuiltinLib::Prelude => prelude_contents::CONTENTS,
        BuiltinLib::Tslib => tslib_contents::CONTENTS,
    }
}

pub fn contents_list(lib: BuiltinLib) -> Vec<(&'static str, &'static str)> {
    contents(lib).to_vec()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibDir {
    Flowlib(PathBuf),
    FlowlibWithLibDomDts(PathBuf),
    Prelude(PathBuf),
    Tslib(PathBuf),
}

#[cfg(unix)]
fn get_euid() -> u32 {
    unsafe { libc::geteuid() }
}

#[cfg(not(unix))]
fn get_euid() -> u32 {
    0
}

/// Returns the directory under `parent_dir` within which the flowlib files will
/// be extracted. This directory is named uniquely based on the flowlib contents, as well as the effective
/// user ID (euid) of the current process. The euid is used to ensure that
/// the directory is writable by the current user.
pub fn libdir(builtin_lib: BuiltinLib, parent_dir: &Path) -> LibDir {
    let euid = get_euid();
    let basename = format!("flowlib_{}_{}", hash(builtin_lib), euid);
    let path = parent_dir.join(&basename);
    match builtin_lib {
        BuiltinLib::Flowlib => LibDir::Flowlib(path),
        BuiltinLib::FlowlibWithLibDomDts => LibDir::FlowlibWithLibDomDts(path),
        BuiltinLib::Prelude => LibDir::Prelude(path),
        BuiltinLib::Tslib => LibDir::Tslib(path),
    }
}

pub fn path_of_libdir(libdir: &LibDir) -> &Path {
    match libdir {
        LibDir::Prelude(path) => path,
        LibDir::Flowlib(path) => path,
        LibDir::FlowlibWithLibDomDts(path) => path,
        LibDir::Tslib(path) => path,
    }
}

fn mkdir(libdir: &LibDir) {
    let path = path_of_libdir(libdir);
    let parent_dir = path.parent().expect("libdir path should have a parent");
    sys_utils::mkdir_no_fail(parent_dir)
        .unwrap_or_else(|e| panic!("mkdir_no_fail({:?}): {}", parent_dir, e));
    sys_utils::mkdir_no_fail(path).unwrap_or_else(|e| panic!("mkdir_no_fail({:?}): {}", path, e));
}

fn write_flowlib(dir: &Path, (filename, contents): &(&str, &str)) {
    let file = dir.join(filename);
    fs::write(&file, contents).expect("failed to write flowlib file");
}

pub fn extract(libdir: &LibDir) {
    mkdir(libdir);
    let (path, lib) = match libdir {
        LibDir::Prelude(path) => (path.as_path(), BuiltinLib::Prelude),
        LibDir::Flowlib(path) => (path.as_path(), BuiltinLib::Flowlib),
        LibDir::FlowlibWithLibDomDts(path) => (path.as_path(), BuiltinLib::FlowlibWithLibDomDts),
        LibDir::Tslib(path) => (path.as_path(), BuiltinLib::Tslib),
    };
    for entry in contents(lib) {
        write_flowlib(path, entry);
    }
}

pub fn extract_if_missing(libdir: &LibDir) {
    let sentinel_name = match libdir {
        LibDir::Flowlib(_) => "lib.es5.js",
        LibDir::FlowlibWithLibDomDts(_) => "lib.dom.d.ts",
        LibDir::Prelude(_) => "prelude.js",
        LibDir::Tslib(_) => "lib.d.ts",
    };
    let libdir_path = path_of_libdir(libdir);
    let sentinel = libdir_path.join(sentinel_name);
    if !sentinel.exists() {
        extract(libdir);
    }
}

fn libdir_from_files_libdir(files_libdir: &flow_common::files::LibDir) -> LibDir {
    match files_libdir {
        flow_common::files::LibDir::Prelude(path) => LibDir::Prelude(path.clone()),
        flow_common::files::LibDir::Flowlib(path) => LibDir::Flowlib(path.clone()),
        flow_common::files::LibDir::FlowlibWithLibDomDts(path) => {
            LibDir::FlowlibWithLibDomDts(path.clone())
        }
        flow_common::files::LibDir::Tslib(path) => LibDir::Tslib(path.clone()),
    }
}

pub fn extract_if_missing_or_exit(files_libdir_opt: Option<&flow_common::files::LibDir>) {
    let Some(files_libdir) = files_libdir_opt else {
        return;
    };
    let libdir = libdir_from_files_libdir(files_libdir);
    match std::panic::catch_unwind(|| extract_if_missing(&libdir)) {
        Ok(()) => {}
        Err(err) => {
            let panic_msg = err
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| err.downcast_ref::<String>().map(|s| s.as_str()))
                .unwrap_or("unknown error");
            let libdir_path = path_of_libdir(&libdir);
            eprintln!(
                "Could not extract flowlib files into {}: {}",
                libdir_path.display(),
                panic_msg
            );
            flow_common_exit_status::exit(
                flow_common_exit_status::FlowExitStatus::CouldNotExtractFlowlibs,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BuiltinLib;
    use super::contents;

    #[test]
    fn dom_extra_to_be_removed_is_only_in_flowlib() {
        let contains_dom_extra = |lib| {
            contents(lib)
                .iter()
                .any(|(filename, _)| *filename == "dom_extra_to_be_removed.js")
        };

        assert!(contains_dom_extra(BuiltinLib::Flowlib));
        assert!(
            [
                BuiltinLib::FlowlibWithLibDomDts,
                BuiltinLib::Prelude,
                BuiltinLib::Tslib,
            ]
            .into_iter()
            .all(|lib| !contains_dom_extra(lib))
        );
    }
}
