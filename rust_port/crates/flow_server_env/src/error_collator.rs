/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::options::Options;
use flow_common_errors::error_codes::ErrorCode;
use flow_common_errors::error_utils::ConcreteLocPrintableErrorSet;
use flow_common_errors::error_utils::PrintableError;
use flow_common_errors::error_utils::code_of_printable_error;
use flow_common_errors::error_utils::lsp_output;
use flow_common_utils::checked_set::CheckedSet;
use flow_data_structure_wrapper::overlay_map::OverlayMap;
use flow_data_structure_wrapper::overlay_map::OverlayMapBase;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast::Program;
use flow_parser::file_key::FileKey;
use flow_parser::file_key::FileKeyInner;
use flow_parser::loc::Loc;
use flow_parser::loc::Position;
use flow_typing_errors::error_message::EDuplicateModuleProviderData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_suppressions::ErrorSuppressions;
use flow_typing_errors::flow_error::ErrorSet;
use flow_typing_errors::flow_error::error_of_msg;
use flow_typing_errors::intermediate_error::make_intermediate_error;
use flow_typing_errors::intermediate_error::to_printable_error;
use flow_typing_errors::intermediate_error_types::IntermediateError;
use murmur3::murmur3_32;

use crate::collated_errors::CollatedErrors;
use crate::collated_errors::ErrorStateTimestamp;
use crate::collated_errors::ErrorStateTimestamps;
use crate::collated_errors::OverlayCollatedErrors;
use crate::monitor_rpc;
use crate::server_env::Env;
use crate::server_env::EnvCellReadGuard;
use crate::server_env::Errors;
use crate::server_env::OverlayErrors;
use crate::server_status;

fn add_suppression_warnings(
    root: &Path,
    checked: &CheckedSet,
    unused: &ErrorSuppressions,
    warnings: &mut OverlayMap<FileKey, ConcreteLocPrintableErrorSet>,
) {
    // For each unused suppression, create an warning
    let all_locs = unused.all_unused_locs();
    for loc in &all_locs {
        let source_file = match &loc.source {
            Some(x) => x.dupe(),
            None => FileKey::new(FileKeyInner::SourceFile("-".to_string())),
        };
        // In lazy mode, dependencies are modules which we typecheck not because we care about
        // them, but because something important (a focused file or a focused file's dependent)
        // needs these dependencies. Therefore, we might not typecheck a dependency's dependents.
        //
        // This means there might be an unused suppression comment warning in a dependency which
        // only shows up in lazy mode. To avoid this, we'll just avoid raising this kind of
        // warning in any dependency.
        if !checked.mem_dependency(&source_file) {
            let msg = ErrorMessage::EUnusedSuppression(loc.dupe());
            let flow_error = error_of_msg(source_file.dupe(), msg);
            let intermediate =
                make_intermediate_error(Loc::dupe, false, &flow_error, FileKey::is_lib_file);
            let err = to_printable_error(
                Loc::dupe,
                |_: &FileKey| -> Option<Arc<Program<Loc, Loc>>> { None },
                Some(root),
                intermediate,
            );
            let mut file_warnings = match warnings.get(&source_file) {
                Some(existing) => existing.clone(),
                None => ConcreteLocPrintableErrorSet::empty(),
            };
            file_warnings.add(err);
            warnings.insert(source_file, file_warnings);
        }
    }
}

fn collate_duplicate_providers<'a, I>(
    root: &Path,
    duplicate_providers: I,
) -> Vec<(PrintableError<Loc>, FileKey, FileKey)>
where
    I: IntoIterator<Item = (&'a FlowSmolStr, &'a (FileKey, vec1::Vec1<FileKey>))>,
{
    let pos = Position { line: 1, column: 0 };
    let mut acc = Vec::new();
    for (module_name, (provider_file, duplicates)) in duplicate_providers {
        let provider = Loc {
            source: Some(provider_file.dupe()),
            start: pos,
            end: pos,
        };
        for duplicate in duplicates {
            let conflict = Loc {
                source: Some(duplicate.dupe()),
                start: pos,
                end: pos,
            };
            let msg =
                ErrorMessage::EDuplicateModuleProvider(Box::new(EDuplicateModuleProviderData {
                    module_name: module_name.dupe(),
                    provider: provider.dupe(),
                    conflict,
                }));
            let flow_error = error_of_msg(duplicate.dupe(), msg);
            let intermediate =
                make_intermediate_error(Loc::dupe, false, &flow_error, FileKey::is_lib_file);
            let err = to_printable_error(
                Loc::dupe,
                |_: &FileKey| -> Option<Arc<Program<Loc, Loc>>> { None },
                Some(root),
                intermediate,
            );
            acc.push((err, provider_file.dupe(), duplicate.dupe()));
        }
    }
    acc
}

