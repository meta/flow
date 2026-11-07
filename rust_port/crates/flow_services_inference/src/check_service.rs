/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::LazyCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::ALocTable;
use flow_aloc::LazyALocTable;
use flow_aloc::aloc_representation_do_not_use;
use flow_check_cache::CheckCache;
use flow_common::docblock::Docblock;
use flow_common::files;
use flow_common::flow_import_specifier::FlowImportSpecifier;
use flow_common::flow_import_specifier::Userland;
use flow_common::options::Options;
use flow_common::reason;
use flow_common::reason::VirtualReasonDesc::*;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_heap::parse::TypedParse;
use flow_heap::parsing_heaps::Transaction;
use flow_heap::resolved_requires::Dependency;
use flow_heap::resolved_requires::ResolvedModule;
use flow_parser::ast;
use flow_parser::dts_file_kind::DtsFileKind;
use flow_parser::file_key::FileKey;
use flow_parser::file_key::FileKeyInner;
use flow_parser::loc::LOC_NONE;
use flow_parser::loc::Loc;
use flow_parser_utils::file_sig::FileSig;
use flow_type_sig::compact_table::Index;
use flow_type_sig::packed_type_sig::Module as PackedTypeSigModule;
use flow_type_sig::type_sig_options::TypeSigOptions;
use flow_type_sig::type_sig_pack as Pack;
use flow_typing::merge::copy_into;
use flow_typing::merge::mk_builtins;
use flow_typing::type_inference;
use flow_typing_context::ArenaId;
use flow_typing_context::ComponentId;
use flow_typing_context::Context;
use flow_typing_context::ContextArena;
use flow_typing_context::ContextId;
use flow_typing_context::ContextStore;
use flow_typing_context::Cx;
use flow_typing_context::LazyType;
use flow_typing_context::LocalDefEntry;
use flow_typing_context::MasterContext;
use flow_typing_context::Metadata;
use flow_typing_context::MkBuiltins;
use flow_typing_context::ModuleTypeThunk;
use flow_typing_context::RemoteRefEntry;
use flow_typing_context::ResolveRequire;
use flow_typing_context::ResolvedRequire;
use flow_typing_context::SigFile;
use flow_typing_context::SigFileId;
use flow_typing_context::docblock_overrides;
use flow_typing_context::mk_context_metadata;
use flow_typing_type::type_;
use flow_typing_type::type_::ModuleType;
use flow_typing_type::type_::Type;
use flow_typing_utils::annotation_inference;
use flow_typing_utils::type_sig_merge;
use flow_typing_utils::type_sig_merge::Exports;

/// Makes the context of a file in the arena of the given store with the first id. Its dependencies
/// are made in the dependency arena with the second id, which the store has to contain as well.
pub type MakeCx = Box<
    dyn for<'cx> FnMut(
        ContextStore<'cx>,
        ArenaId,
        ArenaId,
        FileKey,
        Vec<(FlowImportSpecifier, ResolvedModule)>,
        Arc<ast::Program<Loc, Loc>>,
        Arc<Docblock>,
        LazyALocTable,
    ) -> Context<'cx>,
>;

pub struct CheckFileAndCompEnv {
    pub make_cx: MakeCx,
    pub check_file: Box<
        dyn FnMut(
            &mut Context<'_>,
            &FileKey,
            &TypeSigOptions,
            Arc<FileSig>,
            &Metadata,
            &[flow_parser::ast::Comment<Loc>],
            &ast::Program<Loc, Loc>,
            &ast::Program<ALoc, ALoc>,
            Option<Arc<PackedTypeSigModule<Loc>>>,
        ) -> Result<
            ast::Program<ALoc, (ALoc, Type)>,
            flow_utils_concurrency::job_error::JobError,
        >,
    >,
    pub compute_env: Box<
        dyn FnMut(
            &mut Context<'_>,
            &TypeSigOptions,
            &ast::Program<Loc, Loc>,
            &ast::Program<ALoc, ALoc>,
            &FileSig,
        ) -> Result<(), flow_utils_concurrency::job_error::JobError>,
    >,
}

