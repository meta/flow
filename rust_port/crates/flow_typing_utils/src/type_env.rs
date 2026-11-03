/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use dupe::OptionDupedExt;
use flow_aloc::ALoc;
use flow_common::reason::Reason;
use flow_common::reason::ReasonDesc;
use flow_common::reason::VirtualReasonDesc;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_data_structure_wrapper::vector::FlowVector;
use flow_env_builder::assertion_call_target::AssertionTarget;
use flow_env_builder::env_api::CacheableEnvError;
use flow_env_builder::env_api::DeclareNamespaceReadPathElement;
use flow_env_builder::env_api::DefLocType;
use flow_env_builder::env_api::EnvEntry;
use flow_env_builder::env_api::EnvInfo;
use flow_env_builder::env_api::EnvKey;
use flow_env_builder::env_api::EnvSet;
use flow_env_builder::env_api::Read as EnvRead;
use flow_env_builder::env_api::Refinement;
use flow_env_builder::env_api::RefinementKind;
use flow_env_builder::env_api::ValKind;
use flow_env_builder::env_api::WriteLoc;
use flow_env_builder::env_api::has_assigning_write;
use flow_env_builder::name_def_types::Def as NameDef;
use flow_env_builder::name_def_types::Import as NameDefImport;
use flow_env_builder::name_def_types::ScopeKind;
use flow_env_builder::provider_api;
use flow_parser::ast::statement::ImportKind as AstImportKind;
use flow_parser::loc_sig::LocSig;
use flow_typing_context::Context;
use flow_typing_context::Cx;
use flow_typing_context::PossiblyRefinedWriteState;
use flow_typing_context::TypingMode;
use flow_typing_flow_js::flow_js;
use flow_typing_flow_js::flow_js::FlowJs;
use flow_typing_flow_js::tvar_resolver;
use flow_typing_key::Key;
use flow_typing_loc_env::loc_env::LocEnv;
use flow_typing_loc_env::loc_env::TypeEntry;
use flow_typing_type::type_::ClassBinding;
use flow_typing_type::type_::ConstrainedAssignmentData;
use flow_typing_type::type_::HintEvalResult;
use flow_typing_type::type_::LazyHintT;
use flow_typing_type::type_::Predicate;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeGuardKind;
use flow_typing_type::type_::UseOp;
use flow_utils_concurrency::job_error::JobError;

use crate::predicate_kit;
use crate::type_operation_utils;

/// lookup modes:
///
/// - ForValue is a lookup from a syntactic value location, i.e. standard JS code
/// - ForType is a lookup from a syntactic type location,
///   e.g. annotations, interface declarations etc.
/// - ForTypeof is a lookup from a typeof expression (necessarily in a type location)
///
/// Rules:
///
/// 1. ForValue lookups give errors if they retrieve type aliases (note: we
///    have a single namespace, so any name resolves uniquely to either a
///    value or type)
///
/// 2. ForValue lookups give errors if they forward reference non-hoisted
///    things (lets or consts)
///
/// 3. ForType lookups may return values or type aliases, since some values
///    also denote types - e.g. a generator function F also denotes the type
///    of the objects it creates. Of course many values don't also have a type
///    denotation and thus errors in type position. But we don't know the type
///    of a symbol during local inference as a rule, so errors of this kind are
///    not raised here.
///
/// 4. ForTypeof lookups are in fact ForValue lookups, but due to the order in
///    which AST traversal takes place, these lookups may legitimately violate
///    rule #2, hence the need for a special mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupMode {
    ForValue,
    ForType,
    ForTypeof,
}

pub fn get_class_entries<'cx>(cx: &mut Context<'cx>) -> Vec<ClassBinding> {
    let env = cx.environment();
    let class_stack = &env.class_stack;
    let class_bindings = &env.class_bindings;
    let mut lst = Vec::new();
    for l in class_stack {
        if let Some(c) = class_bindings.get(&EnvKey::new(DefLocType::OrdinaryNameLoc, l.dupe())) {
            lst.push(c.clone());
        }
    }
    lst
}

pub fn with_class_stack<'cx, A>(
    cx: &mut Context<'cx>,
    class_stack: FlowVector<ALoc>,
    f: impl FnOnce(&mut Context<'cx>) -> A,
) -> A {
    let old_class_stack = {
        let env = cx.environment_mut();
        std::mem::replace(&mut env.class_stack, class_stack)
    };
    let res = f(cx);
    let env = cx.environment_mut();
    env.class_stack = old_class_stack;
    res
}

fn recorded_declare_namespace_member_opt<'cx>(
    lookup_mode: LookupMode,
    cx: &mut Context<'cx>,
    name: &FlowSmolStr,
    loc: ALoc,
) -> Option<Type> {
    fn namespace_type_member_opt<'cx>(
        cx: &mut Context<'cx>,
        namespace_t: &Type,
        name: &FlowSmolStr,
        use_loc: ALoc,
    ) -> Option<Type> {
        use flow_common::reason::Name;
        use flow_typing_type::type_::TypeInner;
        use flow_typing_type::type_::property;
        use flow_typing_type::type_util;

        let namespace_t = cx.find_resolved(namespace_t)?;
        match namespace_t.deref() {
            TypeInner::NamespaceT(ns) => {
                let props = cx.find_props_opt(ns.types_tmap.dupe())?;
                let prop = props
                    .get(&Name::new(name.dupe()))
                    .and_then(property::read_t)?;
                Some(type_util::mod_reason_of_t(
                    &|r: Reason| r.reposition(use_loc.dupe()),
                    &prop,
                ))
            }
            _ => None,
        }
    }

    fn namespace_value_member_opt<'cx>(
        cx: &mut Context<'cx>,
        namespace_t: &Type,
        name: &FlowSmolStr,
        use_loc: ALoc,
    ) -> Option<Type> {
        fn object_value_member_opt<'cx>(
            cx: &mut Context<'cx>,
            obj_t: &Type,
            name: &FlowSmolStr,
            use_loc: ALoc,
        ) -> Option<Type> {
            use flow_common::reason::Name;
            use flow_typing_type::type_::DefTInner;
            use flow_typing_type::type_::TypeInner;
            use flow_typing_type::type_::property;
            use flow_typing_type::type_util;

            let obj_t = cx.find_resolved(obj_t)?;
            match obj_t.deref() {
                TypeInner::DefT(_, def_t) => match def_t.deref() {
                    DefTInner::ObjT(obj) => {
                        let props = cx.find_props_opt(obj.props_tmap.dupe())?;
                        let prop = props
                            .get(&Name::new(name.dupe()))
                            .and_then(property::read_t)?;
                        Some(type_util::mod_reason_of_t(
                            &|r: Reason| r.reposition(use_loc.dupe()),
                            &prop,
                        ))
                    }
                    _ => None,
                },
                _ => None,
            }
        }

        use flow_typing_type::type_::TypeInner;

        let namespace_t = cx.find_resolved(namespace_t)?;
        match namespace_t.deref() {
            TypeInner::NamespaceT(ns) => {
                object_value_member_opt(cx, &ns.values_type, name, use_loc)
            }
            TypeInner::DefT(_, _) => object_value_member_opt(cx, &namespace_t, name, use_loc),
            _ => None,
        }
    }

    fn namespace_member_opt<'cx>(
        lookup_mode: LookupMode,
        cx: &mut Context<'cx>,
        namespace_t: &Type,
        name: &FlowSmolStr,
        use_loc: ALoc,
    ) -> Option<Type> {
        match lookup_mode {
            LookupMode::ForType => namespace_type_member_opt(cx, namespace_t, name, use_loc),
            LookupMode::ForValue | LookupMode::ForTypeof => {
                namespace_value_member_opt(cx, namespace_t, name, use_loc)
            }
        }
    }

    fn declare_namespace_read_path<'cx>(
        cx: &mut Context<'cx>,
        loc: &ALoc,
    ) -> Option<FlowVector<DeclareNamespaceReadPathElement<ALoc>>> {
        cx.environment()
            .var_info
            .declare_namespace_read_paths
            .get(loc)
            .duped()
    }

    fn resolved_loc_env_write_opt<'cx>(
        cx: &mut Context<'cx>,
        kind: DefLocType,
        loc: ALoc,
    ) -> Option<Type> {
        let env = cx.environment();
        let entry_key = EnvKey::new(kind, loc.dupe());
        if !has_assigning_write(entry_key, &env.var_info.env_entries) {
            return None;
        }
        let TypeEntry { t, state } = env.find_write(kind, loc)?;
        if state.is_forced() {
            Some(t.dupe())
        } else {
            None
        }
    }

    fn namespace_types_at_path_prefix<'cx>(
        cx: &mut Context<'cx>,
        namespace_path: &FlowVector<DeclareNamespaceReadPathElement<ALoc>>,
        prefix_len: usize,
        loc: ALoc,
    ) -> Vec<Type> {
        fn resolved_loc_env_writes<'cx>(
            cx: &mut Context<'cx>,
            locs: &FlowVector<ALoc>,
        ) -> Vec<Type> {
            locs.iter()
                .filter_map(|loc| {
                    resolved_loc_env_write_opt(cx, DefLocType::OrdinaryNameLoc, loc.dupe())
                })
                .collect()
        }

        fn namespace_types_for_root_path_element<'cx>(
            cx: &mut Context<'cx>,
            root: &DeclareNamespaceReadPathElement<ALoc>,
        ) -> Vec<Type> {
            let builtin_namespace = |cx: &mut Context<'cx>| {
                cx.builtin_type_opt(root.name.as_str())
                    .or_else(|| cx.builtin_value_opt(root.name.as_str()))
                    .map(|(_, t)| t)
            };
            // Installed lib namespaces are already fully merged.
            if cx.is_global_lib_context() {
                if let Some(namespace_t) = builtin_namespace(cx) {
                    vec![namespace_t]
                } else if cx
                    .file()
                    .as_str()
                    .starts_with(flow_parser::file_key::FLOWLIB_MARKER)
                {
                    Vec::new()
                } else {
                    resolved_loc_env_writes(cx, &root.locs)
                }
            } else {
                // A local namespace shadows a same-named builtin.
                let namespace_types = resolved_loc_env_writes(cx, &root.locs);
                if namespace_types.is_empty() {
                    builtin_namespace(cx).into_iter().collect()
                } else {
                    namespace_types
                }
            }
        }

        fn namespace_types_for_child_path_element<'cx>(
            cx: &mut Context<'cx>,
            namespace_types: &[Type],
            child: &DeclareNamespaceReadPathElement<ALoc>,
            use_loc: ALoc,
        ) -> Vec<Type> {
            let member_lookup = |cx: &mut Context<'cx>| {
                namespace_types
                    .iter()
                    .filter_map(|namespace_t| {
                        namespace_type_member_opt(cx, namespace_t, &child.name, use_loc.dupe())
                            .or_else(|| {
                                namespace_value_member_opt(
                                    cx,
                                    namespace_t,
                                    &child.name,
                                    use_loc.dupe(),
                                )
                            })
                    })
                    .collect()
            };
            if cx.is_global_lib_context() {
                member_lookup(cx)
            } else {
                let namespace_types = resolved_loc_env_writes(cx, &child.locs);
                if namespace_types.is_empty() {
                    member_lookup(cx)
                } else {
                    namespace_types
                }
            }
        }

        let mut path = namespace_path.iter().take(prefix_len);
        let Some(root) = path.next() else {
            return Vec::new();
        };
        let mut namespace_types = namespace_types_for_root_path_element(cx, root);
        for namespace in path {
            namespace_types =
                namespace_types_for_child_path_element(cx, &namespace_types, namespace, loc.dupe());
        }
        namespace_types
    }

    let namespace_path = declare_namespace_read_path(cx, &loc)?;
    for prefix_len in (1..=namespace_path.len()).rev() {
        for namespace_t in
            namespace_types_at_path_prefix(cx, &namespace_path, prefix_len, loc.dupe())
        {
            if let Some(t) = namespace_member_opt(lookup_mode, cx, &namespace_t, name, loc.dupe()) {
                return Some(t);
            }
        }
    }
    None
}