pub fn update_local_collated_errors<F, G, H>(
    loc_of_aloc: &F,
    get_ast: &G,
    is_lib: &H,
    root: &Path,
    file_options: &flow_common::files::FileOptions,
    node_modules_errors: bool,
    unsuppressable_error_codes: &BTreeSet<FlowSmolStr>,
    suppressions: &ErrorSuppressions,
    errors: &BTreeMap<FileKey, ErrorSet>,
    acc: &mut OverlayCollatedErrors,
) where
    F: Fn(&ALoc) -> Loc,
    G: Fn(&FileKey) -> Option<Arc<Program<Loc, Loc>>>,
    H: Fn(&FileKey) -> bool + Clone,
{
    for (filename, file_errs) in errors {
        let file_options = Some(file_options);
        // It is okay to ignore suppressions here, since local errors, ie. parse
        // errors, parse exceptions and docblock errors, are insuppressible.
        let mut unused = ErrorSuppressions::empty();
        let (file_errs, _file_suppressed) = suppressions.filter_suppressed_errors(
            root,
            file_options,
            node_modules_errors,
            unsuppressable_error_codes,
            loc_of_aloc,
            get_ast,
            is_lib,
            file_errs,
            &mut unused,
        );
        acc.collated_local_errors.insert(filename.dupe(), file_errs);
    }
}

// Note on suppressions: The suppressions within the Server.errors parameter
// only account for the files that were most recently checked. Due to bugs in the
// checker it is possible for errors that are included in that same set to appear
// in files other than the ones in checked_files. To effectively suppress these
// we need to pass in an additional `all_suppressions` parameter. This is the
// suppression set we pass over to Error_suppressions.filter_suppressed_errors.
// Finally, we compute unused suppression warnings over the set of suppressions
// that we found during checking (not all_suppressions).
#[allow(clippy::too_many_arguments)]
fn acc_fun<F, G, H, ErrorBase, SuppressedBase>(
    loc_of_aloc: &F,
    get_ast: &G,
    is_lib: &H,
    options: &Options,
    all_suppressions: &ErrorSuppressions,
    unsuppressable_error_codes: &BTreeSet<FlowSmolStr>,
    filename: &FileKey,
    file_errs: &ErrorSet,
    errors: &mut OverlayMap<FileKey, ConcreteLocPrintableErrorSet, ErrorBase>,
    suppressed: &mut OverlayMap<
        FileKey,
        Vec<(IntermediateError<ALoc>, BTreeSet<Loc>)>,
        SuppressedBase,
    >,
    unused: &mut ErrorSuppressions,
) where
    F: Fn(&ALoc) -> Loc,
    G: Fn(&FileKey) -> Option<Arc<Program<Loc, Loc>>>,
    H: Fn(&FileKey) -> bool + Clone,
    ErrorBase: OverlayMapBase<FileKey, ConcreteLocPrintableErrorSet>,
    SuppressedBase: OverlayMapBase<FileKey, Vec<(IntermediateError<ALoc>, BTreeSet<Loc>)>>,
{
    let root = &*options.root;
    let file_options = Some(&*options.file_options);
    let (file_errs, file_suppressed) = all_suppressions.filter_suppressed_errors(
        root,
        file_options,
        options.node_modules_errors,
        unsuppressable_error_codes,
        loc_of_aloc,
        get_ast,
        is_lib,
        file_errs,
        unused,
    );
    errors.insert(filename.dupe(), file_errs);
    suppressed.insert(filename.dupe(), file_suppressed);
}

#[allow(clippy::too_many_arguments)]
fn update_collated_errors_impl<
    'a,
    F,
    G,
    H,
    LocalErrors,
    DuplicateProviders,
    MergeErrors,
    Warnings,
