/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ops::Deref;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::aloc_representation_do_not_use;
use flow_common::flow_import_specifier::FlowImportSpecifier;
use flow_common::reason;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc::RProperty;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_env_builder::assertion_call_target;
use flow_env_builder::assertion_call_target::AssertionInfo;
use flow_env_builder::assertion_call_target::AssertionKind;
use flow_env_builder::assertion_call_target::AssertionTarget;
use flow_env_builder::assertion_call_target::CalleeRootKind;
use flow_env_builder::assertion_call_target::CalleeTarget;
use flow_env_builder::assertion_call_target::ImportedName;
use flow_parser::ast;
use flow_parser::loc::Loc;
use flow_parser_utils::file_sig::FileSig;
use flow_type_sig::compact_table::Index;
use flow_type_sig::compact_table::Table;
use flow_type_sig::packed_type_sig::Module;
use flow_type_sig::packed_type_sig::TargetedModule;
use flow_type_sig::packed_type_sig::TargetedRoot;
use flow_type_sig::type_sig_options::TypeSigOptions;
use flow_type_sig::type_sig_utils;
use flow_typing_context::Context;
use flow_typing_context::ResolvedRequire;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_utils::annotation_inference;
use flow_typing_utils::type_sig_merge;

pub(crate) struct PackedAnalysis {
    canonical_candidates: Vec<CalleeTarget<ALoc>>,
    all_candidates: Vec<CalleeTarget<ALoc>>,
    local: Option<PackedLocalAnalysis>,
}

struct PackedLocalAnalysis {
    existing: Option<ExistingLocalAnalysis>,
    parsed: Option<ParsedLocalAnalysis>,
}

struct ExistingLocalAnalysis {
    module: Arc<Module<Loc>>,
    roots: BTreeMap<ALoc, TargetedRoot<Loc>>,
}

