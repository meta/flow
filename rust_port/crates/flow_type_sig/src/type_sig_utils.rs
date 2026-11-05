/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::rc::Rc;

use dupe::Dupe;
use flow_common::packed_locs;
use flow_common::platform_set::PlatformSet;
use flow_common::type_strictness::TypeStrictnessKind;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast::Program;
use flow_parser::file_key::FileKey;
use flow_parser::loc::LOC_NONE;
use flow_parser::loc::Loc;
use flow_parser_utils::file_sig::FileSig;

use crate::compact_table::Index;
use crate::compact_table::Table;
use crate::packed_type_sig::Builtins;
use crate::packed_type_sig::Module;
use crate::packed_type_sig::ModuleDef;
use crate::packed_type_sig::TargetedModule;
use crate::packed_type_sig::TargetedRoot;
use crate::signature_error::BindingValidation;
use crate::type_sig::Errno;
use crate::type_sig_mark as mark;
use crate::type_sig_options::TypeSigOptions;
use crate::type_sig_pack as pack;
use crate::type_sig_parse as parse;

type BuiltinModules<'arena, 'ast> = BTreeMap<
    FlowSmolStr,
    (
        parse::LocNode<'ast>,
        Rc<RefCell<parse::Exports<'arena, 'ast>>>,
    ),
>;