pub fn has_hint<'cx>(cx: &mut Context<'cx>, loc: ALoc) -> bool {
    if !matches!(cx.typing_mode(), TypingMode::CheckingMode) {
        return false;
    }
    let env = cx.environment();
    env.hint_map
        .get(&EnvKey::new(DefLocType::OrdinaryNameLoc, loc))
        .map(|h| h.0)
        .unwrap_or(false)
}

pub fn get_hint<'cx>(cx: &Context<'cx>, loc: ALoc) -> LazyHintT<Cx> {
    if !matches!(cx.typing_mode(), TypingMode::CheckingMode) {
        return LazyHintT(
            false,
            Rc::new(|_cx: &mut Context<'_>, _e, _s, _r| Ok(HintEvalResult::NoHint)),
        );
    }
    let env = cx.environment();
    env.hint_map
        .get(&EnvKey::new(DefLocType::OrdinaryNameLoc, loc))
        .cloned()
        .unwrap_or_else(|| {
            LazyHintT(
                false,
                Rc::new(|_cx: &mut Context<'_>, _e, _s, _r| Ok(HintEvalResult::NoHint)),
            )
        })
}

pub fn set_scope_kind<'cx>(cx: &mut Context<'cx>, k: ScopeKind) -> ScopeKind {
    let env = cx.environment_mut();
    let old = env.scope_kind;
    env.scope_kind = k;
    old
}

pub fn in_class_scope<'cx, A>(
    cx: &mut Context<'cx>,
    loc: ALoc,
    f: impl FnOnce(&mut Context<'cx>) -> A,
) -> A {
    let old_stack = {
        let env = cx.environment_mut();
        let old_stack = env.class_stack.clone();
        env.class_stack.push(loc);
        old_stack
    };
    let res = f(cx);
    let env = cx.environment_mut();
    env.class_stack = old_stack;
    res
}

fn is_var_kind<'cx>(cx: &Context<'cx>, k: ScopeKind) -> bool {
    cx.environment().scope_kind == k
}

pub fn in_async_scope<'cx>(cx: &Context<'cx>) -> bool {
    is_var_kind(cx, ScopeKind::Async)
        || is_var_kind(cx, ScopeKind::AsyncGenerator)
        || is_var_kind(cx, ScopeKind::AsyncComponentOrHookBody)
}

pub fn var_scope_kind<'cx>(cx: &Context<'cx>) -> ScopeKind {
    cx.environment().scope_kind
}

pub fn in_global_scope<'cx>(cx: &Context<'cx>) -> bool {
    is_var_kind(cx, ScopeKind::Global)
}

pub fn in_toplevel_scope<'cx>(cx: &Context<'cx>) -> bool {
    is_var_kind(cx, ScopeKind::Module)
}

pub fn is_provider<'cx>(cx: &Context<'cx>, id_loc: ALoc) -> bool {
    let env = cx.environment();
    env.var_info.providers.is_provider(&id_loc)
}

// We don't want the new-env to throw if we encounter some new case in the wild
// for which we did not adequately prepare. Instead, we return `any` in prod
// mode, but still crash in build mode.
//
// Rust port note: per plan.md Phase D the cancel/timeout cascade rejects
// `catch_unwind` entirely — a real Rust panic is a bug, let it crash. The
// inner `f` here only panics on missing env entries (which is a bug). We keep
// this wrapper as a structural mirror of the OCaml signature and call `f()`
// directly. The OCaml prod fallback (`AnyT::at + add_output`) is unreachable
// in the Rust port because we don't catch the panic.
fn with_debug_exn<'cx>(_cx: &mut Context<'cx>, _loc: ALoc, f: impl FnOnce() -> Type) -> Type {
    f()
}

// See `with_debug_exn` above for the rationale: cancel/timeout uses Result;
// `f`'s remaining panics are real bugs we let crash.
fn with_debug_exn_error<'cx, A: 'static>(
    cx: &mut Context<'cx>,
    _loc: ALoc,
    f: impl FnOnce(&mut Context<'cx>) -> A,
    _error: impl FnOnce() -> A,
) -> A {
    f(cx)
}

pub fn t_option_value_exn<'cx>(cx: &mut Context<'cx>, loc: ALoc, t: Option<Type>) -> Type {
    with_debug_exn(cx, loc.dupe(), || match t {
        Some(t) => t,
        None => panic!("Missing location entry: {}", loc.debug_to_string(true)),
    })
}

// ***********
// * Helpers *
// ***********

pub fn checked_find_loc_env_write_opt<'cx>(
    cx: &mut Context<'cx>,
    kind: DefLocType,
    loc: ALoc,
) -> Option<Type> {
    let env = cx.environment();
    let entry_key = EnvKey::new(kind, loc.dupe());
    let env_entry = env.var_info.env_entries.get(&entry_key).cloned();
    let write = env.find_write(kind, loc.dupe());
    match (env_entry, write) {
        (Some(EnvEntry::NonAssigningWrite), None) => None,
        (Some(EnvEntry::NonAssigningWrite), Some(TypeEntry { t, .. })) => Some(t.dupe()),
        (_, None) => {
            use flow_typing_errors::error_message::ErrorMessage;
            use flow_typing_errors::error_message::InternalError;
            flow_js::add_output_non_speculating(
                cx,
                ErrorMessage::EInternal(Box::new((
                    loc.dupe(),
                    InternalError::MissingEnvWrite(loc),
                ))),
            );
            None
        }
        (_, Some(TypeEntry { t, .. })) => Some(t.dupe()),
    }
}

pub fn checked_find_loc_env_write<'cx>(cx: &mut Context<'cx>, kind: DefLocType, loc: ALoc) -> Type {
    let t = checked_find_loc_env_write_opt(cx, kind, loc.dupe());
    t_option_value_exn(cx, loc, t)
}

fn find_var_opt<'a>(var_info: &'a EnvInfo<ALoc>, loc: &ALoc) -> Result<&'a EnvRead<ALoc>, ALoc> {
    match var_info.env_values.get_ordinary(loc) {
        Some(x) => Ok(x),
        None => Err(loc.dupe()),
    }
}

/// Whether the name read at `use_loc` can head a computed key, that is,
/// whether it binds a value that is not a namespace.
///
/// A class and an enum bind a value, so they head one, even though they name a
/// type as well: a bracketed key is read as an expression, and `C` there is the
/// class object. A namespace does not, because `A.B` may be reaching a type
/// through it, and a name that only names a type does not either.
///
/// The answer comes from the environment built before any typing, so it forces
/// nothing and is available while an annotation is still being converted.
pub fn heads_computed_key<'cx>(cx: &mut Context<'cx>, use_loc: &ALoc) -> bool {
    use flow_analysis::bindings::Kind;
    use flow_analysis::bindings::Namespace;

    let env = cx.environment();
    // One name can be bound in both namespaces, as `const K` next to `type K`.
    // The scopes hold one binding per name, so they cannot say which was meant,
    // but this read was already resolved against both, and the type wins in
    // type position. The signature pipeline prefers the type the same way.
    if let Ok(EnvRead { val_kind, .. }) = find_var_opt(&env.var_info, use_loc)
        && matches!(val_kind, ValKind::Type { .. })
    {
        return false;
    }
    match env.var_info.scopes.def_of_use_opt(use_loc) {
        None => false,
        Some(def) => {
            def.kind.namespaces().contains(&Namespace::Value)
                && !matches!(
                    def.kind,
                    Kind::DeclaredNamespace | Kind::Import { namespace: true }
                )
        }
    }
}

pub struct LocalExportBinding {
    pub def_loc: Option<ALoc>,
    pub val_kind: ValKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalClasslikeBindingKind {
    Mono,
    Poly,
    Other,
}

pub type ImportInfo = (AstImportKind, NameDefImport, FlowSmolStr, ALoc);

pub fn local_export_binding_at_loc<'cx>(
    cx: &mut Context<'cx>,
    loc: ALoc,
) -> Option<LocalExportBinding> {
    let env = cx.environment();
    let var_info = &env.var_info;
    match find_var_opt(var_info, &loc) {
        Ok(EnvRead {
            def_loc, val_kind, ..
        }) => Some(LocalExportBinding {
            def_loc: def_loc.clone(),
            val_kind: *val_kind,
        }),
        Err(_) => None,
    }
}

pub fn local_classlike_binding_kind_at_loc<'cx>(
    cx: &mut Context<'cx>,
    use_loc: ALoc,
) -> Option<LocalClasslikeBindingKind> {
    let env = cx.environment();
    let var_info = &env.var_info;
    match find_var_opt(var_info, &use_loc) {
        Err(_) => None,
        Ok(EnvRead { def_loc: None, .. }) => None,
        Ok(EnvRead {
            def_loc: Some(def_loc),
            val_kind,
            ..
        }) => match val_kind {
            ValKind::Type { imported: true, .. } | ValKind::TypeParam | ValKind::TsImport => None,
            ValKind::Type {
                imported: false, ..
            }
            | ValKind::Value
            | ValKind::Internal => {
                let lookup = |kind| env.name_defs.get(&EnvKey::new(kind, def_loc.dupe()));
                let def_opt = match lookup(DefLocType::OrdinaryNameLoc) {
                    Some(entry) => Some(entry),
                    None => lookup(DefLocType::ClassSelfLoc),
                };
                match def_opt {
                    None => None,
                    Some((def, _scope, _stack, _reason, _definition_kind)) => {
                        let poly_or_mono = |has_tparams: bool| {
                            if has_tparams {
                                LocalClasslikeBindingKind::Poly
                            } else {
                                LocalClasslikeBindingKind::Mono
                            }
                        };
                        match def {
                            NameDef::Class(data) => {
                                Some(poly_or_mono(data.class_.tparams.is_some()))
                            }
                            NameDef::Record(data) => {
                                Some(poly_or_mono(data.record.tparams.is_some()))
                            }
                            NameDef::DeclaredClass(data) => {
                                Some(poly_or_mono(data.decl.tparams.is_some()))
                            }
                            NameDef::Interface(_, iface) => {
                                Some(poly_or_mono(iface.tparams.is_some()))
                            }
                            _ => Some(LocalClasslikeBindingKind::Other),
                        }
                    }
                }
            }
        },
    }
}

pub fn import_info_at_loc<'cx>(cx: &mut Context<'cx>, use_loc: ALoc) -> Option<ImportInfo> {
    let env = cx.environment();
    let var_info = &env.var_info;
    match find_var_opt(var_info, &use_loc) {
        Err(_) => None,
        Ok(EnvRead { def_loc: None, .. }) => None,
        Ok(EnvRead {
            def_loc: Some(def_loc),
            val_kind,
            ..
        }) => match val_kind {
            ValKind::Type { imported: true, .. } | ValKind::TsImport => {
                match env
                    .name_defs
                    .get(&EnvKey::new(DefLocType::OrdinaryNameLoc, def_loc.dupe()))
                {
                    Some((NameDef::Import(data), _scope, _stack, _reason, _definition_kind)) => {
                        Some((
                            data.import_kind.clone(),
                            data.import.clone(),
                            data.source.dupe(),
                            data.source_loc.dupe(),
                        ))
                    }
                    _ => None,
                }
            }
            _ => None,
        },
    }
}

fn find_refi(var_info: &EnvInfo<ALoc>, id: i32) -> Refinement<ALoc> {
    (var_info.refinement_of_id)(id)
}

fn find_providers(var_info: &EnvInfo<ALoc>, loc: &ALoc) -> Vec<ALoc> {
    var_info
        .providers
        .providers_of_def(loc)
        .map(|def_providers| {
            def_providers
                .providers
                .iter()
                .map(|p| p.reason.loc().dupe())
                .collect()
        })
        .unwrap_or_default()
}

fn is_def_loc_annotated(var_info: &EnvInfo<ALoc>, loc: &ALoc) -> bool {
    use flow_env_builder::find_providers::State;
    match var_info.providers.providers_of_def(loc) {
        Some(provider_api::DefProviders {
            state: State::AnnotatedVar { .. },
            ..
        }) => true,
        _ => false,
    }
}

