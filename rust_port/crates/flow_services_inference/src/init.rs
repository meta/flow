/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! This module sets up the definitions for JavaScript globals. Eventually, this
//! module should become redundant: we should be able to automatically interpret
//! TypeScript type definition files for these and many other primitives. That
//! said, in some cases handcoding may turn out to be necessary because the type
//! system is not powerful enough to encode the invariants of a library
//! function. In any case, this part of the design must be revisited in the
//! future.

use std::cell::LazyCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::ALocTable;
use flow_common::files;
use flow_common::files::FileOptions;
use flow_common::flow_import_specifier::FlowImportSpecifier;
use flow_common::options::Options;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_heap::parsing_heaps::Transaction;
use flow_imports_exports::exports;
use flow_imports_exports::exports::Exports;
use flow_parser::ast;
use flow_parser::file_key::FileKey;
use flow_parser::loc::Loc;
use flow_type_sig::type_sig_options::TypeSigOptions;
use flow_typing::merge;
use flow_typing::type_inference;
use flow_typing_builtins::builtins::Builtins;
use flow_typing_context::Context;
use flow_typing_context::ContextArena;
use flow_typing_context::ContextStore;
use flow_typing_context::MasterContext;
use flow_typing_context::ResolvedRequire;
use flow_typing_context::mk_context_metadata;
use flow_typing_errors::error_suppressions::ErrorSuppressions;
use flow_typing_errors::flow_error::ErrorSet;
use flow_typing_errors::flow_error::FlowError;
use flow_typing_flow_common::flow_js_utils::add_output_non_speculating;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OrderedLibInput {
    pub(crate) file: FileKey,
}

impl OrderedLibInput {
    pub(crate) fn configured(path: &str) -> Self {
        Self {
            file: flow_common::files::lib_file_key(path),
        }
    }

    fn discovered(file: FileKey) -> Self {
        Self { file }
    }
}

pub(crate) fn assemble_ordered_lib_inputs(
    file_options: &FileOptions,
    configured_libs: &[String],
    discovered_globals: &BTreeSet<FileKey>,
    global_augmentations: &FlowOrdSet<FileKey>,
) -> Vec<OrderedLibInput> {
    // Flow's builtins come first, the `.d.ts` ones too, so that `[libs]` override all of them.
    let (builtin_libs, configured_libs): (Vec<&String>, Vec<&String>) = configured_libs
        .iter()
        .partition(|path| files::is_in_flowlib(file_options, path));
    // `FileKey` stores a root-relative, `/`-separated path suffix, so its `Ord` is already the
    // canonical cross-platform order the global scope needs; a `BTreeSet` iterates in it.
    let (builtin_globals, discovered_globals): (Vec<&FileKey>, Vec<&FileKey>) = discovered_globals
        .iter()
        .partition(|file| files::is_in_flowlib(file_options, &file.to_absolute()));
    let discovered_global = |file: &FileKey| {
        assert!(
            !file.is_lib_file(),
            "configured LibFile cannot be a parse-discovered global"
        );
        OrderedLibInput::discovered(file.dupe())
    };
    builtin_libs
        .into_iter()
        .map(|path| OrderedLibInput::configured(path))
        .chain(builtin_globals.into_iter().map(discovered_global))
        .chain(
            configured_libs
                .into_iter()
                .map(|path| OrderedLibInput::configured(path)),
        )
        .chain(discovered_globals.into_iter().map(discovered_global))
        .chain(
            global_augmentations
                .iter()
                .map(|file| OrderedLibInput::discovered(file.dupe())),
        )
        .collect()
}

