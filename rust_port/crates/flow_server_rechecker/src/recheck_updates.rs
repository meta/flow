/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeSet;
use std::path::Path;

use dupe::Dupe;
use flow_common::files;
use flow_common::files::FileOptions;
use flow_common::flow_version;
use flow_common::options::Options;
use flow_common_exit_status::FlowExitStatus;
use flow_common_semver::semver;
use flow_config::FlowConfig;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_heap::parsing_heaps::Transaction;
use flow_parser::file_key::FileKey;
use flow_parsing::parsing_service;
use flow_server_files::server_files_js;
use flow_services_module::PackageIncompatibleReturn;

pub enum Error {
    RecoverableShouldReinitNonLazily {
        msg: String,
        updates: FlowOrdSet<FileKey>,
        changed_declaration_files: FlowOrdSet<FileKey>,
    },
    Unrecoverable {
        msg: String,
        exit_status: FlowExitStatus,
    },
}

// WARNING! Be careful when adding new incompatibilities to this function. While dfind will
// return any file which changes within the watched directories, watchman only watches for
// specific extensions and files. Make sure to update the watchman_expression_terms in our
// watchman file watcher!
fn is_incompatible_package_json(
    options: &Options,
    transaction: &Transaction,
    want: &dyn Fn(&str) -> bool,
    sroot: &str,
    file_options: &FileOptions,
    f: &str,
) -> PackageIncompatibleReturn {
    let is_incompatible = |filename_str: &str| -> PackageIncompatibleReturn {
        let filename = FileKey::json_file_of_absolute(filename_str);
        match std::fs::read_to_string(filename_str).ok() {
            None => PackageIncompatibleReturn::Incompatible(
                flow_services_module::PackageIncompatibleReason::Unknown,
            ),
            Some(content) => {
                let result = parsing_service::parse_package_json_file(
                    options,
                    Ok(content.as_str()),
                    &filename,
                )
                .map_err(|_| ());
                let old_package = transaction
                    .get_package_info(&filename)
                    .map(|pkg| Ok((*pkg).clone()));
                flow_services_module::package_incompatible(&filename, old_package, result)
            }
        }
    };
    if (f.starts_with(sroot) || files::is_included(file_options, f))
        && Path::new(f)
            .file_name()
            .is_some_and(|n| n == "package.json")
        && want(f)
    {
        is_incompatible(f)
    } else {
        PackageIncompatibleReturn::Compatible
    }
}

fn get_updated_flowconfig(config_path: &str) -> Result<(FlowConfig, String), Error> {
    match flow_config::get(config_path) {
        Ok((config, _warnings, hash)) => Ok((config, hash)),
        Err(_) => Err(Error::Unrecoverable {
            msg: format!(
                "Failed to parse updated .flowconfig at {}; server must restart",
                config_path
            ),
            exit_status: FlowExitStatus::FlowconfigChanged,
        }),
    }
}

fn assert_compatible_flowconfig_version(config: &FlowConfig) -> Result<(), Error> {
    let not_satisfied = |version_constraint: &str| -> bool {
        match semver::satisfies(Some(true), version_constraint, flow_version::version()) {
            Ok(result) => !result,
            Err(_) => true,
        }
    };
    match config.version.as_deref() {
        Some(version_constraint) if not_satisfied(version_constraint) => {
            let msg = format!(
                "Wrong version of Flow. The config specifies version {} but this is version {}",
                version_constraint,
                flow_version::version(),
            );
            Err(Error::Unrecoverable {
                msg,
                exit_status: FlowExitStatus::FlowconfigChanged,
            })
        }
        _ => Ok(()),
    }
}

fn assert_compatible_flowconfig_change(options: &Options, config_path: &str) -> Result<(), Error> {
    let old_hash = &options.flowconfig_hash;
    let (new_config, new_hash) = get_updated_flowconfig(config_path)?;
    if old_hash.as_str() == new_hash.as_str() {
        Ok(())
    } else {
        let msg = format!(
            ".flowconfig changed (hash {} -> {}); server must restart",
            old_hash, new_hash
        );
        assert_compatible_flowconfig_version(&new_config)?;
        Err(Error::Unrecoverable {
            msg,
            exit_status: FlowExitStatus::FlowconfigChanged,
        })
    }
}

fn check_for_flowconfig_change(
    options: &Options,
    skip_incompatible: bool,
    config_path: &str,
    updates: &BTreeSet<String>,
) -> Result<(), Error> {
    let config_changed = !skip_incompatible && updates.contains(config_path);
    if !config_changed {
        Ok(())
    } else {
        assert_compatible_flowconfig_change(options, config_path)
    }
}