fn typed_builtin_module_opt(
    cx: &mut Context<'_>,
    builtin_module_name: &Userland,
) -> Option<ModuleTypeThunk> {
    match cx.builtin_module_opt(builtin_module_name) {
        Some((reason, lazy_module)) => {
            let s = type_::constraint::forcing_state::ModuleTypeForcingState::of_lazy_module(
                reason,
                lazy_module,
            );
            let thunk = annotation_inference::force_module_type_thunk(cx.id(), s);
            Some(thunk)
        }
        None => None,
    }
}

fn unknown_module_t(cx: &mut Context<'_>, module_name: &FlowImportSpecifier) -> ResolvedRequire {
    match module_name {
        FlowImportSpecifier::Userland(user_land_module_name) => {
            match typed_builtin_module_opt(cx, user_land_module_name) {
                Some(typed) => ResolvedRequire::TypedModule(typed),
                None => ResolvedRequire::MissingModule,
            }
        }
    }
}

fn unchecked_module_t(
    cx: &mut Context<'_>,
    file_key: &FileKey,
    mref: &FlowImportSpecifier,
) -> ResolvedRequire {
    let loc = ALoc::of_loc(Loc {
        source: Some(file_key.dupe()),
        ..LOC_NONE
    });
    match mref {
        FlowImportSpecifier::Userland(user_land_module_name) => {
            match typed_builtin_module_opt(cx, user_land_module_name) {
                Some(typed) => ResolvedRequire::TypedModule(typed),
                None => ResolvedRequire::UncheckedModule(loc),
            }
        }
    }
}

/// The dependency arena `id` of the store of `cx`, the context of a file that imports dependencies.
fn dependency_arena<'s>(cx: &'s mut Context<'_>, id: ArenaId) -> &'s mut ContextArena {
    cx.store_mut()
        .arena_mut(id)
        .expect("a context that imports a dependency belongs to a store of the dependency arena")
}

fn get_lint_severities(
    metadata: &Metadata,
    options: &Options,
) -> flow_lint_settings::lint_settings::LintSettings<flow_lint_settings::severity::Severity> {
    let lint_severities = options.lint_severities.clone();
    let strict_mode = &options.strict_mode;
    flow_typing::merge::get_lint_severities(metadata, strict_mode, lint_severities)
}