>(
    loc_of_aloc: &F,
    get_ast: &G,
    is_lib: &H,
    options: &Options,
    checked_files: &CheckedSet,
    all_suppressions: &ErrorSuppressions,
    local_errors: LocalErrors,
    duplicate_providers: DuplicateProviders,
    merge_errors: MergeErrors,
    warnings: Warnings,
    suppressions: &ErrorSuppressions,
    acc: &mut OverlayCollatedErrors,
) where
    F: Fn(&ALoc) -> Loc,
    G: Fn(&FileKey) -> Option<Arc<Program<Loc, Loc>>>,
    H: Fn(&FileKey) -> bool + Clone,
    LocalErrors: IntoIterator<Item = (&'a FileKey, &'a ErrorSet)>,
    DuplicateProviders: IntoIterator<Item = (&'a FlowSmolStr, &'a (FileKey, vec1::Vec1<FileKey>))>,
    MergeErrors: IntoIterator<Item = (&'a FileKey, &'a ErrorSet)>,
    Warnings: IntoIterator<Item = (&'a FileKey, &'a ErrorSet)>,
{
    let root = &*options.root;
    let unsuppressable_error_codes: BTreeSet<FlowSmolStr> =
        options.unsuppressable_error_codes.iter().cloned().collect();

    monitor_rpc::status_update(server_status::Event::CollatingErrorsStart);

    // `duplicate_providers` is the whole map, so it replaces the collated errors instead of adding
    // to them: an error stays only while its module still has that provider and duplicate.
    acc.collated_duplicate_providers_errors =
        collate_duplicate_providers(root, duplicate_providers);

    let mut unused = (*suppressions).clone();
    for (filename, file_errs) in local_errors {
        acc_fun(
            loc_of_aloc,
            get_ast,
            is_lib,
            options,
            all_suppressions,
            &unsuppressable_error_codes,
            filename,
            file_errs,
            &mut acc.collated_local_errors,
            &mut acc.collated_suppressed_errors,
            &mut unused,
        );
    }

    for (filename, file_errs) in merge_errors {
        acc_fun(
            loc_of_aloc,
            get_ast,
            is_lib,
            options,
            all_suppressions,
            &unsuppressable_error_codes,
            filename,
            file_errs,
            &mut acc.collated_merge_errors,
            &mut acc.collated_suppressed_errors,
            &mut unused,
        );
    }

    let mut warning_results: OverlayMap<FileKey, ConcreteLocPrintableErrorSet> = OverlayMap::new();
    for (filename, file_errs) in warnings {
        acc_fun(
            loc_of_aloc,
            get_ast,
            is_lib,
            options,
            all_suppressions,
            &unsuppressable_error_codes,
            filename,
            file_errs,
            &mut warning_results,
            &mut acc.collated_suppressed_errors,
            &mut unused,
        );
    }

    // Compute "unused suppression warnings" based on the suppression set that
    // emerged from the current checked set, not the entire set of suppressions.
    add_suppression_warnings(root, checked_files, &unused, &mut warning_results);
    for (file, errs) in warning_results.into_delta_values() {
        let merged = match acc.collated_warning_map.get(&file) {
            Some(existing) => {
                let mut merged = existing.clone();
                merged.union(&errs);
                merged
            }
            None => errs,
        };
        acc.collated_warning_map.insert(file, merged);
    }
}

pub fn update_collated_errors<F, G, H>(
    loc_of_aloc: &F,
    get_ast: &G,
    is_lib: &H,
    options: &Options,
    checked_files: &CheckedSet,
    all_suppressions: &ErrorSuppressions,
    errors: &Errors,
    acc: &mut OverlayCollatedErrors,
) where
    F: Fn(&ALoc) -> Loc,
    G: Fn(&FileKey) -> Option<Arc<Program<Loc, Loc>>>,
    H: Fn(&FileKey) -> bool + Clone,
{
    let Errors {
        local_errors,
        duplicate_providers,
        merge_errors,
        warnings,
        suppressions,
    } = errors;
    update_collated_errors_impl(
        loc_of_aloc,
        get_ast,
        is_lib,
        options,
        checked_files,
        all_suppressions,
        local_errors,
        duplicate_providers,
        merge_errors,
        warnings,
        suppressions,
        acc,
    );
}

pub fn update_collated_overlay_errors<F, G, H>(
    loc_of_aloc: &F,
    get_ast: &G,
    is_lib: &H,
    options: &Options,
    checked_files: &CheckedSet,
    all_suppressions: &ErrorSuppressions,
    errors: &OverlayErrors,
    acc: &mut OverlayCollatedErrors,
) where
    F: Fn(&ALoc) -> Loc,
    G: Fn(&FileKey) -> Option<Arc<Program<Loc, Loc>>>,
    H: Fn(&FileKey) -> bool + Clone,
{
    let OverlayErrors {
        local_errors,
        duplicate_providers,
        merge_errors,
        warnings,
        suppressions,
        ..
    } = errors;
    update_collated_errors_impl(
        loc_of_aloc,
        get_ast,
        is_lib,
        options,
        checked_files,
        all_suppressions,
        local_errors,
        duplicate_providers,
        merge_errors,
        warnings,
        suppressions,
        acc,
    );
}