pub fn provider_type_for_def_loc<'cx>(
    intersect: bool,
    cx: &mut Context<'cx>,
    def_loc: ALoc,
) -> Type {
    let var_info = cx.environment().var_info.dupe();
    let providers: Vec<Type> = find_providers(&var_info, &def_loc)
        .into_iter()
        .filter_map(|loc| {
            match checked_find_loc_env_write_opt(cx, DefLocType::OrdinaryNameLoc, loc.dupe()) {
                Some(w) => Some(w),
                None => match var_info.env_entries.get_ordinary(&loc) {
                    None | Some(EnvEntry::NonAssigningWrite) => None,
                    _ => {
                        use flow_env_builder::env_api::EnvInvariantFailure;
                        use flow_typing_errors::error_message::ErrorMessage;
                        use flow_typing_errors::error_message::InternalError;
                        flow_js::add_output_non_speculating(
                            cx,
                            ErrorMessage::EInternal(Box::new((
                                loc.dupe(),
                                InternalError::EnvInvariant(EnvInvariantFailure::Impossible(
                                    format!(
                                        "Missing provider write at {} for {}",
                                        flow_common::reason::string_of_aloc(None, &loc),
                                        flow_common::reason::string_of_aloc(None, &def_loc),
                                    )
                                    .into(),
                                )),
                            ))),
                        );
                        None
                    }
                },
            }
        })
        .collect();

    use flow_common::reason::mk_reason;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::inter_rep;
    use flow_typing_type::type_::mixed_t;
    use flow_typing_type::type_::union_rep;
    match providers.len() {
        0 => mixed_t::make(mk_reason(None, def_loc)),
        1 => providers.into_iter().next().unwrap(),
        _ => {
            let mut iter = providers.into_iter();
            let t1 = iter.next().unwrap();
            let t2 = iter.next().unwrap();
            let ts: Vec<Type> = iter.collect();
            let reason = mk_reason(None, def_loc);
            if intersect {
                Type::new(TypeInner::IntersectionT(
                    reason,
                    inter_rep::make(t1, t2, ts.into()),
                ))
            } else {
                Type::new(TypeInner::UnionT(
                    reason,
                    union_rep::make(None, union_rep::UnionKind::ProvidersKind, t1, t2, ts.into()),
                ))
            }
        }
    }
}

// *************
// *  Reading  *
// *************

fn merge_actually_refined_refining_locs(
    pair: (Option<FlowOrdSet<ALoc>>, Option<FlowOrdSet<ALoc>>),
) -> Option<FlowOrdSet<ALoc>> {
    match pair {
        (None, l @ Some(_)) | (l @ Some(_), None) => l,
        (None, None) => None,
        (Some(l1), Some(l2)) => Some(l1.union(l2)),
    }
}

/// Computes the phi type for a node given all its lower bounds
/// Currently, this just produces a new type variable with the types of
/// all the incoming writes as lower bounds. In the future, however, we
/// may want to compute a more specific least upper bound for these writes.
fn phi<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    states: Vec<PossiblyRefinedWriteState>,
) -> Result<PossiblyRefinedWriteState, JobError> {
    match states.len() {
        1 => Ok(states.into_iter().next().unwrap()),
        _ => {
            let tvar = flow_typing_tvar::mk(cx, reason.dupe());
            let mut actually_refined_refining_locs_ref: Option<FlowOrdSet<ALoc>> = None;
            let mut errors = Vec::new();
            for state in states {
                let PossiblyRefinedWriteState {
                    t,
                    errors: state_errors,
                    actually_refined_refining_locs,
                } = state;
                flow_js::flow_t_non_speculating(cx, (&t, &tvar))?;
                actually_refined_refining_locs_ref = merge_actually_refined_refining_locs((
                    actually_refined_refining_locs_ref,
                    actually_refined_refining_locs,
                ));
                errors.extend(state_errors);
            }
            tvar_resolver::resolve(cx, tvar_resolver::default_no_lowers, true, &tvar);
            Ok(PossiblyRefinedWriteState {
                t: tvar,
                errors,
                actually_refined_refining_locs: actually_refined_refining_locs_ref,
            })
        }
    }
}

fn read_pred_func_info_exn<'cx>(
    cx: &mut Context<'cx>,
    loc: ALoc,
) -> Result<Box<flow_typing_type::type_::PredFuncallInfo>, JobError> {
    use flow_typing_type::type_::AnySource;
    use flow_typing_type::type_::PredFuncallInfo;
    use flow_typing_type::type_::any_t;
    use flow_typing_type::type_::unknown_use;
    let loc_clone = loc.dupe();
    let loc_err = loc.dupe();

    with_debug_exn_error(
        cx,
        loc,
        move |cx| {
            let lazy_info = {
                let env = cx.environment();
                env.pred_func_map
                    .get(&loc_clone)
                    .unwrap_or_else(|| {
                        panic!(
                            "pred_func_map key not found: {}",
                            loc_clone.debug_to_string(true)
                        )
                    })
                    .dupe()
            };
            Ok(Box::new(lazy_info.full.try_get_forced(cx)?.clone()))
        },
        move || {
            Ok(Box::new(PredFuncallInfo(
                unknown_use(),
                loc_err.dupe(),
                any_t::at(AnySource::AnyError(None), loc_err),
                None,
                Rc::from([]),
            )))
        },
    )
}