fn check_for_package_json_changes(
    is_incompatible_package_json: &dyn Fn(&str) -> PackageIncompatibleReturn,
    skip_incompatible: bool,
    updates: &BTreeSet<String>,
) -> Result<(), Error> {
    let incompatible_packages: Vec<(&String, flow_services_module::PackageIncompatibleReason)> =
        updates
            .iter()
            .filter_map(|file| match is_incompatible_package_json(file) {
                PackageIncompatibleReturn::Compatible => None,
                PackageIncompatibleReturn::Incompatible(reason) => Some((file, reason)),
            })
            .collect();
    if !skip_incompatible && !incompatible_packages.is_empty() {
        let messages: String = incompatible_packages
            .iter()
            .rev()
            .map(|(file, reason)| format!("Modified package: {} ({})", file, reason))
            .collect::<Vec<_>>()
            .join("\n");
        Err(Error::Unrecoverable {
            msg: format!("{}\nPackages changed in an incompatible way", messages),
            exit_status: FlowExitStatus::FlowconfigChanged,
        })
    } else {
        Ok(())
    }
}

fn check_for_lib_changes(
    options: &Options,
    transaction: &Transaction,
    root: &Path,
    skip_incompatible: bool,
    filtered_updates: &FlowOrdSet<FileKey>,
    updates: &BTreeSet<String>,
) -> Result<(), Error> {
    if skip_incompatible {
        return Ok(());
    }

    let flow_typed_path = files::get_flowtyped_path(root)
        .to_string_lossy()
        .to_string();
    let is_changed_lib = |filename: &String| -> bool {
        let is_lib = files::is_configured_lib_file(&options.file_options, filename)
            || *filename == flow_typed_path;
        is_lib && parsing_service::did_content_change(transaction, &files::lib_file_key(filename))
    };
    let libs: BTreeSet<String> = updates
        .iter()
        .filter(|f| is_changed_lib(f))
        .cloned()
        .collect();
    let changed_declaration_files =
        parsing_service::changed_declaration_files(options, transaction, filtered_updates.iter());
    if !libs.is_empty() || !changed_declaration_files.is_empty() {
        let messages = libs
            .iter()
            .rev()
            .map(|file| format!("Modified lib file: {file}"))
            .chain(
                changed_declaration_files
                    .iter()
                    .map(|file| format!("Modified TypeScript declaration file: {}", file.as_str())),
            )
            .collect::<Vec<_>>()
            .join("\n");
        Err(Error::RecoverableShouldReinitNonLazily {
            msg: format!("{}\nLib files changed in an incompatible way", messages),
            updates: filtered_updates.dupe(),
            changed_declaration_files,
        })
    } else {
        Ok(())
    }
}

// Note: is_included may be expensive. Check the in-root match first.
// At this point, we already checked whether libdef files are touched.
// If so, we already decided that the server should restart, and this
// function won't be called.
fn filter_wanted_updates(
    file_options: &FileOptions,
    sroot: &str,
    want: &dyn Fn(&str) -> bool,
    updates: &BTreeSet<String>,
) -> FlowOrdSet<FileKey> {
    let mut acc = FlowOrdSet::new();
    for f in updates {
        if files::is_flow_file(file_options, f)
            && ((file_options.implicitly_include_root && f.starts_with(sroot))
                || files::is_included(file_options, f))
            && want(f)
        {
            let filename = files::filename_from_string(file_options, f);
            acc.insert(filename);
        }
    }
    acc
}

pub fn process_updates(
    skip_incompatible: bool,
    options: &Options,
    transaction: &Transaction,
    updates: &BTreeSet<String>,
) -> Result<FlowOrdSet<FileKey>, Error> {
    let file_options = &options.file_options;
    let root = &*options.root;
    let config_path = server_files_js::config_file(&options.flowconfig_name, root);
    let sroot = format!("{}{}", root.to_string_lossy(), std::path::MAIN_SEPARATOR);
    let want = |f: &str| -> bool { files::wanted(file_options, false, f) };
    let do_filter = |updates: &BTreeSet<String>| -> FlowOrdSet<FileKey> {
        filter_wanted_updates(file_options, &sroot, &want, updates)
    };
    let is_incompatible_pj = |f: &str| -> PackageIncompatibleReturn {
        is_incompatible_package_json(options, transaction, &want, &sroot, file_options, f)
    };
    check_for_flowconfig_change(options, skip_incompatible, &config_path, updates)?;
    if !options.incremental_package_json {
        check_for_package_json_changes(&is_incompatible_pj, skip_incompatible, updates)?;
    }
    let filtered_updates = do_filter(updates);
    check_for_lib_changes(
        options,
        transaction,
        root,
        skip_incompatible,
        &filtered_updates,
        updates,
    )?;
    Ok(filtered_updates)
}