fn get_with_separate_warnings_internal(
    env: &Env,
) -> (
    ConcreteLocPrintableErrorSet,
    EnvCellReadGuard<CollatedErrors>,
) {
    let collated_errors = env.collated_errors();
    let mut collated_errorset = ConcreteLocPrintableErrorSet::empty();
    for (err, _, _) in &collated_errors.collated_duplicate_providers_errors {
        collated_errorset.add(err.clone());
    }
    for errs in collated_errors.collated_local_errors.values() {
        collated_errorset.union(errs);
    }
    for errs in collated_errors.collated_merge_errors.values() {
        collated_errorset.union(errs);
    }
    (collated_errorset, collated_errors)
}

fn type_error_stat(collated_errors: &OverlayCollatedErrors) -> (bool, bool, bool, bool) {
    let collated_merge_errors = &collated_errors.collated_merge_errors;
    let (
        have_type_errors,
        all_type_errors_in_one_file,
        have_subtyping_errors,
        all_subtyping_errors_in_one_file,
    ) = collated_merge_errors.values().fold(
        (false, true, false, true),
        |(
            have_type_errors_before,
            all_errors_in_one_file,
            have_subtyping_errors_before,
            all_subtyping_errors_in_one_file,
        ),
         errors| {
            let have_type_errors_in_file = !errors.is_empty();
            let all_type_errors_in_one_file =
                all_errors_in_one_file && !(have_type_errors_before && have_type_errors_in_file);
            let have_subtyping_errors_in_file = errors.exists(|error| {
                matches!(
                    code_of_printable_error(error),
                    Some(
                        ErrorCode::IncompatibleExact
                            | ErrorCode::IncompatibleFunctionIndexer
                            | ErrorCode::IncompatibleIndexer
                            | ErrorCode::IncompatibleType
                            | ErrorCode::IncompatibleTypeGuard
                            | ErrorCode::IncompatibleUse
                            | ErrorCode::IncompatibleVariance
                            | ErrorCode::PropMissing
                    )
                )
            });
            let all_subtyping_errors_in_one_file = all_subtyping_errors_in_one_file
                && !(have_subtyping_errors_before && have_subtyping_errors_in_file);
            (
                have_type_errors_before || have_type_errors_in_file,
                all_type_errors_in_one_file,
                have_subtyping_errors_before || have_subtyping_errors_in_file,
                all_subtyping_errors_in_one_file,
            )
        },
    );
    let have_type_errors_and_all_in_one_file = have_type_errors && all_type_errors_in_one_file;
    let have_subtyping_errors_and_all_in_one_file =
        have_subtyping_errors && all_subtyping_errors_in_one_file;
    (
        have_type_errors,
        have_type_errors_and_all_in_one_file,
        have_subtyping_errors,
        have_subtyping_errors_and_all_in_one_file,
    )
}

pub struct PerErrorInfo {
    pub error_code: String,
    pub line_agnostic_hash: String,
    pub error_message: Option<String>,
}

pub struct PerFileErrors<'a> {
    pub file_path: &'a str,
    pub errors: Vec<PerErrorInfo>,
}