fn parse_libs<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    arenas: &'arena bumpalo::Bump,
    ordered_asts: &[(&'ast Program<Loc, Loc>, TypeStrictnessKind)],
    global_augmentation_asts: &[(&'ast Program<Loc, Loc>, TypeStrictnessKind, &FileSig)],
) -> (
    parse::scope::Scopes<'arena, 'ast>,
    parse::Tables<'arena, 'ast>,
    (
        BTreeMap<FlowSmolStr, parse::BindingNode<'arena, 'ast>>,
        BTreeMap<FlowSmolStr, parse::BindingNode<'arena, 'ast>>,
        BuiltinModules<'arena, 'ast>,
        BuiltinModules<'arena, 'ast>,
    ),
) {
    let mut scopes = parse::scope::Scopes::new();
    let global_scope = parse::scope::create_global(&mut scopes);
    let mut tbls = parse::Tables::new(arenas, TypeStrictnessKind::Flow);
    for (ast, strictness_kind) in ordered_asts {
        tbls.set_strictness_kind(*strictness_kind);
        // For builtins, each lib file is parsed separately into an unresolved global
        // scope. The parsed lib files are then combined before resolving names. The
        // proceses of combining deals with overridden definitions. Finally, the
        // resolved builtins are merged. Declared modules can depend on each other, so
        // they are treated like a cycle.
        for stmt in ast.statements.iter() {
            parse::statement(opts, global_scope, &mut scopes, &mut tbls, stmt);
        }
    }
    for (ast, strictness_kind, file_sig) in global_augmentation_asts {
        tbls.set_strictness_kind(*strictness_kind);
        let module_scope = parse::scope::create_augmentation_module(&mut scopes, global_scope);
        let parent = if file_sig.requires().is_empty() {
            module_scope
        } else {
            global_scope
        };
        let augmentation_scope =
            parse::scope::push_declare_global(&mut scopes, parent, global_scope);
        for stmt in ast.statements.iter() {
            if let flow_parser::ast::statement::StatementInner::DeclareNamespace {
                inner: decl, ..
            } = &**stmt
                && matches!(
                    decl.id,
                    flow_parser::ast::statement::declare_namespace::Id::Global(_)
                )
            {
                parse::declare_global(
                    opts,
                    augmentation_scope,
                    &mut scopes,
                    &mut tbls,
                    decl.as_ref(),
                );
            } else if let flow_parser::ast::statement::StatementInner::NamespaceExportDeclaration {
                inner,
                ..
            } = &**stmt
                // A UMD global aliases this copy of the module, which cannot resolve imports.
                && file_sig.requires().is_empty()
            {
                parse::scope::bind_umd_global(&mut scopes, &mut tbls, module_scope, &inner.id);
            } else {
                parse::statement(opts, module_scope, &mut scopes, &mut tbls, stmt);
            }
        }
    }
    parse::scope::bind_global_this(&mut scopes, &mut tbls, global_scope, LOC_NONE);
    let builtins = parse::scope::builtins_exn(&scopes, global_scope);
    (scopes, tbls, builtins)
}

fn create_pack_cx(additional_errors: Vec<BindingValidation<parse::LocNode<'_>>>) -> pack::Cx {
    pack::Cx::new(
        additional_errors
            .into_iter()
            .map(|e| {
                Errno::BindingValidationError(Box::new(
                    e.map(&mut (), |_, node| node.0.index_exn()),
                ))
            })
            .collect(),
    )
}

fn pack_builtins<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    mut scopes: parse::scope::Scopes<'arena, 'ast>,
    mut tbls: parse::Tables<'arena, 'ast>,
    (global_values, global_types, global_modules, global_umd_modules): (
        BTreeMap<FlowSmolStr, parse::BindingNode<'arena, 'ast>>,
        BTreeMap<FlowSmolStr, parse::BindingNode<'arena, 'ast>>,
        BuiltinModules<'arena, 'ast>,
        BuiltinModules<'arena, 'ast>,
    ),
) -> (Vec<Errno<Index<Loc>>>, Table<Loc>, Builtins<Loc>) {
    // mark - forces lazys, which may append to tbls.additional_errors
    let mut marker = mark::Marker::new(&[]);
    for b in global_values.values() {
        mark::mark_binding(opts, &mut scopes, &mut tbls, &mut marker, b);
    }
    for b in global_types.values() {
        mark::mark_binding(opts, &mut scopes, &mut tbls, &mut marker, b);
    }
    for (loc, exports) in global_modules.values().chain(global_umd_modules.values()) {
        mark::mark_builtin_module(opts, &mut scopes, &mut tbls, loc, &exports.borrow());
    }
    mark::mark_errors(&mut marker, &tbls.additional_errors);

    // compact
    // Read additional_errors after mark, since lazy forcing may add errors.
    let parse::Tables {
        locs,
        module_refs,
        local_defs,
        remote_refs,
        pattern_defs,
        patterns,
        additional_errors,
        strictness_kind: _,
    } = tbls;
    let locs = locs.compact_without_merge();
    let module_refs = module_refs.compact_without_merge();
    let local_defs = local_defs.compact_without_merge();
    let remote_refs = remote_refs.compact_without_merge();
    let pattern_defs = pattern_defs.compact_without_merge();
    let patterns = patterns.compact_without_merge();

    // copy
    let mut cx = create_pack_cx(additional_errors);
    let (locs, _) = locs.copy(|loc| loc.dupe());
    let (module_refs, _) = module_refs.copy(|u| u.dupe());
    let (local_defs, _) = local_defs.copy(|binding| pack::pack_local_binding(&mut cx, binding));
    let (remote_refs, _) = remote_refs.copy(|binding| pack::pack_remote_binding(binding));
    let (pattern_defs, _) = pattern_defs.copy(|parsed| pack::pack_parsed(&mut cx, parsed));
    let (patterns, _) = patterns.copy(|pattern| pack::pack_pattern(pattern));

    let (global_values, global_types, global_modules, global_umd_modules) = {
        let global_values: BTreeMap<FlowSmolStr, Index<_>> = global_values
            .into_iter()
            .map(|(k, v)| (k, pack::pack_builtin(v)))
            .collect();
        let global_types: BTreeMap<FlowSmolStr, Index<_>> = global_types
            .into_iter()
            .map(|(k, v)| (k, pack::pack_builtin(v)))
            .collect();
        let mut pack_modules =
            |modules: BuiltinModules<'arena, 'ast>| -> BTreeMap<FlowSmolStr, ModuleDef<Index<Loc>>> {
                modules
                    .into_iter()
                    .map(|(name, (loc, exports))| {
                        let (loc, module_kind) =
                            pack::pack_builtin_module(&mut cx, &name, (loc, &exports.borrow()));
                        (name, ModuleDef { loc, module_kind })
                    })
                    .collect()
            };
        let global_modules = pack_modules(global_modules);
        let global_umd_modules = pack_modules(global_umd_modules);

        (
            global_values,
            global_types,
            global_modules,
            global_umd_modules,
        )
    };
    let builtins = Builtins {
        module_refs,
        local_defs,
        remote_refs,
        pattern_defs,
        patterns,
        global_values,
        global_types,
        global_modules,
        global_umd_modules,
    };

    (cx.take_errs(), locs, builtins)
}

fn parse_module<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    arenas: &'arena bumpalo::Bump,
    strict: bool,
    platform_availability_set: Option<PlatformSet>,
    source: Option<FileKey>,
    ast: &'ast Program<Loc, Loc>,
) -> (
    parse::ScopeId,
    parse::scope::Scopes<'arena, 'ast>,
    parse::Tables<'arena, 'ast>,
    parse::LocNode<'arena>,
    Rc<RefCell<parse::Exports<'arena, 'ast>>>,
) {
    let mut scopes = parse::scope::Scopes::new();
    let strictness_kind =
        TypeStrictnessKind::from_is_typescript(opts.is_ts_file || opts.is_dts_file);
    let scope = parse::scope::create_module(&mut scopes, strict, platform_availability_set);
    let mut tbls = parse::Tables::new(arenas, strictness_kind);
    let file_loc = tbls.push_loc(Loc { source, ..LOC_NONE });
    for stmt in ast.statements.iter() {
        parse::statement(opts, scope, &mut scopes, &mut tbls, stmt);
    }
    let exports = parse::scope::exports_exn(&mut scopes, scope);
    (scope, scopes, tbls, file_loc, exports)
}

fn parse_libdef_file_as_empty_module<'arena: 'ast, 'ast>(
    arenas: &'arena bumpalo::Bump,
    strictness_kind: TypeStrictnessKind,
    strict: bool,
    platform_availability_set: Option<PlatformSet>,
    source: Option<FileKey>,
) -> (
    parse::ScopeId,
    parse::scope::Scopes<'arena, 'ast>,
    parse::Tables<'arena, 'ast>,
    parse::LocNode<'arena>,
    Rc<RefCell<parse::Exports<'arena, 'ast>>>,
) {
    let mut scopes = parse::scope::Scopes::new();
    let scope = parse::scope::create_module(&mut scopes, strict, platform_availability_set);
    let mut tbls = parse::Tables::new(arenas, strictness_kind);
    let file_loc = tbls.push_loc(Loc { source, ..LOC_NONE });
    let exports = parse::scope::exports_exn(&mut scopes, scope);
    (scope, scopes, tbls, file_loc, exports)
}