struct ParsedLocalAnalysis {
    locs: Table<Loc>,
    targeted: TargetedModule<Loc>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalCallee {
    root_binding_loc: ALoc,
    property_path: Vec<FlowSmolStr>,
    root_kind: CalleeRootKind,
    optional_receiver_paths: Vec<usize>,
}

impl CanonicalCallee {
    fn of_target(target: &CalleeTarget<ALoc>) -> Self {
        Self {
            root_binding_loc: target.root_binding_loc.dupe(),
            property_path: target.property_path.clone(),
            root_kind: target.root_kind,
            optional_receiver_paths: target.optional_receiver_paths.clone(),
        }
    }
}

pub(crate) fn pack(
    opts: &TypeSigOptions,
    cx: &mut Context<'_>,
    ast: &ast::Program<Loc, Loc>,
    aloc_ast: &ast::Program<ALoc, ALoc>,
    scope_info: &flow_analysis::scope_api::ScopeInfo<ALoc>,
    provider_info: &flow_env_builder::provider_api::Info<ALoc>,
    current_type_sig: Option<Arc<Module<Loc>>>,
    file_sig: &FileSig,
) -> Option<PackedAnalysis> {
    let imports = assertion_call_target::file_sig_descriptors(file_sig);
    let candidates = assertion_call_target::collect(aloc_ast, scope_info, provider_info, imports);
    if candidates.is_empty() {
        return None;
    }
    let canonical_candidates: Vec<CalleeTarget<ALoc>> = candidates
        .iter()
        .map(|candidate| (CanonicalCallee::of_target(candidate), candidate.clone()))
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .collect();
    // Only roots bound in this file are merged here: unannotated locals need
    // their semantic types to detect assertion behavior used without an
    // annotation. Import roots resolve through the import machinery instead,
    // and global roots (e.g. lib declarations) through builtins below.
    let here = cx.file().dupe();
    let roots = canonical_candidates
        .iter()
        .filter(|candidate| {
            !candidate.is_global
                && candidate.root_binding_loc.source() == Some(&here)
                && (candidate.import.is_none() || !candidate.provider_locs.is_empty())
        })
        .map(|candidate| candidate.root_binding_loc.dupe())
        .collect::<BTreeSet<_>>();
    let source = Some(here.dupe());
    let aloc_tables = cx.aloc_tables();
    let reused_roots = current_type_sig
        .as_ref()
        .map(|module| {
            module
                .local_defs
                .iter()
                .enumerate()
                .filter_map(|(index, def)| {
                    let keyed_root_loc = aloc_representation_do_not_use::make_keyed(
                        source.dupe(),
                        def.id_loc().as_usize() as u32,
                    );
                    let root_loc = ALoc::of_loc(keyed_root_loc.to_loc_with_tables(aloc_tables));
                    roots
                        .contains(&root_loc)
                        .then_some((root_loc, TargetedRoot::LocalDef(Index::new(index))))
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let fallback_roots = roots
        .iter()
        .filter(|root| !reused_roots.contains_key(*root))
        .map(|root| root.to_loc_exn().dupe())
        .collect::<BTreeSet<_>>();
    let existing = current_type_sig.and_then(|module| {
        (!reused_roots.is_empty()).then_some(ExistingLocalAnalysis {
            module,
            roots: reused_roots,
        })
    });
    let parsed = if fallback_roots.is_empty() {
        None
    } else {
        let arena = bumpalo::Bump::new();
        let use_defs = scope_info
            .defs_of_all_uses()
            .iter()
            .map(|(use_loc, def)| {
                (
                    use_loc.to_loc_exn().dupe(),
                    def.locs.iter().map(|loc| loc.to_loc_exn().dupe()).collect(),
                )
            })
            .collect();
        let (_, locs, targeted) = type_sig_utils::parse_and_pack_targets(
            opts,
            &arena,
            cx.is_strict(),
            cx.metadata().overridable.available_platforms.clone(),
            Some(cx.file().dupe()),
            ast,
            &fallback_roots,
            use_defs,
        );
        Some(ParsedLocalAnalysis { locs, targeted })
    };
    let local = (existing.is_some() || parsed.is_some())
        .then_some(PackedLocalAnalysis { existing, parsed });
    Some(PackedAnalysis {
        canonical_candidates,
        all_candidates: candidates,
        local,
    })
}

pub(crate) struct ResolvedAnalysis {
    assertion_calls: BTreeMap<ALoc, AssertionInfo>,
}

pub(crate) fn resolve<'cx>(cx: &mut Context<'cx>, analysis: PackedAnalysis) -> ResolvedAnalysis {
    let Ok(result) = cx.with_suppressed_errors(|cx| -> Result<_, !> {
        let merged = analysis
            .local
            .map(|local| MergedLocalAnalysis::new(cx, local));
        // Classify once per canonical callee, then fan out to all call sites.
        // Locals resolve from merged signature types; imports use dependency types.
        let mut classified = BTreeMap::new();
        for candidate in &analysis.canonical_candidates {
            let canonical = CanonicalCallee::of_target(candidate);
            // The reason must carry a loc: concretization can emit inspection
            // errors, and sourceless errors panic in `add_output` even under
            // suppression.
            let reason = reason::mk_reason(
                reason::VirtualReasonDesc::RIdentifier(candidate.root_name.dupe()),
                candidate.callee_loc.dupe(),
            );
            if candidate.import.is_some() {
                if let Some(info) = imported_assertion_info(cx, &reason, candidate) {
                    classified.insert(canonical, info);
                }
                continue;
            }
            let root_type = merged
                .as_ref()
                .and_then(|merged| merged.root(&candidate.root_binding_loc));
            let assertion = root_type
                .and_then(|root_type| receiver_type(cx, &reason, candidate.root_kind, root_type))
                .and_then(|root_type| callee_type_for_classification(cx, candidate, &root_type, 0))
                .as_ref()
                .and_then(|callee_type| assertion_info_of_type(cx, &reason, callee_type));
            match assertion {
                Some(info) => {
                    classified.insert(canonical, info);
                }
                None if candidate.is_global => {
                    // Global roots (e.g. lib declarations) are never merged here.
                    // Their annotation lives in the lib itself, so a guard refines directly.
                    if let Some((_, global_t)) = cx.builtin_type_opt(candidate.root_name.as_str())
                        && let Some(callee_type) =
                            callee_type_for_classification(cx, candidate, &global_t, 0)
                        && let Some(info) = assertion_info_of_type(cx, &reason, &callee_type)
                    {
                        classified.insert(canonical, info);
                    }
                }
                None => {}
            }
        }
        let mut assertion_calls = BTreeMap::new();
        for candidate in &analysis.all_candidates {
            let canonical = CanonicalCallee::of_target(candidate);
            if let Some(info) = classified.get(&canonical) {
                assertion_calls.insert(candidate.callee_loc.dupe(), *info);
            }
        }
        Ok(ResolvedAnalysis { assertion_calls })
    });
    result
}

pub(crate) fn assertion_calls(analysis: &ResolvedAnalysis) -> &BTreeMap<ALoc, AssertionInfo> {
    &analysis.assertion_calls
}

fn assertion_of_function(function: &flow_typing_type::type_::FunType) -> Option<AssertionInfo> {
    let guard = function.type_guard.as_deref()?;
    if !guard.is_asserts() {
        return None;
    }
    let target = if guard.param_name.1 == "this" {
        AssertionTarget::Receiver
    } else {
        AssertionTarget::Parameter(
            function
                .params
                .iter()
                .position(|param| param.0.as_ref() == Some(&guard.param_name.1))?,
        )
    };
    Some(AssertionInfo {
        target,
        kind: if guard.type_guard.is_some() {
            AssertionKind::TypeGuard
        } else {
            AssertionKind::Bare
        },
    })
}

/// Resolve tvars/annotations and evaluate deferred heads.
/// Runs under error suppression; the reason only labels inspection diagnostics.
fn normalized_assertion_head(cx: &mut Context<'_>, reason: &Reason, type_: &Type) -> Option<Type> {
    let type_ = cx.find_resolved(type_)?;
    let type_ = annotation_inference::concretize_type(cx, reason.dupe(), type_);
    cx.find_resolved(&type_)
}

fn imported_assertion_info(
    cx: &mut Context<'_>,
    reason: &Reason,
    candidate: &CalleeTarget<ALoc>,
) -> Option<AssertionInfo> {
    let import = candidate.import.as_ref()?;
    if !import.is_value {
        return None;
    }
    let ResolvedRequire::TypedModule(module) =
        cx.find_require(&FlowImportSpecifier::userland(import.source.dupe()))
    else {
        return None;
    };
    let Ok(module) = module(cx) else {
        return None;
    };
    // Mirror checking's ESM default-import typing
    // (`import_default_t_kit::on_module_t_with_env`): a default import
    // from a CJS module is the whole `cjs_export`, not the tmap's
    // `default` entry.
    if matches!(
        import.remote,
        ImportedName::CommonJS | ImportedName::Default
    ) && let Some((_, export)) = &module.module_export_types.cjs_export
    {
        let callee_type = callee_type_for_classification(cx, candidate, export, 0)?;
        return assertion_info_of_type(cx, reason, &callee_type);
    }
    let exports = cx.find_exports(module.module_export_types.value_exports_tmap.dupe());
    let (export_name, prefix) = match &import.remote {
        ImportedName::Default => ("default", 0),
        ImportedName::Named(name) => (name.as_str(), 0),
        ImportedName::Namespace => (candidate.property_path.first()?.as_str(), 1),
        ImportedName::CommonJS => {
            // Mirror checking's `require()` typing
            // (`cjs_require_t_kit::on_module_t`): without
            // `cjs_export` (handled above) the value is the namespace
            // object, unless `automatic_require_default` applies to a
            // non-lib module that has a default export.
            let automatic_require_default =
                cx.automatic_require_default() && !cx.is_lib_reason_def(&module.module_reason);
            if automatic_require_default && exports.get(&Name::new("default")).is_some() {
                ("default", 0)
            } else {
                (candidate.property_path.first()?.as_str(), 1)
            }
        }
    };
    let export = exports.get(&Name::new(export_name))?;
    let callee_type = callee_type_for_classification(cx, candidate, &export.type_, prefix)?;
    assertion_info_of_type(cx, reason, &callee_type)
}

/// Classify a callee type that has already been navigated.
fn assertion_info_of_type(
    cx: &mut Context<'_>,
    reason: &Reason,
    type_: &Type,
) -> Option<AssertionInfo> {
    let type_ = normalized_assertion_head(cx, reason, type_)?;
    match type_.deref() {
        TypeInner::DefT(_, def_t) => match def_t.deref() {
            DefTInner::FunT(_, function) => assertion_of_function(function),
            DefTInner::PolyT(poly) => assertion_info_of_type(cx, reason, &poly.t_out),
            // Callable objects (e.g. node's `assert` module, whose exports
            // object carries the `asserts` call signature): classify through
            // the call type.
            DefTInner::ObjT(obj) if let Some(id) = obj.call_t.as_ref() => {
                let call_t = cx.find_call(*id);
                assertion_info_of_type(cx, reason, &call_t)
            }
            _ => None,
        },
        // Unions never classify, even when all members agree: TS does not
        // narrow calls on union callees. Intersections never classify either.
        // TODO: ban assertion functions in intersections at their definition.
        _ => None,
    }
}

/// Navigate `property_path` from `root_type` via `get_prop` lookup.
/// Shared by locals (roots from merged signatures) and imports (roots from
/// dependency exports).
fn callee_type_for_classification(
    cx: &mut Context<'_>,
    candidate: &CalleeTarget<ALoc>,
    root_type: &Type,
    prefix: usize,
) -> Option<Type> {
    let root_type = cx.find_resolved(root_type)?;
    let reason = reason::mk_reason(RProperty(None), candidate.callee_loc.dupe());
    for index in &candidate.optional_receiver_paths {
        // An import namespace is non-nullish; selecting an export consumes
        // that part of the path before navigation starts.
        if *index < prefix {
            continue;
        }
        let receiver = property_type(
            cx,
            &candidate.callee_loc,
            &candidate.property_path[prefix..*index],
            root_type.dupe(),
        );
        if !definitely_nonnullish(cx, &reason, &receiver) {
            return None;
        }
    }
    Some(property_type(
        cx,
        &candidate.callee_loc,
        &candidate.property_path[prefix..],
        root_type,
    ))
}

fn definitely_nonnullish(cx: &mut Context<'_>, reason: &Reason, type_: &Type) -> bool {
    let Some(type_) = normalized_assertion_head(cx, reason, type_) else {
        return false;
    };
    match type_.deref() {
        TypeInner::DefT(_, def_t) => match def_t.deref() {
            DefTInner::FunT(..)
            | DefTInner::ObjT(_)
            | DefTInner::InstanceT(_)
            | DefTInner::ClassT(_) => true,
            DefTInner::PolyT(poly) => definitely_nonnullish(cx, reason, &poly.t_out),
            _ => false,
        },
        _ => false,
    }
}

fn receiver_type(
    cx: &mut Context<'_>,
    reason: &Reason,
    kind: CalleeRootKind,
    type_: &Type,
) -> Option<Type> {
    if matches!(kind, CalleeRootKind::Binding | CalleeRootKind::StaticThis) {
        return Some(type_.dupe());
    }
    let type_ = normalized_assertion_head(cx, reason, type_)?;
    if let TypeInner::DefT(_, def_t) = type_.deref()
        && let DefTInner::PolyT(poly) = def_t.deref()
    {
        return receiver_type(cx, reason, kind, &poly.t_out);
    }
    let instance = annotation_inference::mk_instance(cx, None, reason.dupe(), None, type_);
    let instance = normalized_assertion_head(cx, reason, &instance)?;
    match kind {
        CalleeRootKind::InstanceThis => Some(instance),
        CalleeRootKind::InstanceSuper | CalleeRootKind::StaticSuper => {
            let TypeInner::DefT(_, def_t) = instance.deref() else {
                return None;
            };
            let DefTInner::InstanceT(inner) = def_t.deref() else {
                return None;
            };
            let parent = normalized_assertion_head(cx, reason, &inner.super_)?;
            if kind == CalleeRootKind::InstanceSuper {
                return Some(parent);
            }
            let TypeInner::DefT(_, def_t) = parent.deref() else {
                return None;
            };
            let DefTInner::InstanceT(inner) = def_t.deref() else {
                return None;
            };
            Some(inner.static_.dupe())
        }
        _ => None,
    }
}

fn property_type<'cx>(
    cx: &mut Context<'cx>,
    callee_loc: &ALoc,
    property_path: &[FlowSmolStr],
    root_type: Type,
) -> Type {
    let strictness_kind = cx.type_strictness_kind();
    property_path.iter().fold(root_type, |type_, property| {
        let name = Name::new(property.dupe());
        let reason = reason::mk_reason(RProperty(Some(name.dupe())), callee_loc.dupe());
        annotation_inference::get_prop(
            cx,
            flow_typing_type::type_::unknown_use(),
            reason,
            None,
            name,
            type_,
            strictness_kind,
        )
    })
}

struct MergedLocalAnalysis {
    existing: Option<type_sig_merge::TargetedMergeResult<ALoc>>,
    parsed: Option<type_sig_merge::TargetedMergeResult<Loc>>,
}

impl MergedLocalAnalysis {
    fn new(cx: &mut Context<'_>, local: PackedLocalAnalysis) -> Self {
        let existing = local.existing.map(|existing| {
            type_sig_merge::merge_packed_target_roots(
                cx,
                Some(cx.file().dupe()),
                &existing.module,
                existing.roots,
            )
        });
        let parsed = local
            .parsed
            .map(|parsed| type_sig_merge::merge_targeted_module(cx, parsed.locs, parsed.targeted));
        Self { existing, parsed }
    }

    fn root(&self, loc: &ALoc) -> Option<&Type> {
        self.existing
            .as_ref()
            .and_then(|merged| merged.roots().get(loc))
            .or_else(|| {
                self.parsed
                    .as_ref()
                    .and_then(|merged| merged.roots().get(loc.to_loc_exn()))
            })
    }
}