pub fn compute_per_file_errors(
    with_context_limit: usize,
    collated_errors: &OverlayCollatedErrors,
) -> Vec<PerFileErrors<'_>> {
    let mut context_count = 0;
    fn process_errors<'a>(
        with_context_limit: usize,
        context_count: &mut usize,
        filename: &'a FileKey,
        errors: &ConcreteLocPrintableErrorSet,
        mut acc: Vec<PerFileErrors<'a>>,
    ) -> Vec<PerFileErrors<'a>> {
        if errors.is_empty() {
            acc
        } else {
            let file_path = match filename.inner() {
                FileKeyInner::SourceFile(s)
                | FileKeyInner::JsonFile(s)
                | FileKeyInner::ResourceFile(s)
                | FileKeyInner::LibFile(s) => s.as_str(),
            };
            let error_infos = errors.iter().fold(Vec::new(), |mut acc, error| {
                let lsp = lsp_output::lsp_of_error(false, error);
                let code_str = lsp.code;
                let hash_input = format!("{}:{}", code_str, lsp.message);
                let hash = format!(
                    "{:x}",
                    murmur3_32(&mut hash_input.as_bytes(), 0)
                        .expect("reading from a byte slice should not fail")
                        & 0x3fff_ffff
                );
                let error_message = if *context_count < with_context_limit {
                    *context_count += 1;
                    let msg = lsp.message;
                    let msg = if msg.len() > 400 {
                        let mut end = 400;
                        while !msg.is_char_boundary(end) {
                            end -= 1;
                        }
                        format!("{}...", &msg[..end])
                    } else {
                        msg
                    };
                    Some(msg)
                } else {
                    None
                };
                acc.push(PerErrorInfo {
                    error_code: code_str,
                    line_agnostic_hash: hash,
                    error_message,
                });
                acc
            });
            acc.push(PerFileErrors {
                file_path,
                errors: error_infos,
            });
            acc
        }
    }
    let acc =
        collated_errors
            .collated_merge_errors
            .iter()
            .fold(Vec::new(), |acc, (filename, errors)| {
                process_errors(
                    with_context_limit,
                    &mut context_count,
                    filename,
                    errors,
                    acc,
                )
            });
    collated_errors
        .collated_local_errors
        .iter()
        .fold(acc, |acc, (filename, errors)| {
            process_errors(
                with_context_limit,
                &mut context_count,
                filename,
                errors,
                acc,
            )
        })
}

pub struct ErrorResolutionStat {
    pub time_to_resolve_all_type_errors: Option<f64>,
    pub time_to_resolve_all_type_errors_in_one_file: Option<f64>,
    pub time_to_resolve_all_subtyping_errors: Option<f64>,
    pub time_to_resolve_all_subtyping_errors_in_one_file: Option<f64>,
    pub rounds_to_resolve_all_type_errors: Option<u32>,
    pub rounds_to_resolve_all_type_errors_in_one_file: Option<u32>,
    pub rounds_to_resolve_all_subtyping_errors: Option<u32>,
    pub rounds_to_resolve_all_subtyping_errors_in_one_file: Option<u32>,
}