/// process all lib files: parse, infer, and add the symbols they define
/// to the builtins object.
///
/// Note: we support overrides of definitions found earlier in the list of
/// files by those of the same name found in later ones, so caller must
/// preserve lib path declaration order in the (flattened) list of files
/// passed.
///
/// returns (success, parse and signature errors, exports)
fn load_lib_files<'cx>(
    arena: &'cx mut ContextArena,
    options: &Options,
    reader: &Transaction,
    all_unordered_libs: Arc<BTreeSet<FlowSmolStr>>,
    files: &[OrderedLibInput],
) -> (bool, MasterContext, Option<Context<'cx>>, Exports) {
    let mut ok = true;
    let mut ordered_asts: Vec<(
        flow_common::type_strictness::TypeStrictnessKind,
        Arc<ast::Program<Loc, Loc>>,
    )> = Vec::new();
    let mut global_augmentation_asts = Vec::new();
    for OrderedLibInput { file } in files {
        match reader.get_ast(file) {
            Some(ast) => {
                let ast = (
                    flow_common::type_strictness::TypeStrictnessKind::from_is_typescript(
                        flow_common::files::has_ts_ext(file),
                    ),
                    ast,
                );
                if reader.has_ts_global_augmentation(file) {
                    let file_sig = reader.get_file_sig_unsafe(file);
                    global_augmentation_asts.push((ast.0, ast.1, file_sig));
                } else {
                    ordered_asts.push(ast);
                }
            }
            None => {
                flow_hh_logger::info!("Failed to find {:?} in parsing heap.", file);
                ok = false;
            }
        }
    }
    ordered_asts.reverse();

    let (builtin_exports, master_cx, cx_opt) = if ok {
        let sig_opts = TypeSigOptions::builtin_options(options);
        let (builtin_errors, master_cx) = merge::merge_lib_files(
            &sig_opts,
            all_unordered_libs,
            &ordered_asts,
            &global_augmentation_asts,
        );
        match master_cx {
            MasterContext::EmptyMasterContext => {
                (Exports::empty(), MasterContext::EmptyMasterContext, None)
            }
            MasterContext::NonEmptyMasterContext {
                ref builtin_leader_file_key,
                ref all_unordered_libs,
                ref global_libdefs,
                ref builtins,
            } => {
                let mut metadata = mk_context_metadata(options, global_libdefs.dupe());
                metadata.overridable.checked = false;
                let ccx = arena.add_component();
                let cx = arena.add_context(
                    ccx,
                    metadata,
                    builtin_leader_file_key.dupe(),
                    all_unordered_libs.dupe(),
                    {
                        let builtin_leader_file_key = builtin_leader_file_key.dupe();
                        Rc::new(LazyCell::new(Box::new(move || {
                            Rc::new(ALocTable::empty(builtin_leader_file_key))
                        })
                            as Box<dyn FnOnce() -> Rc<ALocTable>>))
                    },
                    Rc::new(|_cx: &mut Context<'_>, _: &FlowImportSpecifier| {
                        ResolvedRequire::MissingModule
                    }),
                    Rc::new(|_cx: &mut Context<'_>| Builtins::empty()),
                    flow_utils_concurrency::check_budget::CheckBudget::new(
                        options
                            .merge_timeout
                            .map(std::time::Duration::from_secs_f64),
                    ),
                );
                let mut cx = Context::new(ContextStore::new(vec![arena]), cx);
                let file_keys_with_comments: Vec<(FileKey, &[flow_parser::ast::Comment<Loc>])> =
                    ordered_asts
                        .iter()
                        .map(|(_, ast)| ast)
                        .chain(global_augmentation_asts.iter().map(|(_, ast, _)| ast))
                        .map(|ast| {
                            let loc = &ast.loc;
                            let source = loc
                                .source
                                .as_ref()
                                .expect("libdef ast missing source")
                                .dupe();
                            let comments: &[flow_parser::ast::Comment<Loc>] = &ast.all_comments;
                            (source, comments)
                        })
                        .collect();
                let (severity_cover, suppressions, suppression_errors) =
                    type_inference::scan_for_suppressions(
                        true,
                        &options.lint_severities,
                        file_keys_with_comments,
                    );
                cx.reset_errors(builtin_errors);
                cx.add_severity_covers(severity_cover);
                cx.add_error_suppressions(suppressions);
                for error in suppression_errors {
                    add_output_non_speculating(&mut cx, error);
                }
                (
                    exports::of_builtins(&builtins.builtins),
                    master_cx,
                    Some(cx),
                )
            }
        }
    } else {
        (Exports::empty(), MasterContext::EmptyMasterContext, None)
    };
    (ok, master_cx, cx_opt, builtin_exports)
}

pub struct InitResult {
    pub ok: bool,
    pub errors: BTreeMap<FileKey, ErrorSet>,
    pub warnings: BTreeMap<FileKey, ErrorSet>,
    pub suppressions: ErrorSuppressions,
    pub exports: Exports,
    pub master_cx: Arc<MasterContext>,
}

fn error_set_to_filemap(map: &mut BTreeMap<FileKey, ErrorSet>, err_set: ErrorSet) {
    err_set.fold((), |(), error: FlowError<ALoc>| {
        let file = error.source_file().dupe();
        match map.get(&file) {
            None => {
                map.insert(file, ErrorSet::singleton(error));
            }
            Some(set) => {
                let mut new_set = set.clone();
                new_set.add(error);
                map.insert(file, new_set);
            }
        }
    });
}

/// initialize builtins:
/// parse and do local inference on library files, and set up master context.
/// returns list of (lib file, success) pairs.
pub(crate) fn init(
    options: &Options,
    reader: &Transaction,
    all_unordered_libs: Arc<BTreeSet<FlowSmolStr>>,
    lib_files: Vec<OrderedLibInput>,
) -> InitResult {
    let mut arena = ContextArena::new();
    let (ok, master_cx, cx_opt, exports) =
        load_lib_files(&mut arena, options, reader, all_unordered_libs, &lib_files);
    let (errors, warnings, suppressions) = match cx_opt {
        None => (BTreeMap::new(), BTreeMap::new(), ErrorSuppressions::empty()),
        Some(cx) => {
            let errors = cx.errors();
            let suppressions = cx.error_suppressions().clone();
            let severity_cover = cx.severity_cover().dupe();
            let include_suppressions = cx.include_suppressions();
            let aloc_tables = cx.aloc_tables();
            let mut suppressions = suppressions;
            let (errors, warnings) = suppressions.filter_lints(
                errors,
                aloc_tables,
                include_suppressions,
                &severity_cover,
            );
            let mut error_map = BTreeMap::new();
            let mut warning_map = BTreeMap::new();
            for file in aloc_tables.keys() {
                error_map.insert(file.dupe(), ErrorSet::empty());
                warning_map.insert(file.dupe(), ErrorSet::empty());
            }
            error_set_to_filemap(&mut error_map, errors);
            error_set_to_filemap(&mut warning_map, warnings);
            (error_map, warning_map, suppressions)
        }
    };
    InitResult {
        ok,
        errors,
        warnings,
        suppressions,
        exports,
        master_cx: Arc::new(master_cx),
    }
}