/// Returns [true] iff the input type is potentially a predicate function.
fn maybe_predicate_function<'cx>(
    cx: &mut Context<'cx>,
    t: &Type,
    assertion: bool,
) -> Result<bool, JobError> {
    use std::ops::Deref;

    use flow_typing_flow_common::flow_js_utils::FlowJsException;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::ImplicitInstantiationTvarData;
    use flow_typing_type::type_::ImplicitInstantiationTvarKind;
    use flow_typing_type::type_::PolyTData;
    use flow_typing_type::type_::TypeInner;

    fn simplify_callee<'cx>(cx: &mut Context<'cx>, func_t: &Type) -> Result<Type, JobError> {
        let errors = cx.errors();
        let result = cx.run_and_rolled_back_cache(|cx| {
            use flow_typing_type::type_::UseT;
            use flow_typing_type::type_::UseTInner;
            use flow_typing_type::type_::type_collector::TypeCollector;
            use flow_typing_type::type_::union_rep::UnionKind;
            let reason = flow_typing_type::type_util::reason_of_t(func_t);
            let collector = TypeCollector::create();
            let u = UseT::new(UseTInner::CallT(Box::new(
                flow_typing_type::type_::CallTData {
                    use_op: flow_typing_type::type_::unknown_use(),
                    reason: reason.dupe(),
                    call_action: Box::new(flow_typing_type::type_::CallAction::ConcretizeCallee(
                        collector.dupe(),
                    )),
                    return_hint: flow_typing_type::type_::hint_unavailable(),
                },
            )));
            flow_js::flow_non_speculating(cx, (func_t, &u))?;
            Ok(flow_typing_type::type_util::union_of_ts(
                reason.dupe(),
                collector.collect_to_vec(),
                Some(UnionKind::ResolvedKind),
            ))
        });
        cx.reset_errors(errors);
        result
    }

    fn matches_position(fun_t: &flow_typing_type::type_::FunType, assertion: bool) -> bool {
        fun_t
            .type_guard
            .as_deref()
            .is_some_and(|guard| matches!(guard.kind, TypeGuardKind::Asserts) == assertion)
    }

    fn on_ground(t: &Type, assertion: bool) -> bool {
        match t.deref() {
            TypeInner::AnyT(_, _)
            | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
                kind: ImplicitInstantiationTvarKind::Placeholder,
                ..
            }) => false,
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                DefTInner::FunT(_, fun_t) => matches_position(fun_t, assertion),
                DefTInner::PolyT(box PolyTData { t_out, .. }) => match t_out.deref() {
                    TypeInner::DefT(_, inner_def_t) => match inner_def_t.deref() {
                        DefTInner::FunT(_, fun_t) => matches_position(fun_t, assertion),
                        _ => false,
                    },
                    _ => false,
                },
                _ => false,
            },
            _ => true, // yes: safe option
        }
    }

    fn on_non_inter<'cx>(
        cx: &mut Context<'cx>,
        t: &Type,
        assertion: bool,
    ) -> Result<bool, JobError> {
        match t.deref() {
            TypeInner::DefT(_, _) => Ok(on_ground(t, assertion)),
            _ => {
                let simplified = simplify_callee(cx, t)?;
                Ok(on_ground(&simplified, assertion))
            }
        }
    }

    fn on_concrete<'cx>(
        cx: &mut Context<'cx>,
        t: &Type,
        assertion: bool,
    ) -> Result<bool, JobError> {
        match t.deref() {
            TypeInner::IntersectionT(_, rep) => {
                for m in rep.members_iter() {
                    if on_t(cx, m, assertion)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => on_non_inter(cx, t, assertion),
        }
    }

    fn on_t<'cx>(cx: &mut Context<'cx>, t: &Type, assertion: bool) -> Result<bool, JobError> {
        let reason = flow_typing_type::type_util::reason_of_t(t);
        let cts = match FlowJs::possible_concrete_types_for_inspection(cx, reason, t) {
            Ok(v) => v,
            Err(FlowJsException::WorkerCanceled(c)) => {
                return Err(JobError::Canceled(c));
            }
            Err(FlowJsException::TimedOut(t)) => {
                return Err(JobError::TimedOut(t));
            }
            Err(FlowJsException::DebugThrow { loc }) => {
                return Err(JobError::DebugThrow { loc });
            }
            Err(err) => panic!("Non speculating: {:?}", err),
        };
        for ct in cts.iter() {
            if on_concrete(cx, ct, assertion)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    on_t(cx, t, assertion)
}

fn predicate_of_refinement<'cx>(
    cx: &mut Context<'cx>,
    kind: &RefinementKind<ALoc>,
) -> Result<Option<Predicate>, JobError> {
    use flow_common::reason::Name;
    use flow_common::reason::mk_reason;
    use flow_env_builder::env_api::InstanceofContext;
    use flow_typing_type::type_::ArrayLengthOp;
    use flow_typing_type::type_::BinaryTest;
    use flow_typing_type::type_::PredicateInner;

    fn pred<'cx>(
        cx: &mut Context<'cx>,
        kind: &RefinementKind<ALoc>,
    ) -> Result<Option<Predicate>, JobError> {
        Ok(match kind {
            RefinementKind::AndR(r1, r2) => match (pred(cx, r1)?, pred(cx, r2)?) {
                (Some(p1), Some(p2)) => Some(Predicate::new(PredicateInner::AndP(p1, p2))),
                (Some(p), None) | (None, Some(p)) => Some(p),
                (None, None) => None,
            },
            RefinementKind::OrR(r1, r2) => match (pred(cx, r1)?, pred(cx, r2)?) {
                (Some(p1), Some(p2)) => Some(Predicate::new(PredicateInner::OrP(p1, p2))),
                _ => None,
            },
            RefinementKind::NotR(r) => {
                pred(cx, r)?.map(|p| Predicate::new(PredicateInner::NotP(p)))
            }
            RefinementKind::TruthyR => Some(Predicate::new(PredicateInner::TruthyP)),
            RefinementKind::NullR => Some(Predicate::new(PredicateInner::NullP)),
            RefinementKind::UndefinedR => Some(Predicate::new(PredicateInner::VoidP)),
            RefinementKind::MaybeR => Some(Predicate::new(PredicateInner::MaybeP)),
            RefinementKind::InstanceOfR { expr, context } => {
                let loc = expr.loc().dupe();
                flow_typing_debug::verbose::print_if_verbose_lazy(cx, None, None, None, || {
                    vec![format!(
                        "reading from location {} (in instanceof refinement)",
                        flow_common::reason::string_of_aloc(None, &loc)
                    )]
                });
                let t = checked_find_loc_env_write(cx, DefLocType::ExpressionLoc, loc.dupe());
                match context {
                    InstanceofContext::InstanceOfExpr => {
                        type_operation_utils::type_assertions::assert_instanceof_rhs(cx, &t)?;
                    }
                    InstanceofContext::MatchInstancePattern => {
                        type_operation_utils::type_assertions::assert_match_instance_pattern_constructor(cx, &t)?;
                    }
                }
                Some(Predicate::new(PredicateInner::BinaryP(
                    BinaryTest::InstanceofTest,
                    t,
                )))
            }
            RefinementKind::IsArrayR => Some(Predicate::new(PredicateInner::ArrP)),
            RefinementKind::ArrLenR { op, n } => {
                let op = match op {
                    flow_env_builder::env_api::ArrayLengthOp::ArrLenEqual => {
                        ArrayLengthOp::ArrLenEqual
                    }
                    flow_env_builder::env_api::ArrayLengthOp::ArrLenGreaterThanEqual => {
                        ArrayLengthOp::ArrLenGreaterThanEqual
                    }
                };
                Some(Predicate::new(PredicateInner::ArrLenP { op, n: *n }))
            }
            RefinementKind::BoolR(loc) => {
                Some(Predicate::new(PredicateInner::BoolP(Box::new(loc.dupe()))))
            }
            RefinementKind::FunctionR => Some(Predicate::new(PredicateInner::FunP)),
            RefinementKind::NumberR(loc) => {
                Some(Predicate::new(PredicateInner::NumP(Box::new(loc.dupe()))))
            }
            RefinementKind::BigIntR(loc) => Some(Predicate::new(PredicateInner::BigIntP(
                Box::new(loc.dupe()),
            ))),
            RefinementKind::ObjectR => Some(Predicate::new(PredicateInner::ObjP)),
            RefinementKind::StringR(loc) => {
                Some(Predicate::new(PredicateInner::StrP(Box::new(loc.dupe()))))
            }
            RefinementKind::SymbolR(loc) => Some(Predicate::new(PredicateInner::SymbolP(
                Box::new(loc.dupe()),
            ))),
            RefinementKind::SingletonBoolR { loc, sense: _, lit } => Some(Predicate::new(
                PredicateInner::SingletonBoolP(Box::new((loc.dupe(), *lit))),
            )),
            RefinementKind::SingletonStrR { loc, sense, lit } => {
                Some(Predicate::new(PredicateInner::SingletonStrP(Box::new((
                    loc.dupe(),
                    *sense,
                    lit.as_str().to_string(),
                )))))
            }
            RefinementKind::SingletonNumR { loc, sense, lit } => {
                use flow_typing_type::type_::NumberLiteral;
                Some(Predicate::new(PredicateInner::SingletonNumP(Box::new((
                    loc.dupe(),
                    *sense,
                    NumberLiteral(lit.0, lit.1.dupe()),
                )))))
            }
            RefinementKind::SingletonBigIntR { loc, sense, lit } => {
                use flow_typing_type::type_::BigIntLiteral;
                Some(Predicate::new(PredicateInner::SingletonBigIntP(Box::new(
                    (loc.dupe(), *sense, BigIntLiteral(lit.0, lit.1.dupe())),
                ))))
            }
            RefinementKind::SentinelR { prop, other_loc } => {
                flow_typing_debug::verbose::print_if_verbose_lazy(cx, None, None, None, || {
                    vec![format!(
                        "reading from location {} (in sentinel refinement)",
                        flow_common::reason::string_of_aloc(None, other_loc)
                    )]
                });
                let other_t =
                    checked_find_loc_env_write(cx, DefLocType::ExpressionLoc, other_loc.dupe());
                Some(Predicate::new(PredicateInner::BinaryP(
                    BinaryTest::SentinelProp(prop.dupe()),
                    other_t,
                )))
            }
            RefinementKind::EqR(loc) => {
                flow_typing_debug::verbose::print_if_verbose_lazy(cx, None, None, None, || {
                    vec![format!(
                        "reading from location {} (in eq refinement)",
                        flow_common::reason::string_of_aloc(None, loc)
                    )]
                });
                let other_t = checked_find_loc_env_write(cx, DefLocType::ExpressionLoc, loc.dupe());
                Some(Predicate::new(PredicateInner::BinaryP(
                    BinaryTest::EqTest,
                    other_t,
                )))
            }
            RefinementKind::LatentR {
                func,
                index,
                targs: _,
                arguments: _,
            } => {
                let func_loc = func.loc().dupe();
                let lazy_info = {
                    let env = cx.environment();
                    env.pred_func_map.get(&func_loc).duped()
                };
                match lazy_info {
                    Some(info) => {
                        // Only the callee is needed to decide whether this is a predicate
                        // function; the arguments are typed lazily by `info.full` below.
                        let t = info.callee.try_get_forced(cx)?;
                        if maybe_predicate_function(cx, t, false)? {
                            Some(Predicate::new(PredicateInner::LatentP(
                                read_pred_func_info_exn(cx, func_loc)?,
                                index.dupe(),
                            )))
                        } else {
                            None
                        }
                    }
                    None => None,
                }
            }
            RefinementKind::AssertionR {
                func,
                target,
                targs: _,
                arguments: _,
            } => {
                // The callee was already proven by the pre-resolution analysis,
                // so unlike `LatentR` there is no callee-type check here.
                let func_loc = func.loc().dupe();
                let lazy_info = {
                    let env = cx.environment();
                    env.pred_func_map.get(&func_loc).duped()
                };
                match lazy_info {
                    Some(_) => {
                        let info = read_pred_func_info_exn(cx, func_loc)?;
                        Some(Predicate::new(match target {
                            AssertionTarget::Parameter(index) => {
                                PredicateInner::LatentP(info, Rc::from([*index as i32].as_slice()))
                            }
                            AssertionTarget::Receiver => PredicateInner::LatentThisP(info),
                        }))
                    }
                    None => None,
                }
            }
            RefinementKind::LatentThisR {
                func,
                targs: _,
                arguments: _,
            } => {
                let func_loc = func.loc().dupe();
                let lazy_info = {
                    let env = cx.environment();
                    env.pred_func_map.get(&func_loc).duped()
                };
                match lazy_info {
                    Some(info) => {
                        let t = info.callee.try_get_forced(cx)?;
                        if maybe_predicate_function(cx, t, false)? {
                            Some(Predicate::new(PredicateInner::LatentThisP(
                                read_pred_func_info_exn(cx, func_loc)?,
                            )))
                        } else {
                            None
                        }
                    }
                    None => None,
                }
            }
            RefinementKind::PropNullishR { propname, loc } => {
                let reason = mk_reason(
                    VirtualReasonDesc::RProperty(Some(Name::new(propname.dupe()))),
                    loc.dupe(),
                );
                Some(Predicate::new(PredicateInner::NotP(Predicate::new(
                    PredicateInner::PropNonMaybeP(propname.dupe(), reason),
                ))))
            }
            RefinementKind::PropIsExactlyNullR { propname, loc } => {
                let reason = mk_reason(
                    VirtualReasonDesc::RProperty(Some(Name::new(propname.dupe()))),
                    loc.dupe(),
                );
                Some(Predicate::new(PredicateInner::PropIsExactlyNullP(
                    propname.dupe(),
                    reason,
                )))
            }
            RefinementKind::PropNonVoidR { propname, loc } => {
                let reason = mk_reason(
                    VirtualReasonDesc::RProperty(Some(Name::new(propname.dupe()))),
                    loc.dupe(),
                );
                Some(Predicate::new(PredicateInner::PropNonVoidP(
                    propname.dupe(),
                    reason,
                )))
            }
            RefinementKind::PropTruthyR { propname, loc } => {
                let reason = mk_reason(
                    VirtualReasonDesc::RProperty(Some(Name::new(propname.dupe()))),
                    loc.dupe(),
                );
                Some(Predicate::new(PredicateInner::PropTruthyP(
                    propname.dupe(),
                    reason,
                )))
            }
            RefinementKind::PropExistsR { propname, loc } => {
                let reason = mk_reason(
                    VirtualReasonDesc::RProperty(Some(Name::new(propname.dupe()))),
                    loc.dupe(),
                );
                Some(Predicate::new(PredicateInner::PropExistsP {
                    propname: propname.dupe(),
                    reason,
                }))
            }
            RefinementKind::ImpossibleR => Some(Predicate::new(PredicateInner::ImpossibleP)),
        })
    }
    pred(cx, kind)
}

fn refine<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    loc: ALoc,
    refi: Option<&Refinement<ALoc>>,
    res: PossiblyRefinedWriteState,
) -> Result<PossiblyRefinedWriteState, JobError> {
    use flow_common::reason::mk_reason;

    match refi {
        None => Ok(res),
        Some(refinement) => {
            let Refinement {
                refining_locs,
                kind,
            } = refinement;
            let PossiblyRefinedWriteState {
                t,
                errors,
                actually_refined_refining_locs,
            } = res;
            let predicate = predicate_of_refinement(cx, kind)?;
            let refined_reason = mk_reason(reason.desc(true).cloned(), loc);
            let (t, actually_refined_refining_locs_prime) = match predicate {
                None => (t, None),
                Some(predicate) => {
                    match predicate_kit::run_predicate_track_changes(
                        cx,
                        &t,
                        &predicate,
                        refined_reason,
                    )? {
                        predicate_kit::PredicateResult::TypeUnchanged(_) => (t, None),
                        predicate_kit::PredicateResult::TypeChanged(new_t) => {
                            (new_t, Some(refining_locs.dupe()))
                        }
                    }
                }
            };
            Ok(PossiblyRefinedWriteState {
                t,
                errors,
                actually_refined_refining_locs: merge_actually_refined_refining_locs((
                    actually_refined_refining_locs,
                    actually_refined_refining_locs_prime,
                )),
            })
        }
    }
}