/// This function is designed to be applied up to the unit argument and returns a
/// function which can be called repeatedly. The returned function closes over an
/// environment which defines caches that can be re-used when checking multiple
/// files.
///
/// The returned functions read the builtins of `cache`, so they can only be used with stores of
/// `cache`, until its builtins are cleared.
pub fn mk_check_file(
    transaction: Arc<Transaction>,
    options: Arc<Options>,
    all_unordered_libs: Arc<BTreeSet<FlowSmolStr>>,
    master_cx: &MasterContext,
    cache: &mut CheckCache,
) -> CheckFileAndCompEnv {
    let base_metadata = mk_context_metadata(&options, master_cx.global_libdefs());

    let mk_builtins_fn = cache.builtins(|arena| mk_builtins(&base_metadata, master_cx, arena));

    /// Create a type representing the exports of a dependency. For checked
    /// dependencies, we will create a "sig tvar" with a lazy thunk that evaluates
    /// to a ModuleT type.
    fn dep_module_t(
        cx: &mut Context<'_>,
        mref: &FlowImportSpecifier,
        resolved_module: Result<Dependency, Option<FlowImportSpecifier>>,
        transaction: &Arc<Transaction>,
        all_unordered_libs: &Arc<BTreeSet<FlowSmolStr>>,
        base_metadata: &Metadata,
        mk_builtins_fn: &MkBuiltins,
        dependencies: ArenaId,
    ) -> ResolvedRequire {
        match resolved_module {
            Err(mapped_name) => {
                let m = mapped_name.as_ref().unwrap_or(mref);
                unknown_module_t(cx, m)
            }
            Ok(m) => match transaction.get_provider(&m) {
                None => {
                    let modulename = m.to_modulename();
                    unknown_module_t(
                        cx,
                        &FlowImportSpecifier::userland(FlowSmolStr::from(modulename.as_str())),
                    )
                }
                // A global libdef (a `[libs]` file) resolves like any other file,
                // but it is not a module: report a dedicated "not a module" error
                // at the import site instead of merging it.
                Some(dep_file_key)
                    if files::is_lib_file(all_unordered_libs, &dep_file_key)
                        || transaction.get_dts_file_kind(&dep_file_key)
                            == Some(DtsFileKind::GlobalLibdef) =>
                {
                    ResolvedRequire::GlobalLibdefModule(dep_file_key)
                }
                Some(dep_file_key) => match dep_file_key.inner() {
                    FileKeyInner::ResourceFile(_) => {
                        let (reason, lazy_module) = type_sig_merge::merge_resource_module_t(
                            cx,
                            dep_file_key.dupe(),
                            &dep_file_key.to_absolute(),
                        );
                        let thunk = annotation_inference::force_module_type_thunk(
                            cx.id(),
                            type_::constraint::forcing_state::ModuleTypeForcingState::of_lazy_module(
                                reason,
                                lazy_module,
                            ),
                        );
                        ResolvedRequire::TypedModule(thunk)
                    }
                    _ => match transaction.get_typed_parse(&dep_file_key) {
                        Some(parse) => ResolvedRequire::TypedModule(sig_module_t(
                            &dep_file_key,
                            parse,
                            transaction,
                            all_unordered_libs,
                            base_metadata,
                            mk_builtins_fn,
                            dependencies,
                        )),
                        None => unchecked_module_t(cx, &dep_file_key, mref),
                    },
                },
            },
        }
    }

    fn sig_module_t(
        file_key: &FileKey,
        parse: TypedParse,
        transaction: &Arc<Transaction>,
        all_unordered_libs: &Arc<BTreeSet<FlowSmolStr>>,
        base_metadata: &Metadata,
        mk_builtins_fn: &MkBuiltins,
        dependencies: ArenaId,
    ) -> ModuleTypeThunk {
        let file_key = file_key.dupe();
        let transaction = transaction.dupe();
        let all_unordered_libs = all_unordered_libs.dupe();
        let base_metadata = base_metadata.clone();
        let mk_builtins_fn = mk_builtins_fn.dupe();
        let parse_for_leader = parse.dupe();
        let file_key_for_leader = file_key.dupe();
        // A dependency can be parsed into the transaction without being merged, in
        // which case it belongs to no component and leads its own — the same key merge
        // would have recorded for a singleton component.
        let leader: LazyCell<FileKey, Box<dyn FnOnce() -> FileKey>> =
            LazyCell::new(Box::new(move || {
                parse_for_leader
                    .leader()
                    .unwrap_or_else(|| file_key_for_leader.dupe())
            }));
        Rc::new(move |cx: &mut Context<'_>| {
            cx.add_reachable_dep(file_key.dupe());
            let (dep_cx, file) = match dependency_arena(cx, dependencies).dependency(&file_key) {
                Some(dependency) => dependency,
                None => {
                    let ccx = dependency_arena(cx, dependencies)
                        .dependency_component(LazyCell::force(&leader).dupe());
                    let dependency = dep_file(
                        cx,
                        dependencies,
                        ccx,
                        &file_key,
                        parse.dupe(),
                        &transaction,
                        &all_unordered_libs,
                        &base_metadata,
                        &mk_builtins_fn,
                    );
                    dependency_arena(cx, dependencies).add_dependency(file_key.dupe(), dependency);
                    dependency
                }
            };
            let exports = cx.sig_file(file).exports.dupe();
            copy_into(cx, dep_cx, exports.as_ref())
        })
    }

    /// Create a Type_sig_merge.file record for a dependency, which we use to
    /// convert signatures into types. This function reads the signature for a file
    /// from shared memory and creates thunks (either lazy tvars or lazy types)
    /// that resolve to types. The context of the dependency and its signature file are made in
    /// the dependency arena, which the store of `cx`, the importing context, contains.
    fn dep_file(
        cx: &mut Context<'_>,
        dependencies: ArenaId,
        ccx: ComponentId,
        file_key: &FileKey,
        parse: TypedParse,
        transaction: &Arc<Transaction>,
        all_unordered_libs: &Arc<BTreeSet<FlowSmolStr>>,
        base_metadata: &Metadata,
        mk_builtins_fn: &MkBuiltins,
    ) -> (ContextId, SigFileId) {
        let source = Some(file_key.dupe());

        let aloc_table = {
            let file_key = file_key.dupe();
            let packed_aloc_table = parse.aloc_table_unsafe(&file_key);
            Rc::new(LazyCell::new(Box::new(move || {
                Rc::new(ALocTable::unpack(file_key.dupe(), &packed_aloc_table))
            })
                as Box<dyn FnOnce() -> Rc<ALocTable>>))
        };

        let aloc = {
            let source = source.dupe();
            move |i: &Index<Loc>| -> ALoc {
                aloc_representation_do_not_use::make_keyed(source.dupe(), i.as_usize() as u32)
            }
        };
        let aloc = Rc::new(aloc);

        let type_sig = parse.type_sig_unsafe(file_key);

        let resolved_requires_data = parse.resolved_requires_unsafe();
        let requires = parse.requires();

        let resolved_requires: Rc<
            BTreeMap<FlowImportSpecifier, Rc<flow_lazy::Lazy<Cx, ResolvedRequire>>>,
        > = Rc::new(
            requires
                .iter()
                .zip(resolved_requires_data.get_resolved_modules().iter())
                .map(|(mref, m)| {
                    let key = mref.dupe();
                    let mref = mref.dupe();
                    let m = m.to_result();
                    let transaction = transaction.dupe();
                    let base_metadata = base_metadata.clone();
                    let mk_builtins_fn = mk_builtins_fn.dupe();
                    let all_unordered_libs = all_unordered_libs.dupe();
                    let resolved: Rc<flow_lazy::Lazy<Cx, ResolvedRequire>> =
                        Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                            dep_module_t(
                                cx,
                                &mref,
                                m,
                                &transaction,
                                &all_unordered_libs,
                                &base_metadata,
                                &mk_builtins_fn,
                                dependencies,
                            )
                        }));
                    (key, resolved)
                })
                .collect(),
        );

        let cx_id = {
            let docblock = parse.docblock_unsafe(file_key);
            let mut metadata = docblock_overrides(&docblock, file_key, base_metadata.clone());
            metadata.overridable.is_ts_global_augmentation =
                transaction.has_ts_global_augmentation(file_key);
            let resolved_requires = resolved_requires.dupe();
            let resolve_require: ResolveRequire =
                Rc::new(move |cx: &mut Context<'_>, mref: &FlowImportSpecifier| {
                    let lazy_val = resolved_requires.get(mref).unwrap_or_else(|| {
                        panic!(
                            "dep_file resolve_require: module reference not found: {:?}",
                            mref
                        )
                    });
                    lazy_val.get_forced(cx).dupe()
                });
            let budget = cx.budget();
            dependency_arena(cx, dependencies).add_context(
                ccx,
                metadata,
                file_key.dupe(),
                all_unordered_libs.dupe(),
                aloc_table,
                resolve_require,
                mk_builtins_fn.dupe(),
                budget,
            )
        };

        let dependencies_table = type_sig.module_refs.map(|mref| {
            (
                mref.dupe(),
                resolved_requires
                    .get(&FlowImportSpecifier::Userland(mref.dupe()))
                    .unwrap()
                    .dupe(),
            )
        });

        let file_loc = ALoc::of_loc(Loc {
            source: Some(file_key.dupe()),
            ..LOC_NONE
        });
        let reason = reason::mk_reason(RExports, file_loc);

        let file_id = dependency_arena(cx, dependencies).add_sig_file(|file_id| {
            let reason_for_forcing = reason.dupe();
            let aloc_for_exports = aloc.dupe();
            let module_kind = type_sig.module_kind.clone();
            let lazy_module: Rc<flow_lazy::Lazy<Cx, ModuleType>> =
                Rc::new(flow_lazy::Lazy::new(move |src_cx: &mut Context<'_>| {
                    let type_export =
                        |te: &Pack::TypeExport<Index<Loc>>| -> type_sig_merge::LazyExport {
                            let te = te.clone();
                            let reason = reason.dupe();
                            let aloc = aloc_for_exports.dupe();
                            Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                                let te = te.map(&|i| (*aloc)(i));
                                type_sig_merge::merge_type_export(
                                    cx,
                                    &cx.sig_file(file_id),
                                    reason,
                                    &te,
                                )
                            }))
                        };
                    let cjs_exports_fn =
                        |packed: &Pack::Packed<Index<Loc>>| -> type_sig_merge::LazyCjsExport {
                            let packed = packed.clone();
                            let aloc = aloc_for_exports.dupe();
                            Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                                let packed = packed.map(&|i| (*aloc)(i));
                                type_sig_merge::merge_cjs_export_t(
                                    cx,
                                    &cx.sig_file(file_id),
                                    &packed,
                                )
                            }))
                        };
                    let es_export =
                        |export: &Pack::Export<Index<Loc>>| -> type_sig_merge::LazyExport {
                            let export = export.clone();
                            let aloc = aloc_for_exports.dupe();
                            Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                                let export = export.map(&|i| (*aloc)(i));
                                type_sig_merge::merge_export(cx, &cx.sig_file(file_id), &export)
                            }))
                        };
                    let cjs_module = |type_exports_arr: &[Pack::TypeExport<Index<Loc>>],
                                      exports_opt: &Option<Pack::Packed<Index<Loc>>>,
                                      info: &Pack::CJSModuleInfo<Index<Loc>>|
                     -> Exports {
                        let Pack::CJSModuleInfo {
                            type_export_keys,
                            type_stars,
                            strict,
                            platform_availability_set,
                            strictness_kind,
                        } = &info.map(&|i| (*aloc_for_exports)(i));

                        assert_eq!(
                            type_export_keys.len(),
                            type_exports_arr.len(),
                            "type_export_keys and type_exports length mismatch"
                        );
                        let type_exports: BTreeMap<FlowSmolStr, type_sig_merge::LazyExport> =
                            type_export_keys
                                .iter()
                                .zip(type_exports_arr.iter())
                                .map(|(name, te)| (name.dupe(), type_export(te)))
                                .collect();

                        let exports: Option<type_sig_merge::LazyCjsExport> =
                            exports_opt.as_ref().map(cjs_exports_fn);

                        Exports::CJSExports {
                            type_exports,
                            exports,
                            type_stars: type_stars.clone(),
                            strict: *strict,
                            platform_availability_set: platform_availability_set.clone(),
                            strictness_kind: *strictness_kind,
                        }
                    };
                    let es_module = |type_exports_arr: &[Pack::TypeExport<Index<Loc>>],
                                     exports_arr: &[Pack::Export<Index<Loc>>],
                                     ts_pending_arr: &[Pack::TsPendingExport<Index<Loc>>],
                                     info: &Pack::ESModuleInfo<Index<Loc>>|
                     -> Exports {
                        let Pack::ESModuleInfo {
                            type_export_keys,
                            type_stars,
                            export_keys,
                            stars,
                            ts_pending_keys,
                            strict,
                            platform_availability_set,
                            strictness_kind,
                        } = &info.map(&|i| (*aloc_for_exports)(i));

                        assert_eq!(
                            type_export_keys.len(),
                            type_exports_arr.len(),
                            "type_export_keys and type_exports length mismatch"
                        );
                        let type_exports: BTreeMap<FlowSmolStr, type_sig_merge::LazyExport> =
                            type_export_keys
                                .iter()
                                .zip(type_exports_arr.iter())
                                .map(|(name, te)| (name.dupe(), type_export(te)))
                                .collect();

                        assert_eq!(
                            export_keys.len(),
                            exports_arr.len(),
                            "export_keys and exports length mismatch"
                        );
                        let exports: BTreeMap<FlowSmolStr, type_sig_merge::LazyExport> =
                            export_keys
                                .iter()
                                .zip(exports_arr.iter())
                                .map(|(name, export)| (name.dupe(), es_export(export)))
                                .collect();

                        assert_eq!(
                            ts_pending_keys.len(),
                            ts_pending_arr.len(),
                            "ts_pending_keys and ts_pending_arr length mismatch"
                        );
                        let ts_pending: Vec<(
                            FlowSmolStr,
                            type_sig_merge::LazyTsPendingClassified,
                        )> = ts_pending_keys
                            .iter()
                            .zip(ts_pending_arr.iter())
                            .map(|(name, pending)| {
                                let pending = pending.clone();
                                let aloc = aloc_for_exports.dupe();
                                let strictness_kind = *strictness_kind;
                                let classified: type_sig_merge::LazyTsPendingClassified =
                                    Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                                        let pending = pending.map(&|i| (*aloc)(i));
                                        type_sig_merge::classify_ts_pending_export(
                                            cx,
                                            &cx.sig_file(file_id),
                                            &pending,
                                            strictness_kind,
                                        )
                                    }));
                                (name.dupe(), classified)
                            })
                            .collect();

                        Exports::ESExports {
                            type_exports,
                            exports,
                            ts_pending,
                            type_stars: type_stars.clone(),
                            stars: stars.clone(),
                            strict: *strict,
                            platform_availability_set: platform_availability_set.clone(),
                        }
                    };
                    let exports_info = match &module_kind {
                        Pack::ModuleKind::CJSModule {
                            type_exports,
                            exports,
                            info,
                        } => cjs_module(type_exports, exports, info),
                        Pack::ModuleKind::ESModule {
                            type_exports,
                            exports,
                            ts_pending,
                            info,
                        } => es_module(type_exports, exports, ts_pending, info),
                    };
                    let module_type_lazy = type_sig_merge::merge_exports(
                        src_cx,
                        &src_cx.sig_file(file_id),
                        reason,
                        exports_info,
                    );
                    module_type_lazy.get_forced(src_cx).dupe()
                }));
            let s = type_::constraint::forcing_state::ModuleTypeForcingState::of_lazy_module(
                reason_for_forcing,
                lazy_module,
            );
            let exports = annotation_inference::force_module_type_thunk(cx_id, s);

            let local_def = |def: &Pack::PackedDef<Index<Loc>>| {
                let aloc = aloc.dupe();
                let def = def.clone();
                let entry: Rc<flow_lazy::Lazy<Cx, LocalDefEntry>> =
                    Rc::new(flow_lazy::Lazy::new(move |_cx: &mut Context<'_>| {
                        let def = Rc::new(def.map(
                            &mut (),
                            |_, loc: &Index<Loc>| (*aloc)(loc),
                            |_, t: &Pack::Packed<Index<Loc>>| t.map(&|i| (*aloc)(i)),
                        ));
                        let loc = def.id_loc();
                        let name = def.name().dupe();
                        let binding_kind = type_sig_merge::def_binding_kind(&def);
                        let reason = type_sig_merge::def_reason(&def);
                        let type_ = move |const_decl: bool,
                                          cx: &mut Context<'_>,
                                          reason: reason::Reason,
                                          def: Rc<Pack::PackedDef<ALoc>>|
                              -> Type {
                            let reason_for_tvar = reason.dupe();
                            let resolved: LazyType =
                                Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                                    type_sig_merge::merge_def(
                                        cx,
                                        &cx.sig_file(file_id),
                                        reason,
                                        &def,
                                        const_decl,
                                    )
                                }));
                            annotation_inference::mk_sig_tvar(cx, reason_for_tvar, resolved)
                        };
                        let reason2 = reason.dupe();
                        let def2 = def.dupe();
                        let general: LazyType =
                            Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                                type_(false, cx, reason, def)
                            }));
                        let const_: LazyType =
                            Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                                type_(true, cx, reason2, def2)
                            }));
                        (loc, name, binding_kind, general, const_)
                    }));
                entry
            };

            let remote_ref = |rref: &Pack::RemoteRef<Index<Loc>>| {
                let aloc = aloc.dupe();
                let rref = rref.clone();
                let entry: Rc<flow_lazy::Lazy<Cx, RemoteRefEntry>> =
                    Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                        let remote_ref = rref.map(&|i| (*aloc)(i));
                        let loc = remote_ref.loc().dupe();
                        let name = remote_ref.name().dupe();
                        let reason = type_sig_merge::remote_ref_reason(&remote_ref);
                        let reason2 = reason.dupe();
                        let remote_ref2 = remote_ref.clone();
                        let resolved: LazyType =
                            Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                                type_sig_merge::merge_remote_ref(
                                    cx,
                                    &cx.sig_file(file_id),
                                    reason2,
                                    &remote_ref2,
                                )
                            }));
                        let t = annotation_inference::mk_sig_tvar(cx, reason.dupe(), resolved);
                        let reason2 = reason.dupe();
                        let remote_ref2 = remote_ref.clone();
                        let resolved_for_extends: LazyType =
                            Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                                type_sig_merge::merge_remote_ref_for_extends(
                                    cx,
                                    &cx.sig_file(file_id),
                                    reason2,
                                    &remote_ref2,
                                )
                            }));
                        let t_for_extends: LazyType =
                            Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                                annotation_inference::mk_sig_tvar(cx, reason, resolved_for_extends)
                            }));
                        (loc, name, t, t_for_extends)
                    }));
                entry
            };

            let pattern_def = |def: &Pack::Packed<Index<Loc>>| {
                let aloc = aloc.dupe();
                let def = def.clone();
                let t: LazyType = Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                    let def = def.map(&|i| (*aloc)(i));
                    type_sig_merge::merge(FlowOrdMap::new(), cx, &cx.sig_file(file_id), &def)
                }));
                t
            };

            let pattern = |p: &Pack::Pattern<Index<Loc>>| {
                let aloc = aloc.dupe();
                let p = p.clone();
                let t: LazyType = Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                    let p = p.map(&|i| (*aloc)(i));
                    type_sig_merge::merge_pattern(cx, &cx.sig_file(file_id), &p)
                }));
                t
            };

            SigFile {
                dependencies: dependencies_table,
                exports,
                local_defs: type_sig.local_defs.map(local_def),
                remote_refs: type_sig.remote_refs.map(remote_ref),
                pattern_defs: type_sig.pattern_defs.map(pattern_def),
                patterns: type_sig.patterns.map(pattern),
            }
        });
        (cx_id, file_id)
    }

    let options = options.dupe();

    let make_cx: MakeCx = {
        let transaction = transaction.dupe();
        let base_metadata = base_metadata.clone();
        let mk_builtins_fn = mk_builtins_fn.dupe();
        let options = options.dupe();
        Box::new(
            move |mut store,
                  own,
                  dependencies,
                  file_key,
                  resolved_modules: Vec<(FlowImportSpecifier, ResolvedModule)>,
                  _ast,
                  docblock: Arc<Docblock>,
                  aloc_table| {
                let arena = store
                    .arena_mut(own)
                    .expect("the store has the arena the context is made in");
                let ccx = arena.add_component();
                let mut metadata = docblock_overrides(&docblock, &file_key, base_metadata.clone());
                metadata.overridable.is_ts_global_augmentation =
                    transaction.has_ts_global_augmentation(&file_key);
                // The requires are resolved in the context, so it resolves none until it exists.
                let unresolved_require: ResolveRequire =
                    Rc::new(|_cx: &mut Context<'_>, mref: &FlowImportSpecifier| {
                        panic!("resolve_require: requires are not resolved yet: {:?}", mref)
                    });
                let budget = flow_utils_concurrency::check_budget::CheckBudget::new(
                    options
                        .merge_timeout
                        .map(std::time::Duration::from_secs_f64),
                );
                let cx_id = arena.add_context(
                    ccx,
                    metadata.clone(),
                    file_key.dupe(),
                    all_unordered_libs.dupe(),
                    aloc_table,
                    unresolved_require,
                    mk_builtins_fn.dupe(),
                    budget,
                );
                let mut cx = Context::new(store, cx_id);
                let resolved_requires: HashMap<FlowImportSpecifier, ResolvedRequire> =
                    resolved_modules
                        .iter()
                        .map(|(mref, m)| {
                            let r = dep_module_t(
                                &mut cx,
                                mref,
                                m.to_result(),
                                &transaction,
                                &all_unordered_libs,
                                &base_metadata,
                                &mk_builtins_fn,
                                dependencies,
                            );
                            (mref.dupe(), r)
                        })
                        .collect();
                cx.set_resolve_require(Rc::new(
                    move |_cx: &mut Context<'_>, mref: &FlowImportSpecifier| {
                        resolved_requires
                            .get(mref)
                            .unwrap_or_else(|| {
                                panic!("resolve_require: module reference not found: {:?}", mref)
                            })
                            .dupe()
                    },
                ));
                cx
            },
        )
    };

    let check_file = {
        let options = options.dupe();
        Box::new(
            move |cx: &mut Context<'_>,
                  file_key: &FileKey,
                  type_sig_options: &TypeSigOptions,
                  file_sig: Arc<FileSig>,
                  metadata: &Metadata,
                  comments: &[flow_parser::ast::Comment<Loc>],
                  ast: &ast::Program<Loc, Loc>,
                  aloc_ast: &ast::Program<ALoc, ALoc>,
                  current_type_sig: Option<Arc<PackedTypeSigModule<Loc>>>| {
                // Dependency contexts this check switches to, including ones cached
                // from earlier files, report annotation inference errors here.
                let id = cx.id();
                cx.set_annot_error_dst(id);
                let lint_severities = get_lint_severities(metadata, &options);
                type_inference::infer_file(
                    &lint_severities,
                    cx,
                    file_key,
                    file_sig,
                    metadata,
                    comments,
                    ast,
                    aloc_ast,
                    type_sig_options,
                    current_type_sig,
                )
            },
        )
    };

    let compute_env = Box::new(
        move |cx: &mut Context<'_>,
              type_sig_options: &TypeSigOptions,
              ast: &ast::Program<Loc, Loc>,
              aloc_ast: &ast::Program<ALoc, ALoc>,
              file_sig: &FileSig| {
            let id = cx.id();
            cx.set_annot_error_dst(id);
            type_inference::initialize_env(cx, type_sig_options, None, ast, aloc_ast, file_sig)
        },
    );

    CheckFileAndCompEnv {
        make_cx,
        check_file,
        compute_env,
    }
}