fn merge_locs(loc0: &Loc, loc1: &Loc) -> Option<Loc> {
    match packed_locs::compare_locs(loc0, loc1) {
        std::cmp::Ordering::Less => None,
        std::cmp::Ordering::Equal => Some(loc0.dupe()),
        std::cmp::Ordering::Greater => {
            panic!("out of order {:?} > {:?}", loc0, loc1)
        }
    }
}

/// Lenient loc merging for targeted packing only.
///
/// Targeted packing binds annotated lexical roots (parameters,
/// function-local definitions) that live mid-span of lazily-parsed
/// signatures. When those signatures splice in later, their tails sit
/// ahead of the roots in chain order while sorting after them, so
/// source order is unachievable here no matter where the roots are
/// placed. Out-of-order entries survive
/// unmerged; every other push still lands in order, so
/// adjacent-duplicate merging is unaffected where it applies.
///
/// The main pack path uses [`merge_locs`], which panics on out-of-order
/// locations to preserve the ordering invariant.
fn merge_locs_allow_unordered(loc0: &Loc, loc1: &Loc) -> Option<Loc> {
    match packed_locs::compare_locs(loc0, loc1) {
        std::cmp::Ordering::Less => None,
        std::cmp::Ordering::Equal => Some(loc0.dupe()),
        std::cmp::Ordering::Greater => None,
    }
}