fn possibly_refined_write_state_of_state<'cx>(
    lookup_mode: LookupMode,
    val_kind: ValKind,
    cx: &mut Context<'cx>,
    loc: ALoc,
    reason: Reason,
    write_locs: &[WriteLoc<ALoc>],
    val_id: Option<i32>,
    refi: Option<&Refinement<ALoc>>,
) -> Result<PossiblyRefinedWriteState, JobError> {
    fn base(t: Type, errors: Vec<CacheableEnvError<ALoc>>) -> PossiblyRefinedWriteState {
        PossiblyRefinedWriteState {
            t,
            errors,
            actually_refined_refining_locs: None,
        }
    }

    fn loop_fn<'cx>(
        lookup_mode: LookupMode,
        cx: &mut Context<'cx>,
        loc: ALoc,
        reason: Reason,
        states: &[WriteLoc<ALoc>],
        val_id: Option<i32>,
        refi: Option<&Refinement<ALoc>>,
    ) -> Result<PossiblyRefinedWriteState, JobError> {
        use flow_common::reason::mk_reason;
        use flow_typing_type::type_::AnySource;
        use flow_typing_type::type_::TypeInner;
        use flow_typing_type::type_::any_t;

        let find_write_exn = |cx: &mut Context<'cx>, kind: DefLocType, reason: &Reason| -> Type {
            checked_find_loc_env_write(cx, kind, reason.loc().dupe())
        };

        let compute_state =
            |cx: &mut Context<'cx>| {
                let mut mapped = Vec::with_capacity(states.len());
                for entry in states {
                    let mapped_entry = match (entry, lookup_mode) {
                    (WriteLoc::Undefined(r) | WriteLoc::Uninitialized(r), _) => {
                        base(flow_typing_type::type_::void::make(r.dupe()), vec![])
                    }
                    (WriteLoc::Number(r), _) => base(
                        flow_typing_type::type_::num_module_t::make(r.dupe()),
                        vec![],
                    ),
                    (WriteLoc::DeclaredFunction(dloc), _) => {
                        base(provider_type_for_def_loc(true, cx, dloc.dupe()), vec![])
                    }
                    (WriteLoc::Undeclared(_, def_loc), LookupMode::ForType) => base(
                        checked_find_loc_env_write(cx, DefLocType::OrdinaryNameLoc, def_loc.dupe()),
                        vec![],
                    ),
                    (
                        WriteLoc::Undeclared(name, def_loc),
                        LookupMode::ForValue | LookupMode::ForTypeof,
                    ) => base(
                        any_t::make(AnySource::AnyError(None), reason.dupe()),
                        vec![CacheableEnvError::ReferencedBeforeDeclaration {
                            name: name.dupe(),
                            def_loc: def_loc.dupe(),
                        }],
                    ),
                    (WriteLoc::EmptyArray { reason: wr, .. } | WriteLoc::Write(wr), _) => {
                        flow_typing_debug::verbose::print_if_verbose_lazy(
                            cx,
                            None,
                            None,
                            None,
                            || {
                                vec![format!(
                                    "reading {} from location {}",
                                    flow_common::reason::string_of_aloc(None, &loc),
                                    flow_common::reason::string_of_aloc(None, wr.loc()),
                                )]
                            },
                        );
                        base(find_write_exn(cx, DefLocType::OrdinaryNameLoc, wr), vec![])
                    }
                    (WriteLoc::IllegalWrite(wr), _) => {
                        flow_typing_debug::verbose::print_if_verbose_lazy(
                            cx,
                            None,
                            None,
                            None,
                            || {
                                vec![format!(
                                    "reading {} from illegal write location {}",
                                    flow_common::reason::string_of_aloc(None, &loc),
                                    flow_common::reason::string_of_aloc(None, wr.loc()),
                                )]
                            },
                        );
                        base(any_t::make(AnySource::AnyError(None), wr.dupe()), vec![])
                    }
                    (
                        WriteLoc::Refinement {
                            refinement_id,
                            writes,
                            write_id,
                        },
                        _,
                    ) => {
                        let new_refi = {
                            let env = cx.environment();
                            Some(find_refi(&env.var_info, *refinement_id))
                        };
                        loop_fn(
                            lookup_mode,
                            cx,
                            loc.dupe(),
                            reason.dupe(),
                            writes,
                            *write_id,
                            new_refi.as_ref(),
                        )?
                    }
                    (WriteLoc::Global(name), LookupMode::ForValue | LookupMode::ForTypeof) => {
                        match flow_typing_flow_common::flow_js_utils::lookup_builtin_value_result(
                            cx,
                            name.as_str(),
                            reason.dupe(),
                        ) {
                            Ok(t) => base(t, vec![]),
                            Err((t, errs)) => base(t, errs),
                        }
                    }
                    (WriteLoc::Global(name), LookupMode::ForType) => {
                        match flow_typing_flow_common::flow_js_utils::lookup_builtin_type_result(
                            cx,
                            name.as_str(),
                            reason.dupe(),
                        ) {
                            Ok(t) => base(t, vec![]),
                            Err((t, errs)) => base(t, errs),
                        }
                    }
                    (WriteLoc::GlobalThis(r), _) => {
                        base(Type::new(TypeInner::ObjProtoT(r.dupe())), vec![])
                    }
                    (WriteLoc::IllegalThis(wr), _) => {
                        flow_typing_debug::verbose::print_if_verbose(
                            cx,
                            None,
                            None,
                            None,
                            vec![format!(
                                "reading illegal this({}) from location {}",
                                flow_common::reason::string_of_aloc(None, &loc),
                                flow_common::reason::string_of_aloc(None, wr.loc()),
                            )],
                        );
                        base(any_t::make(AnySource::AnyError(None), wr.dupe()), vec![])
                    }
                    (WriteLoc::FunctionThis(wr), _) => {
                        flow_typing_debug::verbose::print_if_verbose(
                            cx,
                            None,
                            None,
                            None,
                            vec![format!(
                                "reading function this({}) from location {}",
                                flow_common::reason::string_of_aloc(None, &loc),
                                flow_common::reason::string_of_aloc(None, wr.loc()),
                            )],
                        );
                        base(find_write_exn(cx, DefLocType::FunctionThisLoc, wr), vec![])
                    }
                    (WriteLoc::ClassInstanceThis(wr), _) => {
                        flow_typing_debug::verbose::print_if_verbose(
                            cx,
                            None,
                            None,
                            None,
                            vec![format!(
                                "reading instance this({}) from location {}",
                                flow_common::reason::string_of_aloc(None, &loc),
                                flow_common::reason::string_of_aloc(None, wr.loc()),
                            )],
                        );
                        base(find_write_exn(cx, DefLocType::ClassInstanceThisLoc, wr), vec![])
                    }
                    (WriteLoc::ClassStaticThis(wr), _) => {
                        flow_typing_debug::verbose::print_if_verbose(
                            cx,
                            None,
                            None,
                            None,
                            vec![format!(
                                "reading static this({}) from location {}",
                                flow_common::reason::string_of_aloc(None, &loc),
                                flow_common::reason::string_of_aloc(None, wr.loc()),
                            )],
                        );
                        base(find_write_exn(cx, DefLocType::ClassStaticThisLoc, wr), vec![])
                    }
                    (WriteLoc::ClassInstanceSuper(wr), _) => {
                        flow_typing_debug::verbose::print_if_verbose(
                            cx,
                            None,
                            None,
                            None,
                            vec![format!(
                                "reading instance super({}) from location {}",
                                flow_common::reason::string_of_aloc(None, &loc),
                                flow_common::reason::string_of_aloc(None, wr.loc()),
                            )],
                        );
                        base(
                            find_write_exn(cx, DefLocType::ClassInstanceSuperLoc, wr),
                            vec![],
                        )
                    }
                    (WriteLoc::ClassStaticSuper(wr), _) => {
                        flow_typing_debug::verbose::print_if_verbose(
                            cx,
                            None,
                            None,
                            None,
                            vec![format!(
                                "reading {} from illegal write location {}",
                                flow_common::reason::string_of_aloc(None, &loc),
                                flow_common::reason::string_of_aloc(None, wr.loc()),
                            )],
                        );
                        base(find_write_exn(cx, DefLocType::ClassStaticSuperLoc, wr), vec![])
                    }
                    (WriteLoc::ModuleScoped(_), _) => {
                        base(any_t::at(AnySource::AnnotatedAny, loc.dupe()), vec![])
                    }
                    (WriteLoc::Unreachable(uloc), _) => {
                        let r = mk_reason(None, uloc.dupe());
                        base(flow_typing_type::type_::empty_t::make(r), vec![])
                    }
                    (WriteLoc::Projection(ploc), _) => base(
                        checked_find_loc_env_write(cx, DefLocType::OrdinaryNameLoc, ploc.dupe()),
                        vec![],
                    ),
                };
                    mapped.push(mapped_entry);
                }
                phi(cx, reason.dupe(), mapped)
            };

        let state = match val_id {
            Some(id) => {
                if !matches!(cx.typing_mode(), TypingMode::CheckingMode) {
                    compute_state(cx)?
                } else {
                    let for_value =
                        matches!(lookup_mode, LookupMode::ForValue | LookupMode::ForTypeof);
                    match cx.env_cache_find_opt(for_value, id) {
                        None => {
                            let state = compute_state(cx)?;
                            cx.add_env_cache_entry(for_value, id, state.clone());
                            state
                        }
                        Some(state) => state,
                    }
                }
            }
            None => compute_state(cx)?,
        };
        refine(cx, reason, loc, refi, state)
    }

    let state = loop_fn(
        lookup_mode,
        cx,
        loc.dupe(),
        reason,
        write_locs,
        val_id,
        refi,
    )?;
    tvar_resolver::resolve(cx, tvar_resolver::default_no_lowers, true, &state.t);
    if let Some(ref locs) = state.actually_refined_refining_locs {
        if val_kind != ValKind::Internal {
            cx.add_refined_location(loc, locs.clone());
        }
    }
    Ok(state)
}

fn type_of_state<'cx>(
    lookup_mode: LookupMode,
    val_kind: ValKind,
    cx: &mut Context<'cx>,
    loc: ALoc,
    reason: Reason,
    write_locs: &[WriteLoc<ALoc>],
    val_id: Option<i32>,
    refi: Option<&Refinement<ALoc>>,
) -> Result<Type, JobError> {
    let PossiblyRefinedWriteState {
        t,
        errors,
        actually_refined_refining_locs: _,
    } = possibly_refined_write_state_of_state(
        lookup_mode,
        val_kind,
        cx,
        loc.dupe(),
        reason,
        write_locs,
        val_id,
        refi,
    )?;
    for err in errors {
        flow_typing_flow_common::flow_js_utils::emit_cacheable_env_error(cx, loc.dupe(), err);
    }
    Ok(t)
}

fn read_entry<'cx>(
    lookup_mode: LookupMode,
    cx: &mut Context<'cx>,
    loc: ALoc,
    reason: Reason,
) -> Result<Result<Type, ALoc>, JobError> {
    use flow_common::reason::Name;
    use flow_typing_errors::error_message::BindingError;
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_type::type_::AnySource;
    use flow_typing_type::type_::any_t;

    let read = {
        let env = cx.environment();
        let var_info = &env.var_info;
        match find_var_opt(var_info, &loc) {
            Err(loc) => Err(loc),
            Ok(read) => Ok(read.clone()),
        }
    };
    Ok(match read {
        Err(loc) => Err(loc),
        Ok(EnvRead {
            def_loc,
            write_locs,
            val_kind,
            name,
            id,
        }) => match (&val_kind, &name, &def_loc, lookup_mode) {
            (ValKind::TypeParam, Some(name), Some(def_loc), LookupMode::ForType)
                if write_locs
                    .iter()
                    .any(|write| matches!(write, WriteLoc::Uninitialized(_))) =>
            {
                flow_js::add_output_non_speculating(
                    cx,
                    ErrorMessage::EBindingError(Box::new((
                        BindingError::EReferencedBeforeDeclaration,
                        loc.dupe(),
                        Name::new(name.dupe()),
                        def_loc.dupe(),
                    ))),
                );
                Ok(any_t::at(AnySource::AnyError(None), loc))
            }
            (
                ValKind::TypeParam,
                Some(name),
                Some(def_loc),
                LookupMode::ForValue | LookupMode::ForTypeof,
            ) => {
                flow_js::add_output_non_speculating(
                    cx,
                    ErrorMessage::EBindingError(Box::new((
                        BindingError::ETypeInValuePosition {
                            imported: false,
                            type_only_namespace: false,
                            name: name.dupe(),
                        },
                        loc.dupe(),
                        Name::new(name.dupe()),
                        def_loc.dupe(),
                    ))),
                );
                Ok(any_t::at(AnySource::AnyError(None), loc))
            }
            (
                ValKind::Type {
                    imported,
                    type_only_namespace,
                },
                Some(name),
                Some(def_loc),
                LookupMode::ForValue,
            ) => {
                flow_js::add_output_non_speculating(
                    cx,
                    ErrorMessage::EBindingError(Box::new((
                        BindingError::ETypeInValuePosition {
                            imported: *imported,
                            type_only_namespace: *type_only_namespace,
                            name: name.dupe(),
                        },
                        loc.dupe(),
                        Name::new(name.dupe()),
                        def_loc.dupe(),
                    ))),
                );
                Ok(any_t::at(AnySource::AnyError(None), loc))
            }
            (
                ValKind::Type {
                    imported: _,
                    type_only_namespace: true,
                },
                Some(_),
                Some(_),
                LookupMode::ForTypeof,
            ) => {
                let t = type_of_state(
                    lookup_mode,
                    val_kind,
                    cx,
                    loc,
                    reason,
                    &write_locs,
                    id,
                    None,
                )?;
                Ok(t)
            }
            (
                ValKind::Type {
                    imported,
                    type_only_namespace,
                },
                Some(name),
                Some(def_loc),
                LookupMode::ForTypeof,
            ) => {
                flow_js::add_output_non_speculating(
                    cx,
                    ErrorMessage::EBindingError(Box::new((
                        BindingError::ETypeInValuePosition {
                            imported: *imported,
                            type_only_namespace: *type_only_namespace,
                            name: name.dupe(),
                        },
                        loc.dupe(),
                        Name::new(name.dupe()),
                        def_loc.dupe(),
                    ))),
                );
                Ok(any_t::at(AnySource::AnyError(None), loc))
            }
            (
                ValKind::TsImport,
                Some(name),
                Some(def_loc),
                LookupMode::ForValue | LookupMode::ForTypeof,
            ) => {
                let t = type_of_state(
                    lookup_mode,
                    val_kind,
                    cx,
                    loc.dupe(),
                    reason.dupe(),
                    &write_locs,
                    id,
                    None,
                )?;
                let ts_import_resolved_to_type_only = match cx.find_resolved(&t) {
                    Some(resolved) => {
                        use std::ops::Deref;

                        use flow_typing_type::type_::DefTInner;
                        use flow_typing_type::type_::PolyTData;
                        use flow_typing_type::type_::TypeInner;
                        match resolved.deref() {
                            TypeInner::DefT(_, def_t) => match def_t.deref() {
                                DefTInner::ClassT(_) => false,
                                DefTInner::PolyT(box PolyTData { t_out, .. })
                                    if matches!(t_out.deref(), TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::ClassT(_))) =>
                                {
                                    false
                                }
                                DefTInner::EnumObjectT { .. } => false,
                                DefTInner::ReactAbstractComponentT(_) => false,
                                _ => {
                                    flow_typing_flow_common::flow_js_utils::import_type_t_kit::canonicalize_imported_type(cx, reason.dupe(), &resolved).is_some()
                                }
                            },
                            TypeInner::NamespaceT(_) => {
                                flow_typing_flow_common::flow_js_utils::import_export_utils::is_ts_import_type_only(cx, def_loc)
                            }
                            _ => {
                                flow_typing_flow_common::flow_js_utils::import_type_t_kit::canonicalize_imported_type(cx, reason.dupe(), &resolved).is_some()
                            }
                        }
                    }
                    None => false,
                };
                if ts_import_resolved_to_type_only {
                    flow_js::add_output_non_speculating(
                        cx,
                        ErrorMessage::EBindingError(Box::new((
                            BindingError::ETypeInValuePosition {
                                imported: false,
                                type_only_namespace: false,
                                name: name.dupe(),
                            },
                            loc.dupe(),
                            Name::new(name.dupe()),
                            def_loc.dupe(),
                        ))),
                    );
                    Ok(any_t::at(AnySource::AnyError(None), loc))
                } else {
                    Ok(t)
                }
            }
            _ => Ok(type_of_state(
                lookup_mode,
                val_kind,
                cx,
                loc,
                reason,
                &write_locs,
                id,
                None,
            )?),
        },
    })
}