pub fn update_error_state_timestamps(
    collated_errors: &mut OverlayCollatedErrors,
) -> ErrorResolutionStat {
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let init_timestamps = collated_errors.error_state_timestamps;
    let current = ErrorStateTimestamp {
        time: current_time,
        round: init_timestamps.round + 1,
    };
    let (
        have_type_errors,
        have_type_errors_and_all_in_one_file,
        have_subtyping_errors,
        have_subtyping_errors_and_all_in_one_file,
    ) = type_error_stat(collated_errors);
    let timestamp_start_of_non_zero_type_errors = if have_type_errors {
        init_timestamps
            .timestamp_start_of_non_zero_type_errors
            .or(Some(current))
    } else {
        None
    };
    let timestamp_start_of_non_zero_type_errors_all_in_one_file =
        if have_type_errors_and_all_in_one_file {
            init_timestamps
                .timestamp_start_of_non_zero_type_errors_all_in_one_file
                .or(Some(current))
        } else {
            None
        };
    let timestamp_start_of_non_zero_subtyping_errors = if have_subtyping_errors {
        init_timestamps
            .timestamp_start_of_non_zero_subtyping_errors
            .or(Some(current))
    } else {
        None
    };
    let timestamp_start_of_non_zero_subtyping_errors_all_in_one_file =
        if have_subtyping_errors_and_all_in_one_file {
            init_timestamps
                .timestamp_start_of_non_zero_subtyping_errors_all_in_one_file
                .or(Some(current))
        } else {
            None
        };
    let time_to_resolve_errors = |init_error_time: Option<ErrorStateTimestamp>,
                                  current_error_time: Option<ErrorStateTimestamp>|
     -> Option<f64> {
        match (init_error_time, current_error_time) {
            // If we start with no errors, then we are not resolving any errors.
            (None, _) => None,
            // If we end with errors, then we are not fully resolving all errors.
            (_, Some(_)) => None,
            (Some(t_start), None) => Some(current.time - t_start.time),
        }
    };
    let rounds_to_resolve_errors = |init_error_time: Option<ErrorStateTimestamp>,
                                    current_error_time: Option<ErrorStateTimestamp>|
     -> Option<u32> {
        match (init_error_time, current_error_time) {
            (None, _) => None,
            (_, Some(_)) => None,
            (Some(t_start), None) => Some(current.round - t_start.round),
        }
    };
    let time_to_resolve_all_type_errors = time_to_resolve_errors(
        init_timestamps.timestamp_start_of_non_zero_type_errors,
        timestamp_start_of_non_zero_type_errors,
    );
    let time_to_resolve_all_type_errors_in_one_file = time_to_resolve_errors(
        init_timestamps.timestamp_start_of_non_zero_type_errors_all_in_one_file,
        timestamp_start_of_non_zero_type_errors,
    );
    let time_to_resolve_all_subtyping_errors = time_to_resolve_errors(
        init_timestamps.timestamp_start_of_non_zero_subtyping_errors,
        timestamp_start_of_non_zero_subtyping_errors,
    );
    let time_to_resolve_all_subtyping_errors_in_one_file = time_to_resolve_errors(
        init_timestamps.timestamp_start_of_non_zero_subtyping_errors_all_in_one_file,
        timestamp_start_of_non_zero_subtyping_errors,
    );
    let rounds_to_resolve_all_type_errors = rounds_to_resolve_errors(
        init_timestamps.timestamp_start_of_non_zero_type_errors,
        timestamp_start_of_non_zero_type_errors,
    );
    let rounds_to_resolve_all_type_errors_in_one_file = rounds_to_resolve_errors(
        init_timestamps.timestamp_start_of_non_zero_type_errors_all_in_one_file,
        timestamp_start_of_non_zero_type_errors,
    );
    let rounds_to_resolve_all_subtyping_errors = rounds_to_resolve_errors(
        init_timestamps.timestamp_start_of_non_zero_subtyping_errors,
        timestamp_start_of_non_zero_subtyping_errors,
    );
    let rounds_to_resolve_all_subtyping_errors_in_one_file = rounds_to_resolve_errors(
        init_timestamps.timestamp_start_of_non_zero_subtyping_errors_all_in_one_file,
        timestamp_start_of_non_zero_subtyping_errors,
    );
    collated_errors.error_state_timestamps = ErrorStateTimestamps {
        round: current.round,
        timestamp_start_of_non_zero_type_errors,
        timestamp_start_of_non_zero_type_errors_all_in_one_file,
        timestamp_start_of_non_zero_subtyping_errors,
        timestamp_start_of_non_zero_subtyping_errors_all_in_one_file,
    };
    ErrorResolutionStat {
        time_to_resolve_all_type_errors,
        time_to_resolve_all_type_errors_in_one_file,
        time_to_resolve_all_subtyping_errors,
        time_to_resolve_all_subtyping_errors_in_one_file,
        rounds_to_resolve_all_type_errors,
        rounds_to_resolve_all_type_errors_in_one_file,
        rounds_to_resolve_all_subtyping_errors,
        rounds_to_resolve_all_subtyping_errors_in_one_file,
    }
}

pub fn get_without_suppressed(
    env: &Env,
) -> (ConcreteLocPrintableErrorSet, ConcreteLocPrintableErrorSet) {
    let (errors, collated_errors) = get_with_separate_warnings_internal(env);
    let mut warnings = ConcreteLocPrintableErrorSet::empty();
    for errs in collated_errors.collated_warning_map.values() {
        warnings.union(errs);
    }
    (errors, warnings)
}

pub fn get(
    env: &Env,
) -> (
    ConcreteLocPrintableErrorSet,
    ConcreteLocPrintableErrorSet,
    Vec<(IntermediateError<ALoc>, BTreeSet<Loc>)>,
) {
    let (errors, collated_errors) = get_with_separate_warnings_internal(env);
    let mut warnings = ConcreteLocPrintableErrorSet::empty();
    for errs in collated_errors.collated_warning_map.values() {
        warnings.union(errs);
    }
    let mut all_suppressed = Vec::new();
    for errs in collated_errors.collated_suppressed_errors.values() {
        all_suppressed.extend(errs.iter().cloned());
    }
    (errors, warnings, all_suppressed)
}

/// Returns the collated error set plus the warning map, with the read lock scoped to this
/// function so the returned value outlives no guard.
pub fn get_with_separate_warnings(
    env: &Env,
) -> (
    ConcreteLocPrintableErrorSet,
    BTreeMap<FileKey, ConcreteLocPrintableErrorSet>,
) {
    let (errors, collated_errors) = get_with_separate_warnings_internal(env);
    (errors, collated_errors.collated_warning_map.clone())
}