fn pack<'arena, 'ast>(
    opts: &TypeSigOptions,
    locs_to_dirtify: &[Loc],
    source: Option<FileKey>,
    (_scope, mut scopes, mut tbls, file_loc, exports): (
        parse::ScopeId,
        parse::scope::Scopes<'arena, 'ast>,
        parse::Tables<'arena, 'ast>,
        parse::LocNode<'arena>,
        Rc<RefCell<parse::Exports<'arena, 'ast>>>,
    ),
) -> (Vec<Errno<Index<Loc>>>, Table<Loc>, Module<Loc>) {
    // mark
    let mut marker = mark::Marker::new(locs_to_dirtify);
    mark::mark_exports(
        opts,
        &mut scopes,
        &mut tbls,
        &mut marker,
        &file_loc,
        &exports.borrow(),
    );
    mark::mark_errors(&mut marker, &tbls.additional_errors);

    // Destructure tbls after marking is complete
    let parse::Tables {
        locs,
        module_refs,
        local_defs,
        remote_refs,
        pattern_defs,
        patterns,
        additional_errors,
        strictness_kind: _,
    } = tbls;

    // compact
    let locs = locs.compact_with_merge(merge_locs);
    let module_refs = module_refs.compact_without_merge();
    let local_defs = local_defs.compact_without_merge();
    let remote_refs = remote_refs.compact_without_merge();
    let pattern_defs = pattern_defs.compact_without_merge();
    let patterns = patterns.compact_without_merge();

    // copy
    let mut cx = create_pack_cx(additional_errors);
    let (locs, _) = locs.copy(|loc| loc.dupe());
    let (module_refs, _) = module_refs.copy(|u| u.dupe());
    let (local_defs, dirty_local_defs) =
        local_defs.copy(|binding| pack::pack_local_binding(&mut cx, binding));
    let (remote_refs, _) = remote_refs.copy(|binding| pack::pack_remote_binding(binding));
    let (pattern_defs, dirty_pattern_defs) =
        pattern_defs.copy(|parsed| pack::pack_parsed(&mut cx, parsed));
    let (patterns, _) = patterns.copy(|pattern| pack::pack_pattern(pattern));

    let module_kind = {
        pack::pack_exports(
            &mut cx,
            &file_loc,
            &source
                .as_ref()
                .map(|s| FlowSmolStr::new(s.as_str()))
                .unwrap_or_else(|| FlowSmolStr::new_inline("<unnamed>")),
            &exports.borrow(),
        )
    };

    let module = Module {
        module_kind,
        module_refs,
        local_defs,
        dirty_local_defs,
        remote_refs,
        pattern_defs,
        dirty_pattern_defs,
        patterns,
    };

    (cx.take_errs(), locs, module)
}