fn read_entry_exn<'cx>(
    lookup_mode: LookupMode,
    cx: &mut Context<'cx>,
    loc: ALoc,
    reason: Reason,
) -> Result<Type, JobError> {
    use flow_typing_type::type_::AnySource;
    use flow_typing_type::type_::any_t;
    let loc_clone = loc.dupe();
    let loc_for_fallback = loc.dupe();
    with_debug_exn_error(
        cx,
        loc,
        move |cx| match read_entry(lookup_mode, cx, loc_clone.dupe(), reason)? {
            Err(err_loc) => panic!(
                "LocEnvEntryNotFound {}",
                flow_common::reason::string_of_aloc(None, &err_loc)
            ),
            Ok(x) => Ok(x),
        },
        move || Ok(any_t::at(AnySource::AnyError(None), loc_for_fallback)),
    )
}

pub fn read_to_predicate<'cx>(
    cx: &mut Context<'cx>,
    read: &EnvRead<ALoc>,
) -> Result<Option<Predicate>, JobError> {
    use flow_typing_type::type_::PredicateInner;

    let var_info = {
        let env = cx.environment();
        env.var_info.dupe()
    };
    let mut predicates: Vec<Predicate> = Vec::new();
    for wl in read.write_locs.iter() {
        if let WriteLoc::Refinement {
            refinement_id,
            writes: _,
            write_id: _,
        } = wl
        {
            let refi = find_refi(&var_info, *refinement_id);
            if let Some(p) = predicate_of_refinement(cx, &refi.kind)? {
                predicates.push(p);
            }
        }
    }
    if predicates.is_empty() {
        Ok(None)
    } else {
        let mut iter = predicates.into_iter();
        let first = iter.next().unwrap();
        let result = iter.fold(first, |acc, p| Predicate::new(PredicateInner::OrP(acc, p)));
        Ok(Some(result))
    }
}

pub fn checked_type_guard_at_return<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    param_loc: ALoc,
    return_loc: ALoc,
    pos_write_locs: &[WriteLoc<ALoc>],
    neg_refi: &EnvRead<ALoc>,
) -> Result<Result<(Type, Option<Predicate>), Vec<ALoc>>, JobError> {
    match checked_type_guard_param_at_return(cx, reason, param_loc, return_loc, pos_write_locs)? {
        Ok(t) => Ok(Ok((t, read_to_predicate(cx, neg_refi)?))),
        Err(invalid_writes) => Ok(Err(invalid_writes)),
    }
}

/// The type of a type guard parameter at a return point, or the locations of
/// any writes that reassign it before that point.
pub fn checked_type_guard_param_at_return<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    param_loc: ALoc,
    return_loc: ALoc,
    write_locs: &[WriteLoc<ALoc>],
) -> Result<Result<Type, Vec<ALoc>>, JobError> {
    fn is_invalid(
        param_loc: &ALoc,
        acc: (bool, Vec<ALoc>),
        write_loc: &WriteLoc<ALoc>,
    ) -> (bool, Vec<ALoc>) {
        let (acc_result, mut acc_locs) = acc;
        match write_loc {
            WriteLoc::Write(r) if r.loc() == param_loc => (acc_result, acc_locs),
            // `this` is never assignable, so its binding write is always the one
            // the type guard refers to. Only non-static methods can carry a `this`
            // type guard, so this is the only `this` write that can appear here.
            WriteLoc::ClassInstanceThis(_) => (acc_result, acc_locs),
            WriteLoc::Refinement { writes, .. } => {
                writes.iter().fold((acc_result, acc_locs), |acc, wl| {
                    is_invalid(param_loc, acc, wl)
                })
            }
            WriteLoc::Write(r) => {
                acc_locs.push(r.loc().dupe());
                (true, acc_locs)
            }
            _ => (true, acc_locs),
        }
    }
    let (is_invalid_result, invalid_writes) = write_locs
        .iter()
        .fold((false, vec![]), |acc, wl| is_invalid(&param_loc, acc, wl));
    if is_invalid_result {
        return Ok(Err(invalid_writes));
    }
    let t = type_of_state(
        LookupMode::ForValue,
        ValKind::Internal,
        cx,
        return_loc,
        reason,
        write_locs,
        None,
        None,
    )?;
    Ok(Ok(t))
}

pub fn inferred_type_guard_at_return<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    return_loc: ALoc,
    write_locs: &[WriteLoc<ALoc>],
) -> Result<Type, JobError> {
    type_of_state(
        LookupMode::ForValue,
        ValKind::Internal,
        cx,
        return_loc,
        reason,
        write_locs,
        None,
        None,
    )
}

pub fn ref_entry_exn<'cx>(
    lookup_mode: LookupMode,
    cx: &mut Context<'cx>,
    loc: ALoc,
    reason: Reason,
) -> Result<Type, JobError> {
    let t = read_entry_exn(lookup_mode, cx, loc.dupe(), reason)?;
    flow_js::reposition_non_speculating(cx, loc, t)
}

pub fn find_write<'cx>(cx: &mut Context<'cx>, kind: DefLocType, reason: Reason) -> Type {
    let loc = reason.loc().dupe();
    match checked_find_loc_env_write_opt(cx, kind, loc) {
        Some(t) => t,
        None => flow_typing_type::type_::any_t::error(reason),
    }
}

pub fn get_refinement<'cx>(
    cx: &mut Context<'cx>,
    desc: impl Into<Option<ReasonDesc>>,
    loc: ALoc,
) -> Result<Option<Type>, JobError> {
    let reason = flow_common::reason::mk_reason(desc, loc.dupe());
    match read_entry(LookupMode::ForValue, cx, loc.dupe(), reason)? {
        Ok(x) => Ok(Some(flow_js::reposition_non_speculating(
            cx,
            loc.dupe(),
            x,
        )?)),
        Err(_) => {
            let env = cx.environment();
            match env.var_info.env_refinement_invalidation_info.get(&loc) {
                None => {}
                Some(info) => cx.add_aggressively_invalidated_location(loc, info.clone()),
            }
            Ok(None)
        }
    }
}

pub fn get_var<'cx>(
    lookup_mode: Option<LookupMode>,
    cx: &mut Context<'cx>,
    name: &str,
    loc: ALoc,
) -> Result<Type, JobError> {
    let lookup_mode = lookup_mode.unwrap_or(LookupMode::ForValue);
    read_entry_exn(
        lookup_mode,
        cx,
        loc.dupe(),
        flow_common::reason::mk_reason(VirtualReasonDesc::RIdentifier(name.into()), loc),
    )
}

pub fn query_var<'cx>(
    lookup_mode: Option<LookupMode>,
    cx: &mut Context<'cx>,
    name: &FlowSmolStr,
    desc: Option<ReasonDesc>,
    loc: ALoc,
) -> Result<Type, JobError> {
    let desc = match desc {
        Some(d) => d,
        None => VirtualReasonDesc::RIdentifier(name.dupe()),
    };
    let lookup_mode = lookup_mode.unwrap_or(LookupMode::ForValue);
    read_entry_exn(
        lookup_mode,
        cx,
        loc.dupe(),
        flow_common::reason::mk_reason(desc, loc),
    )
}

pub fn lookup_type_identifier<'cx>(
    lookup_mode: LookupMode,
    cx: &mut Context<'cx>,
    name: &FlowSmolStr,
    loc: ALoc,
) -> Result<Type, JobError> {
    fn is_unresolved_read_in_declare_namespace<'cx>(cx: &mut Context<'cx>, loc: &ALoc) -> bool {
        let env = cx.environment();
        if env.var_info.declare_namespace_read_paths.get(loc).is_none() {
            return false;
        }
        match find_var_opt(&env.var_info, loc) {
            Err(_) => true,
            Ok(read) => {
                !read.write_locs.is_empty()
                    && read
                        .write_locs
                        .iter()
                        .all(|write| matches!(write, WriteLoc::Global(_)))
            }
        }
    }

    fn query_var_opt<'cx>(
        lookup_mode: LookupMode,
        cx: &mut Context<'cx>,
        name: &FlowSmolStr,
        loc: ALoc,
    ) -> Result<Option<Type>, JobError> {
        if is_unresolved_read_in_declare_namespace(cx, &loc) {
            return Ok(None);
        }
        let reason =
            flow_common::reason::mk_reason(VirtualReasonDesc::RIdentifier(name.dupe()), loc.dupe());
        match read_entry(lookup_mode, cx, loc, reason)? {
            Ok(t) => Ok(Some(t)),
            Err(_) => Ok(None),
        }
    }

    fn flowlib_namespace_member_global_lookup<'cx>(
        lookup_mode: LookupMode,
        cx: &mut Context<'cx>,
        name: &FlowSmolStr,
        loc: ALoc,
    ) -> Option<Type> {
        if cx
            .file()
            .as_str()
            .starts_with(flow_parser::file_key::FLOWLIB_MARKER)
            && is_unresolved_read_in_declare_namespace(cx, &loc)
        {
            let reason =
                flow_common::reason::mk_reason(VirtualReasonDesc::RIdentifier(name.dupe()), loc);
            match lookup_mode {
                LookupMode::ForType => {
                    Some(flow_typing_flow_common::flow_js_utils::lookup_builtin_type(
                        cx,
                        name.as_str(),
                        reason,
                    ))
                }
                LookupMode::ForValue | LookupMode::ForTypeof => Some(
                    flow_typing_flow_common::flow_js_utils::lookup_builtin_value(
                        cx,
                        name.as_str(),
                        reason,
                    ),
                ),
            }
        } else {
            None
        }
    }

    if let Some(t) = query_var_opt(lookup_mode, cx, name, loc.dupe())? {
        return Ok(t);
    }
    if let Some(t) = recorded_declare_namespace_member_opt(lookup_mode, cx, name, loc.dupe()) {
        return Ok(t);
    }
    if let Some(t) = flowlib_namespace_member_global_lookup(lookup_mode, cx, name, loc.dupe()) {
        return Ok(t);
    }
    let t = query_var(Some(lookup_mode), cx, name, None, loc.dupe())?;
    Ok(t)
}