pub fn parse_and_pack_builtins<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    arenas: &'arena bumpalo::Bump,
    ordered_asts: &[(&'ast Program<Loc, Loc>, TypeStrictnessKind)],
    global_augmentation_asts: &[(&'ast Program<Loc, Loc>, TypeStrictnessKind, &FileSig)],
) -> (Vec<Errno<Index<Loc>>>, Table<Loc>, Builtins<Loc>) {
    let (scopes, tbls, globals) = parse_libs(opts, arenas, ordered_asts, global_augmentation_asts);
    pack_builtins(opts, scopes, tbls, globals)
}

pub fn parse_and_pack_module<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    arenas: &'arena bumpalo::Bump,
    strict: bool,
    platform_availability_set: Option<PlatformSet>,
    source: Option<FileKey>,
    ast: &'ast Program<Loc, Loc>,
) -> (Vec<Errno<Index<Loc>>>, Table<Loc>, Module<Loc>) {
    let parsed = if opts.is_lib_file {
        // We will generate an empty file_sig for libdef files in parsing_service.
        // Therefore, we should also generate an empty type sig for libdef files, so that
        // both will consistently report that there are no imports and exports in libdef files.
        // If they are inconsistent, we can potentially crash like what's reported in
        // https://github.com/meta/flow/issues/9262
        parse_libdef_file_as_empty_module(
            arenas,
            TypeStrictnessKind::from_is_typescript(opts.is_ts_file || opts.is_dts_file),
            strict,
            platform_availability_set,
            source.dupe(),
        )
    } else {
        parse_module(
            opts,
            arenas,
            strict,
            platform_availability_set,
            source.dupe(),
            ast,
        )
    };
    pack(opts, &opts.locs_to_dirtify, source, parsed)
}

/// Packs only the bindings at `target_locs` and their signature dependencies.
pub fn parse_and_pack_targets<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    arenas: &'arena bumpalo::Bump,
    strict: bool,
    platform_availability_set: Option<PlatformSet>,
    source: Option<FileKey>,
    ast: &'ast Program<Loc, Loc>,
    target_locs: &BTreeSet<Loc>,
    use_defs: BTreeMap<Loc, Vec<Loc>>,
) -> (Vec<Errno<Index<Loc>>>, Table<Loc>, TargetedModule<Loc>) {
    if target_locs.is_empty() {
        return (
            vec![],
            Table::empty(),
            TargetedModule {
                module: Module {
                    module_kind: pack::ModuleKind::CJSModule {
                        type_exports: vec![],
                        exports: None,
                        info: pack::CJSModuleInfo {
                            type_export_keys: vec![],
                            type_stars: vec![],
                            strict,
                            platform_availability_set,
                        },
                    },
                    module_refs: Table::empty(),
                    local_defs: Table::empty(),
                    dirty_local_defs: vec![],
                    remote_refs: Table::empty(),
                    pattern_defs: Table::empty(),
                    dirty_pattern_defs: vec![],
                    patterns: Table::empty(),
                },
                roots: BTreeMap::new(),
            },
        );
    }

    let (scope, mut scopes, mut tbls, _file_loc, exports) =
        parse_module(opts, arenas, strict, platform_availability_set, source, ast);
    let mut roots = scopes.value_bindings_at_locs(target_locs);
    // Parameters and function-local bindings never enter scopes, so roots
    // naming them resolve to nothing. Bind the annotated ones from their
    // annotations so targeted analyses can address them.
    if roots.len() < target_locs.len() {
        let missing: BTreeSet<Loc> = target_locs
            .iter()
            .filter(|loc| !roots.contains_key(*loc))
            .map(|loc| loc.dupe())
            .collect();
        roots.extend(parse::bind_lexical_annotation_roots(
            opts,
            scope,
            &mut scopes,
            &mut tbls,
            ast,
            &missing,
        ));
    }

    scopes.set_use_defs(use_defs);
    let mut marker = mark::Marker::new(&opts.locs_to_dirtify);
    for binding in roots.values() {
        mark::mark_binding(opts, &mut scopes, &mut tbls, &mut marker, binding);
    }
    mark::mark_errors(&mut marker, &tbls.additional_errors);

    let parse::Tables {
        locs,
        module_refs,
        local_defs,
        remote_refs,
        pattern_defs,
        patterns,
        additional_errors,
        strictness_kind: _,
    } = tbls;

    let locs = locs.compact_with_merge(merge_locs_allow_unordered);
    let module_refs = module_refs.compact_without_merge();
    let local_defs = local_defs.compact_without_merge();
    let remote_refs = remote_refs.compact_without_merge();
    let pattern_defs = pattern_defs.compact_without_merge();
    let patterns = patterns.compact_without_merge();

    let packed_roots = roots
        .into_iter()
        .filter_map(|(loc, binding)| {
            let root = match binding {
                parse::BindingNode::LocalBinding(node) => {
                    TargetedRoot::LocalDef(node.0.index_opt()?)
                }
                parse::BindingNode::RemoteBinding(node) => {
                    TargetedRoot::RemoteRef(node.0.index_opt()?)
                }
            };
            Some((loc, root))
        })
        .collect();

    let mut cx = create_pack_cx(additional_errors);
    let (locs, _) = locs.copy(|loc| loc.dupe());
    let (module_refs, _) = module_refs.copy(|u| u.dupe());
    let (local_defs, dirty_local_defs) =
        local_defs.copy(|binding| pack::pack_local_binding(&mut cx, binding));
    let (remote_refs, _) = remote_refs.copy(pack::pack_remote_binding);
    let (pattern_defs, dirty_pattern_defs) =
        pattern_defs.copy(|parsed| pack::pack_parsed(&mut cx, parsed));
    let (patterns, _) = patterns.copy(pack::pack_pattern);

    let exports = exports.borrow();
    let module_kind = pack::ModuleKind::CJSModule {
        type_exports: vec![],
        exports: None,
        info: pack::CJSModuleInfo {
            type_export_keys: vec![],
            type_stars: vec![],
            strict: exports.strict,
            platform_availability_set: exports.platform_availability_set,
        },
    };
    let module = Module {
        module_kind,
        module_refs,
        local_defs,
        dirty_local_defs,
        remote_refs,
        pattern_defs,
        dirty_pattern_defs,
        patterns,
    };

    (
        cx.take_errs(),
        locs,
        TargetedModule {
            module,
            roots: packed_roots,
        },
    )
}