pub fn intrinsic_ref<'cx>(
    cx: &mut Context<'cx>,
    desc: Option<ReasonDesc>,
    name: &FlowSmolStr,
    loc: ALoc,
) -> Result<Option<(Type, ALoc)>, JobError> {
    let desc = match desc {
        Some(d) => d,
        None => VirtualReasonDesc::RIdentifier(name.dupe()),
    };
    let reason = flow_common::reason::mk_reason(desc, loc.dupe());
    let read = {
        let env = cx.environment();
        let var_info = &env.var_info;
        match find_var_opt(var_info, &loc) {
            Err(loc) => Err(loc),
            Ok(read) => Ok(read.clone()),
        }
    };
    match read {
        Err(_) => Ok(None),
        Ok(EnvRead {
            def_loc,
            write_locs,
            val_kind,
            name,
            id,
        }) => match (val_kind, name, def_loc) {
            (ValKind::Type { .. }, Some(_), Some(_)) => Ok(None),
            (_, _, None) => Ok(None),
            (_, _, Some(def_loc)) => {
                let PossiblyRefinedWriteState { t, .. } = possibly_refined_write_state_of_state(
                    LookupMode::ForValue,
                    val_kind,
                    cx,
                    loc.dupe(),
                    reason,
                    &write_locs,
                    id,
                    None,
                )?;
                Ok(Some((
                    flow_js::reposition_non_speculating(cx, loc, t)?,
                    def_loc,
                )))
            }
        },
    }
}

pub fn var_ref<'cx>(
    lookup_mode: Option<LookupMode>,
    cx: &mut Context<'cx>,
    desc: Option<ReasonDesc>,
    name: &FlowSmolStr,
    loc: ALoc,
) -> Result<Type, JobError> {
    let lookup_mode = lookup_mode.unwrap_or(LookupMode::ForValue);
    let t = query_var(Some(lookup_mode), cx, name, desc, loc.dupe())?;
    flow_js::reposition_non_speculating(cx, loc, t)
}

pub fn sig_var_ref<'cx>(
    lookup_mode: Option<LookupMode>,
    cx: &mut Context<'cx>,
    desc: Option<ReasonDesc>,
    name: &FlowSmolStr,
    loc: ALoc,
) -> Result<Type, JobError> {
    let lookup_mode = lookup_mode.unwrap_or(LookupMode::ForValue);
    let desc = match desc {
        Some(d) => d,
        None => VirtualReasonDesc::RIdentifier(name.dupe()),
    };
    let reason = flow_common::reason::mk_reason(desc, loc.dupe());
    let t = query_var(Some(lookup_mode), cx, name, None, loc)?;
    Ok(Type::new(flow_typing_type::type_::TypeInner::AnnotT(
        reason, t, true,
    )))
}

pub fn read_class_self_type<'cx>(cx: &mut Context<'cx>, loc: ALoc) -> Type {
    match checked_find_loc_env_write_opt(cx, DefLocType::ClassSelfLoc, loc.dupe()) {
        Some(t) => t,
        None => {
            // When checked_find_loc_env_write_opt returns None, we are reading an unreachable entry
            flow_typing_type::type_::empty_t::why(flow_common::reason::mk_reason(None, loc))
        }
    }
}

pub fn is_global_var<'cx>(cx: &Context<'cx>, loc: ALoc) -> bool {
    let env = cx.environment();
    match find_var_opt(&env.var_info, &loc) {
        Ok(read) => flow_env_builder::env_api::is_global_var(read),
        Err(_) => false,
    }
}

pub fn local_scope_entry_exists<'cx>(cx: &mut Context<'cx>, loc: ALoc) -> bool {
    !is_global_var(cx, loc)
}

pub fn has_var_read<'cx>(cx: &mut Context<'cx>, loc: &ALoc) -> bool {
    let env = cx.environment();
    find_var_opt(&env.var_info, loc).is_ok()
}

pub fn get_var_declared_type<'cx>(
    lookup_mode: Option<LookupMode>,
    is_declared_function: Option<bool>,
    cx: &mut Context<'cx>,
    loc: ALoc,
) -> Type {
    let lookup_mode = lookup_mode.unwrap_or(LookupMode::ForValue);
    let is_declared_function = is_declared_function.unwrap_or(false);

    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_errors::error_message::InternalError;
    use flow_typing_type::type_::AnySource;
    use flow_typing_type::type_::any_t;

    match lookup_mode {
        LookupMode::ForType => {
            match checked_find_loc_env_write_opt(cx, DefLocType::OrdinaryNameLoc, loc.dupe()) {
                Some(t) => t,
                None => {
                    flow_js::add_output_non_speculating(
                        cx,
                        ErrorMessage::EInternal(Box::new((
                            loc.dupe(),
                            InternalError::MissingEnvWrite(loc.dupe()),
                        ))),
                    );
                    any_t::at(AnySource::AnyError(None), loc)
                }
            }
        }
        _ => provider_type_for_def_loc(is_declared_function, cx, loc),
    }
}

pub fn constraining_type<'cx>(
    default: Type,
    cx: &mut Context<'cx>,
    name: &FlowSmolStr,
    loc: ALoc,
) -> Type {
    let env = cx.environment();
    let var_info = &env.var_info;

    match var_info.env_entries.get_ordinary(&loc) {
        Some(EnvEntry::NonAssigningWrite) => default,
        _ => {
            let providers: Option<Vec<Type>> = find_providers(var_info, &loc)
                .into_iter()
                .map(|ploc| checked_find_loc_env_write_opt(cx, DefLocType::OrdinaryNameLoc, ploc))
                .collect();
            match providers {
                None => default,
                Some(ps) if ps.is_empty() => default,
                Some(ps) if ps.len() == 1 => ps.into_iter().next().unwrap(),
                Some(ps) => {
                    let mut iter = ps.into_iter();
                    let t1 = iter.next().unwrap();
                    let t2 = iter.next().unwrap();
                    let ts: Vec<Type> = iter.collect();
                    use flow_typing_type::type_::TypeInner;
                    use flow_typing_type::type_::union_rep;
                    Type::new(TypeInner::UnionT(
                        flow_common::reason::mk_reason(
                            VirtualReasonDesc::RIdentifier(name.dupe()),
                            loc,
                        ),
                        union_rep::make(
                            None,
                            union_rep::UnionKind::ProvidersKind,
                            t1,
                            t2,
                            ts.into(),
                        ),
                    ))
                }
            }
        }
    }
}

// *************
// *  Writing  *
// *************

// Subtypes the given type against the providers for a def loc. Should be used on assignments to
// non-import value bindings
fn subtype_against_providers<'cx>(
    cx: &mut Context<'cx>,
    use_op: &UseOp,
    potential_global_name: Option<&str>,
    t: &Type,
    loc: ALoc,
) -> Result<(), JobError> {
    use flow_typing_errors::error_message::EBuiltinNameLookupFailedData;
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_type::type_::UseT;
    use flow_typing_type::type_::UseTInner;
    use flow_typing_type::type_::VirtualFrameUseOp;
    use flow_typing_type::type_::VirtualUseOp;

    let var_info = cx.environment().var_info.dupe();
    let var_info = &*var_info;
    let providers = &var_info.providers;
    let scopes = &var_info.scopes;
    // We only perform a subtyping check if this is an assigning write. We call
    // writes to immutable bindings non-assigning writes. For example:
    // const x: number = 3;
    // x = 'string';
    //
    // Since the x = 'string' doesn't actually assign a value to x, we should
    // not perform a subtyping check and a second error saying string is incompatible
    // with number. We should only emit an error saying that a const cannot be reassigned.
    match var_info.env_entries.get_ordinary(&loc) {
        Some(EnvEntry::NonAssigningWrite) => Ok(()),
        Some(EnvEntry::GlobalWrite(_)) => {
            if is_provider(cx, loc.dupe())
                && let Some(name) = potential_global_name
                && cx.builtin_value_opt(name).is_none()
            {
                flow_js::add_output_non_speculating(
                    cx,
                    ErrorMessage::EBuiltinNameLookupFailed(Box::new(
                        EBuiltinNameLookupFailedData {
                            loc,
                            name: name.into(),
                        },
                    )),
                );
            }
            Ok(())
        }
        _ => {
            if !is_provider(cx, loc.dupe()) {
                let general = provider_type_for_def_loc(false, cx, loc.dupe());
                if is_def_loc_annotated(var_info, &loc) {
                    flow_js::flow_non_speculating(
                        cx,
                        (t, &UseT::new(UseTInner::UseT(use_op.clone(), general))),
                    )
                } else {
                    let use_op = match scopes.def_of_use_opt(&loc) {
                        Some(def) => {
                            let declaration = def.locs.first().dupe();
                            let name = def.actual_name.dupe();
                            let provider_locs =
                                match provider_api::Info::providers_of_def(providers, &loc) {
                                    Some(def_providers) => def_providers.providers.clone(),
                                    None => vec![],
                                };
                            VirtualUseOp::Frame(
                                Arc::new(VirtualFrameUseOp::ConstrainedAssignment(Box::new(
                                    ConstrainedAssignmentData {
                                        name,
                                        declaration,
                                        providers: provider_locs
                                            .iter()
                                            .map(|p| p.reason.loc().dupe())
                                            .collect(),
                                    },
                                ))),
                                Arc::new(use_op.dupe()),
                            )
                        }
                        None => use_op.dupe(),
                    };
                    cx.add_post_inference_subtyping_check(t.dupe(), use_op, general);
                    Ok(())
                }
            } else {
                Ok(())
            }
        }
    }
}

pub fn make_env_entries_under_resolution<'cx>(cx: &mut Context<'cx>, entries: EnvSet<ALoc>) {
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_errors::error_message::InternalError;
    use flow_typing_type::type_::any_t;
    use flow_typing_type::type_util;

    let env = cx.environment_mut();
    let update = |env: &mut LocEnv<Cx>, key: &EnvKey<ALoc>| {
        let def_loc_kind = key.def_loc_type;
        let loc = key.loc.dupe();
        match env.types.get(key) {
            None => {}
            Some(type_entry) => {
                let t = type_entry.t.dupe();
                let reason = type_util::reason_of_t(&t);
                let reason_clone = reason.dupe();
                let loc_clone = loc.dupe();
                let new_state: flow_lazy::Lazy<Cx, Type> =
                    flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                        flow_js::add_output_non_speculating(
                            cx,
                            ErrorMessage::EInternal(Box::new((
                                loc_clone,
                                InternalError::ForcedReadOfUnderResolutionTvar(def_loc_kind),
                            ))),
                        );
                        any_t::error(reason_clone)
                    });
                env.update_state(def_loc_kind, loc, new_state);
            }
        }
    };
    for key in entries.iter() {
        update(env, key);
    }
}

// Resolve `t` with the entry in the loc_env's map. This allows it to be looked up for Write
// entries reported by the name_resolver as well as providers for the provider analysis
pub fn resolve_env_entry<'cx>(
    cx: &mut Context<'cx>,
    t: Type,
    kind: DefLocType,
    add_array_or_object_literal_declaration_tracking: bool,
    loc: ALoc,
) {
    use std::ops::Deref;

    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_errors::error_message::InternalError;
    use flow_typing_flow_common::flow_js_utils;
    use flow_typing_type::type_::DefT as DefTType;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::TypeInner;
    flow_typing_debug::verbose::print_if_verbose(
        cx,
        None,
        None,
        None,
        vec![format!(
            "writing to {:?} {}",
            kind,
            loc.debug_to_string(true)
        )],
    );
    let key = EnvKey::new(kind, loc.dupe());
    let (env_entry, type_entry) = {
        let env = cx.environment();
        (
            env.var_info.env_entries.get(&key).cloned(),
            env.find_write(kind, loc.dupe()).cloned(),
        )
    };
    match (env_entry, type_entry) {
        (Some(EnvEntry::NonAssigningWrite), _) => {}
        (_, None) => {
            flow_js::add_output_non_speculating(
                cx,
                ErrorMessage::EInternal(Box::new((
                    loc.dupe(),
                    InternalError::MissingEnvWrite(loc),
                ))),
            );
        }
        (_, Some(type_entry)) => {
            let existing_t = type_entry.t;
            if add_array_or_object_literal_declaration_tracking {
                if let TypeInner::OpenT(tvar) = existing_t.deref() {
                    cx.add_array_or_object_literal_declaration_tracking(
                        tvar.id() as i32,
                        loc.dupe(),
                    );
                }
            }
            tvar_resolver::resolve(cx, tvar_resolver::default_no_lowers, true, &t);
            let t_clone = t.dupe();
            let new_state: flow_lazy::Lazy<Cx, Type> =
                flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                    // Unwrap possible OpenT so that OpenT doesn't wrap another OpenT.
                    // This has to be done lazily, so that we don't force tvars until we
                    // resolved all entries in a component.
                    match t_clone.deref() {
                        TypeInner::OpenT(tvar) => {
                            let r = tvar.reason();
                            let id = tvar.id() as i32;
                            flow_js_utils::merge_tvar(
                                cx,
                                false,
                                |_cx, r| {
                                    Type::new(TypeInner::DefT(
                                        r.dupe(),
                                        DefTType::new(DefTInner::EmptyT),
                                    ))
                                },
                                r,
                                id,
                            )
                        }
                        _ => t_clone.dupe(),
                    }
                });
            cx.environment_mut().update_state(kind, loc, new_state);
        }
    }
}

fn subtype_entry<'cx>(
    cx: &mut Context<'cx>,
    use_op: &UseOp,
    t: &Type,
    loc: ALoc,
) -> Result<(), JobError> {
    use flow_typing_type::type_::UseT;
    use flow_typing_type::type_::UseTInner;

    match checked_find_loc_env_write_opt(cx, DefLocType::OrdinaryNameLoc, loc.dupe()) {
        None => {
            // If we don't see a spot for this write it is because the annotated
            // binding being looked up here is one that caused a redeclaration
            // error
            assert!(matches!(
                cx.environment().var_info.env_entries.get_ordinary(&loc),
                Some(EnvEntry::NonAssigningWrite)
            ));
            Ok(())
        }
        Some(w) => {
            flow_js::flow_non_speculating(cx, (t, &UseT::new(UseTInner::UseT(use_op.clone(), w))))
        }
    }
}

// init_entry is called on variable declarations (not assignments), and `t`
// is the RHS type. If the variable is annotated, we just need to check t against
// its type; but if it's not annotated, the RHS t becomes the variable's type.
fn init_entry<'cx>(
    cx: &mut Context<'cx>,
    use_op: &UseOp,
    t: &Type,
    loc: ALoc,
) -> Result<(), JobError> {
    let env = cx.environment();
    let var_info = &env.var_info;
    if is_def_loc_annotated(var_info, &loc) {
        subtype_entry(cx, use_op, t, loc)
    } else {
        Ok(())
    }
}

pub fn set_var<'cx>(
    cx: &mut Context<'cx>,
    use_op: &UseOp,
    name: &str,
    t: &Type,
    loc: ALoc,
) -> Result<(), JobError> {
    subtype_against_providers(cx, use_op, Some(name), t, loc)
}

pub fn bind_function_param<'cx>(cx: &mut Context<'cx>, t: Type, loc: ALoc) {
    resolve_env_entry(cx, t, DefLocType::FunctionParamLoc, false, loc)
}

pub fn bind_function_this<'cx>(cx: &mut Context<'cx>, t: Type, loc: ALoc) {
    if matches!(cx.typing_mode(), TypingMode::CheckingMode) {
        resolve_env_entry(cx, t, DefLocType::FunctionThisLoc, false, loc)
    }
}

pub fn bind_class_instance_this<'cx>(cx: &mut Context<'cx>, t: Type, loc: ALoc) {
    resolve_env_entry(cx, t, DefLocType::ClassInstanceThisLoc, false, loc)
}

pub fn bind_class_static_this<'cx>(cx: &mut Context<'cx>, t: Type, loc: ALoc) {
    resolve_env_entry(cx, t, DefLocType::ClassStaticThisLoc, false, loc)
}

pub fn bind_class_instance_super<'cx>(cx: &mut Context<'cx>, t: Type, loc: ALoc) {
    resolve_env_entry(cx, t, DefLocType::ClassInstanceSuperLoc, false, loc)
}

pub fn bind_class_static_super<'cx>(cx: &mut Context<'cx>, t: Type, loc: ALoc) {
    resolve_env_entry(cx, t, DefLocType::ClassStaticSuperLoc, false, loc)
}

pub fn bind_class_self_type<'cx>(cx: &mut Context<'cx>, t: Type, loc: ALoc) {
    resolve_env_entry(cx, t, DefLocType::ClassSelfLoc, false, loc)
}

pub fn init_var<'cx>(
    cx: &mut Context<'cx>,
    use_op: &UseOp,
    t: &Type,
    loc: ALoc,
) -> Result<(), JobError> {
    init_entry(cx, use_op, t, loc)
}

pub fn init_let<'cx>(
    cx: &mut Context<'cx>,
    use_op: &UseOp,
    t: &Type,
    loc: ALoc,
) -> Result<(), JobError> {
    init_entry(cx, use_op, t, loc)
}

pub fn init_implicit_let<'cx>(
    cx: &mut Context<'cx>,
    use_op: &UseOp,
    t: &Type,
    loc: ALoc,
) -> Result<(), JobError> {
    init_entry(cx, use_op, t, loc)
}

pub fn init_const<'cx>(
    cx: &mut Context<'cx>,
    use_op: &UseOp,
    t: &Type,
    loc: ALoc,
) -> Result<(), JobError> {
    init_entry(cx, use_op, t, loc)
}

pub fn init_implicit_const<'cx>(
    cx: &mut Context<'cx>,
    use_op: &UseOp,
    t: &Type,
    loc: ALoc,
) -> Result<(), JobError> {
    init_entry(cx, use_op, t, loc)
}

pub fn read_declared_type<'cx>(cx: &mut Context<'cx>, reason: Reason, loc: ALoc) -> Type {
    use flow_typing_type::type_::empty_t;
    match checked_find_loc_env_write_opt(cx, DefLocType::OrdinaryNameLoc, loc) {
        Some(t) => t,
        None => empty_t::make(reason),
    }
}

// Variable Declaration

/// The state a write's lazy tvar currently resolves to. Resolution replaces it in the
/// environment, so the tvar reads it when it is forced rather than when it is created.
fn current_write_state<'cx>(
    cx: &Context<'cx>,
    def_loc_type: DefLocType,
    loc: ALoc,
) -> Rc<flow_lazy::Lazy<Cx, Type>> {
    cx.environment()
        .find_write(def_loc_type, loc)
        .map(|entry| entry.state.dupe())
        .expect("should be initialized together with its tvar")
}

pub fn init_env<'cx>(cx: &mut Context<'cx>, toplevel_scope_kind: ScopeKind) {
    use std::rc::Rc;

    use flow_common::reason::mk_reason;
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_errors::error_message::InternalError;
    use flow_typing_type::type_::any_t;
    use flow_typing_type::type_::mixed_t;
    use flow_typing_type::type_::placeholder_t;
    let entries: Vec<_> = {
        let env = cx.environment();
        env.var_info
            .env_entries
            .iter()
            .map(|(k, v)| (k.dupe(), v.clone()))
            .collect()
    };

    let initialize_entry = |cx: &mut Context<'cx>,
                            def_loc_type: DefLocType,
                            loc: ALoc,
                            env_entry: &EnvEntry<ALoc>| {
        match env_entry {
            EnvEntry::AssigningWrite(reason) | EnvEntry::GlobalWrite(reason) => {
                let reason_clone = reason.dupe();
                let reason_clone2 = reason.dupe();
                let loc_clone = reason.loc().dupe();
                let initial_state: flow_lazy::Lazy<Cx, Type> =
                    flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                        flow_js::add_output_non_speculating(
                            cx,
                            ErrorMessage::EInternal(Box::new((
                                loc_clone,
                                InternalError::ReadOfUnreachedTvar(def_loc_type),
                            ))),
                        );
                        match cx.typing_mode() {
                            TypingMode::CheckingMode => any_t::error(reason_clone),
                            TypingMode::SynthesisMode { .. } | TypingMode::HintEvaluationMode => {
                                placeholder_t::why(reason_clone.dupe(), mixed_t::why(reason_clone))
                            }
                        }
                    });
                let state = Rc::new(initial_state);
                let loc_for_lazy = loc.dupe();
                // During initialization, all these lazy tvars are created, but not all of them are
                // ready for forcing. The ones that are ready for forcing will be separately added to
                // the list after each component resolution.
                let force_post_component = false;
                let t = flow_typing_tvar::mk_fully_resolved_lazy(
                    cx,
                    reason_clone2,
                    force_post_component,
                    move |cx: &mut Context<'_>| {
                        let current_state = current_write_state(cx, def_loc_type, loc_for_lazy);
                        Ok(current_state.get_forced(cx).dupe())
                    },
                );
                let current_env = cx.environment_mut();
                current_env.initialize(def_loc_type, loc, TypeEntry { t, state });
            }
            EnvEntry::NonAssigningWrite => {
                if is_provider(cx, loc.dupe()) {
                    // If an illegal write is considered as a provider, we still need to give it a
                    // slot to prevent crashing in code that queries provider types.
                    let reason = mk_reason(None, loc.dupe());
                    let reason_clone = reason.dupe();
                    let initial_state: flow_lazy::Lazy<Cx, Type> =
                        flow_lazy::Lazy::new(move |_cx: &mut Context<'_>| {
                            any_t::error(reason_clone)
                        });
                    let state = Rc::new(initial_state);
                    let loc_for_lazy = loc.dupe();
                    let t = flow_typing_tvar::mk_fully_resolved_lazy(
                        cx,
                        reason,
                        false,
                        move |cx: &mut Context<'_>| {
                            let current_state = current_write_state(cx, def_loc_type, loc_for_lazy);
                            Ok(current_state.get_forced(cx).dupe())
                        },
                    );
                    let current_env = cx.environment_mut();
                    current_env.initialize(def_loc_type, loc, TypeEntry { t, state });
                }
            }
        }
    };

    for (key, env_entry) in &entries {
        // Array providers must be initialized first
        if key.def_loc_type == DefLocType::ArrayProviderLoc {
            initialize_entry(cx, key.def_loc_type, key.loc.dupe(), env_entry);
        }
    }

    for (key, env_entry) in &entries {
        if key.def_loc_type != DefLocType::ArrayProviderLoc {
            initialize_entry(cx, key.def_loc_type, key.loc.dupe(), env_entry);
        }
    }

    let env = cx.environment_mut();
    env.scope_kind = toplevel_scope_kind;
}

pub fn discriminant_after_negated_cases<'cx>(
    cx: &mut Context<'cx>,
    switch_loc: ALoc,
    refinement_key_opt: Option<&Key>,
) -> Result<Option<Type>, JobError> {
    use flow_common::reason::mk_reason;

    let reason_desc =
        refinement_key_opt.and_then(|refinement_key| refinement_key.reason_desc::<ALoc>());
    Ok(read_entry(
        LookupMode::ForValue,
        cx,
        switch_loc.dupe(),
        mk_reason(reason_desc, switch_loc),
    )?
    .ok())
}

pub fn get_next<'cx>(cx: &mut Context<'cx>, loc: ALoc) -> Result<Type, JobError> {
    use flow_common::reason::mk_reason;

    read_entry_exn(LookupMode::ForValue, cx, loc.dupe(), mk_reason(None, loc))
}
