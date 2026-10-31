/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

use flow_typing_debug::verbose::print_types_if_verbose;
use flow_typing_errors::error_message::CallTypeArityCallee;
use flow_typing_errors::error_message::ECallTypeArityData;
use flow_typing_errors::error_message::EIncompatiblePropData;
use flow_typing_errors::error_message::EIncompatibleTypesWithUseOpData;
use flow_typing_errors::error_message::EPrivateLookupFailedData;
use flow_typing_errors::error_message::EPropNotFoundInLookupData;
use flow_typing_errors::error_message::EPropNotReadableData;
use flow_typing_errors::error_message::EPropNotWritableData;
use flow_typing_errors::error_message::ETupleInvalidTypeSpreadData;
use flow_typing_errors::error_message::EnumInvalidMemberAccessData;
use flow_typing_errors::error_message::EnumInvalidObjectFunctionData;
use flow_typing_errors::error_message::EnumInvalidObjectUtilTypeData;
use flow_typing_errors::error_message::EnumMemberAccess;
use flow_typing_errors::error_message::EnumModificationData;
use flow_typing_errors::error_message::EnumReferenceData;
use flow_typing_errors::error_message::IncompatibleUpperData;
use flow_typing_flow_common::flow_js_utils::get_prop_t_kit::IndexerFallbackMode;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_implicit_instantiation_check::ImplicitInstantiationCheck;
use flow_typing_implicit_instantiation_check::MethodOptionalChain;
use flow_typing_implicit_instantiation_check::Operation;
use flow_typing_type::type_::ArrRestTData;
use flow_typing_type::type_::ArrType::*;
use flow_typing_type::type_::CallElemTData;
use flow_typing_type::type_::CallMData;
use flow_typing_type::type_::CallTData;
use flow_typing_type::type_::ClassExtendsCheckData;
use flow_typing_type::type_::ClassImplementsCheckData;
use flow_typing_type::type_::ConcretizeTData;
use flow_typing_type::type_::CondTData;
use flow_typing_type::type_::ConditionalTData;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner::*;
use flow_typing_type::type_::DestructorSpreadTypeData;
use flow_typing_type::type_::ElemTData;
use flow_typing_type::type_::EvalTypeDestructorTData;
use flow_typing_type::type_::ExtendsUseTData;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::GetElemTData;
use flow_typing_type::type_::GetEnumTData;
use flow_typing_type::type_::HasOwnPropTData;
use flow_typing_type::type_::IndexerFallbackData;
use flow_typing_type::type_::LookupDefaultResult;
use flow_typing_type::type_::LookupTData;
use flow_typing_type::type_::MapTypeTData;
use flow_typing_type::type_::MethodTData;
use flow_typing_type::type_::NonstrictReturningData;
use flow_typing_type::type_::ObjRestGuardKey;
use flow_typing_type::type_::OptionalIndexedAccessTData;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::PrivateMethodTData;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::ReactKitTData;
use flow_typing_type::type_::ReadElemData;
use flow_typing_type::type_::ReposUseTData;
use flow_typing_type::type_::ResolveSpreadTData;
use flow_typing_type::type_::ResolveUnionTData;
use flow_typing_type::type_::ResolvedSpreadArgData;
use flow_typing_type::type_::SealGenericTData;
use flow_typing_type::type_::SetElemTData;
use flow_typing_type::type_::SpecializeTData;
use flow_typing_type::type_::StandaloneCallThisData;
use flow_typing_type::type_::SuperTData;
use flow_typing_type::type_::TestPropTData;
use flow_typing_type::type_::ThisInstanceTData;
use flow_typing_type::type_::ThisTypeAppTData;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeInner::*;
use flow_typing_type::type_::TypeStrictnessKind;
use flow_typing_type::type_::UnresolvedArgData;
use flow_typing_type::type_::UseT;
use flow_typing_type::type_::UseTInner::*;
use flow_typing_type::type_::ValueToTypeReferenceTData;
use flow_typing_type::type_::WriteElemData;
use flow_typing_type::type_::object::ObjectToolObjectMapData;
use flow_typing_type::type_::type_or_type_desc::ErrorTypeDesc;
use flow_typing_type::type_::type_or_type_desc::TypeOrTypeDescT;

use super::InstanceT;
use super::any_helpers::*;
use super::constraint_helpers::*;
use super::eval_helpers::*;
use super::get_prop_helpers::*;
use super::helpers::mk_typeapp_instance_annot;
use super::helpers::*;
use super::inheritance_helpers::*;
use super::instantiation_helpers::*;
use super::multi_arg_helpers::*;
use super::*;
use crate::default_resolve;
use crate::implicit_instantiation;
use crate::object_kit;
use crate::react_kit;
use crate::slice_utils;
use crate::speculation_kit;
use crate::tvar_resolver;

/// Tail call continuation for the trampoline in __flow.
///
/// In OCaml, certain calls in __flow_impl are in tail position and are
/// optimized by the compiler to reuse the stack frame. In Rust, we return
/// these as continuations for the trampoline to process iteratively.
enum TailCall {
    /// Continue by flowing the lower type into the upper use under a fresh
    /// trace. The `Reason` is not part of the flow itself; it is the diagnostic
    /// reason to report for the upper side if this continuation hits the
    /// recursion limit. `resolve_union` uses the reason of the union it is
    /// rebuilding here.
    Flow(Type, UseT<Cx>, Reason),
    /// Replaces a tail call to `rec_flow()` (e.g. from EvalT, OpenT handlers):
    /// uses given trace, propagates LimitExceeded.
    RecFlow(Type, UseT<Cx>, DepthTrace),
}

fn upper_operation_reason_or<CX: ContextFamily>(u: &UseT<CX>, fallback: &Reason) -> Reason {
    if let Some(reason) = use_op_of_use_t(u)
        .and_then(|use_op| flow_js_utils::primary_reason_of_use_op(&use_op).map(|r| r.dupe()))
    {
        return reason;
    }

    match u.deref() {
        UseT(_, t) => reason_of_t(t).dupe(),
        _ => fallback.dupe(),
    }
}

fn is_extends_use_t_root(t: &TypeInner) -> bool {
    match t {
        ObjProtoT(_) | FunProtoT(_) | DefT(_, deref!(NullT)) => true,
        _ => false,
    }
}

/// The properties a `LookupT` has to resolve, each paired with the upper
/// property it must satisfy. Only [`LookupAction::LookupPropsForSubtyping`]
/// carries more than one: every other action resolves the lookup's own
/// `propref` and has no upper property of its own.
fn lookup_targets<'a>(
    propref: &'a PropRef,
    action: &'a LookupAction,
) -> impl Iterator<Item = (&'a PropRef, Option<&'a Property>)> {
    let (props, single) = match action {
        LookupAction::LookupPropsForSubtyping(box LookupPropsForSubtypingData {
            props, ..
        }) => (Some(props), None),
        _ => (None, Some((propref, None))),
    };
    props
        .into_iter()
        .flat_map(|props| props.iter().map(|(propref, up)| (propref, Some(up))))
        .chain(single)
}

/// A strict lookup reached the root of the prototype chain without finding a declared
/// property: report the missing properties, then recover.
///
/// If the lookup carried an indexer that matched the access on the way up, no declared
/// property was ever required to exist, so recovery resolves to the indexer's value
/// instead of `any` — and for a TS-related access there is nothing to report at all.
fn strict_lookup_failed<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    l: &Type,
    reason_op: &Reason,
    strict_obj: &Type,
    propref: &PropRef,
    action: &LookupAction,
    ids: Option<&properties::Set>,
    indexer_fallback: Option<&IndexerFallbackData>,
) -> Result<(), FlowJsException> {
    if !indexer_fallback.is_some_and(|fallback| fallback.suppress_missing_error) {
        match action {
            LookupAction::LookupPropsForSubtyping(box data) => {
                subtyping_kit::add_output_missing_props_from_lookup(cx, env, data, ids)?;
            }
            _ => {
                let (prop_loc, prop_name) = match propref {
                    PropRef::Named { reason, name, .. } => (reason.loc().dupe(), Some(name.dupe())),
                    PropRef::Computed(_) => (reason_op.loc().dupe(), None),
                };
                let suggestion = prop_name.as_ref().zip(ids).and_then(|(name, ids)| {
                    let ids: Vec<_> = ids.iter().duped().collect();
                    prop_typo_suggestion_for_name(cx, &ids, name)
                });
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::EPropNotFoundInLookup(Box::new(EPropNotFoundInLookupData {
                        prop_loc,
                        object: flow_js_utils::type_reference_with_reason_for_error(
                            strict_obj,
                            reason_of_t(strict_obj).dupe(),
                        ),
                        prop_name,
                        use_op: flow_js_utils::use_op_of_lookup_action(action),
                        suggestion,
                    })),
                )?;
            }
        }
    }
    if let Some(fallback) = indexer_fallback {
        return perform_lookup_action(
            cx,
            env,
            trace,
            propref,
            &property::property_type(&fallback.property),
            Some(&fallback.property),
            PropertySource::IndexerProperty,
            &fallback.obj,
            reason_op,
            action,
            None,
        );
    }
    let p = PropertyType::OrdinaryField {
        type_: any_t::error_of_kind(AnyErrorKind::UnresolvedName, reason_op.dupe()),
        polarity: Polarity::Neutral,
    };
    for (propref, up) in lookup_targets(propref, action) {
        perform_lookup_action(
            cx,
            env,
            trace,
            propref,
            &p,
            None,
            PropertySource::DynamicProperty,
            l,
            reason_op,
            action,
            up,
        )?;
    }
    Ok(())
}

/// Same as [`strict_lookup_failed`], for a lookup of a computed key. Only a key
/// that is `any` resolves; anything else is either an internal error or a
/// property that does not exist.
fn strict_computed_lookup_failed<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    l: &Type,
    reason_op: &Reason,
    strict_obj: &Type,
    propref: &PropRef,
    elem_t: &Type,
    action: &LookupAction,
) -> Result<(), FlowJsException> {
    match elem_t.deref() {
        _ if constraint_node_id(elem_t).is_some() => {
            let loc = loc_of_t(elem_t);
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::EInternal(Box::new((loc.dupe(), InternalError::PropRefComputedOpen))),
            )?;
        }
        DefT(_, deref!(SingletonStrT { .. })) => {
            let loc = loc_of_t(elem_t);
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::EInternal(Box::new((
                    loc.dupe(),
                    InternalError::PropRefComputedLiteral,
                ))),
            )?;
        }
        AnyT(_, src) => {
            let src = flow_js_utils::any_mod_src_keep_placeholder(AnySource::Untyped, src);
            let p = PropertyType::OrdinaryField {
                type_: any_t::why(src, reason_op.dupe()),
                polarity: Polarity::Neutral,
            };
            for (propref, up) in lookup_targets(propref, action) {
                perform_lookup_action(
                    cx,
                    env,
                    trace,
                    propref,
                    &p,
                    None,
                    PropertySource::DynamicProperty,
                    l,
                    reason_op,
                    action,
                    up,
                )?;
            }
        }
        _ => match action {
            LookupAction::LookupPropsForSubtyping(box data) => {
                subtyping_kit::add_output_missing_props_from_lookup(cx, env, data, None)?;
            }
            _ => {
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::EPropNotFoundInLookup(Box::new(EPropNotFoundInLookupData {
                        prop_loc: reason_op.loc().dupe(),
                        object: flow_js_utils::type_reference_with_reason_for_error(
                            strict_obj,
                            reason_of_t(strict_obj).dupe(),
                        ),
                        prop_name: None,
                        use_op: flow_js_utils::use_op_of_lookup_action(action),
                        suggestion: None,
                    })),
                )?;
            }
        },
    }
    Ok(())
}

/// Whether a lookup that reached `terminal`, the root of a prototype chain, has
/// run out of places to find `propref`.
fn prop_missing_from_lookup_terminal(terminal: &Type, propref: &PropRef) -> bool {
    match name_of_propref(propref) {
        Some(name) => subtyping_kit::property_is_missing_from_lookup_terminal(terminal, &name),
        // A computed key is never one of the prototype's own methods.
        None => true,
    }
}

/// The propref and action to keep looking up on the prototype of a type that
/// did not have all of `missing`'s targets, or `None` if it had them all. A
/// batched subtyping lookup carries on with only the properties still missing.
fn remaining_lookup(
    action: &LookupAction,
    missing: &[(&PropRef, Option<&Property>)],
) -> Option<(PropRef, LookupAction)> {
    let (first_propref, _) = *missing.first()?;
    let action = match action {
        LookupAction::LookupPropsForSubtyping(box data) if missing.len() < data.props.len() => {
            LookupAction::LookupPropsForSubtyping(Box::new(LookupPropsForSubtypingData {
                props: missing
                    .iter()
                    .filter_map(|(propref, up)| up.map(|up| ((*propref).clone(), up.dupe())))
                    .collect(),
                ..data.clone()
            }))
        }
        _ => action.clone(),
    };
    Some((first_propref.clone(), action))
}

/// Send an indexer key to the key set of a `GetKeysT`. The key goes through
/// `GetKeysDictKeyT` rather than being classified here: the key is commonly a
/// `typeof s` annotation over a tvar that is still unresolved at this point, and
/// only a use type gets to wait for it. Concretizing it here instead re-enters
/// resolution from inside `GetKeysT` dispatch, which recurses without bound on a
/// key type that reaches the object being keyed.
fn flow_dict_key_to_keys<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    dict_key: &Type,
    reason_op: &Reason,
    keys: &UseT<Cx>,
    include_symbols: bool,
) -> Result<(), FlowJsException> {
    rec_flow(
        cx,
        env,
        trace,
        (
            dict_key,
            &UseT::new(GetKeysDictKeyT {
                reason: reason_op.dupe(),
                t_out: Box::new(keys.dupe()),
                include_symbols,
            }),
        ),
    )
}

/// Whether two evals are both `M[K]` for the same non-generic instance type `M` and the same
/// generic `K`. Overriding generic methods restate the same indexed access at a new annotation
/// site (e.g. `ev: HTMLElementEventMap[K]` in every `HTML*Element.addEventListener`), so their
/// eval ids differ, and evaluating either one expands `K` to every key of its bound.
fn same_generic_indexed_access<'cx>(
    cx: &mut Context<'cx>,
    (t1, d1): (&Type, &TypeDestructorT),
    (t2, d2): (&Type, &TypeDestructorT),
) -> bool {
    let (
        Destructor::ElementType {
            index_type: deref!(GenericT(g1)),
        },
        Destructor::ElementType {
            index_type: deref!(GenericT(g2)),
        },
    ) = (d1.2.as_ref(), d2.2.as_ref())
    else {
        return false;
    };
    g1.id == g2.id && flow_js_utils::is_same_non_generic_instance_annot(cx, t1, t2)
}

/// Substitutes `this` into the instance of a this-abstracted class or interface `c`.
fn this_specialize_instance<'cx>(
    cx: &mut Context<'cx>,
    c: &Type,
    inst_r: &Reason,
    i: &InstanceT,
    subst_name: &SubstName,
    this: &Type,
) -> Type {
    if let Some(t) = flow_cache::this_specialize::find(cx, c, this) {
        return t;
    }
    let mut map = FlowOrdMap::new();
    map.insert(subst_name.dupe(), this.dupe());
    let i = type_subst::subst_instance_type(
        cx,
        None,
        true,
        false,
        type_subst::Purpose::Normal,
        &map,
        i,
    );
    let t = Type::new(DefT(inst_r.dupe(), DefT::new(InstanceT(Rc::new(i)))));
    flow_cache::this_specialize::add(cx, c.dupe(), this.dupe(), t.dupe());
    t
}

/// NOTE: Do not call this function directly. Instead, call the wrapper
/// functions `rec_flow`, `join_flow`, or `flow_opt` (described below) inside
/// this module, and the function `flow` outside this module.
///
/// This is a trampoline wrapper that handles tail call optimization for
/// mutually recursive chains in __flow_impl. In OCaml, a `rec_flow` call at
/// the end of a match arm is in tail position and optimized by the compiler.
/// In Rust, we return these as TailCall continuations and process them in a
/// loop to avoid stack overflow.
pub(super) fn __flow<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    (l, u): (&Type, &UseT<Cx>),
    trace: DepthTrace,
) -> Result<(), FlowJsException> {
    let mut tc = __flow_impl(cx, env, (l, u), trace)?;

    // Process tail calls iteratively instead of recursing
    while let Some(tail_call) = tc {
        let (tc_l, tc_u, tc_trace, catch_limit_reason) = match tail_call {
            TailCall::Flow(l, u, reason) => (l, u, DepthTrace::unit_trace(), Some(reason)),
            TailCall::RecFlow(l, u, t) => (l, u, t, None),
        };
        match __flow_impl(cx, env, (&tc_l, &tc_u), tc_trace) {
            Ok(next) => tc = next,
            Err(FlowJsException::LimitExceeded) if let Some(ru) = catch_limit_reason => {
                // flow() handles LimitExceeded by recording an error and continuing
                let rl = reason_of_t(&tc_l).dupe();
                let reasons = match tc_u.deref() {
                    UseT(_, _) => (ru, rl),
                    _ => flow_js_utils::ordered_reasons(cx, (rl, ru)),
                };
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::ERecursionLimit(reasons.0.loc().dupe()),
                );
                break;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn __flow_impl<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    (l, u): (&Type, &UseT<Cx>),
    trace: DepthTrace,
) -> Result<Option<TailCall>, FlowJsException> {
    if flow_typing_type::type_util::ground_subtype_use_t(
        &mut |t| flow_js_utils::update_lit_type_from_annot(cx, env, t),
        l,
        u,
    ) {
        print_types_if_verbose(cx, &trace, None, l, u);
        return Ok(None);
    }
    // Fast path: if l is pointer-equal to the Type inside UseT<Cx>, it's trivially l <: l.
    // This must come after ground_subtype_use_t, which has a side effect of recording
    // singleton literal checks needed by implicit instantiation.
    if let UseT(_, t) = u.deref()
        && l.ptr_eq(t)
    {
        return Ok(None);
    }
    if flow_typing_flow_common::flow_cache::flow_constraint::get(cx, l, u) {
        print_types_if_verbose(cx, &trace, Some("(cached)"), l, u);
        return Ok(None);
    }
    print_types_if_verbose(cx, &trace, None, l, u);

    // limit recursion depth
    recursion_check::check(cx, trace)?;

    // Check if this worker has been told to cancel (or per-file timeout exceeded)
    check_canceled(cx)?;

    // Expect that l is a def type. On the other hand, u may be a use type or a
    // def type: the latter typically when we have annotations.
    if match l.deref() {
        AnyT(_, _) => {
            // Either propagate AnyT through the use type, or short-circuit
            any_propagated(cx, env, trace, l, u)?
        }
        GenericT(box GenericTData {
            bound,
            name,
            reason,
            id,
            no_infer,
        }) => handle_generic(cx, env, trace, *no_infer, bound, reason, id, name, u)?,
        ImplicitInstantiationTvar(data)
            if frozen_implicit_instantiation_tvar(cx, env, l).is_some() =>
        {
            handle_frozen_tvar(cx, env, trace, l, data, u)?
        }
        _ => false,
    } {
        // Either propagate AnyT through the def type, or short-circuit because l <: any trivially
        return Ok(None);
    }
    if match u.deref() {
        UseT(use_op, any @ deref!(AnyT(_, _))) => {
            any_propagated_use(cx, env, trace, use_op, any, l)?
        }
        _ => false,
    } {
        return Ok(None);
    }
    if match l.deref() {
        DefT(_, deref!(EmptyT)) => empty_success(u),
        _ => false,
    } {
        return Ok(None);
    }
    // START OF PATTERN MATCH
    match (l.deref(), u.deref()) {
        // ********
        // * eval *
        // ********
        (EvalT { id: id1, .. }, UseT(_, deref!(EvalT { id: id2, .. }))) if id1 == id2 => {
            if cx.is_verbose() {
                eprintln!("EvalT ~> EvalT fast path");
            }
        }
        (
            EvalT {
                type_: t1,
                defer_use_t: d1,
                ..
            },
            UseT(
                _,
                deref!(EvalT {
                    type_: t2,
                    defer_use_t: d2,
                    ..
                }),
            ),
        ) if same_generic_indexed_access(cx, (t1, d1), (t2, d2)) => {
            if cx.is_verbose() {
                eprintln!("EvalT ~> EvalT same generic indexed access fast path");
            }
        }
        (
            EvalT {
                type_: t,
                defer_use_t,
                id,
            },
            _,
        ) => {
            let TypeDestructorTInner(use_op_prime, reason, d) = defer_use_t.deref();
            let result = eval_helpers::mk_type_destructor(
                cx,
                env,
                trace,
                use_op_prime.dupe(),
                reason,
                t,
                d,
                id.dupe(),
            )?;
            // rec_flow(cx, env, trace, (&result, u))
            // Tail call: in OCaml this is the last call in the match arm
            return Ok(Some(TailCall::RecFlow(
                result,
                u.dupe(),
                DepthTrace::rec_trace(trace),
            )));
        }
        (
            _,
            UseT(
                use_op,
                deref!(EvalT {
                    type_: t,
                    defer_use_t,
                    id,
                }),
            ),
        ) => {
            let TypeDestructorTInner(use_op_prime, reason, d) = defer_use_t.deref();
            let result = eval_helpers::mk_type_destructor(
                cx,
                env,
                trace,
                use_op_prime.dupe(),
                reason,
                t,
                d,
                id.dupe(),
            )?;
            // rec_flow(cx, env, trace, (&result, &UseT::new(UseTInner::ReposUseT(...))))
            // Tail call: in OCaml this is the last call in the match arm
            return Ok(Some(TailCall::RecFlow(
                result,
                UseT::new(ReposUseT(Box::new(ReposUseTData {
                    reason: reason.dupe(),
                    use_desc: false,
                    use_op: use_op.dupe(),
                    type_: l.dupe(),
                }))),
                DepthTrace::rec_trace(trace),
            )));
        }

        (
            _,
            ConcretizeT(box ConcretizeTData {
                kind: ConcretizationKind::ConcretizeForLowerBounds,
                collector,
                ..
            }),
        ) => match active_constraint_node(cx, env, l) {
            Some(node) => {
                for lower in flow_js_utils::possible_types(cx, node.id()) {
                    collector.add_including_empty(lower);
                }
            }
            None => collector.add_including_empty(
                frozen_implicit_instantiation_tvar(cx, env, l).unwrap_or_else(|| l.dupe()),
            ),
        },

        // ******************
        // * process X ~> Y *
        // ******************
        (_, UseT(use_op, t_upper))
            if let Some(node1) = constraint_node_id(l)
                && let Some(node2) = constraint_node_id(t_upper) =>
        {
            if node1.is_open_t() {
                cx.add_array_or_object_literal_declaration_upper_bound(node1.id(), t_upper.dupe());
            }
            flow_constraint_nodes(cx, env, trace, use_op.dupe(), node1, node2)?;
        }
        // ******************
        // * process Y ~> U *
        // ******************
        (_, _) if let Some(node) = active_constraint_node(cx, env, l) => {
            let r = reason_of_t(l);
            if !match u.deref() {
                // We have some simple tvar id based concretization. Bad cyclic types can only
                // come from indirections through OpenT, most of them are already defended with
                // Flow_js_utils.InvalidCyclicTypeValidation and turned to any, but there are gaps
                // (especially EvalT from type sig), so we defend it again here.
                ConcretizeT(box ConcretizeTData { seen, .. }) => {
                    let tvar_id = node.id();
                    if seen.contains(&tvar_id) {
                        true
                    } else {
                        seen.insert(tvar_id);
                        false
                    }
                }
                _ => false,
            } {
                match u.deref() {
                    UseT(_, t2) if node.is_open_t() => {
                        cx.add_array_or_object_literal_declaration_upper_bound(
                            node.id(),
                            t2.dupe(),
                        );
                    }
                    _ => {}
                }
                let u = match r.desc(true) {
                    Some(VirtualReasonDesc::RTypeParam(..)) => type_util::mod_use_op_of_use_t(
                        |op| {
                            UseOp::Frame(
                                Arc::new(VirtualFrameUseOp::ImplicitTypeParam),
                                Arc::new(op.dupe()),
                            )
                        },
                        u,
                    ),
                    _ => u.dupe(),
                };
                let state = constraint_node_constraints(cx, node);
                if let Some((l, u)) = flow_active_constraint_node_to_use(cx, env, trace, state, &u)?
                {
                    return Ok(Some(TailCall::RecFlow(l, u, DepthTrace::rec_trace(trace))));
                }
            }
        }
        // ******************
        // * process L ~> X *
        // ******************
        (_, UseT(use_op, t_open)) if let Some(node) = constraint_node_id(t_open) => {
            if let Some((l, u)) =
                flow_type_to_constraint_node(cx, env, trace, use_op.dupe(), l, node)?
            {
                return Ok(Some(TailCall::RecFlow(l, u, DepthTrace::rec_trace(trace))));
            }
        }
        // ************************
        // * Eval type destructor *
        // ************************
        (
            _,
            EvalTypeDestructorT(box EvalTypeDestructorTData {
                destructor_use_op,
                reason,
                repos,
                destructor,
                tout,
            }),
        ) => {
            let l_repos = match repos {
                None => l.dupe(),
                Some((reason, use_desc)) => {
                    helpers::reposition_reason(cx, env, Some(trace), reason, *use_desc, l)?
                }
            };
            eval_destructor(
                cx,
                env,
                trace,
                destructor_use_op.dupe(),
                reason,
                &l_repos,
                destructor,
                tout,
            )?;
        }

        // *************
        // * Subtyping *
        // *************
        (_, UseT(use_op, u_type)) => {
            subtyping_kit::rec_sub_t(cx, env, use_op.dupe(), l, u_type, trace)?;
        }
        // For l.key !== sentinel when sentinel has a union type, don't split the union. This
        // prevents a drastic blowup of cases which can cause perf problems.
        (
            UnionT(_, _),
            ConcretizeT(box ConcretizeTData {
                kind: ConcretizationKind::ConcretizeForSentinelPropTest,
                collector,
                ..
            }),
        )
        | (
            UnionT(_, _),
            ConcretizeT(box ConcretizeTData {
                kind:
                    ConcretizationKind::ConcretizeForPredicate(
                        PredicateConcretetizerVariant::ConcretizeRHSForLiteralPredicateTest,
                    ),
                collector,
                ..
            }),
        ) => {
            collector.add(l.dupe());
        }
        (
            UnionT(_, rep),
            ConcretizeT(box ConcretizeTData {
                kind:
                    ConcretizationKind::ConcretizeForPredicate(
                        PredicateConcretetizerVariant::ConcretizeKeepOptimizedUnions,
                    ),
                collector,
                ..
            }),
        ) if rep.is_optimized_finally() => {
            collector.add(l.dupe());
        }
        (
            UnionT(_, _),
            ConcretizeT(box ConcretizeTData {
                kind: ConcretizationKind::ConcretizeForMatchArg { keep_unions: true },
                collector,
                ..
            }),
        ) => {
            collector.add(l.dupe());
        }
        (UnionT(_, urep), ConcretizeT(..)) => {
            flow_all_in_union(cx, env, trace, urep, u)?;
        }
        (MaybeT(lreason, t), ConcretizeT(..)) => {
            let lreason = lreason.dupe().replace_desc(VirtualReasonDesc::RNullOrVoid);
            rec_flow(cx, env, trace, (&null::make(lreason.dupe()), u))?;
            rec_flow(cx, env, trace, (&void::make(lreason), u))?;
            rec_flow(cx, env, trace, (t, u))?;
        }
        (
            OptionalT {
                reason: r,
                type_: t,
                use_desc,
            },
            ConcretizeT(..),
        ) => {
            rec_flow(
                cx,
                env,
                trace,
                (&void::why_with_use_desc(*use_desc, r.dupe()), u),
            )?;
            rec_flow(cx, env, trace, (t, u))?;
        }
        (AnnotT(r, t, use_desc), ConcretizeT(..)) => {
            // TODO: directly derive loc and desc from the reason of tvar
            let loc = r.loc().dupe();
            let desc = if *use_desc { Some(r.desc(true)) } else { None };
            let annot_loc = r.annot_loc().map(|l| l.dupe());
            let repos_t =
                helpers::reposition(cx, env, Some(trace), loc, desc, annot_loc, t.dupe())?;
            rec_flow(cx, env, trace, (&repos_t, u))?;
        }
        (DefT(reason, deref!(EmptyT)), ConvertEmptyPropsToMixedT(_, collector)) => {
            collector.add(mixed_t::make(reason.dupe()));
        }
        (_, ConvertEmptyPropsToMixedT(_, collector)) => {
            collector.add(l.dupe());
        }
        // ***************
        // * annotations *
        // ***************

        // Special cases where we want to recursively concretize types within the lower bound.
        (
            UnionT(r, rep),
            ReposUseT(box ReposUseTData {
                reason,
                use_desc,
                use_op,
                type_: l_inner,
            }),
        ) => {
            let rep = {
                let in_implicit = env.in_implicit_instantiation();
                rep.ident_map(false, |t| annot(in_implicit, *use_desc, t))
            };
            let loc = reason.loc().dupe();
            let annot_loc = reason.annot_loc().map(|l| l.dupe());
            let r = r.dupe().reposition_and_opt_annotate(loc, annot_loc);
            let r = if *use_desc {
                r.replace_desc(reason.desc(true).cloned())
            } else {
                r
            };
            rec_flow(
                cx,
                env,
                trace,
                (
                    l_inner,
                    &UseT::new(UseT(use_op.dupe(), Type::new(UnionT(r, rep)))),
                ),
            )?;
        }
        (
            MaybeT(r, u_inner),
            ReposUseT(box ReposUseTData {
                reason,
                use_desc,
                use_op,
                type_: l_inner,
            }),
        ) => {
            let loc = reason.loc().dupe();
            let annot_loc = reason.annot_loc().map(|l| l.dupe());
            let r = r.dupe().reposition_and_opt_annotate(loc, annot_loc);
            let r = if *use_desc {
                r.replace_desc(reason.desc(true).cloned())
            } else {
                r
            };
            let annoted_u = annot(env.in_implicit_instantiation(), *use_desc, u_inner);
            rec_flow(
                cx,
                env,
                trace,
                (
                    l_inner,
                    &UseT::new(UseT(use_op.dupe(), Type::new(MaybeT(r, annoted_u)))),
                ),
            )?;
        }
        (
            OptionalT {
                reason: r,
                type_: u_inner,
                use_desc: use_desc_optional_t,
            },
            ReposUseT(box ReposUseTData {
                reason,
                use_desc,
                use_op,
                type_: l_inner,
            }),
        ) => {
            let loc = reason.loc().dupe();
            let annot_loc = reason.annot_loc().map(|l| l.dupe());
            let r = r.dupe().reposition_and_opt_annotate(loc, annot_loc);
            let r = if *use_desc {
                r.replace_desc(reason.desc(true).cloned())
            } else {
                r
            };
            let annoted_u = annot(env.in_implicit_instantiation(), *use_desc, u_inner);
            rec_flow(
                cx,
                env,
                trace,
                (
                    l_inner,
                    &UseT::new(UseT(
                        use_op.dupe(),
                        Type::new(OptionalT {
                            reason: r,
                            type_: annoted_u,
                            use_desc: *use_desc_optional_t,
                        }),
                    )),
                ),
            )?;
        }
        (
            DefT(
                r,
                deref!(RendersT(deref!(CanonicalRendersForm::StructuralRenders {
                    renders_variant,
                    renders_structural_type: deref!(UnionT(_, rep)),
                }))),
            ),
            ReposUseT(box ReposUseTData {
                reason,
                use_desc,
                use_op,
                type_: l_inner,
            }),
        ) => {
            let rep = {
                let in_implicit = env.in_implicit_instantiation();
                rep.ident_map(false, |t| annot(in_implicit, *use_desc, t))
            };
            let loc = reason.loc().dupe();
            let annot_loc = reason.annot_loc().map(|l| l.dupe());
            let r = r.dupe().reposition_and_opt_annotate(loc, annot_loc);
            let r = if *use_desc {
                r.replace_desc(reason.desc(true).cloned())
            } else {
                r
            };
            rec_flow(
                cx,
                env,
                trace,
                (
                    l_inner,
                    &UseT::new(UseT(
                        use_op.dupe(),
                        Type::new(DefT(
                            r.dupe(),
                            DefT::new(RendersT(Rc::new(CanonicalRendersForm::StructuralRenders {
                                renders_variant: *renders_variant,
                                renders_structural_type: Type::new(UnionT(r, rep)),
                            }))),
                        )),
                    )),
                ),
            )?;
        }
        // Waits for a def type to become concrete, repositions it as an upper UseT<Cx>
        // using the stored reason. This can be used to store a reason as it flows
        // through a tvar.
        (
            _,
            ReposUseT(box ReposUseTData {
                reason,
                use_desc,
                use_op,
                type_: l_inner,
            }),
        ) => {
            let u_repos = helpers::reposition_reason(cx, env, Some(trace), reason, *use_desc, l)?;
            return Ok(Some(TailCall::RecFlow(
                l_inner.dupe(),
                UseT::new(UseT(use_op.dupe(), u_repos)),
                DepthTrace::rec_trace(trace),
            )));
        }

        // The source component of an annotation flows out of the annotated
        // site to downstream uses.
        (AnnotT(r, t, use_desc), _) => {
            let t = helpers::reposition_reason(cx, env, Some(trace), r, *use_desc, t)?;
            return Ok(Some(TailCall::RecFlow(
                t,
                u.dupe(),
                DepthTrace::rec_trace(trace),
            )));
        }

        // ******************
        // * Module exports *
        // ******************
        (
            _,
            ConcretizeT(box ConcretizeTData {
                kind: ConcretizationKind::ConcretizeForCJSExtractNamedExportsAndTypeExports,
                collector,
                ..
            }),
        ) => {
            collector.add(l.dupe());
        }
        // ***************************
        // * optional indexed access *
        // ***************************
        (
            DefT(r, deref!(EmptyT | VoidT | NullT)),
            OptionalIndexedAccessT(box OptionalIndexedAccessTData {
                use_op, tout_tvar, ..
            }),
        ) => {
            rec_flow_t(
                cx,
                env,
                trace,
                use_op.dupe(),
                (
                    &empty_t::why(r.dupe()),
                    &Type::new(OpenT((**tout_tvar).dupe())),
                ),
            )?;
        }
        (MaybeT(_, t), OptionalIndexedAccessT(..))
        | (OptionalT { type_: t, .. }, OptionalIndexedAccessT(..)) => {
            rec_flow(cx, env, trace, (t, u))?;
        }
        (
            UnionT(_, rep),
            OptionalIndexedAccessT(box OptionalIndexedAccessTData {
                use_op,
                reason,
                index,
                tout_tvar,
            }),
        ) => {
            let mut members = rep.members_iter();
            let t0 = members.next().unwrap();
            let t1 = members.next().unwrap();
            let ts: Vec<&Type> = members.collect();
            let mut f = |t: &Type| -> Result<Type, FlowJsException> {
                flow_typing_tvar::mk_no_wrap_where(cx, reason.dupe(), |cx, tvar_reason, tvar_id| {
                    let tvar = Tvar::new(tvar_reason.dupe(), tvar_id as u32);
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (
                            t,
                            &UseT::new(OptionalIndexedAccessT(Box::new(
                                OptionalIndexedAccessTData {
                                    use_op: use_op.dupe(),
                                    reason: reason.dupe(),
                                    index: index.dupe(),
                                    tout_tvar: Box::new(tvar),
                                },
                            ))),
                        ),
                    )?;
                    Ok(())
                })
            };
            let ft0 = f(t0)?;
            let ft1 = f(t1)?;
            let mut fts_vec = Vec::with_capacity(ts.len());
            for t in ts.iter() {
                fts_vec.push(f(t)?);
            }
            let fts: Rc<[Type]> = fts_vec.into();
            let rep = union_rep::make(None, union_rep::UnionKind::UnknownKind, ft0, ft1, fts);
            rec_unify(
                cx,
                env,
                trace,
                unknown_use(),
                UnifyCause::Uncategorized,
                None,
                &Type::new(UnionT(reason.dupe(), rep)),
                &Type::new(OpenT((**tout_tvar).dupe())),
            )?;
        }
        (
            _,
            OptionalIndexedAccessT(box OptionalIndexedAccessTData {
                use_op,
                reason,
                index,
                tout_tvar,
            }),
        ) if !matches!(l.deref(), IntersectionT(_, _)) => {
            let u_new = match index {
                OptionalIndexedAccessIndex::OptionalIndexedAccessStrLitIndex(name) => {
                    let reason_op = reason
                        .dupe()
                        .replace_desc(VirtualReasonDesc::RProperty(Some(name.dupe())));
                    UseT::new(GetPropT(Box::new(GetPropTData {
                        use_op: use_op.dupe(),
                        reason: reason.dupe(),
                        id: None,
                        from_annot: true,
                        skip_optional: false,
                        propref: Box::new(mk_named_prop(reason_op, true, name.dupe())),
                        tout: tout_tvar.clone(),
                        hint: hint_unavailable(),
                    })))
                }
                OptionalIndexedAccessIndex::OptionalIndexedAccessTypeIndex(key_t) => {
                    UseT::new(GetElemT(Box::new(GetElemTData {
                        use_op: use_op.dupe(),
                        reason: reason.dupe(),
                        id: None,
                        from_annot: true,
                        skip_optional: false,
                        access_iterables: false,
                        key_t: key_t.dupe(),
                        tout: tout_tvar.clone(),
                    })))
                }
            };
            rec_flow(cx, env, trace, (l, &u_new))?;
        }
        // ********************
        // * DRO and hooklike *
        // ********************
        (
            OptionalT {
                reason,
                type_,
                use_desc,
            },
            DeepReadOnlyT(tout, dro),
        ) => {
            let r = tout.reason();
            let ReactDro(dro_loc, dro_type) = dro;
            let new_type_ =
                flow_typing_tvar::mk_no_wrap_where(cx, r.dupe(), |cx, tvar_reason, tvar_id| {
                    let tvar = Tvar::new(tvar_reason.dupe(), tvar_id as u32);
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (
                            type_,
                            &UseT::new(DeepReadOnlyT(
                                Box::new(tvar),
                                ReactDro(dro_loc.dupe(), dro_type.clone()),
                            )),
                        ),
                    )?;
                    Ok::<(), FlowJsException>(())
                })?;
            let new_l = Type::new(OptionalT {
                reason: reason.dupe(),
                type_: new_type_,
                use_desc: *use_desc,
            });
            let tout_t = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (&new_l, &tout_t))?;
        }
        (
            OptionalT {
                reason,
                type_,
                use_desc,
            },
            HooklikeT(tout),
        ) => {
            let r = tout.reason();
            let new_type_ =
                flow_typing_tvar::mk_no_wrap_where(cx, r.dupe(), |cx, tvar_reason, tvar_id| {
                    let tvar = Tvar::new(tvar_reason.dupe(), tvar_id as u32);
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (type_, &UseT::new(HooklikeT(Box::new(tvar)))),
                    )?;
                    Ok::<(), FlowJsException>(())
                })?;
            let new_l = Type::new(OptionalT {
                reason: reason.dupe(),
                type_: new_type_,
                use_desc: *use_desc,
            });
            let tout_t = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (&new_l, &tout_t))?;
        }
        (MaybeT(rl, t), DeepReadOnlyT(tout, dro)) => {
            let r = tout.reason();
            let ReactDro(dro_loc, dro_type) = dro;
            let new_t =
                flow_typing_tvar::mk_no_wrap_where(cx, r.dupe(), |cx, tvar_reason, tvar_id| {
                    let tvar = Tvar::new(tvar_reason.dupe(), tvar_id as u32);
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (
                            t,
                            &UseT::new(DeepReadOnlyT(
                                Box::new(tvar),
                                ReactDro(dro_loc.dupe(), dro_type.clone()),
                            )),
                        ),
                    )?;
                    Ok::<(), FlowJsException>(())
                })?;
            let new_l = Type::new(MaybeT(rl.dupe(), new_t));
            let tout_t = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (&new_l, &tout_t))?;
        }
        (MaybeT(rl, t), HooklikeT(tout)) => {
            let r = tout.reason();
            let new_t =
                flow_typing_tvar::mk_no_wrap_where(cx, r.dupe(), |cx, tvar_reason, tvar_id| {
                    let tvar = Tvar::new(tvar_reason.dupe(), tvar_id as u32);
                    rec_flow(cx, env, trace, (t, &UseT::new(HooklikeT(Box::new(tvar)))))?;
                    Ok::<(), FlowJsException>(())
                })?;
            let new_l = Type::new(MaybeT(rl.dupe(), new_t));
            let tout_t = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (&new_l, &tout_t))?;
        }
        (UnionT(reason, rep), DeepReadOnlyT(tout, dro)) => {
            let ReactDro(dro_loc, dro_type) = dro;
            if !rep.is_optimized_finally() {
                rep.optimize_enum_only(cx, |cx, ts| {
                    flow_typing_visitors::type_mapper::union_flatten(cx, ts.duped())
                });
            }
            if rep.check_enum().is_some() {
                let tout_t = Type::new(OpenT((**tout).dupe()));
                rec_flow_t(cx, env, trace, unknown_use(), (l, &tout_t))?;
            } else {
                let dro_loc = dro_loc.dupe();
                let dro_type = dro_type.clone();
                let dro_union = flow_js_utils::map_union(
                    |cx, trace, t, tout| {
                        let tout_tvar = open_tvar(&tout);
                        rec_flow(
                            cx,
                            env,
                            *trace,
                            (
                                t,
                                &UseT::new(DeepReadOnlyT(
                                    Box::new(tout_tvar.dupe()),
                                    ReactDro(dro_loc.dupe(), dro_type.clone()),
                                )),
                            ),
                        )
                    },
                    cx,
                    &trace,
                    rep,
                    reason.dupe(),
                )?;
                let tout_t = Type::new(OpenT((**tout).dupe()));
                rec_flow_t(cx, env, trace, unknown_use(), (&dro_union, &tout_t))?;
            }
        }
        (UnionT(reason, rep), HooklikeT(tout)) => {
            if !rep.is_optimized_finally() {
                rep.optimize_enum_only(cx, |cx, ts| {
                    flow_typing_visitors::type_mapper::union_flatten(cx, ts.duped())
                });
            }
            if rep.check_enum().is_some() {
                let tout_t = Type::new(OpenT((**tout).dupe()));
                rec_flow_t(cx, env, trace, unknown_use(), (l, &tout_t))?;
            } else {
                let hook_union = flow_js_utils::map_union(
                    |cx, trace, t, tout| {
                        let tout_tvar = open_tvar(&tout);
                        rec_flow(
                            cx,
                            env,
                            *trace,
                            (t, &UseT::new(HooklikeT(Box::new(tout_tvar.dupe())))),
                        )
                    },
                    cx,
                    &trace,
                    rep,
                    reason.dupe(),
                )?;
                let tout_t = Type::new(OpenT((**tout).dupe()));
                rec_flow_t(cx, env, trace, unknown_use(), (&hook_union, &tout_t))?;
            }
        }
        (IntersectionT(reason, rep), DeepReadOnlyT(tout, dro)) => {
            let ReactDro(dro_loc, dro_type) = dro;
            let dro_loc = dro_loc.dupe();
            let dro_type = dro_type.clone();
            let dro_inter = flow_js_utils::map_inter(
                |cx, trace, t, tout| {
                    let tout_tvar = open_tvar(&tout);
                    rec_flow(
                        cx,
                        env,
                        *trace,
                        (
                            t,
                            &UseT::new(DeepReadOnlyT(
                                Box::new(tout_tvar.dupe()),
                                ReactDro(dro_loc.dupe(), dro_type.clone()),
                            )),
                        ),
                    )
                },
                cx,
                &trace,
                rep,
                reason.dupe(),
            )?;
            let tout_t = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (&dro_inter, &tout_t))?;
        }
        (DefT(r, deref!(ObjT(o))), DeepReadOnlyT(tout, dro)) => {
            let ReactDro(dro_loc, dro_type) = dro;
            let new_flags = Flags {
                react_dro: Some(ReactDro(dro_loc.dupe(), dro_type.clone())),
                ..o.flags.clone()
            };
            let new_obj = ObjType {
                flags: new_flags,
                ..(**o).clone()
            };
            let new_l = Type::new(DefT(r.dupe(), DefT::new(ObjT(Rc::new(new_obj)))));
            let tout_t = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (&new_l, &tout_t))?;
        }
        #[rustfmt::skip]
        (
            DefT(r, deref!(ArrT(deref!(TupleAT(box TupleATData {
                elem_t,
                elements,
                arity,
                inexact,
                rest,
                strictness_kind,
                literal,
                fresh,
                ..
            }))))),
            DeepReadOnlyT(tout, dro),
        ) => {
            let ReactDro(dro_loc, dro_type) = dro;
            let new_l = Type::new(DefT(
                r.dupe(),
                DefT::new(ArrT(Rc::new(TupleAT(Box::new(TupleATData {
                    react_dro: Some(ReactDro(dro_loc.dupe(), dro_type.clone())),
                    elem_t: elem_t.dupe(),
                    elements: elements.dupe(),
                    arity: *arity,
                    inexact: *inexact,
                    rest: rest.dupe(),
                    strictness_kind: *strictness_kind,
                    literal: *literal,
                    fresh: *fresh,
                }))))),
            ));
            let tout_t = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (&new_l, &tout_t))?;
        }
        #[rustfmt::skip]
        (
            DefT(r, deref!(ArrT(deref!(ArrayAT(box ArrayATData {
                elem_t,
                tuple_view,
                strictness_kind,
                literal,
                fresh,
                ..
            }))))),
            DeepReadOnlyT(tout, dro),
        ) => {
            let ReactDro(dro_loc, dro_type) = dro;
            let new_l = Type::new(DefT(
                r.dupe(),
                DefT::new(ArrT(Rc::new(ArrayAT(Box::new(ArrayATData {
                    react_dro: Some(ReactDro(dro_loc.dupe(), dro_type.clone())),
                    elem_t: elem_t.dupe(),
                    tuple_view: tuple_view.clone(),
                    strictness_kind: *strictness_kind,
                    literal: *literal,
                    fresh: *fresh,
                }))))),
            ));
            let tout_t = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (&new_l, &tout_t))?;
        }
        (
            DefT(
                r,
                deref!(ArrT(deref!(
                    ROArrayAT(box ROArrayATData {
                        elem_t: t, literal, ..
                    })
                ))),
            ),
            DeepReadOnlyT(tout, dro),
        ) => {
            let ReactDro(dro_loc, dro_type) = dro;
            let new_l = Type::new(DefT(
                r.dupe(),
                DefT::new(ArrT(Rc::new(ROArrayAT(Box::new(ROArrayATData {
                    elem_t: t.dupe(),
                    react_dro: Some(ReactDro(dro_loc.dupe(), dro_type.clone())),
                    literal: *literal,
                }))))),
            ));
            let tout_t = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (&new_l, &tout_t))?;
        }
        (DefT(r, deref!(InstanceT(instance))), DeepReadOnlyT(tout, react_dro)) => {
            let new_inst = InstType::new(InstTypeInner {
                inst_react_dro: Some(react_dro.clone()),
                ..instance.inst.deref().clone()
            });
            let new_instance = InstanceT::new(InstanceTInner {
                inst: new_inst,
                ..instance.deref().deref().clone()
            });
            let new_l = Type::new(DefT(r.dupe(), DefT::new(InstanceT(Rc::new(new_instance)))));
            let tout_t = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (&new_l, &tout_t))?;
        }
        (DefT(r, deref!(FunT(s, funtype))), HooklikeT(tout))
            if funtype.effect_ == ReactEffectType::ArbitraryEffect =>
        {
            let new_funtype = FunType {
                effect_: ReactEffectType::AnyEffect,
                ..(**funtype).clone()
            };
            let new_l = Type::new(DefT(
                r.dupe(),
                DefT::new(FunT(s.dupe(), Rc::new(new_funtype))),
            ));
            let tout_t = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (&new_l, &tout_t))?;
        }
        (
            DefT(
                rp,
                deref!(
                    PolyT(box PolyTData {
                        kind: poly_kind,
                        tparams_loc,
                        tparams,
                        t_out: deref!(DefT(r, deref!(FunT(s, funtype)))),
                        id,
                        strictness_kind,
                    })
                ),
            ),
            HooklikeT(tout),
        ) if funtype.effect_ == ReactEffectType::ArbitraryEffect => {
            let new_funtype = FunType {
                effect_: ReactEffectType::AnyEffect,
                ..(**funtype).clone()
            };
            let new_t_out = Type::new(DefT(
                r.dupe(),
                DefT::new(FunT(s.dupe(), Rc::new(new_funtype))),
            ));
            let new_l = Type::new(DefT(
                rp.dupe(),
                DefT::new(PolyT(Box::new(PolyTData {
                    kind: *poly_kind,
                    tparams_loc: tparams_loc.dupe(),
                    tparams: tparams.dupe(),
                    t_out: new_t_out,
                    id: id.dupe(),
                    strictness_kind: *strictness_kind,
                }))),
            ));
            let tout_t = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (&new_l, &tout_t))?;
        }
        (
            DefT(
                r,
                deref!(
                    ObjT(
                        obj @ deref!(ObjType {
                            call_t: Some(id),
                            ..
                        }),
                    )
                ),
            ),
            HooklikeT(tout),
        ) => {
            let call_t = cx.find_call(*id);
            let t = match call_t.deref() {
                DefT(rf, deref!(FunT(s, funtype)))
                    if funtype.effect_ == ReactEffectType::ArbitraryEffect =>
                {
                    let call = Type::new(DefT(
                        rf.dupe(),
                        DefT::new(FunT(
                            s.dupe(),
                            Rc::new(FunType {
                                effect_: ReactEffectType::AnyEffect,
                                ..(**funtype).clone()
                            }),
                        )),
                    ));
                    let new_id = cx.make_call_prop(call);
                    Type::new(DefT(
                        r.dupe(),
                        DefT::new(ObjT(Rc::new(ObjType {
                            call_t: Some(new_id),
                            ..(**obj).clone()
                        }))),
                    ))
                }
                DefT(
                    rp,
                    deref!(
                        PolyT(box PolyTData {
                            kind: poly_kind,
                            tparams_loc,
                            tparams,
                            t_out: deref!(DefT(rf, deref!(FunT(s, funtype)))),
                            id: poly_id,
                            strictness_kind,
                        })
                    ),
                ) if funtype.effect_ == ReactEffectType::ArbitraryEffect => {
                    let new_funtype = FunType {
                        effect_: ReactEffectType::AnyEffect,
                        ..(**funtype).clone()
                    };
                    let new_t_out = Type::new(DefT(
                        rf.dupe(),
                        DefT::new(FunT(s.dupe(), Rc::new(new_funtype))),
                    ));
                    let call = Type::new(DefT(
                        rp.dupe(),
                        DefT::new(PolyT(Box::new(PolyTData {
                            kind: *poly_kind,
                            tparams_loc: tparams_loc.dupe(),
                            tparams: tparams.dupe(),
                            t_out: new_t_out,
                            id: poly_id.dupe(),
                            strictness_kind: *strictness_kind,
                        }))),
                    ));
                    let new_id = cx.make_call_prop(call);
                    Type::new(DefT(
                        r.dupe(),
                        DefT::new(ObjT(Rc::new(ObjType {
                            call_t: Some(new_id),
                            ..(**obj).clone()
                        }))),
                    ))
                }
                _ => l.dupe(),
            };
            let tout_t = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (&t, &tout_t))?;
        }
        (IntersectionT(_, _) | NominalT { .. }, DeepReadOnlyT(tout, _) | HooklikeT(tout)) => {
            let tout_t = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (l, &tout_t))?;
        }
        (DefT(_, deref!(PolyT(box _))), DeepReadOnlyT(tout, _) | HooklikeT(tout)) => {
            let tout_t = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (l, &tout_t))?;
        }
        (
            DefT(r, deref!(InstanceT(instance))),
            GetKeysT { .. }
            | GetValuesT { .. }
            | GetDictValuesT { .. }
            | CallT(..)
            | LookupT(..)
            | SetPropT(..)
            | GetPropT(..)
            | MethodT(box MethodTData {
                use_op: _,
                reason: _,
                prop_reason: _,
                propref: _,
                method_action: _,
            })
            | ObjRestT(_, _, _, _, _)
            | SetElemT(..)
            | GetElemT(..)
            | CallElemT(..),
        ) if let inst = &instance.inst
            && let Some(ReactDro(dro_loc, dro_type)) = inst.inst_react_dro.as_ref()
            && inst.type_args.len() == 2
            && flow_js_utils::is_builtin_class_id("Map", inst.class_id.dupe(), cx) =>
        {
            let (_, _, key_t, _) = &inst.type_args[0];
            let (_, _, val_t, _) = &inst.type_args[1];

            match u.deref() {
                MethodT(box MethodTData {
                    use_op,
                    reason: _,
                    prop_reason: reason,
                    propref: box PropRef::Named { name, .. },
                    method_action: _,
                }) if matches!(name.as_str_opt(), Some("clear" | "delete" | "set")) => {
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EPropNotReadable(Box::new(EPropNotReadableData {
                            prop_loc: reason.loc().dupe(),
                            prop_name: Some(name.dupe()),
                            use_op: UseOp::Frame(
                                Arc::new(VirtualFrameUseOp::ReactDeepReadOnly(Box::new((
                                    dro_loc.dupe(),
                                    dro_type.clone(),
                                )))),
                                Arc::new(use_op.dupe()),
                            ),
                        })),
                    )?;
                }
                GetPropT(
                    box data @ GetPropTData {
                        propref: box PropRef::Named { name, .. },
                        ..
                    },
                ) if matches!(name.as_str_opt(), Some("clear" | "delete" | "set")) => {
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EPropNotReadable(Box::new(EPropNotReadableData {
                            prop_loc: data.reason.loc().dupe(),
                            prop_name: Some(name.dupe()),
                            use_op: UseOp::Frame(
                                Arc::new(VirtualFrameUseOp::ReactDeepReadOnly(Box::new((
                                    dro_loc.dupe(),
                                    dro_type.clone(),
                                )))),
                                Arc::new(data.use_op.dupe()),
                            ),
                        })),
                    )?;
                }
                _ => {
                    let key_t = mk_react_dro(
                        cx,
                        unknown_use(),
                        ReactDro(dro_loc.dupe(), dro_type.clone()),
                        key_t.dupe(),
                    );
                    let val_t = mk_react_dro(
                        cx,
                        unknown_use(),
                        ReactDro(dro_loc.dupe(), dro_type.clone()),
                        val_t.dupe(),
                    );
                    let ro_map = get_builtin_typeapp(
                        cx,
                        env,
                        r,
                        Some(true),
                        "$ReadOnlyMap",
                        vec![key_t, val_t],
                    );
                    let dro_loc = dro_loc.dupe();
                    let dro_type = dro_type.clone();
                    let new_u = type_util::mod_use_op_of_use_t(
                        |use_op: &UseOp| {
                            UseOp::Frame(
                                Arc::new(VirtualFrameUseOp::ReactDeepReadOnly(Box::new((
                                    dro_loc.dupe(),
                                    dro_type.clone(),
                                )))),
                                Arc::new(use_op.dupe()),
                            )
                        },
                        u,
                    );
                    rec_flow(cx, env, trace, (&ro_map, &new_u))?;
                }
            }
        }
        (
            DefT(r, deref!(InstanceT(instance))),
            GetKeysT { .. }
            | GetValuesT { .. }
            | GetDictValuesT { .. }
            | CallT(..)
            | LookupT(..)
            | SetPropT(..)
            | GetPropT(..)
            | MethodT(box MethodTData {
                use_op: _,
                reason: _,
                prop_reason: _,
                propref: _,
                method_action: _,
            })
            | ObjRestT(_, _, _, _, _)
            | SetElemT(..)
            | GetElemT(..)
            | CallElemT(..),
        ) if let inst = &instance.inst
            && let Some(ReactDro(dro_loc, dro_type)) = inst.inst_react_dro.as_ref()
            && inst.type_args.len() == 1
            && flow_js_utils::is_builtin_class_id("Set", inst.class_id.dupe(), cx) =>
        {
            let (_, _, elem_t, _) = &inst.type_args[0];
            match u.deref() {
                MethodT(box MethodTData {
                    use_op,
                    reason: _,
                    prop_reason: reason,
                    propref: box PropRef::Named { name, .. },
                    method_action: _,
                }) if matches!(name.as_str_opt(), Some("add" | "clear" | "delete")) => {
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EPropNotReadable(Box::new(EPropNotReadableData {
                            prop_loc: reason.loc().dupe(),
                            prop_name: Some(name.dupe()),
                            use_op: UseOp::Frame(
                                Arc::new(VirtualFrameUseOp::ReactDeepReadOnly(Box::new((
                                    dro_loc.dupe(),
                                    dro_type.clone(),
                                )))),
                                Arc::new(use_op.dupe()),
                            ),
                        })),
                    )?;
                }
                GetPropT(
                    box data @ GetPropTData {
                        propref: box PropRef::Named { name, .. },
                        ..
                    },
                ) if matches!(name.as_str_opt(), Some("add" | "clear" | "delete")) => {
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EPropNotReadable(Box::new(EPropNotReadableData {
                            prop_loc: data.reason.loc().dupe(),
                            prop_name: Some(name.dupe()),
                            use_op: UseOp::Frame(
                                Arc::new(VirtualFrameUseOp::ReactDeepReadOnly(Box::new((
                                    dro_loc.dupe(),
                                    dro_type.clone(),
                                )))),
                                Arc::new(data.use_op.dupe()),
                            ),
                        })),
                    )?;
                }
                _ => {
                    let elem_t = mk_react_dro(
                        cx,
                        unknown_use(),
                        ReactDro(dro_loc.dupe(), dro_type.clone()),
                        elem_t.dupe(),
                    );
                    let ro_set =
                        get_builtin_typeapp(cx, env, r, Some(true), "$ReadOnlySet", vec![elem_t]);
                    let dro_loc = dro_loc.dupe();
                    let dro_type = dro_type.clone();
                    let new_u = type_util::mod_use_op_of_use_t(
                        |use_op: &UseOp| {
                            UseOp::Frame(
                                Arc::new(VirtualFrameUseOp::ReactDeepReadOnly(Box::new((
                                    dro_loc.dupe(),
                                    dro_type.clone(),
                                )))),
                                Arc::new(use_op.dupe()),
                            )
                        },
                        u,
                    );
                    rec_flow(cx, env, trace, (&ro_set, &new_u))?;
                }
            }
        }
        // ***************
        // * maybe types *
        // ***************

        // The type maybe(T) is the same as null | undefined | UseT<Cx>
        (DefT(r, deref!(NullT | VoidT)), FilterMaybeT(use_op, tout)) => {
            let empty = empty_t::why(r.dupe());
            rec_flow_t(cx, env, trace, use_op.dupe(), (&empty, tout))?;
        }
        (DefT(r, deref!(MixedT(MixedFlavor::MixedEverything))), FilterMaybeT(use_op, tout)) => {
            let mixed = Type::new(DefT(
                r.dupe(),
                DefT::new(MixedT(MixedFlavor::MixedNonMaybe)),
            ));
            rec_flow_t(cx, env, trace, use_op.dupe(), (&mixed, tout))?;
        }
        (OptionalT { type_: tout, .. }, FilterMaybeT(_, _))
        | (MaybeT(_, tout), FilterMaybeT(_, _)) => {
            rec_flow(cx, env, trace, (tout, u))?;
        }
        (DefT(_, deref!(EmptyT)), FilterMaybeT(use_op, tout)) => {
            rec_flow_t(cx, env, trace, use_op.dupe(), (l, tout))?;
        }
        (
            MaybeT(_, _),
            ReposLowerT {
                reason: reason_op,
                use_desc,
                use_t: inner_u,
            },
        ) => {
            // Don't split the maybe type into its constituent members. Instead,
            // reposition the entire maybe type.
            let loc = reason_op.loc().dupe();
            let desc = if *use_desc {
                Some(reason_op.desc(true))
            } else {
                None
            };
            let repos = helpers::reposition(cx, env, Some(trace), loc, desc, None, l.dupe())?;
            rec_flow(cx, env, trace, (&repos, inner_u))?;
        }
        (
            MaybeT(_, _),
            ResolveUnionT(box ResolveUnionTData {
                reason,
                resolved,
                unresolved,
                upper,
                id,
            }),
        ) => {
            if let Some((tc_l, tc_u, reason)) =
                resolve_union(cx, env, trace, reason, *id, resolved, unresolved, l, upper)?
            {
                return Ok(Some(TailCall::Flow(tc_l, tc_u, reason)));
            }
        }
        (MaybeT(reason, t), _)
            if match u.deref() {
                ConditionalT(box ConditionalTData {
                    distributive_tparam_name,
                    ..
                }) => distributive_tparam_name.is_some(),
                _ => true,
            } =>
        {
            let reason = reason.dupe().replace_desc(VirtualReasonDesc::RNullOrVoid);
            let t = type_util::push_type_alias_reason(&reason, t.dupe());
            rec_flow(cx, env, trace, (&null::make(reason.dupe()), u))?;
            rec_flow(cx, env, trace, (&void::make(reason), u))?;
            rec_flow(cx, env, trace, (&t, u))?;
        }

        // ******************
        // * optional types *
        // ******************

        // The type optional(T) is the same as undefined | UseT<Cx>
        (DefT(r, deref!(VoidT)), FilterOptionalT(use_op, tout)) => {
            let empty = empty_t::why(r.dupe());
            rec_flow_t(cx, env, trace, use_op.dupe(), (&empty, tout))?;
        }
        (OptionalT { type_: tout, .. }, FilterOptionalT(_, _)) => {
            rec_flow(cx, env, trace, (tout, u))?;
        }
        (
            OptionalT { .. },
            ReposLowerT {
                reason,
                use_desc,
                use_t: inner_u,
            },
        ) => {
            // Don't split the optional type into its constituent members. Instead,
            // reposition the entire optional type.
            let repos = helpers::reposition_reason(cx, env, Some(trace), reason, *use_desc, l)?;
            rec_flow(cx, env, trace, (&repos, inner_u))?;
        }
        (
            OptionalT { .. },
            ResolveUnionT(box ResolveUnionTData {
                reason,
                resolved,
                unresolved,
                upper,
                id,
            }),
        ) => {
            if let Some((tc_l, tc_u, reason)) =
                resolve_union(cx, env, trace, reason, *id, resolved, unresolved, l, upper)?
            {
                return Ok(Some(TailCall::Flow(tc_l, tc_u, reason)));
            }
        }
        (
            OptionalT {
                reason: r,
                type_: t,
                use_desc,
            },
            _,
        ) if match u.deref() {
            ConditionalT(box ConditionalTData {
                distributive_tparam_name,
                ..
            }) => distributive_tparam_name.is_some(),
            _ => true,
        } =>
        {
            let void_t = void::why_with_use_desc(*use_desc, r.dupe());
            rec_flow(cx, env, trace, (&void_t, u))?;
            rec_flow(cx, env, trace, (t, u))?;
        }
        // *********************
        // * type applications *
        // *********************
        (
            TypeAppT(box TypeAppTData {
                reason: reason_tapp,
                type_: c,
                targs: ts,
                ..
            }),
            ConcretizeT(box ConcretizeTData {
                kind: ConcretizationKind::ConcretizeForClassExtends(this_t),
                ..
            }),
        ) => {
            let reason_op = upper_operation_reason_or(u, reason_tapp);
            FlowJs::instantiate_this_class_with_env(
                cx,
                env,
                trace,
                &reason_op,
                reason_tapp,
                c,
                Some(ts.dupe()),
                this_t,
                &Cont::Upper(Box::new(u.dupe())),
            )?;
        }
        (
            ThisTypeAppT(box ThisTypeAppTData {
                reason: reason_tapp,
                this_t: this,
                type_: c,
                targs: ts,
            }),
            _,
        ) => {
            let reason_op = upper_operation_reason_or(u, reason_tapp);
            let seen = match u.deref() {
                ConcretizeT(box ConcretizeTData { seen, .. }) => Some(seen),
                _ => None,
            };
            if !seen.is_some_and(|seen| seen.is_expanding_type_app(l)) {
                if let Some(seen) = seen {
                    seen.push_expanding_type_app(l.dupe());
                }
                let result = FlowJs::instantiate_this_class_with_env(
                    cx,
                    env,
                    trace,
                    &reason_op,
                    reason_tapp,
                    c,
                    ts.dupe(),
                    this,
                    &Cont::Upper(Box::new(u.dupe())),
                );
                if let Some(seen) = seen {
                    seen.pop_expanding_type_app();
                }
                result?;
            }
        }
        (
            TypeAppT(..),
            ReposLowerT {
                reason,
                use_desc,
                use_t: inner_u,
            },
        ) => {
            let repos = helpers::reposition_reason(cx, env, Some(trace), reason, *use_desc, l)?;
            rec_flow(cx, env, trace, (&repos, inner_u))?;
        }
        (
            TypeAppT(box TypeAppTData {
                reason: reason_tapp,
                use_op,
                type_,
                targs,
                from_value,
                use_desc,
            }),
            MethodT(box MethodTData {
                use_op: _,
                reason: reason_op,
                prop_reason: _,
                propref: _,
                method_action: _,
            })
            | PrivateMethodT(box PrivateMethodTData {
                use_op: _,
                reason: reason_op,
                prop_reason: _,
                name: _,
                class_bindings: _,
                static_: _,
                method_action: _,
            }),
        ) => {
            let t = mk_typeapp_instance_annot(
                cx,
                env,
                Some(trace),
                use_op.dupe(),
                reason_op,
                reason_tapp,
                *from_value,
                Some(*use_desc),
                type_,
                targs.dupe(),
            )?;
            rec_flow(cx, env, trace, (&t, u))?;
        }
        // This is the second step in checking a TypeAppT ~> TypeAppT.
        // The first step is in subtyping_kit.rs, and concretizes the c (the
        // `type_` field) for our upper bound TypeAppT.
        //
        // When we have done that, then we want to concretize the lower bound. We
        // flip all our arguments to ConcretizeTypeAppsT and set the final element
        // to false to signal that we have concretized the upper bound's c.
        //
        // If the upper bound's c is not a PolyT then we will fall down to an
        // incompatible use error.
        (
            DefT(_, deref!(PolyT(_))),
            ConcretizeTypeAppsT(use_op, box (ts2, fv2, op2, r2), box (c1, ts1, fv1, op1, r1), true),
        ) => {
            let c2 = l.dupe();
            rec_flow(
                cx,
                env,
                trace,
                (
                    c1,
                    &UseT::new(ConcretizeTypeAppsT(
                        use_op.dupe(),
                        Box::new((ts1.dupe(), *fv1, op1.dupe(), r1.dupe())),
                        Box::new((c2, ts2.dupe(), *fv2, op2.dupe(), r2.dupe())),
                        false,
                    )),
                ),
            )?;
        }
        // When we have concretized the c for our lower bound TypeAppT then we can
        // finally run our TypeAppT ~> TypeAppT logic. If we have referentially the
        // same PolyT for each TypeAppT then we want to check the type arguments
        // only. (Checked in the match guard.) If we do not have the same PolyT
        // for each TypeAppT then we want to expand our TypeAppTs and compare the
        // expanded results.
        //
        // If the lower bound's c is not a PolyT then we will fall down to an
        // incompatible use error.
        //
        // The upper bound's c should always be a PolyT here since we could not have
        // made it here if it was not given the logic of our earlier case.
        (
            DefT(
                _,
                deref!(
                    PolyT(box PolyTData {
                        tparams_loc,
                        tparams,
                        id: id1,
                        t_out,
                        strictness_kind: lower_strictness_kind,
                        ..
                    })
                ),
            ),
            ConcretizeTypeAppsT(
                use_op,
                box (ts1, fv1, _, r1),
                box (
                    deref!(DefT(
                        _,
                        deref!(
                            PolyT(box PolyTData {
                                id: id2,
                                strictness_kind: upper_strictness_kind,
                                ..
                            })
                        )
                    )),
                    ts2,
                    fv2,
                    _,
                    r2,
                ),
                false,
            ),
        ) if id1 == id2
            && ts1.len() == ts2.len()
            && !flow_js_utils::wraps_utility_type(cx, t_out)
            && fv1 == fv2 =>
        {
            let targs: Vec<_> = ts1
                .iter()
                .zip(ts2.iter())
                .map(|(t1, t2)| (t1.dupe(), t2.dupe()))
                .collect();
            type_app_variance_check(
                cx,
                env,
                trace,
                use_op.dupe(),
                r1,
                r2,
                targs,
                tparams_loc.dupe(),
                &Vec1::try_from_vec(tparams.to_vec()).unwrap(),
                lower_strictness_kind.join(*upper_strictness_kind),
            )?;
        }
        // This is the case which implements the expansion for our
        // TypeAppT ~> TypeAppT when the two PolyTs are unequal.
        (
            DefT(
                _,
                deref!(
                    PolyT(box PolyTData {
                        tparams_loc: tparams_loc1,
                        tparams: xs1,
                        t_out: t1,
                        id: id1,
                        ..
                    })
                ),
            ),
            ConcretizeTypeAppsT(
                use_op,
                box (ts1, fv1, op1, r1),
                box (
                    deref!(DefT(
                        _,
                        deref!(
                            PolyT(box PolyTData {
                                tparams_loc: tparams_loc2,
                                tparams: xs2,
                                t_out: t2,
                                id: id2,
                                ..
                            })
                        )
                    )),
                    ts2,
                    fv2,
                    op2,
                    r2,
                ),
                false,
            ),
        ) => {
            let (op1_eff, op2_eff) = match root_of_use_op(use_op) {
                RootUseOp::UnknownUse => (op1.dupe(), op2.dupe()),
                _ => (use_op.dupe(), use_op.dupe()),
            };
            let t1_inst = mk_typeapp_instance_of_poly(
                cx,
                env,
                trace,
                op2_eff,
                r2,
                r1,
                *fv1,
                id1.dupe(),
                tparams_loc1.dupe(),
                Vec1::try_from_vec(xs1.to_vec()).unwrap(),
                t1,
                ts1.dupe(),
            )?;
            let t2_inst = mk_typeapp_instance_of_poly(
                cx,
                env,
                trace,
                op1_eff,
                r1,
                r2,
                *fv2,
                id2.dupe(),
                tparams_loc2.dupe(),
                Vec1::try_from_vec(xs2.to_vec()).unwrap(),
                t2,
                ts2.dupe(),
            )?;
            rec_flow(
                cx,
                env,
                trace,
                (&t1_inst, &UseT::new(UseT(use_op.dupe(), t2_inst))),
            )?;
        }
        (
            TypeAppT(box TypeAppTData {
                reason: reason_tapp,
                use_op,
                type_,
                targs,
                from_value,
                use_desc: _,
            }),
            _,
        ) => {
            let reason_op = upper_operation_reason_or(u, reason_tapp);
            // Concretization bypasses the constraint cache, each expansion is wrapped in a
            // fresh tvar, and the expansion stack only counts growing targs, so an
            // application reappearing within its own expansion (e.g. `type T<X> = T<X>`)
            // would otherwise be expanded until the recursion limit.
            let seen = match u.deref() {
                ConcretizeT(box ConcretizeTData { seen, .. }) => Some(seen),
                _ => None,
            };
            if !seen.is_some_and(|seen| seen.is_expanding_type_app(l))
                && let Some(env) = instantiation_utils::type_app_expansion::push_unless_loop(
                    cx,
                    env,
                    type_app_expansion::Bound::Lower,
                    type_,
                    targs,
                )
            {
                let env = &env;
                if let Some(seen) = seen {
                    seen.push_expanding_type_app(l.dupe());
                }
                let result = mk_typeapp_instance_annot(
                    cx,
                    env,
                    Some(trace),
                    use_op.dupe(),
                    &reason_op,
                    reason_tapp,
                    *from_value,
                    None,
                    type_,
                    targs.dupe(),
                )
                .and_then(|t| rec_flow(cx, env, trace, (&t, u)));
                if let Some(seen) = seen {
                    seen.pop_expanding_type_app();
                }
                result?;
            }
        }
        // Concretize types for type inspection purpose up to this point. The rest are
        // recorded as lower bound to the target tvar.
        (
            _,
            ConcretizeT(box ConcretizeTData {
                kind: ConcretizationKind::ConcretizeForImportsExports,
                collector,
                ..
            }),
        ) => {
            collector.add(l.dupe());
        }

        // unwrap namespace type into object type, drop all information about types in the namespace
        (
            NamespaceT(ns),
            GetPropT(
                box data @ GetPropTData {
                    use_op: UseOp::Op(deref!(VirtualRootUseOp::GetProperty(prop_reason))),
                    ..
                },
            ),
        ) if ns.namespace_symbol.kind() == flow_common::flow_symbol::SymbolKind::SymbolModule => {
            let new_u = UseT::new(GetPropT(Box::new(GetPropTData {
                use_op: UseOp::Op(Arc::new(VirtualRootUseOp::GetExport(prop_reason.dupe()))),
                reason: data.reason.dupe(),
                id: data.id,
                from_annot: data.from_annot,
                skip_optional: data.skip_optional,
                propref: data.propref.clone(),
                tout: data.tout.clone(),
                hint: data.hint.clone(),
            })));
            rec_flow(cx, env, trace, (&ns.values_type, &new_u))?;
        }
        // Preserve a TS enum namespace through inspection-concretization so that
        // ValueToTypeReferenceTransform can turn it into the union of member
        // literals (see run_on_concrete_type). Without this, the generic unwrap
        // below would strip it to its values_type before the conversion runs.
        (
            NamespaceT(ns),
            ConcretizeT(box ConcretizeTData {
                kind:
                    ConcretizationKind::ConcretizeForInspection
                    | ConcretizationKind::ConcretizeForClassExtends(_),
                collector,
                ..
            }),
        ) if flow_js_utils::is_ts_enum_symbol(&ns.namespace_symbol) => {
            collector.add(l.dupe());
        }
        // unwrap namespace type into object type, drop all information about types in the namespace
        (NamespaceT(ns), _) => {
            rec_flow(cx, env, trace, (&ns.values_type, u))?;
        }

        // ***************************************
        // * transform values to type references *
        // ***************************************
        (
            DefT(
                reason_tapp,
                deref!(
                    PolyT(box PolyTData {
                        tparams_loc,
                        tparams: ids,
                        t_out,
                        id,
                        ..
                    })
                ),
            ),
            ValueToTypeReferenceT(box ValueToTypeReferenceTData {
                use_op,
                reason: reason_op,
                ..
            }),
        ) if !ids.is_empty() && ids.iter().all(|tp| tp.default.is_some()) => {
            // Treat missing type args the same as empty type args (Foo = Foo<>) when all
            // parameters have defaults. Otherwise, fall through to EMissingTypeArgs.
            let t = FlowJs::mk_typeapp_of_poly_with_env(
                cx,
                env,
                trace,
                use_op.dupe(),
                reason_op,
                reason_tapp,
                id.dupe(),
                tparams_loc.dupe(),
                Vec1::try_from_vec(ids.to_vec()).unwrap(),
                t_out,
                Rc::from([]),
            )?;
            rec_flow(cx, env, trace, (&t, u))?;
        }
        (
            _,
            ValueToTypeReferenceT(box ValueToTypeReferenceTData {
                use_op,
                reason: reason_op,
                kind: type_t_kind,
                tout,
            }),
        ) => {
            let t =
                flow_js_utils::value_to_type_reference_transform::run_on_concrete_type_with_env(
                    cx,
                    env,
                    use_op.dupe(),
                    reason_op,
                    *type_t_kind,
                    l.dupe(),
                )?;
            let tout_t = Type::new(OpenT((**tout).dupe()));
            unify_opt(
                cx,
                env,
                Some(trace),
                use_op.dupe(),
                UnifyCause::Uncategorized,
                Some(true),
                &t,
                &tout_t,
            )?;
        }

        // **********************
        // *    opaque types    *
        // **********************

        // Repositioning should happen before opaque types are considered so that we can
        // have the "most recent" location when we do look at the opaque type
        (
            NominalT { .. },
            ReposLowerT {
                reason,
                use_desc,
                use_t: inner_u,
            },
        ) => {
            let repos = helpers::reposition_reason(cx, env, Some(trace), reason, *use_desc, l)?;
            rec_flow(cx, env, trace, (&repos, inner_u))?;
        }

        // An opaque indexer key goes to `ToStringT`, like every key that is not
        // a `unique symbol`. It is re-dispatched here rather than at the general
        // `GetKeysDictKeyT` rule, which sits past the point where an opaque type
        // is unwrapped to its underlying type or supertype: the rules just below
        // are what keep the opaque type itself in the key set.
        (NominalT { .. }, GetKeysDictKeyT { reason, t_out, .. }) => {
            rec_flow(
                cx,
                env,
                trace,
                (
                    l,
                    &UseT::new(ToStringT {
                        orig_t: None,
                        reason: reason.dupe(),
                        t_out: t_out.clone(),
                    }),
                ),
            )?;
        }

        // Store the opaque type when doing `ToStringT`, so we can use that
        // rather than just `string` if the underlying is `string`.
        (
            NominalT {
                reason: _,
                nominal_type,
            },
            ToStringT {
                orig_t: _,
                reason: to_string_reason,
                t_out,
            },
        ) if let nominal::Id::UserDefinedOpaqueTypeId(
            box nominal::UserDefinedOpaqueTypeIdData(nominal_id, _),
        ) = &nominal_type.nominal_id
            && nominal_id.0.source() == Some(cx.file())
            && let nominal::UnderlyingT::OpaqueWithLocal { t } = &nominal_type.underlying_t =>
        {
            rec_flow(
                cx,
                env,
                trace,
                (
                    t,
                    &UseT::new(ToStringT {
                        orig_t: Some(l.dupe()),
                        reason: to_string_reason.dupe(),
                        t_out: t_out.clone(),
                    }),
                ),
            )?;
        }
        (
            NominalT {
                reason: _,
                nominal_type,
            },
            ToStringT {
                orig_t: _,
                reason: to_string_reason,
                t_out,
            },
        ) if let nominal::Id::UserDefinedOpaqueTypeId(_) = &nominal_type.nominal_id
            && let nominal::UnderlyingT::CustomError(box nominal::CustomErrorData {
                t,
                custom_error_loc: _,
            }) = &nominal_type.underlying_t =>
        {
            rec_flow(
                cx,
                env,
                trace,
                (
                    t,
                    &UseT::new(ToStringT {
                        orig_t: Some(l.dupe()),
                        reason: to_string_reason.dupe(),
                        t_out: t_out.clone(),
                    }),
                ),
            )?;
        }

        // Use the upper bound of NominalT if it's available, for operations that must be
        // performed on some concretized types. Object kit resolves an `ExactObjectType` itself,
        // to keep it exact.
        (NominalT { nominal_type, .. }, ObjKitT { .. })
            if let Some(t) = nominal_type.upper_t.as_ref()
                && nominal_type.nominal_id != nominal::Id::ExactObjectType =>
        {
            rec_flow(cx, env, trace, (t, u))?;
        }
        (
            NominalT { nominal_type, .. },
            ReactKitT(box ReactKitTData {
                use_op: _,
                reason: _,
                tool: _,
            }),
        ) if let Some(t) = nominal_type.upper_t.as_ref() => {
            rec_flow(cx, env, trace, (t, u))?;
        }
        // Store the opaque type when doing `ToStringT`, so we can use that
        // rather than just `string` if the supertype is `string`
        (
            NominalT { nominal_type, .. },
            ToStringT {
                orig_t: _,
                reason: to_string_reason,
                t_out,
            },
        ) if let Some(t) = nominal_type.upper_t.as_ref() => {
            rec_flow(
                cx,
                env,
                trace,
                (
                    t,
                    &UseT::new(ToStringT {
                        orig_t: Some(l.dupe()),
                        reason: to_string_reason.dupe(),
                        t_out: t_out.clone(),
                    }),
                ),
            )?;
        }
        // If the type is still in the same file it was defined, we allow it to
        // expose its underlying type information
        (NominalT { nominal_type, .. }, _)
            if let nominal::Id::UserDefinedOpaqueTypeId(
                box nominal::UserDefinedOpaqueTypeIdData(nominal_id, _),
            ) = &nominal_type.nominal_id
                && nominal_id.0.source() == Some(cx.file())
                && let nominal::UnderlyingT::OpaqueWithLocal { t } = &nominal_type.underlying_t =>
        {
            rec_flow(cx, env, trace, (t, u))?;
        }
        (NominalT { nominal_type, .. }, _)
            if let nominal::Id::UserDefinedOpaqueTypeId(
                box nominal::UserDefinedOpaqueTypeIdData(_, _),
            ) = &nominal_type.nominal_id
                && let nominal::UnderlyingT::CustomError(box nominal::CustomErrorData {
                    t,
                    custom_error_loc: _,
                }) = &nominal_type.underlying_t =>
        {
            rec_flow(cx, env, trace, (t, u))?;
        }

        // *****************************************************
        // * keys (NOTE: currently we only support string keys *
        // *****************************************************
        (KeysT(_, _), ToStringT { t_out, .. }) => {
            rec_flow(cx, env, trace, (l, t_out))?;
        }
        // A key set already classifies each member as a property key. Expanding
        // it again repeatedly flattens potentially huge dictionary key sets;
        // recursive shapes can also feed it back into the same `GetKeysT`.
        (
            KeysT(_, _),
            GetKeysDictKeyT {
                t_out,
                include_symbols: true,
                ..
            },
        ) => {
            rec_flow(cx, env, trace, (l, t_out))?;
        }
        (KeysT(reason1, o1), _) => {
            // flow all keys of o1 to u
            rec_flow(
                cx,
                env,
                trace,
                (
                    o1,
                    &UseT::new(GetKeysT {
                        reason: reason1.dupe(),
                        t_out: Box::new(u.dupe()),
                        include_symbols: true,
                    }),
                ),
            )?;
            if let UseT(_, t) = u.deref() {
                tvar_resolver::resolve(cx, tvar_resolver::default_no_lowers, true, t);
            }
        }
        (
            GenericT(box GenericTData { reason, bound, .. }),
            ConcretizeT(box ConcretizeTData {
                kind: ConcretizationKind::ConcretizeForEnumExhaustiveCheck,
                ..
            }),
        ) => {
            let repos = helpers::reposition_reason(cx, env, None, reason, false, bound)?;
            rec_flow(cx, env, trace, (&repos, u))?;
        }
        (
            _,
            ConcretizeT(box ConcretizeTData {
                kind:
                    ConcretizationKind::ConcretizeForInspection
                    | ConcretizationKind::ConcretizeForClassExtends(_),
                collector,
                ..
            }),
        ) => {
            collector.add(l.dupe());
        }
        // helpers
        (
            DefT(
                reason_o,
                deref!(ObjT(deref!(ObjType {
                    props_tmap: mapr,
                    flags,
                    ..
                }))),
            ),
            HasOwnPropT(box HasOwnPropTData {
                use_op,
                reason: reason_op,
                type_: key,
            }),
        ) => {
            let dropped = drop_generic(key.dupe());
            match (dropped.deref(), &flags.obj_kind) {
                // If we have a literal string and that property exists
                (DefT(_, deref!(SingletonStrT { value: x, .. })), _)
                    if cx.has_prop(mapr.dupe(), &Name::new(x.dupe())) => {}
                // If we have a symbol key and that property exists
                (DefT(_, deref!(UniqueSymbolT(data))), _)
                    if cx.has_prop(mapr.dupe(), &Name::symbol(data.symbol.dupe())) => {}
                // If we have a dictionary, try that next
                (
                    _,
                    ObjKind::Indexed(DictType {
                        key: expected_key, ..
                    }),
                ) => {
                    let mod_key = type_util::mod_reason_of_t(&|_| reason_op.dupe(), key);
                    rec_flow_t(cx, env, trace, use_op.dupe(), (&mod_key, expected_key))?;
                }
                _ => {
                    let names_a_prop = match dropped.deref() {
                        DefT(
                            _,
                            deref!(
                                SingletonStrT { .. }
                                    | StrGeneralT(_)
                                    | NumericStrKeyT(_)
                                    | UniqueSymbolT(_)
                            ),
                        ) => true,
                        TemplateLiteralT { .. } => true,
                        _ => false,
                    };
                    let err = if names_a_prop {
                        let (prop, suggestion) = match dropped.deref() {
                            DefT(_, deref!(SingletonStrT { value: prop, .. })) => (
                                Some(Name::new(prop.dupe())),
                                prop_typo_suggestion(cx, &[mapr.dupe()], prop.as_str()),
                            ),
                            // Name the missing symbol key, mirroring the `InstanceT`
                            // branch. Without this the error degrades to the computed
                            // "an index signature ... is missing" wording. There is no
                            // typo suggestion: a symbol key has no string spelling to
                            // match against string property names.
                            DefT(_, deref!(UniqueSymbolT(data))) => {
                                (Some(Name::symbol(data.symbol.dupe())), None)
                            }
                            _ => (None, None),
                        };
                        ErrorMessage::EPropNotFoundInLookup(Box::new(EPropNotFoundInLookupData {
                            prop_name: prop,
                            prop_loc: reason_op.loc().dupe(),
                            object: flow_js_utils::type_reference_with_reason_for_error(
                                l,
                                reason_o.dupe(),
                            ),
                            use_op: use_op.dupe(),
                            suggestion,
                        }))
                    } else {
                        let keys =
                            Type::new(KeysT(reason_o.dupe().replace_desc_new(None), l.dupe()));
                        flow_js_utils::incompatible_types_error(key, &keys, use_op.dupe(), None)
                    };
                    flow_js_utils::add_output_with_env(cx, env, err)?;
                }
            }
        }
        (
            DefT(reason_o, deref!(InstanceT(instance_t))),
            HasOwnPropT(box HasOwnPropTData {
                use_op,
                reason: reason_op,
                type_: key,
            }),
        ) if let Some(x) = match key.deref() {
            DefT(_, deref!(SingletonStrT { value, .. })) => Some(Name::new(value.dupe())),
            // A `unique symbol` key resolves to a distinct named member by its
            // nominal identity, mirroring the `ObjT` branch above.
            DefT(_, deref!(UniqueSymbolT(data))) => Some(Name::symbol(data.symbol.dupe())),
            GenericT(box GenericTData {
                bound: deref!(DefT(_, deref!(SingletonStrT { value, .. }))),
                ..
            }) => Some(Name::new(value.dupe())),
            _ => None,
        } =>
        {
            let (prop_ids, dict_keys) = flow_js_utils::key_sources_of_instance_t(
                |reason, t| helpers::possible_concrete_types_for_inspection(cx, env, reason, t),
                instance_t,
            )?;
            if prop_ids
                .iter()
                .any(|prop_id| cx.has_prop(prop_id.dupe(), &x))
            {
                // no-op
            } else {
                let dict_key = match dict_keys.len() {
                    0 => None,
                    1 => dict_keys.into_iter().next(),
                    _ => Some(type_util::union_of_ts(reason_op.dupe(), dict_keys, None)),
                };
                match dict_key {
                    Some(dict_key) => {
                        let mod_key = type_util::mod_reason_of_t(&|_| reason_op.dupe(), key);
                        rec_flow_t(cx, env, trace, use_op.dupe(), (&mod_key, &dict_key))?;
                    }
                    None => {
                        let err = ErrorMessage::EPropNotFoundInLookup(Box::new(
                            EPropNotFoundInLookupData {
                                prop_name: Some(x.dupe()),
                                prop_loc: reason_op.loc().dupe(),
                                object: flow_js_utils::type_reference_with_reason_for_error(
                                    l,
                                    reason_o.dupe(),
                                ),
                                use_op: use_op.dupe(),
                                suggestion: prop_typo_suggestion_for_name(cx, &prop_ids, &x),
                            },
                        ));
                        flow_js_utils::add_output_with_env(cx, env, err)?;
                    }
                }
            }
        }
        (
            DefT(reason_o, deref!(InstanceT(_))),
            HasOwnPropT(box HasOwnPropTData {
                use_op,
                reason: reason_op,
                type_: key,
            }),
        ) => {
            // Same as for objects above: a key that cannot spell a property
            // name is not a missing property, it is not a key at all.
            let dropped = drop_generic(key.dupe());
            let names_a_prop = match dropped.deref() {
                DefT(_, deref!(SingletonStrT { .. } | StrGeneralT(_) | NumericStrKeyT(_))) => true,
                TemplateLiteralT { .. } => true,
                _ => false,
            };
            let err = if names_a_prop {
                ErrorMessage::EPropNotFoundInLookup(Box::new(EPropNotFoundInLookupData {
                    prop_name: None,
                    prop_loc: reason_op.loc().dupe(),
                    object: flow_js_utils::type_reference_with_reason_for_error(l, reason_o.dupe()),
                    use_op: use_op.dupe(),
                    suggestion: None,
                }))
            } else {
                let keys = Type::new(KeysT(reason_o.dupe().replace_desc_new(None), l.dupe()));
                flow_js_utils::incompatible_types_error(key, &keys, use_op.dupe(), None)
            };
            flow_js_utils::add_output_with_env(cx, env, err)?;
        }
        // AnyT has every prop
        (
            AnyT(_, _),
            HasOwnPropT(box HasOwnPropTData {
                use_op: _,
                reason: _,
                type_: _,
            }),
        ) => {}
        (
            DefT(
                _,
                deref!(ObjT(deref!(ObjType {
                    flags,
                    props_tmap,
                    ..
                }))),
            ),
            GetKeysT {
                reason: reason_op,
                t_out: keys,
                include_symbols,
            },
        ) => {
            let dict_t = obj_type::get_dict_opt(&flags.obj_kind);
            // Type-level `keyof` includes symbols, while runtime key
            // enumeration (`Object.keys`) omits them, matching JavaScript.
            let keylist = flow_js_utils::keylist_of_props(
                env,
                &cx.find_props(props_tmap.dupe()),
                reason_op,
                *include_symbols,
            );
            let union_t = type_util::union_of_ts(reason_op.dupe(), keylist, None);
            rec_flow(cx, env, trace, (&union_t, keys))?;
            if let Some(DictType { key: dict_key, .. }) = dict_t {
                flow_dict_key_to_keys(cx, env, trace, dict_key, reason_op, keys, *include_symbols)?;
            }
        }
        (
            DefT(_, deref!(InstanceT(instance_t))),
            GetKeysT {
                reason: reason_op,
                t_out: keys,
                include_symbols,
            },
        ) => {
            let (prop_ids, dict_keys) = flow_js_utils::key_sources_of_instance_t(
                |reason, t| helpers::possible_concrete_types_for_inspection(cx, env, reason, t),
                instance_t,
            )?;
            let keylist =
                flow_js_utils::keylist_of_prop_ids(cx, env, &prop_ids, reason_op, *include_symbols);
            let union_t = type_util::union_of_ts(reason_op.dupe(), keylist, None);
            rec_flow(cx, env, trace, (&union_t, keys))?;
            for dict_key in dict_keys {
                flow_dict_key_to_keys(
                    cx,
                    env,
                    trace,
                    &dict_key,
                    reason_op,
                    keys,
                    *include_symbols,
                )?;
            }
        }
        // keyof any -> any: any in any out.
        (
            AnyT(_, src),
            GetKeysT {
                reason: reason_op,
                t_out: keys,
                include_symbols: _,
            },
        ) => {
            let any = any_t::why(*src, reason_op.dupe());
            rec_flow(cx, env, trace, (&any, keys))?;
        }

        // In general, typechecking is monotonic in the sense that more constraints
        // produce more errors. However, sometimes we may want to speculatively try
        // out constraints, backtracking if they produce errors (and removing the
        // errors produced). This is useful to typecheck union types and
        // intersection types: see below.
        // NOTE: It is important that any def type that simplifies to a union or
        // intersection of other def types be processed before we process unions
        // and intersections: otherwise we may get spurious errors.

        // **********
        // * values *
        // **********
        (DefT(_, deref!(ObjT(o))), GetValuesT(reason, values)) => {
            let values_l = flow_js_utils::get_values_type_of_obj_t(cx, o, reason.dupe());
            rec_flow_t(cx, env, trace, unknown_use(), (&values_l, values))?;
        }
        (DefT(_, deref!(InstanceT(instance_t))), GetValuesT(reason, values)) => {
            let inst = &instance_t.inst;
            let values_l = flow_js_utils::get_values_type_of_instance_t(
                cx,
                inst.own_props.dupe(),
                inst.inst_dict.as_ref(),
                reason.dupe(),
            );
            rec_flow_t(cx, env, trace, unknown_use(), (&values_l, values))?;
        }
        (DefT(_, deref!(ArrT(arr))), GetValuesT(reason, t_out)) => {
            let elem_t = elemt_of_arrtype(arr);
            let mod_elem = type_util::mod_reason_of_t(&|_| reason.dupe(), &elem_t);
            rec_flow_t(cx, env, trace, unknown_use(), (&mod_elem, t_out))?;
        }
        // Any will always be ok
        (AnyT(_, src), GetValuesT(reason, values)) => {
            let any = any_t::why(*src, reason.dupe());
            rec_flow_t(cx, env, trace, unknown_use(), (&any, values))?;
        }

        // ***********************************************
        // * Values of a dictionary - `mixed` otherwise. *
        // ***********************************************
        #[rustfmt::skip]
        (
            DefT(_, deref!(ObjT(deref!(ObjType {
                flags: Flags {
                    obj_kind: ObjKind::Indexed(DictType { value, dict_polarity, .. }),
                    ..
                },
                props_tmap,
                ..
            })))),
            GetDictValuesT(_, result),
        ) if Polarity::compat(*dict_polarity, Polarity::Positive)
                && cx.find_props(props_tmap.dupe()).is_empty() =>
        {
            rec_flow(cx, env, trace, (value, result))?;
        }
        (DefT(_, deref!(ObjT(_) | InstanceT(_))), GetDictValuesT(reason, result)) => {
            let mixed = mixed_t::why(reason.dupe());
            rec_flow(cx, env, trace, (&mixed, result))?;
        }
        // Any will always be ok
        (AnyT(_, src), GetDictValuesT(reason, result)) => {
            let any = any_t::why(*src, reason.dupe());
            rec_flow(cx, env, trace, (&any, result))?;
        }

        // ********************************
        // * union and intersection types *
        // ********************************
        // We don't want to miss any union optimizations because of unevaluated type destructors, so
        // if our union contains any of these problematic types, we force it to resolve its elements before
        // considering its upper bound
        (
            _,
            ResolveUnionT(box ResolveUnionTData {
                reason,
                resolved,
                unresolved,
                upper,
                id,
            }),
        ) => {
            if let Some((tc_l, tc_u, reason)) =
                resolve_union(cx, env, trace, reason, *id, resolved, unresolved, l, upper)?
            {
                return Ok(Some(TailCall::Flow(tc_l, tc_u, reason)));
            }
        }
        (UnionT(reason, rep), FilterMaybeT(use_op, tout)) => {
            let void_t = void::why(reason.dupe());
            let null_t = null::why(reason.dupe());
            let filter_null_and_void = |t: &Type| -> bool {
                type_util::quick_subtype(None::<&mut fn(&Type)>, t, &void_t)
                    || type_util::quick_subtype(None::<&mut fn(&Type)>, t, &null_t)
            };
            match rep.check_enum() {
                Some(_) => {
                    let filtered = flow_js_utils::remove_predicate_from_union(
                        reason.dupe(),
                        cx,
                        filter_null_and_void,
                        rep,
                    );
                    rec_flow_t(cx, env, trace, use_op.dupe(), (&filtered, tout))?;
                }
                None => {
                    let use_op_clone = use_op.dupe();
                    let non_maybe_union = flow_js_utils::map_union(
                        |cx_inner, trace_inner, t_inner, tout_inner| {
                            rec_flow(
                                cx_inner,
                                env,
                                *trace_inner,
                                (
                                    t_inner,
                                    &UseT::new(FilterMaybeT(
                                        use_op_clone.dupe(),
                                        tout_inner.dupe(),
                                    )),
                                ),
                            )?;
                            Ok(())
                        },
                        cx,
                        &trace,
                        rep,
                        reason.dupe(),
                    )?;
                    rec_flow_t(cx, env, trace, use_op.dupe(), (&non_maybe_union, tout))?;
                }
            }
        }
        (UnionT(reason, rep), _) if rep.members_iter().any(flow_js_utils::is_union_resolvable) => {
            flow_js_utils::iter_resolve_union(
                |cx_inner, trace_inner, (t_inner, u_inner)| {
                    rec_flow(cx_inner, env, *trace_inner, (&t_inner, &u_inner))?;
                    Ok(())
                },
                cx,
                &trace,
                reason.dupe(),
                rep,
                u.dupe(),
            )?;
        }
        // Don't split the union type into its constituent members.
        // Instead, reposition the entire union type.
        (
            UnionT(_, _),
            ReposLowerT {
                reason,
                use_desc,
                use_t: u_inner,
            },
        ) => {
            let repos_l = helpers::reposition_reason(cx, env, Some(trace), reason, *use_desc, l)?;
            rec_flow(cx, env, trace, (&repos_l, u_inner))?;
        }
        (
            UnionT(_, _),
            SealGenericT(box SealGenericTData {
                loc: _,
                id,
                name,
                cont,
                no_infer,
            }),
        ) => {
            let reason = reason_of_t(l);
            let generic = Type::new(GenericT(Box::new(GenericTData {
                reason: reason.dupe(),
                id: id.clone(),
                name: name.dupe(),
                bound: l.dupe(),
                no_infer: *no_infer,
            })));
            continue_(cx, env, trace, &generic, cont)?;
        }
        (UnionT(_, _), ObjKitT(use_op, reason, resolve_tool, tool, tout)) => {
            object_kit::run_with_env(
                trace,
                cx,
                env,
                use_op.dupe(),
                reason,
                resolve_tool,
                tool,
                l,
                tout,
            )?;
        }
        // Shortcut for indexed accesses with the same type as the dict key.
        #[rustfmt::skip]
        (
            UnionT(_, _),
            ElemT(box ElemTData {
                use_op,
                reason,
                obj: deref!(DefT(_, deref!(ObjT(deref!(ObjType {
                    flags: flags @ Flags {
                        obj_kind: ObjKind::Indexed(DictType { key, value, dict_polarity, .. }),
                        ..
                    },
                    ..
                }))))),
                action: box ElemAction::ReadElem(box ReadElemData { tout, .. }),
            }),
        ) if flow_typing_flow_common::concrete_type_eq::eq_with_env(cx, env, l, key)
            && Polarity::compat(*dict_polarity, Polarity::Positive) =>
        {
            let value = helpers::reposition_reason(cx, env, Some(trace), reason, false, value)?;
            let value = match &flags.react_dro {
                Some(dro) => {
                    let frame_use_op = VirtualUseOp::Frame(
                        Arc::new(FrameUseOp::ReactDeepReadOnly(Box::new((
                            dro.0.dupe(),
                            dro.1.clone(),
                        )))),
                        Arc::new(use_op.dupe()),
                    );
                    mk_react_dro(cx, frame_use_op, dro.clone(), value)
                }
                None => value,
            };
            let open_tout = Type::new(OpenT(tout.dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (&value, &open_tout))?;
        }
        (
            UnionT(_, rep),
            ElemT(box ElemTData {
                use_op,
                reason,
                obj,
                action:
                    box ElemAction::ReadElem(box ReadElemData {
                        id,
                        from_annot: true,
                        skip_optional,
                        access_iterables,
                        tout,
                    }),
            }),
        ) => {
            let distribute = |cx: &mut Context<'cx>| -> Result<(), FlowJsException> {
                let reason = reason
                    .dupe()
                    .update_desc(|d| d.map(VirtualReasonDesc::invalidate_rtype_alias));
                let mut members = rep.members_iter();
                let t0 = members.next().unwrap();
                let t1 = members.next().unwrap();
                let ts: Vec<_> = members.duped().collect();
                let mut f = |t: &Type| -> Result<Type, FlowJsException> {
                    flow_typing_tvar::mk_no_wrap_where(
                        cx,
                        reason.dupe(),
                        |cx, tvar_reason, tvar_id| {
                            let tvar = Tvar::new(tvar_reason.dupe(), tvar_id as u32);
                            let action = ElemAction::ReadElem(Box::new(ReadElemData {
                                id: *id,
                                from_annot: true,
                                skip_optional: *skip_optional,
                                access_iterables: *access_iterables,
                                tout: tvar,
                            }));
                            rec_flow(
                                cx,
                                env,
                                trace,
                                (
                                    t,
                                    &UseT::new(ElemT(Box::new(ElemTData {
                                        use_op: use_op.dupe(),
                                        reason: reason.dupe(),
                                        obj: obj.dupe(),
                                        action: Box::new(action),
                                    }))),
                                ),
                            )
                        },
                    )
                };
                let ft0 = f(t0)?;
                let ft1 = f(t1)?;
                let mut fts_vec = Vec::with_capacity(ts.len());
                for t in ts.iter() {
                    fts_vec.push(f(t)?);
                }
                let fts: Rc<[_]> = fts_vec.into();
                let new_rep =
                    union_rep::make(None, union_rep::UnionKind::UnknownKind, ft0, ft1, fts);
                let union_t = Type::new(UnionT(reason, new_rep));
                let open_tout = Type::new(OpenT(tout.dupe()));
                rec_flow_t(cx, env, trace, unknown_use(), (&union_t, &open_tout))?;
                Ok(())
            };
            // Fast path for large Union-of-keys ~> ElemT on an ObjT without a dict.
            // Instead of distributing the union and flowing each key through
            // GetPropT individually (which is O(n) flows for n keys), directly
            // look up each key in the property map (O(1) hash lookup per key)
            // and collect the value types into a union.
            //
            // We only apply this for large unions (>100 members) to avoid adding
            // overhead to the common small-union case. The semantics are equivalent:
            // skip_optional and no_unchecked_indexed_access are irrelevant here since
            // we require no dict and only match properties that exist in the prop map.
            match obj.deref() {
                DefT(_, deref!(ObjT(obj_t)))
                    if obj_type::get_dict_opt(&obj_t.flags.obj_kind).is_none()
                        && rep.members_iter().count() > 100 =>
                {
                    let props = cx.find_props(obj_t.props_tmap.dupe());
                    let members: Vec<_> = rep.members_iter().collect();
                    let value_types: Vec<_> = members
                        .iter()
                        .filter_map(|t| {
                            lookup_prop_type_direct(cx, use_op, &obj_t.flags.react_dro, &props, t)
                        })
                        .collect();
                    if value_types.len() == members.len() {
                        let reason = reason
                            .dupe()
                            .update_desc(|d| d.map(VirtualReasonDesc::invalidate_rtype_alias));
                        match value_types.as_slice() {
                            [] => {}
                            [t] => {
                                let open_tout = Type::new(OpenT(tout.dupe()));
                                rec_flow_t(cx, env, trace, unknown_use(), (t, &open_tout))?;
                            }
                            [t0, t1, ts @ ..] => {
                                let new_rep = union_rep::make(
                                    None,
                                    union_rep::UnionKind::UnknownKind,
                                    t0.dupe(),
                                    t1.dupe(),
                                    ts.iter().duped().collect::<Vec<_>>().into(),
                                );
                                let union_t = Type::new(UnionT(reason, new_rep));
                                let open_tout = Type::new(OpenT(tout.dupe()));
                                rec_flow_t(cx, env, trace, unknown_use(), (&union_t, &open_tout))?;
                            }
                        }
                    } else {
                        distribute(cx)?;
                    }
                }
                _ => {
                    distribute(cx)?;
                }
            }
        }
        (UnionT(_, rep), _)
            if match u.deref() {
                ConditionalT(box ConditionalTData {
                    distributive_tparam_name,
                    ..
                }) => distributive_tparam_name.is_some(),
                _ => true,
            } =>
        {
            // For homogeneous enum unions that have already been optimized,
            // try a single representative member speculatively. If it fails,
            // emit just that one error (all members would fail identically).
            // If it succeeds, fall through to normal distribution to catch
            // members that might fail.
            // Note: we intentionally skip optimize_enum_only here (unlike the
            // TypeCastT and UseT handlers) because the catch-all runs on ALL
            // use types and forcing optimization can trigger tvar resolution
            // with side-effect errors.
            let flowed_single = {
                match rep.check_enum_with_tag() {
                    Some((enums, Some(_))) if enums.len() > 1 => {
                        let mut members = rep.members_iter();
                        match members.next() {
                            None => false,
                            Some(representative) => {
                                let representative = representative.dupe();
                                drop(members);
                                match speculation_kit::try_singleton_throw_on_failure(
                                    cx,
                                    env,
                                    trace,
                                    representative.dupe(),
                                    u.dupe(),
                                ) {
                                    Err(FlowJsException::SpeculationSingletonError) => {
                                        let u = match type_util::use_op_of_use_t(u) {
                                            None => u.dupe(),
                                            Some(use_op) => {
                                                let use_op =
                                                    flow_js_utils::union_representative_use_op(
                                                        cx,
                                                        l,
                                                        &representative,
                                                        use_op,
                                                    );
                                                type_util::mod_use_op_of_use_t(|_| use_op.dupe(), u)
                                            }
                                        };
                                        rec_flow(cx, env, trace, (&representative, &u))?;
                                        true
                                    }
                                    Ok(()) => false,
                                    Err(e) => return Err(e),
                                }
                            }
                        }
                    }
                    _ => false,
                }
            };
            if !flowed_single {
                flow_all_in_union(cx, env, trace, rep, u)?;
            }
        }
        (_, FilterOptionalT(use_op, u_inner)) => {
            rec_flow_t(cx, env, trace, use_op.dupe(), (l, u_inner))?;
        }
        (_, FilterMaybeT(use_op, u_inner)) => {
            rec_flow_t(cx, env, trace, use_op.dupe(), (l, u_inner))?;
        }
        // special treatment for some operations on intersections: these
        // rules fire for particular UBs whose constraints can (or must)
        // be resolved against intersection LBs as a whole, instead of
        // by decomposing the intersection into its parts.

        // lookup of properties
        (
            IntersectionT(_, rep),
            LookupT(box LookupTData {
                reason,
                lookup_kind,
                indexer_fallback,
                try_ts_on_failure,
                propref,
                lookup_action,
                ids,
                method_accessible,
                ignore_dicts,
            }),
        ) => {
            let ts: Vec<_> = rep.members_iter().duped().collect();
            assert!(!ts.is_empty());

            // Since s could be in any object type in the list ts, we try to look it
            // up in the first element of ts, pushing the rest into the list
            // try_ts_on_failure (see below).
            let hd = &ts[0];
            let mut new_try_ts = ts[1..].to_vec();
            new_try_ts.extend(try_ts_on_failure.iter().duped());
            rec_flow(
                cx,
                env,
                trace,
                (
                    hd,
                    &UseT::new(LookupT(Box::new(LookupTData {
                        reason: reason.dupe(),
                        lookup_kind: lookup_kind.clone(),
                        indexer_fallback: indexer_fallback.clone(),
                        try_ts_on_failure: new_try_ts.into(),
                        propref: propref.clone(),
                        lookup_action: lookup_action.clone(),
                        ids: ids.dupe(),
                        method_accessible: *method_accessible,
                        ignore_dicts: *ignore_dicts,
                    }))),
                ),
            )?;
        }
        // Cases of an intersection need to produce errors on non-existent
        // properties instead of a default, so that other cases may be tried
        // instead and succeed.
        (IntersectionT(_, _), GetPropT(box data)) if data.id.is_some() => {
            rec_flow(
                cx,
                env,
                trace,
                (
                    l,
                    &UseT::new(GetPropT(Box::new(GetPropTData {
                        use_op: data.use_op.dupe(),
                        reason: data.reason.dupe(),
                        id: None,
                        from_annot: data.from_annot,
                        skip_optional: data.skip_optional,
                        propref: data.propref.clone(),
                        tout: data.tout.clone(),
                        hint: data.hint.clone(),
                    }))),
                ),
            )?;
        }
        (
            IntersectionT(_, _),
            TestPropT(box TestPropTData {
                use_op,
                reason,
                id: _,
                propref,
                tout,
                hint,
            }),
        ) => {
            rec_flow(
                cx,
                env,
                trace,
                (
                    l,
                    &UseT::new(GetPropT(Box::new(GetPropTData {
                        use_op: use_op.dupe(),
                        reason: reason.dupe(),
                        id: None,
                        from_annot: false,
                        skip_optional: false,
                        propref: propref.clone(),
                        tout: tout.clone(),
                        hint: hint.clone(),
                    }))),
                ),
            )?;
        }
        // extends
        (
            IntersectionT(_, rep),
            ExtendsUseT(box ExtendsUseTData {
                use_op,
                reason,
                targs: try_ts_on_failure,
                true_t: ext_l,
                false_t: ext_u,
            }),
        ) => {
            let mut members = rep.members_iter();
            let t = members.next().unwrap();
            let ts: Vec<_> = members.duped().collect();
            let mut new_try_ts = ts;
            new_try_ts.extend(try_ts_on_failure.iter().duped());
            rec_flow(
                cx,
                env,
                trace,
                (
                    t,
                    &UseT::new(ExtendsUseT(Box::new(ExtendsUseTData {
                        use_op: use_op.dupe(),
                        reason: reason.dupe(),
                        targs: new_try_ts.into(),
                        true_t: ext_l.dupe(),
                        false_t: ext_u.dupe(),
                    }))),
                ),
            )?;
        }
        // consistent override of properties
        (
            IntersectionT(_, rep),
            SuperT(box SuperTData {
                use_op,
                reason,
                derived_type: derived,
            }),
        ) => {
            for t in rep.members_iter() {
                let u_inner = match use_op {
                    VirtualUseOp::Op(
                        deref!(
                            VirtualRootUseOp::ClassExtendsCheck(box ClassExtendsCheckData {
                                def_loc,
                                def,
                                ..
                            })
                        ),
                    ) => {
                        let new_use_op = VirtualUseOp::Op(Arc::new(
                            VirtualRootUseOp::ClassExtendsCheck(Box::new(ClassExtendsCheckData {
                                def_loc: def_loc.dupe(),
                                def: def.clone(),
                                extends_loc: type_util::loc_of_t(t).dupe(),
                                extends: TypeOrTypeDescT::Type(t.dupe()),
                            })),
                        ));
                        UseT::new(SuperT(Box::new(SuperTData {
                            use_op: new_use_op,
                            reason: reason.dupe(),
                            derived_type: derived.clone(),
                        })))
                    }
                    _ => u.dupe(),
                };
                rec_flow(cx, env, trace, (t, &u_inner))?;
            }
        }
        // structural subtype multiple inheritance
        (IntersectionT(_, rep), ImplementsT(use_op, this)) => {
            for t in rep.members_iter() {
                let u_inner =
                    match use_op {
                        VirtualUseOp::Op(
                            deref!(
                                VirtualRootUseOp::ClassImplementsCheck(
                                    box ClassImplementsCheckData { def, name, .. },
                                )
                            ),
                        ) => {
                            let new_use_op =
                                VirtualUseOp::Op(Arc::new(VirtualRootUseOp::ClassImplementsCheck(
                                    Box::new(ClassImplementsCheckData {
                                        def: def.dupe(),
                                        name: name.dupe(),
                                        implements_loc: type_util::ref_loc_of_t(t).dupe(),
                                        implements: TypeOrTypeDescT::Type(t.dupe()),
                                    }),
                                )));
                            UseT::new(ImplementsT(new_use_op, this.dupe()))
                        }
                        _ => u.dupe(),
                    };
                rec_flow(cx, env, trace, (t, &u_inner))?;
            }
        }
        // predicates: prevent a predicate upper bound from prematurely decomposing
        // an intersection lower bound
        (
            IntersectionT(_, _),
            ConcretizeT(box ConcretizeTData {
                reason: _,
                kind: ConcretizationKind::ConcretizeForPredicate(_),
                seen: _,
                collector,
            }),
        ) => {
            collector.add(l.dupe());
        }
        // This duplicates the `ReposLowerT` arm near the end of this match but
        // has to appear here to preempt the `IntersectionT` arm in between so
        // that we reposition the entire intersection.
        (
            IntersectionT(_, _),
            ReposLowerT {
                reason,
                use_desc,
                use_t: u_inner,
            },
        ) => {
            let repos_l = helpers::reposition_reason(cx, env, Some(trace), reason, *use_desc, l)?;
            rec_flow(cx, env, trace, (&repos_l, u_inner))?;
        }
        (IntersectionT(_, _), ObjKitT(use_op, reason, resolve_tool, tool, tout)) => {
            object_kit::run_with_env(
                trace,
                cx,
                env,
                use_op.dupe(),
                reason,
                resolve_tool,
                tool,
                l,
                tout,
            )?;
        }
        (
            IntersectionT(_, _),
            SealGenericT(box SealGenericTData {
                loc: _,
                id,
                name,
                cont,
                no_infer,
            }),
        ) => {
            let reason = reason_of_t(l);
            let generic = Type::new(GenericT(Box::new(GenericTData {
                reason: reason.dupe(),
                id: id.clone(),
                name: name.dupe(),
                bound: l.dupe(),
                no_infer: *no_infer,
            })));
            continue_(cx, env, trace, &generic, cont)?;
        }
        (
            IntersectionT(_, _),
            CallT(box CallTData {
                call_action: box CallAction::ConcretizeCallee(tout),
                ..
            }),
        ) => {
            tout.add(l.dupe());
        }
        (
            IntersectionT(_, _),
            ConcretizeT(box ConcretizeTData {
                reason: _,
                kind:
                    ConcretizationKind::ConcretizeForOptionalChain
                    | ConcretizationKind::ConcretizeForEnumExhaustiveCheck
                    | ConcretizationKind::ConcretizeForOperatorsChecking
                    | ConcretizationKind::ConcretizeForComputedObjectKeys
                    | ConcretizationKind::ConcretizeForObjectAssign
                    | ConcretizationKind::ConcretizeForDestructuring,
                seen: _,
                collector,
            }),
        ) => {
            collector.add(l.dupe());
        }
        (IntersectionT(_, rep), _) => {
            // We only call CalleeRecorder here for sig-help information. As far as
            // the typed AST is concerned when dealing with intersections we record
            // the specific branch that was selected. Therefore, we do not record
            // intersections when they hit a CallT constraint. The only time when an
            // intersection is allowed is when we have exhausted the branches of a
            // speculation job (this is a Flow error) and fall back to the
            // intersection as the type for the callee node. (This happens in
            // Default_resolver.)
            callee_recorder::add_callee_use(env, callee_recorder::Kind::SigHelp, l.dupe(), u);
            speculation_kit::try_intersection(cx, env, trace, u.dupe(), l.dupe(), rep)?;
        }
        (
            _,
            ConcretizeT(box ConcretizeTData {
                reason: _,
                kind: ConcretizationKind::ConcretizeForOptionalChain,
                seen: _,
                collector,
            }),
        ) => {
            collector.add(l.dupe());
        }

        // *************************
        // * Resolving rest params *
        // *************************

        // `any` is obviously fine as a spread element. `Object` is fine because
        // any Iterable can be spread, and `Object` is the any type that covers
        // iterable objects.
        (
            AnyT(r, src),
            ResolveSpreadT(box ResolveSpreadTData {
                use_op,
                reason: reason_op,
                resolve_spread_type: resolve_spread,
            }),
        ) => {
            let mut rrt_resolved = resolve_spread.rrt_resolved.dupe();
            rrt_resolved.push_front(ResolvedParam::ResolvedAnySpreadArg(r.dupe(), *src));
            resolve_spread_list_rec(
                cx,
                env,
                Some(trace),
                use_op.dupe(),
                reason_op,
                rrt_resolved,
                resolve_spread.rrt_unresolved.dupe(),
                resolve_spread.rrt_resolve_to.clone(),
            )?;
        }
        (
            _,
            ResolveSpreadT(box ResolveSpreadTData {
                use_op,
                reason: reason_op,
                resolve_spread_type: resolve_spread,
            }),
        ) => {
            let reason = reason_of_t(l);
            let (lt, generic) = match l.deref() {
                GenericT(box GenericTData {
                    bound,
                    id,
                    reason: gen_reason,
                    ..
                }) => {
                    let repos =
                        helpers::reposition_reason(cx, env, None, gen_reason, false, bound)?;
                    (repos, Some(id.clone()))
                }
                _ => (l.dupe(), None),
            };
            let arrtype = match lt.deref() {
                DefT(_, deref!(ArrT(arrtype))) => {
                    // Arrays
                    match (&resolve_spread.rrt_resolve_to, arrtype.as_ref()) {
                        (
                            SpreadResolve::ResolveSpreadsToTupleType { inexact, .. },
                            ArrayAT(box ArrayATData {
                                tuple_view: None, ..
                            })
                            | ROArrayAT(box ROArrayATData { .. })
                            | TupleAT(box TupleATData { rest: Some(_), .. }),
                        ) if *inexact || !resolve_spread.rrt_unresolved.is_empty() => {
                            // Only the last spread of a tuple type can have an unknown number of
                            // elements, which become its rest element.
                            let error = match arrtype.as_ref() {
                                TupleAT(_) => {
                                    let loc = match resolve_spread.rrt_unresolved.first() {
                                        Some(UnresolvedParam::UnresolvedArg(
                                            box UnresolvedArgData(elem, _),
                                        )) => elem.reason.loc().dupe(),
                                        Some(UnresolvedParam::UnresolvedSpreadArg(t)) => {
                                            reason_of_t(t).loc().dupe()
                                        }
                                        None => reason_op.loc().dupe(),
                                    };
                                    ErrorMessage::ETupleElementAfterRestSpread(loc)
                                }
                                _ => ErrorMessage::ETupleInvalidTypeSpread(Box::new(
                                    ETupleInvalidTypeSpreadData {
                                        spread_loc: reason_op.loc().dupe(),
                                        argument:
                                            flow_js_utils::type_reference_with_reason_for_error(
                                                l,
                                                reason.dupe(),
                                            ),
                                    },
                                )),
                            };
                            flow_js_utils::add_output_with_env(cx, env, error)?;
                            // Continue as if `any` was spread, which makes the tuple type `any`.
                            return Ok(Some(TailCall::RecFlow(
                                any_t::error(reason.dupe()),
                                u.dupe(),
                                DepthTrace::rec_trace(trace),
                            )));
                        }
                        _ => (**arrtype).clone(),
                    }
                }
                _ => {
                    // Non-array non-any iterables, opaque arrays, etc
                    enum ResolveTo {
                        Iterable,
                        Array,
                        Tuple,
                    }
                    let resolve_to = match &resolve_spread.rrt_resolve_to {
                        // Spreading iterables in a type context is always OK
                        SpreadResolve::ResolveSpreadsToMultiflowSubtypeFull(_, _) => {
                            ResolveTo::Iterable
                        }
                        // Otherwise we're spreading values
                        SpreadResolve::ResolveSpreadsToArray { .. }
                        | SpreadResolve::ResolveSpreadsToArrayLiteral { .. }
                        | SpreadResolve::ResolveSpreadsToMultiflowCallFull(_, _) => {
                            // Babel's "loose mode" array spread transform deviates from
                            // the spec by assuming the spread argument is always an
                            // array. If the babel_loose_array_spread option is set, model
                            // this assumption.
                            if cx.babel_loose_array_spread() {
                                ResolveTo::Array
                            } else {
                                ResolveTo::Iterable
                            }
                        }
                        SpreadResolve::ResolveSpreadsToTupleType { .. } => ResolveTo::Tuple,
                    };
                    let elem_t = flow_typing_tvar::mk(cx, reason.dupe());
                    let resolve_to_type = match resolve_to {
                        ResolveTo::Iterable => {
                            let targs = vec![
                                elem_t.dupe(),
                                unsoundness::why(UnsoundnessKind::ResolveSpread, reason.dupe()),
                                unsoundness::why(UnsoundnessKind::ResolveSpread, reason.dupe()),
                            ];
                            let iterable_reason = reason.dupe().replace_desc_new(None);
                            get_builtin_typeapp(cx, env, &iterable_reason, None, "Iterable", targs)
                        }
                        ResolveTo::Array => {
                            let arr_reason = reason.dupe().replace_desc_new(None);
                            Type::new(DefT(
                                arr_reason,
                                DefT::new(ArrT(Rc::new(ROArrayAT(Box::new(ROArrayATData {
                                    elem_t: elem_t.dupe(),
                                    react_dro: None,
                                    literal: false,
                                }))))),
                            ))
                        }
                        ResolveTo::Tuple => {
                            flow_js_utils::add_output_with_env(
                                cx,
                                env,
                                ErrorMessage::ETupleInvalidTypeSpread(Box::new(
                                    ETupleInvalidTypeSpreadData {
                                        spread_loc: reason_op.loc().dupe(),
                                        argument:
                                            flow_js_utils::type_reference_with_reason_for_error(
                                                l,
                                                reason.dupe(),
                                            ),
                                    },
                                )),
                            )?;
                            // Continue as if `any` was spread, which makes the tuple type `any`.
                            return Ok(Some(TailCall::RecFlow(
                                any_t::error(reason.dupe()),
                                u.dupe(),
                                DepthTrace::rec_trace(trace),
                            )));
                        }
                    };
                    rec_flow_t(cx, env, trace, unknown_use(), (l, &resolve_to_type))?;
                    if !flow_js_utils::tvar_visitors::has_unresolved_tvars_or_placeholders(cx, l) {
                        tvar_resolver::resolve(cx, tvar_resolver::default_no_lowers, true, &elem_t);
                    }
                    ArrayAT(Box::new(ArrayATData {
                        elem_t,
                        tuple_view: None,
                        react_dro: None,
                        strictness_kind: cx.type_strictness_kind(),
                        literal: false,
                        fresh: false,
                    }))
                }
            };
            let elemt = elemt_of_arrtype(&arrtype);
            match &resolve_spread.rrt_resolve_to {
                // Any ResolveSpreadsTo* which does some sort of constant folding needs to
                // carry an id around to break the infinite recursion that constant
                // constant folding can trigger
                SpreadResolve::ResolveSpreadsToTupleType {
                    id: cfe_id,
                    elem_t: cfe_elem_t,
                    tout,
                    ..
                }
                | SpreadResolve::ResolveSpreadsToArrayLiteral {
                    id: cfe_id,
                    elem_t: cfe_elem_t,
                    tout,
                    ..
                } => {
                    // You might come across code like
                    //
                    // for (let x = 1; x < 3; x++) { foo = [...foo, x]; }
                    //
                    // where every time you spread foo, you flow another type into foo. So
                    // each time `l ~> ResolveSpreadT` is processed, it might produce a new
                    // `l ~> ResolveSpreadT` with a new `l`.
                    //
                    // Here is how we avoid this:
                    //
                    // 1. We use ConstFoldExpansion to detect when we see a ResolveSpreadT
                    //    upper bound multiple times
                    // 2. When a ResolveSpreadT upper bound multiple times, we change it into
                    //     a ResolveSpreadT upper bound that resolves to a more general type.
                    //    This should prevent more distinct lower bounds from flowing in
                    // 3. rec_flow caches (l,u) pairs.
                    let reason_elemt = reason_of_t(&elemt);
                    let pos = resolve_spread.rrt_resolved.len() as i32;
                    const_fold_expansion::guard(
                        cx,
                        *cfe_id,
                        (reason_elemt.dupe(), pos),
                        |cx, recursion_depth| -> Result<(), FlowJsException> {
                            match recursion_depth {
                                0 => {
                                    // The first time we see this, we process it normally
                                    let mut rrt_resolved = resolve_spread.rrt_resolved.dupe();
                                    rrt_resolved.push_front(ResolvedParam::ResolvedSpreadArg(
                                        Box::new(ResolvedSpreadArgData(
                                            reason.dupe(),
                                            arrtype.clone(),
                                            generic.clone(),
                                        )),
                                    ));
                                    resolve_spread_list_rec(
                                        cx,
                                        env,
                                        Some(trace),
                                        use_op.dupe(),
                                        reason_op,
                                        rrt_resolved,
                                        resolve_spread.rrt_unresolved.dupe(),
                                        resolve_spread.rrt_resolve_to.clone(),
                                    )?;
                                }
                                1 => {
                                    let (literal, fresh) = match &resolve_spread.rrt_resolve_to {
                                        SpreadResolve::ResolveSpreadsToArrayLiteral {
                                            literal,
                                            fresh,
                                            ..
                                        } => (*literal, *fresh),
                                        _ => (false, false),
                                    };
                                    // To avoid infinite recursion, let's deconstruct to a simpler case
                                    // where we no longer resolve to a tuple but instead just resolve to
                                    // an array.
                                    rec_flow(
                                        cx,
                                        env,
                                        trace,
                                        (
                                            l,
                                            &UseT::new(ResolveSpreadT(Box::new(
                                                ResolveSpreadTData {
                                                    use_op: use_op.dupe(),
                                                    reason: reason_op.dupe(),
                                                    resolve_spread_type: Box::new(
                                                        ResolveSpreadType {
                                                            rrt_resolved: resolve_spread
                                                                .rrt_resolved
                                                                .dupe(),
                                                            rrt_unresolved: resolve_spread
                                                                .rrt_unresolved
                                                                .dupe(),
                                                            rrt_resolve_to:
                                                                SpreadResolve::ResolveSpreadsToArray {
                                                                    elem_t: cfe_elem_t.dupe(),
                                                                    tout: tout.dupe(),
                                                                    literal,
                                                                    fresh,
                                                                },
                                                        },
                                                    ),
                                                },
                                            ))),
                                        ),
                                    )?;
                                }
                                // We've already deconstructed, so there's nothing left to do
                                _ => {}
                            }
                            Ok(())
                        },
                    )?;
                }
                SpreadResolve::ResolveSpreadsToMultiflowCallFull(cfe_id, _)
                | SpreadResolve::ResolveSpreadsToMultiflowSubtypeFull(cfe_id, _) => {
                    let reason_elemt = reason_of_t(&elemt);
                    let pos = resolve_spread.rrt_resolved.len() as i32;
                    const_fold_expansion::guard(
                        cx,
                        *cfe_id,
                        (reason_elemt.dupe(), pos),
                        |cx, recursion_depth| -> Result<(), FlowJsException> {
                            match recursion_depth {
                                0 => {
                                    // The first time we see this, we process it normally
                                    let mut rrt_resolved = resolve_spread.rrt_resolved.dupe();
                                    rrt_resolved.push_front(ResolvedParam::ResolvedSpreadArg(
                                        Box::new(ResolvedSpreadArgData(
                                            reason.dupe(),
                                            arrtype.clone(),
                                            generic.clone(),
                                        )),
                                    ));
                                    resolve_spread_list_rec(
                                        cx,
                                        env,
                                        Some(trace),
                                        use_op.dupe(),
                                        reason_op,
                                        rrt_resolved,
                                        resolve_spread.rrt_unresolved.dupe(),
                                        resolve_spread.rrt_resolve_to.clone(),
                                    )?;
                                }
                                1 => {
                                    //  Consider
                                    //
                                    // function foo(...args) { foo(1, ...args); }
                                    //  foo();
                                    //
                                    // Because args is unannotated, we try to infer it. However, due to
                                    // the constant folding we do with spread arguments, we'll first
                                    // infer that it is [], then [] | [1], then [] | [1] | [1,1] ...etc
                                    //
                                    // We can recognize that we're stuck in a constant folding loop. But
                                    // how to break it?
                                    //
                                    // In this case, we are constant folding by recognizing when args is
                                    // a tuple or an array literal. We can break the loop by turning
                                    // tuples or array literals into simple arrays.
                                    let new_arrtype = match &arrtype {
                                        // These can get us into constant folding loops
                                        ArrayAT(box ArrayATData {
                                            elem_t,
                                            tuple_view: Some(_),
                                            react_dro,
                                            strictness_kind,
                                            ..
                                        }) => ArrayAT(Box::new(ArrayATData {
                                            elem_t: elem_t.dupe(),
                                            tuple_view: None,
                                            react_dro: react_dro.clone(),
                                            strictness_kind: *strictness_kind,
                                            literal: arrtype.literal(),
                                            fresh: arrtype.fresh(),
                                        })),
                                        TupleAT(box TupleATData {
                                            elem_t,
                                            react_dro,
                                            strictness_kind,
                                            ..
                                        }) => ArrayAT(Box::new(ArrayATData {
                                            elem_t: elem_t.dupe(),
                                            tuple_view: None,
                                            react_dro: react_dro.clone(),
                                            strictness_kind: *strictness_kind,
                                            literal: arrtype.literal(),
                                            fresh: arrtype.fresh(),
                                        })),
                                        // These cannot
                                        ArrayAT(box ArrayATData {
                                            tuple_view: None, ..
                                        })
                                        | ROArrayAT(box ROArrayATData { .. }) => arrtype.clone(),
                                    };
                                    let mut rrt_resolved = resolve_spread.rrt_resolved.dupe();
                                    rrt_resolved.push_front(ResolvedParam::ResolvedSpreadArg(
                                        Box::new(ResolvedSpreadArgData(
                                            reason.dupe(),
                                            new_arrtype,
                                            generic.clone(),
                                        )),
                                    ));
                                    resolve_spread_list_rec(
                                        cx,
                                        env,
                                        Some(trace),
                                        use_op.dupe(),
                                        reason_op,
                                        rrt_resolved,
                                        resolve_spread.rrt_unresolved.dupe(),
                                        resolve_spread.rrt_resolve_to.clone(),
                                    )?;
                                }
                                _ => {}
                            }
                            Ok(())
                        },
                    )?;
                }
                // no caching
                SpreadResolve::ResolveSpreadsToArray { .. } => {
                    let mut rrt_resolved = resolve_spread.rrt_resolved.dupe();
                    rrt_resolved.push_front(ResolvedParam::ResolvedSpreadArg(Box::new(
                        ResolvedSpreadArgData(reason.dupe(), arrtype.clone(), generic.clone()),
                    )));
                    resolve_spread_list_rec(
                        cx,
                        env,
                        Some(trace),
                        use_op.dupe(),
                        reason_op,
                        rrt_resolved,
                        resolve_spread.rrt_unresolved.dupe(),
                        resolve_spread.rrt_resolve_to.clone(),
                    )?;
                }
            }
        }
        // ********************
        // * conditional type *
        // ********************
        (DefT(_, deref!(EmptyT)), ConditionalT(box ConditionalTData { use_op, tout, .. })) => {
            let open_tout = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, use_op.dupe(), (l, &open_tout))?;
        }
        (
            DefT(reason_tapp, deref!(PolyT(box PolyTData { tparams, t_out, .. }))),
            ConditionalT(box ConditionalTData {
                use_op,
                reason: reason_op,
                ..
            }),
        ) => {
            let t_ = implicit_instantiation::kit::run_monomorphize(
                cx,
                env,
                trace,
                use_op.dupe(),
                reason_op,
                reason_tapp,
                Vec1::try_from_vec(tparams.to_vec()).unwrap(),
                t_out,
            )?;
            rec_flow(cx, env, trace, (&t_, u))?;
        }
        (
            _,
            ConditionalT(box ConditionalTData {
                use_op,
                reason,
                distributive_tparam_name: Some(name),
                infer_tparams,
                extends_t,
                true_t,
                false_t,
                tout,
            }),
        ) => {
            let subst = flow_js_utils::mk_distributive_tparam_subst_fn(
                cx,
                Some(use_op.dupe()),
                name.dupe(),
                l.dupe(),
            );
            let new_infer_tparams: Vec<_> = infer_tparams
                .iter()
                .map(|tparam| {
                    TypeParam::new(TypeParamInner {
                        bound: subst(cx, tparam.bound.dupe()),
                        reason: tparam.reason.dupe(),
                        name: tparam.name.dupe(),
                        polarity: tparam.polarity,
                        default: tparam.default.dupe(),
                        is_this: tparam.is_this,
                        is_const: tparam.is_const,
                    })
                })
                .collect();
            let extends_t = subst(cx, extends_t.dupe());
            let true_t = subst(cx, true_t.dupe());
            let false_t = subst(cx, false_t.dupe());
            rec_flow(
                cx,
                env,
                trace,
                (
                    l,
                    &UseT::new(ConditionalT(Box::new(ConditionalTData {
                        use_op: use_op.dupe(),
                        reason: reason.dupe(),
                        distributive_tparam_name: None,
                        infer_tparams: new_infer_tparams.into(),
                        extends_t,
                        true_t,
                        false_t,
                        tout: tout.clone(),
                    }))),
                ),
            )?;
        }
        (
            _,
            ConditionalT(box ConditionalTData {
                use_op,
                reason,
                distributive_tparam_name: None,
                infer_tparams,
                extends_t,
                true_t,
                false_t,
                tout,
            }),
        ) => {
            let result = implicit_instantiation::kit::run_conditional(
                cx,
                env,
                trace,
                use_op.dupe(),
                reason,
                infer_tparams,
                l,
                extends_t,
                true_t,
                false_t,
            )?;
            let open_tout = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, use_op.dupe(), (&result, &open_tout))?;
        }
        // singleton lower bounds are equivalent to the corresponding
        // primitive with a literal constraint. These conversions are
        // low precedence to allow equality exploits above, such as
        // the UnionT membership check, to fire.
        // TODO we can move to a single representation for singletons -
        // either SingletonFooT or (FooT <literal foo>) - if we can
        // ensure that their meaning as upper bounds is unambiguous.
        // Currently a SingletonFooT means the constrained type,
        // but the literal in (FooT <literal>) is a no-op.
        // Abstractly it should be totally possible to scrub literals
        // from the latter kind of flow, but it's unclear how difficult
        // it would be in practice.
        (
            DefT(_, deref!(SingletonStrT { .. } | SingletonNumT { .. } | SingletonBoolT { .. })),
            ReposLowerT {
                reason,
                use_desc,
                use_t: u_inner,
            },
        ) => {
            let repos_l = helpers::reposition_reason(cx, env, Some(trace), reason, *use_desc, l)?;
            rec_flow(cx, env, trace, (&repos_l, u_inner))?;
        }
        // NullProtoT is necessary as an upper bound, to distinguish between
        // ObjT ~> NullProtoT constraints and ObjT ~> NullT ones, but as a lower
        // bound, it's the same as NullT
        (NullProtoT(reason), _) => {
            let null_t = Type::new(DefT(reason.dupe(), DefT::new(NullT)));
            rec_flow(cx, env, trace, (&null_t, u))?;
        }
        // ********************
        // * mixin conversion *
        // ********************

        // A class can be viewed as a mixin by extracting its immediate properties,
        // and "erasing" its static and super
        #[rustfmt::skip]
        (
            DefT(class_reason, deref!(ClassT(deref!(ThisInstanceT(box ThisInstanceTData {
                instance,
                is_this,
                subst_name,
                ..
            }))))),
            MixinT(r, collector),
        ) => {
            let inst = &instance.inst;
            let static_ = Type::new(ObjProtoT(r.dupe()));
            let super_ = Type::new(ObjProtoT(r.dupe()));
            let new_instance = InstanceT::new(InstanceTInner {
                static_,
                super_,
                implements: Rc::from([]),
                inst: inst.clone(),
            });
            let new_this_inst = Type::new(ThisInstanceT(Box::new(ThisInstanceTData {
                reason: r.dupe(),
                instance: new_instance,
                is_this: *is_this,
                subst_name: subst_name.dupe(),
            })));
            let new_l = Type::new(DefT(class_reason.dupe(), DefT::new(ClassT(new_this_inst))));
            collector.add(new_l);
        }

        #[rustfmt::skip]
        (
            DefT(_, deref!(PolyT(box PolyTData {
                kind: poly_kind,
                tparams_loc,
                tparams: xs,
                t_out: deref!(DefT(class_r, deref!(ClassT(deref!(ThisInstanceT(box ThisInstanceTData {
                    instance,
                    is_this,
                    subst_name,
                    ..
                })))))),
                strictness_kind,
                ..
            }))),
            MixinT(r, collector),
        ) => {
            let inst = &instance.inst;
            let static_ = Type::new(ObjProtoT(r.dupe()));
            let super_ = Type::new(ObjProtoT(r.dupe()));
            let new_instance = InstanceT::new(InstanceTInner {
                static_,
                super_,
                implements: Rc::from([]),
                inst: inst.clone(),
            });
            let new_this_inst = Type::new(ThisInstanceT(Box::new(ThisInstanceTData {
                reason: r.dupe(),
                instance: new_instance,
                is_this: *is_this,
                subst_name: subst_name.dupe(),
            })));
            let class_t = Type::new(DefT(class_r.dupe(), DefT::new(ClassT(new_this_inst))));
            let poly_t = type_util::poly_type(
                poly::Id::generate_id(),
                tparams_loc.dupe(),
                Vec1::try_from_vec(xs.to_vec()).unwrap(),
                class_t,
                *strictness_kind,
                *poly_kind,
            );
            collector.add(poly_t);
        }
        (AnyT(_, src), MixinT(r, collector)) => {
            let any = any_t::why(*src, r.dupe());
            collector.add(any);
        }
        // TODO: it is conceivable that other things (e.g. functions) could also be
        // viewed as mixins (e.g. by extracting properties in their prototypes), but
        // such enhancements are left as future work.

        // ***************************************
        // * generic function may be specialized *
        // ***************************************

        // Instantiate a polymorphic definition using the supplied type
        // arguments. Use the instantiation cache if directed to do so by the
        // operation. (SpecializeT operations are created when processing TypeAppT
        // types, so the decision to cache or not originates there.)
        #[rustfmt::skip]
        (
            DefT(_, deref!(PolyT(box PolyTData {
                tparams_loc, tparams: xs, t_out: t, id, ..
            }))),
            SpecializeT(box SpecializeTData {
                use_op,
                reason: reason_op,
                reason2: reason_tapp,
                targs: ts,
                upper,
            }),
        ) => {
            let ts_val = ts
                .as_ref()
                .map(|rc| rc.dupe())
                .unwrap_or_else(|| Rc::from([]));
            let t_ = FlowJs::mk_typeapp_of_poly_with_env(
                cx,
                env,
                trace,
                use_op.dupe(),
                reason_op,
                reason_tapp,
                id.dupe(),
                tparams_loc.dupe(),
                Vec1::try_from_vec(xs.to_vec()).unwrap(),
                t,
                ts_val,
            )?;
            if let Some(upper) = upper {
                continue_(cx, env, trace, &t_, upper)?;
            }
        }
        // empty targs specialization of non-polymorphic classes is a no-op
        (
            DefT(_, deref!(ClassT(_))),
            SpecializeT(box SpecializeTData {
                use_op: _,
                reason: _,
                reason2: _,
                targs: None,
                upper,
            }),
        ) => {
            if let Some(upper) = upper {
                continue_(cx, env, trace, l, upper)?;
            }
        }
        // Explicit type arguments on a construct-signature value specialize the
        // signature rather than the interface that contains it.
        (
            DefT(reason_l, deref!(InstanceT(instance))),
            SpecializeT(box SpecializeTData {
                use_op,
                reason: reason_op,
                reason2: reason_tapp,
                targs: Some(ts),
                upper,
            }),
        ) => {
            let concretize =
                |cx: &mut Context<'_>, t: &Type| -> Result<Vec<Type>, FlowJsException> {
                    helpers::possible_concrete_types_for_inspection(cx, env, reason_of_t(t), t)
                };
            let construct_ts = flow_js_utils::collect_construct_ts(&concretize, cx, l)?;
            // Each overload that accepts the type arguments is specialized on its own,
            // and checking the arguments without a result only needs the first. Without
            // one, the combined signature is, so that specializing it reports the
            // mismatch.
            let accepting_ts = flow_js_utils::construct_ts_for_targs(&construct_ts, ts.len());
            let construct_ts: Vec<Type> = if accepting_ts.is_empty() {
                flow_js_utils::combine_construct_ts(construct_ts)
                    .into_iter()
                    .collect()
            } else {
                accepting_ts
            };
            match construct_ts.as_slice() {
                [construct_t, ..] => {
                    if let Some(upper) = upper {
                        let specialized_ts = construct_ts
                            .iter()
                            .map(|construct_t| {
                                let collector = type_collector::TypeCollector::create();
                                rec_flow(
                                    cx,
                                    env,
                                    trace,
                                    (
                                        construct_t,
                                        &UseT::new(SpecializeT(Box::new(SpecializeTData {
                                            use_op: use_op.dupe(),
                                            reason: reason_op.dupe(),
                                            reason2: reason_tapp.dupe(),
                                            targs: Some(ts.dupe()),
                                            upper: Some(Box::new(Cont::Collect(collector.dupe()))),
                                        }))),
                                    ),
                                )?;
                                Ok(collector.union_opt(reason_tapp.dupe()).unwrap_or_else(|| {
                                    crate::tvar_resolver::default_no_lowers(reason_tapp)
                                }))
                            })
                            .collect::<Result<Vec<_>, FlowJsException>>()?;
                        let inst = InstType::new(InstTypeInner {
                            inst_construct_t: flow_js_utils::combine_construct_ts(specialized_ts)
                                .map(|t| cx.make_call_prop(t)),
                            ..(*instance.inst).clone()
                        });
                        let specialized_interface = Type::new(DefT(
                            reason_l.dupe(),
                            DefT::new(InstanceT(Rc::new(InstanceT::new(InstanceTInner {
                                inst,
                                static_: instance.static_.dupe(),
                                super_: instance.super_.dupe(),
                                implements: instance.implements.dupe(),
                            })))),
                        ));
                        continue_(cx, env, trace, &specialized_interface, upper)?;
                    } else {
                        rec_flow(
                            cx,
                            env,
                            trace,
                            (
                                construct_t,
                                &UseT::new(SpecializeT(Box::new(SpecializeTData {
                                    use_op: use_op.dupe(),
                                    reason: reason_op.dupe(),
                                    reason2: reason_tapp.dupe(),
                                    targs: Some(ts.dupe()),
                                    upper: None,
                                }))),
                            ),
                        )?;
                    }
                }
                [] => {
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        flow_js_utils::incompatible_type_error(
                            l,
                            IncompatibleUpperData {
                                loc: flow_js_utils::error_message_loc_of_upper(u),
                                kind: flow_js_utils::error_message_kind_of_upper(u),
                            },
                            Some(use_op.dupe()),
                        ),
                    )?;
                    let any = any_t::make(AnySource::AnyError(None), reason_l.dupe());
                    if let Some(upper) = upper {
                        continue_(cx, env, trace, &any, upper)?;
                    }
                }
            }
        }
        // A value with a construct signature (or failing that, a `prototype`)
        // is class-like, so it can be extended. `class_sig` specializes the
        // extends clause eagerly, and that is the step that used to report the
        // whole thing as "not inheritable" — an interface has nothing to
        // specialize, so pass it through the way a non-polymorphic class goes
        // through above. The instance it contributes to the derived class is
        // worked out by [ThisSpecializeT] below.
        (
            DefT(reason_l, deref!(InstanceT(_))),
            SpecializeT(box SpecializeTData {
                use_op,
                reason: _,
                reason2: _,
                targs: None,
                upper,
            }),
        ) => {
            let concretize =
                |cx: &mut Context<'_>, t: &Type| -> Result<Vec<Type>, FlowJsException> {
                    helpers::possible_concrete_types_for_inspection(cx, env, reason_of_t(t), t)
                };
            if !flow_js_utils::is_inheritable(&concretize, cx, l)? {
                // Not inheritable. Report what the fallthrough would have.
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    flow_js_utils::incompatible_type_error(
                        l,
                        IncompatibleUpperData {
                            loc: flow_js_utils::error_message_loc_of_upper(u),
                            kind: flow_js_utils::error_message_kind_of_upper(u),
                        },
                        Some(use_op.dupe()),
                    ),
                )?;
                let any = any_t::make(AnySource::AnyError(None), reason_l.dupe());
                if let Some(upper) = upper {
                    continue_(cx, env, trace, &any, upper)?;
                }
            } else if let Some(upper) = upper {
                continue_(cx, env, trace, l, upper)?;
            }
        }
        (
            AnyT(_, _),
            SpecializeT(box SpecializeTData {
                use_op: _,
                reason: _,
                reason2: _,
                targs: _,
                upper,
            }),
        ) => {
            if let Some(upper) = upper {
                continue_(cx, env, trace, l, upper)?;
            }
        }
        // this-specialize a this-abstracted class by substituting This
        #[rustfmt::skip]
        (
            DefT(_, deref!(ClassT(deref!(ThisInstanceT(box ThisInstanceTData {
                reason: inst_r,
                instance: i,
                subst_name,
                ..
            }))))),
            ThisSpecializeT(r, this, k),
        ) => {
            let def_t = this_specialize_instance(cx, l, inst_r, i, subst_name, this);
            continue_repos(cx, env, trace, r, false, &def_t, k)?;
        }
        // this-specialize a this-abstracted interface
        (
            ThisInstanceT(box ThisInstanceTData {
                reason: inst_r,
                instance: i,
                subst_name,
                ..
            }),
            ThisSpecializeT(r, this, k),
        ) => {
            let def_t = this_specialize_instance(cx, l, inst_r, i, subst_name, this);
            continue_repos(cx, env, trace, r, false, &def_t, k)?;
        }
        // this-specialization of non-this-abstracted classes is a no-op
        (DefT(_, deref!(ClassT(i))), ThisSpecializeT(r, _this, k)) => {
            // TODO: check that this is a subtype of i?
            continue_repos(cx, env, trace, r, false, i, k)?;
        }
        // Extending a value with a construct signature: the instance the
        // derived class inherits from is what the signature returns.
        // TypeScript's [resolveBaseTypesOfClass] does the same, and takes the
        // first signature. Unlike TypeScript, a value with no construct
        // signature falls back to its `prototype`; see [construct_base_instance].
        (DefT(reason_l, deref!(InstanceT(_))), ThisSpecializeT(r, this, k)) => {
            let concretize = |cx: &mut Context<'_>, t: &Type, this_t: Option<&Type>| match this_t {
                Some(this_t) => helpers::possible_concrete_types_for_class_extends(
                    cx,
                    env,
                    reason_of_t(t),
                    t,
                    this_t,
                ),
                None => helpers::possible_concrete_types_for_inspection(cx, env, reason_of_t(t), t),
            };
            match flow_js_utils::construct_base_instance(&concretize, cx, l, this)? {
                Some(base) => continue_repos(cx, env, trace, r, false, &base, k)?,
                // No instance to inherit after all. The eager specialization
                // of the extends clause has already reported that; carry on
                // with `any` rather than reporting it twice.
                None => {
                    let any = any_t::make(AnySource::AnyError(None), reason_l.dupe());
                    continue_repos(cx, env, trace, r, false, &any, k)?
                }
            }
        }
        (AnyT(_, _), ThisSpecializeT(r, _, k)) => {
            continue_repos(cx, env, trace, r, false, l, k)?;
        }
        (
            DefT(_, deref!(PolyT(box _))),
            ReposLowerT {
                reason,
                use_desc,
                use_t: u_inner,
            },
        ) => {
            let repos_l = helpers::reposition_reason(cx, env, Some(trace), reason, *use_desc, l)?;
            rec_flow(cx, env, trace, (&repos_l, u_inner))?;
        }
        (
            DefT(_, deref!(ClassT(deref!(ThisInstanceT(..))))),
            ReposLowerT {
                reason,
                use_desc,
                use_t: u_inner,
            },
        ) => {
            let repos_l = helpers::reposition_reason(cx, env, Some(trace), reason, *use_desc, l)?;
            rec_flow(cx, env, trace, (&repos_l, u_inner))?;
        }
        // Special case for `_ instanceof C` where C is polymorphic
        #[rustfmt::skip]
        (
            DefT(reason_tapp, deref!(PolyT(box PolyTData {
                tparams_loc, tparams: ids, t_out: t, ..
            }))),
            ConcretizeT(box ConcretizeTData {
                reason: reason_op,
                kind:
                    ConcretizationKind::ConcretizeForPredicate(
                        PredicateConcretetizerVariant::ConcretizeRHSForInstanceOfPredicateTest,
                    ),
                ..
            }),
        ) => {
            let new_l = instantiate_poly_default_args(
                cx,
                env,
                trace,
                unknown_use(),
                reason_op,
                reason_tapp,
                tparams_loc.dupe(),
                Vec1::try_from_vec(ids.to_vec()).unwrap(),
                t.dupe(),
            )?;
            rec_flow(cx, env, trace, (&new_l, u))?;
        }
        (
            DefT(_, deref!(PolyT(box _))),
            ConcretizeT(box ConcretizeTData {
                kind: ConcretizationKind::ConcretizeForPredicate(_),
                collector,
                ..
            }),
        ) => {
            collector.add(l.dupe());
        }
        (
            DefT(_, deref!(PolyT(box _))),
            ConcretizeT(box ConcretizeTData {
                kind:
                    ConcretizationKind::ConcretizeForDestructuring
                    | ConcretizationKind::ConcretizeForUnusedPromiseCheck,
                collector,
                ..
            }),
        ) => {
            collector.add(l.dupe());
        }
        // A polymorphic value is not empty, so the conditional type switch below passes it on
        // without instantiating it.
        (
            DefT(_, deref!(PolyT(box _))),
            CondT(box CondTData {
                opt_type: then_t_opt,
                collector,
                ..
            }),
        ) => {
            collector.add(then_t_opt.as_ref().unwrap_or(l).dupe());
        }

        // The rules below are hit when a polymorphic type appears outside a
        // type application expression - i.e. not followed by a type argument list
        // delimited by angle brackets.
        // We want to require full expressions in type positions like annotations,
        // but allow use of polymorphically-typed values - for example, in class
        // extends clauses and at function call sites - without explicit type
        // arguments, since typically they're easily inferred from context.
        //
        // We are calling the static callable method of a class. We need to be careful
        // not to apply the targs at this point, because this PolyT represents the class
        // and not the static function that's being called. We implicitly instantiate
        // the instance's tparams using the bounds and then forward the result original call
        // instead of consuming the method call's type arguments.
        //
        // We use the bounds to explicitly instantiate so that we don't create yet another implicit
        // instantiation here that would be un-annotatable.
        #[rustfmt::skip]
        (
            DefT(reason_tapp, deref!(PolyT(box PolyTData {
                tparams_loc,
                tparams: ids,
                t_out: t @ deref!(DefT(_, deref!(ClassT(deref!(ThisInstanceT(..)))))),
                ..
            }))),
            CallT(box CallTData {
                use_op,
                reason: reason_op,
                ..
            }),
        ) => {
            let targs: Vec<Targ> = ids
                .iter()
                .map(|tparam| Targ::ExplicitArg(tparam.bound.dupe()))
                .collect();
            let t_ = instantiation_helpers::instantiate_with_targs(
                cx,
                env,
                trace,
                use_op.dupe(),
                reason_op,
                reason_tapp,
                (
                    tparams_loc.dupe(),
                    Vec1::try_from_vec(ids.to_vec()).unwrap(),
                    t.dupe(),
                ),
                targs,
            )?;
            rec_flow(cx, env, trace, (&t_, u))?;
        }
        // Reading the statics of a polymorphic class instantiates the instance's tparams with
        // their bounds too, rather than implicitly instantiating them from the operation: the
        // statics cannot reference them, so only a `this` type in a static member observes the
        // instantiation, and nothing in the statics constrains them. The prototype is the
        // instance rather than a static, so reading it still goes through the rule below.
        #[rustfmt::skip]
        (
            DefT(reason_tapp, deref!(PolyT(box PolyTData {
                tparams_loc,
                tparams: ids,
                t_out: t @ deref!(DefT(_, deref!(ClassT(deref!(ThisInstanceT(..)))))),
                ..
            }))),
            _,
        ) if flow_js_utils::object_like_op(u)
            && !matches!(
                u.deref(),
                GetPropT(box GetPropTData { propref: box PropRef::Named { name, .. }, .. })
                    if name == &Name::new("prototype")
            ) =>
        {
            let reason_op = upper_operation_reason_or(u, reason_tapp);
            let use_op = use_op_of_use_t(u).unwrap_or_else(unknown_use);
            let ids = Vec1::try_from_vec(ids.to_vec()).unwrap();
            let targs = flow_js_utils::instantiate_poly_param_upper_bounds(cx, &ids);
            let (t_, _) = FlowJs::instantiate_poly_with_targs_with_env(
                cx,
                env,
                trace,
                use_op,
                &reason_op,
                reason_tapp,
                None,
                None,
                (tparams_loc.dupe(), ids, t.dupe()),
                targs,
            )?;
            rec_flow(cx, env, trace, (&t_, u))?;
        }
        // We use the ConcretizeCallee action to simplify types for hint decomposition.
        // After having instantiated polymorphic classes on static calls (case above),
        // we can just return the remaining polymorphic types, since there is not
        // much we can do about them here. These will be handled by the hint
        // decomposition code that has some knowledge of the call arguments.
        (
            DefT(_, deref!(PolyT(box _))),
            CallT(box CallTData {
                call_action: box CallAction::ConcretizeCallee(tout),
                ..
            }),
        ) => {
            tout.add(l.dupe());
        }

        // Calls to polymorphic functions may cause non-termination, e.g. when the
        // results of the calls feed back as subtle variations of the original
        // arguments. This is similar to how we may have non-termination with
        // method calls on type applications. Thus, it makes sense to replicate
        // the specialization caching mechanism used in TypeAppT ~> MethodT to
        // avoid non-termination in PolyT ~> CallT.
        //
        // As it turns out, we need a bit more work here. A call may invoke
        // different cases of an overloaded polymorphic function on different
        // arguments, so we use the reasons of arguments in addition to the reason
        // of the call as keys for caching instantiations.
        //
        // On the other hand, even the reasons of arguments may not offer sufficient
        // distinguishing power when the arguments have not been concretized:
        // differently typed arguments could be incorrectly summarized by common
        // type variables they flow to, causing spurious errors.
        //
        // NOTE: This is probably not the final word on non-termination with
        // generics. We need to separate the double duty of reasons in the current
        // implementation as error positions and as caching keys. As error
        // positions we should be able to subject reasons to arbitrary tweaking,
        // without fearing regressions in termination guarantees.
        #[rustfmt::skip]
        (
            DefT(reason_tapp, deref!(PolyT(box PolyTData {
                tparams_loc, tparams: ids, t_out: t, ..
            }))),
            CallT(box CallTData {
                use_op,
                reason: reason_op,
                call_action: box CallAction::Funcalltype(box calltype),
                return_hint,
            }),
        ) => {
            let lparts = (
                reason_tapp.dupe(),
                tparams_loc.dupe(),
                Vec1::try_from_vec(ids.to_vec()).unwrap(),
                t.dupe(),
            );
            let uparts = (
                use_op.dupe(),
                reason_op.dupe(),
                calltype.call_targs.dupe(),
                return_hint.clone(),
            );
            let l_clone = l.dupe();
            let tparams_loc_c = tparams_loc.dupe();
            let ids_c = ids.dupe();
            let t_c = t.dupe();
            let use_op_c = use_op.dupe();
            let reason_op_c = reason_op.dupe();
            let calltype_c = calltype.clone();
            let check = move || {
                flow_typing_implicit_instantiation_check::ImplicitInstantiationCheck::of_call(
                    l_clone.dupe(),
                    (
                        tparams_loc_c.dupe(),
                        Vec1::try_from_vec(ids_c.to_vec()).unwrap(),
                        t_c.dupe(),
                    ),
                    use_op_c.dupe(),
                    reason_op_c.dupe(),
                    flow_typing_implicit_instantiation_check::Call::from_funcall(
                        calltype_c.clone(),
                    ),
                )
            };
            let t_ = instantiate_poly_call_or_new(cx, env, trace, lparts, uparts, &check)?;
            let new_calltype = FuncallType {
                call_targs: None,
                ..calltype.clone()
            };
            let new_u = UseT::new(CallT(Box::new(CallTData {
                use_op: use_op.dupe(),
                reason: reason_op.dupe(),
                call_action: Box::new(CallAction::Funcalltype(Box::new(new_calltype))),
                return_hint: return_hint.clone(),
            })));
            rec_flow(cx, env, trace, (&t_, &new_u))?;
        }
        #[rustfmt::skip]
        (
            DefT(reason_tapp, deref!(PolyT(box PolyTData {
                tparams_loc, tparams: ids, t_out: t, ..
            }))),
            ConstructorT(box ctor_data),
        ) => {
            let lparts = (
                reason_tapp.dupe(),
                tparams_loc.dupe(),
                Vec1::try_from_vec(ids.to_vec()).unwrap(),
                t.dupe(),
            );
            let uparts = (
                ctor_data.use_op.dupe(),
                ctor_data.reason.dupe(),
                ctor_data.targs.dupe(),
                ctor_data.return_hint.clone(),
            );
            let l_clone = l.dupe();
            let tparams_loc_c = tparams_loc.dupe();
            let ids_c = ids.dupe();
            let t_c = t.dupe();
            let use_op_c = ctor_data.use_op.dupe();
            let reason_op_c = ctor_data.reason.dupe();
            let targs_c = ctor_data.targs.dupe();
            let args_c = ctor_data.args.dupe();
            let check = move || {
                flow_typing_implicit_instantiation_check::ImplicitInstantiationCheck::of_ctor(
                    l_clone.dupe(),
                    (
                        tparams_loc_c.dupe(),
                        Vec1::try_from_vec(ids_c.to_vec()).unwrap(),
                        t_c.dupe(),
                    ),
                    use_op_c.dupe(),
                    reason_op_c.dupe(),
                    targs_c.dupe(),
                    args_c.dupe(),
                )
            };
            let t_ = instantiate_poly_call_or_new(cx, env, trace, lparts, uparts, &check)?;
            let new_u = UseT::new(ConstructorT(Box::new(ConstructorTData {
                use_op: ctor_data.use_op.dupe(),
                reason: ctor_data.reason.dupe(),
                targs: None,
                args: ctor_data.args.dupe(),
                tout: ctor_data.tout.dupe(),
                return_hint: ctor_data.return_hint.clone(),
                specialized_ctor: ctor_data.specialized_ctor.clone(),
            })));
            rec_flow(cx, env, trace, (&t_, &new_u))?;
        }
        #[rustfmt::skip]
        (
            DefT(reason_tapp, deref!(PolyT(box PolyTData {
                tparams_loc, tparams: ids, t_out: t, ..
            }))),
            ReactKitT(box ReactKitTData {
                use_op,
                reason: reason_op,
                tool: box react::Tool::CreateElement(box react::CreateElementData {
                    jsx_props,
                    return_hint,
                    targs,
                    tout,
                    specialized_component,
                    specialized_props,
                    ..
                }),
            }),
        ) => {
            let lparts = (
                reason_tapp.dupe(),
                tparams_loc.dupe(),
                Vec1::try_from_vec(ids.to_vec()).unwrap(),
                t.dupe(),
            );
            let uparts = (
                use_op.dupe(),
                reason_op.dupe(),
                targs.dupe(),
                return_hint.clone(),
            );
            let l_clone = l.dupe();
            let tparams_loc_c = tparams_loc.dupe();
            let ids_c = ids.dupe();
            let t_c = t.dupe();
            let use_op_c = use_op.dupe();
            let reason_op_c = reason_op.dupe();
            let jsx_props_c = jsx_props.dupe();
            let targs_c = targs.dupe();
            let check = move || {
                flow_typing_implicit_instantiation_check::ImplicitInstantiationCheck::of_react_jsx(
                    l_clone.dupe(),
                    (
                        tparams_loc_c.dupe(),
                        Vec1::try_from_vec(ids_c.to_vec()).unwrap(),
                        t_c.dupe(),
                    ),
                    use_op_c.dupe(),
                    reason_op_c.dupe(),
                    jsx_props_c.dupe(),
                    targs_c.dupe(),
                )
            };
            let (t_, inferred_targs) =
                instantiate_poly_call_or_new_with_soln(cx, env, trace, lparts, uparts, &check)?;
            let new_u = UseT::new(ReactKitT(Box::new(ReactKitTData {
                use_op: use_op.dupe(),
                reason: reason_op.dupe(),
                tool: Box::new(react::Tool::<Cx>::CreateElement(Box::new(
                    react::CreateElementData {
                        component: t_.dupe(),
                        jsx_props: jsx_props.dupe(),
                        return_hint: return_hint.clone(),
                        targs: None,
                        tout: tout.dupe(),
                        inferred_targs: Some(inferred_targs.into()),
                        specialized_component: specialized_component.clone(),
                        specialized_props: specialized_props.clone(),
                    },
                ))),
            })));
            rec_flow(cx, env, trace, (&t_, &new_u))?;
        }
        (
            DefT(
                r,
                deref!(ObjT(deref!(ObjType {
                    call_t: Some(id),
                    ..
                }))),
            ),
            ReactKitT(box ReactKitTData {
                use_op: _,
                reason: _,
                tool:
                    box (react::Tool::CreateElement(..)
                    | react::Tool::GetConfig { .. }
                    | react::Tool::ConfigCheck { .. }),
            }),
        ) if {
            let found = cx.find_call(id.dupe());
            match found.deref() {
                #[rustfmt::skip]
                DefT(_, deref!(PolyT(box PolyTData {
                    t_out: deref!(DefT(_, deref!(FunT(_, _)))), ..
                }))) => {
                    let fun_t_repos = type_util::mod_reason_of_t(&|_| r.dupe(), &found);
                    rec_flow(cx, env, trace, (&fun_t_repos, u))?;
                    true
                }
                _ => false,
            }
        } => {}
        (
            DefT(reason_tapp, deref!(PolyT(box PolyTData { tparams, t_out, .. }))),
            ReactKitT(box ReactKitTData {
                use_op,
                reason: reason_op,
                tool: box react::Tool::GetConfig { .. },
            }),
        ) => {
            let t_ = implicit_instantiation::kit::run_monomorphize(
                cx,
                env,
                trace,
                use_op.dupe(),
                reason_op,
                reason_tapp,
                Vec1::try_from_vec(tparams.to_vec()).unwrap(),
                t_out,
            )?;
            rec_flow(cx, env, trace, (&t_, u))?;
        }

        // ******************************
        // * functions statics - part A *
        // ******************************
        #[rustfmt::skip]
        (
            DefT(reason, deref!(FunT(static_, _)))
            | DefT(_, deref!(PolyT(box PolyTData {
                t_out: deref!(DefT(reason, deref!(FunT(static_, _)))), ..
            }))),
            MethodT(box MethodTData {
                use_op,
                reason: reason_call,
                prop_reason: reason_lookup,
                propref,
                method_action: action,
            }),
        ) => {
            // Class methods carry `dummy_static` statics, so the lookup below
            // resolves on `any` and never reaches the `FunProtoT` arm that
            // records `Function.prototype` accesses. Record it here instead --
            // the receiver is a function, so `.call`/`.apply`/`.bind` can only
            // be the `Function.prototype` ones.
            let method_static = if let PropRef::Named {
                reason: reason_prop,
                name,
                ..
            } = &**propref
                && matches!(name.as_str_opt(), Some("apply" | "bind" | "call"))
                && matches!(
                    static_.deref(),
                    AnyT(_, AnySource::Unsound(UnsoundnessKind::DummyStatic))
                ) {
                cx.record_fun_proto_method_lookup(reason_prop.loc().dupe());
                if name.as_str_opt() == Some("bind") {
                    Type::new(FunProtoT(reason_lookup.dupe()))
                } else {
                    static_.dupe()
                }
            } else {
                static_.dupe()
            };
            let method_type = flow_typing_tvar::mk_no_wrap_where(
                cx,
                reason_lookup.dupe(),
                |cx, tvar_reason, tvar_id| {
                    let tout = Tvar::new(tvar_reason.dupe(), tvar_id as u32);
                    let use_t = UseT::new(GetPropT(Box::new(GetPropTData {
                        use_op: use_op.dupe(),
                        reason: reason_lookup.dupe(),
                        id: None,
                        from_annot: false,
                        skip_optional: false,
                        propref: propref.clone(),
                        tout: Box::new(tout),
                        hint: hint_unavailable(),
                    })));
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (
                            &method_static,
                            &UseT::new(ReposLowerT {
                                reason: reason.dupe(),
                                use_desc: false,
                                use_t: Box::new(use_t),
                            }),
                        ),
                    )
                },
            )?;
            apply_method_action(
                cx,
                env,
                trace,
                &method_type,
                use_op.dupe(),
                reason_call.dupe(),
                l.dupe(),
                action,
            )?;
        }
        // As above, the statics of a polymorphic function do not depend on its type
        // parameters, so the function is not instantiated to read them.
        #[rustfmt::skip]
        (
            DefT(_, deref!(PolyT(box PolyTData {
                t_out: t_out @ deref!(DefT(_, deref!(FunT(_, _)))), ..
            }))),
            _,
        ) if flow_js_utils::object_like_op(u) => {
            rec_flow(cx, env, trace, (t_out, u))?;
        }
        #[rustfmt::skip]
        (
            DefT(reason_tapp, deref!(PolyT(box PolyTData {
                tparams_loc, tparams: ids, t_out: t, ..
            }))),
            _,
        ) => {
            let reason_op = upper_operation_reason_or(u, reason_tapp);
            let use_op = match use_op_of_use_t(u) {
                Some(use_op) => use_op,
                None => unknown_use(),
            };
            let poly_t = (
                tparams_loc.dupe(),
                Vec1::try_from_vec(ids.to_vec())
                    .expect("should have nonempty polymorphic type parameters"),
                t.dupe(),
            );
            let method = match u.deref() {
                MethodT(data) if !env.in_implicit_instantiation() => {
                    match data.method_action.as_ref() {
                        MethodAction::CallM(call) => {
                            Some((data, &call.methodcalltype, &call.return_hint, None))
                        }
                        MethodAction::ChainM(chain) => Some((
                            data,
                            &chain.methodcalltype,
                            &chain.return_hint,
                            Some(MethodOptionalChain {
                                exp_reason: chain.exp_reason.dupe(),
                                lhs_reason: chain.lhs_reason.dupe(),
                                lhs_expression: chain.lhs_expression.dupe(),
                            }),
                        )),
                        MethodAction::NoMethodAction(_) => None,
                    }
                }
                _ => None,
            };
            let (t_, _) = if let Some((data, call, return_hint, optional_chain)) = method {
                let check = ImplicitInstantiationCheck {
                    lhs: l.dupe(),
                    poly_t,
                    operation: (
                        use_op.dupe(),
                        reason_op.dupe(),
                        Operation::Method {
                            prop_reason: data.prop_reason.dupe(),
                            propref: data.propref.as_ref().clone(),
                            call: call.clone(),
                            optional_chain,
                        },
                    ),
                };
                implicit_instantiation::kit::run_call(
                    cx,
                    env,
                    &check,
                    return_hint,
                    trace,
                    use_op,
                    &reason_op,
                    reason_tapp,
                )?
            } else {
                let unify_bounds = matches!(
                    u.deref(),
                    MethodT(box MethodTData {
                        method_action: box MethodAction::NoMethodAction(_),
                        ..
                    })
                );
                FlowJs::instantiate_poly_with_env(
                    cx,
                    env,
                    trace,
                    use_op,
                    &reason_op,
                    reason_tapp,
                    Some(unify_bounds),
                    poly_t,
                )?
            };
            rec_flow(cx, env, trace, (&t_, u))?;
        }
        // when a this-abstracted class flows to upper bounds, fix the class
        #[rustfmt::skip]
        (
            DefT(class_r, deref!(ClassT(deref!(ThisInstanceT(box ThisInstanceTData {
                reason: r,
                instance: i,
                is_this: this,
                subst_name,
            }))))),
            _,
        ) => {
            let reason = upper_operation_reason_or(u, r);
            let fixed =
                flow_js_utils::fix_this_instance(cx, reason, r.dupe(), i, *this, subst_name.dupe());
            let new_l = Type::new(DefT(class_r.dupe(), DefT::new(ClassT(fixed))));
            rec_flow(cx, env, trace, (&new_l, u))?;
        }
        (
            ThisInstanceT(box ThisInstanceTData {
                reason: r,
                instance: i,
                is_this: this,
                subst_name,
            }),
            _,
        ) => {
            let reason = upper_operation_reason_or(u, r);
            let fixed =
                flow_js_utils::fix_this_instance(cx, reason, r.dupe(), i, *this, subst_name.dupe());
            rec_flow(cx, env, trace, (&fixed, u))?;
        }

        // *****************************
        // * React Abstract Components *
        // *****************************
        (
            DefT(r, deref!(ReactAbstractComponentT(box _))),
            TestPropT(..) | GetPropT(..) | SetPropT(..) | GetElemT(..) | SetElemT(..),
        ) => {
            let statics = FlowJs::get_builtin_type_with_env(
                cx,
                env,
                Some(trace),
                r,
                None,
                "React$AbstractComponentStatics",
            )?;
            rec_flow(cx, env, trace, (&statics, u))?;
        }
        // Components can never be called
        (
            DefT(r, deref!(ReactAbstractComponentT(box _))),
            CallT(box CallTData {
                use_op,
                reason,
                call_action: box CallAction::Funcalltype(box calltype),
                ..
            }),
        ) => {
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::ECannotCallReactComponent {
                    component: flow_js_utils::type_reference_with_reason_for_error(l, r.dupe()),
                },
            )?;
            let any_err = any_t::error(reason.dupe());
            let open_tout = Type::new(OpenT(calltype.call_tout.dupe()));
            rec_flow_t(cx, env, trace, use_op.dupe(), (&any_err, &open_tout))?;
        }

        // ***********************************************
        // * function types deconstruct into their parts *
        // ***********************************************

        // FunT ~> CallT
        (
            DefT(_, deref!(FunT(_, _))),
            CallT(box CallTData {
                call_action: box CallAction::ConcretizeCallee(tout),
                ..
            }),
        ) => {
            tout.add(l.dupe());
        }
        (
            DefT(reason_fundef, deref!(FunT(_, funtype))),
            CallT(box CallTData {
                use_op,
                reason: reason_callsite,
                call_action: box CallAction::Funcalltype(box calltype),
                return_hint: _,
            }),
        ) => {
            let return_t = match &funtype.effect_ {
                ReactEffectType::HookDecl(_) | ReactEffectType::HookAnnot => {
                    if cx.react_rule_enabled(
                        flow_common::options::ReactRule::DeepReadOnlyHookReturns,
                    ) {
                        let ret_reason = reason_of_t(&funtype.return_t);
                        let destructor = Destructor::ReactDRO(Box::new(ReactDro(
                            reason_fundef.def_loc().dupe(),
                            DroType::HookReturn,
                        )));
                        Type::new(EvalT {
                            type_: funtype.return_t.dupe(),
                            defer_use_t: TypeDestructorT::new(TypeDestructorTInner(
                                unknown_use(),
                                ret_reason.dupe(),
                                Rc::new(destructor),
                            )),
                            id: eval::Id::generate_id(),
                        })
                    } else {
                        funtype.return_t.dupe()
                    }
                }
                ReactEffectType::ArbitraryEffect | ReactEffectType::AnyEffect => {
                    funtype.return_t.dupe()
                }
            };
            let (o1, _) = &funtype.this_t;
            let t1 = &return_t;
            let o2 = &calltype.call_this_t;
            let call_targs = &calltype.call_targs;
            let tins2 = &calltype.call_args_tlist;
            let t2 = &calltype.call_tout;
            let call_strict_arity = calltype.call_strict_arity;
            let call_specialized_callee = &calltype.call_specialized_callee;
            let this_use_op = if let RootUseOp::FunCall(data) = root_of_use_op(use_op) {
                UseOp::Frame(
                    Arc::new(FrameUseOp::StandaloneCallThis(Box::new(
                        StandaloneCallThisData {
                            op: data.op.dupe(),
                            fn_: data.fn_.dupe(),
                            receiver_loc: type_util::ref_loc_of_t(o1).dupe(),
                            receiver: Box::new(TypeOrTypeDescT::Type(o1.dupe())),
                        },
                    ))),
                    Arc::new(use_op.dupe()),
                )
            } else {
                use_op.dupe()
            };
            rec_flow(
                cx,
                env,
                trace,
                (o2, &UseT::new(UseT(this_use_op, o1.dupe()))),
            )?;
            callee_recorder::add_callee(
                env,
                callee_recorder::Kind::All,
                l.dupe(),
                call_specialized_callee.as_ref(),
            );
            if call_targs.is_some() {
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::ECallTypeArity(Box::new(ECallTypeArityData {
                        call_loc: reason_callsite.loc().dupe(),
                        is_new: false,
                        callee: CallTypeArityCallee::Type(flow_js_utils::type_reference_for_error(
                            l,
                        )),
                        expected_arity: 0,
                    })),
                )?;
            }
            let modified_funtype = FunType {
                return_t: return_t.dupe(),
                ..(**funtype).clone()
            };
            if call_strict_arity {
                multiflow_call(
                    cx,
                    env,
                    trace,
                    use_op.dupe(),
                    reason_callsite,
                    tins2,
                    &modified_funtype,
                )?;
            } else {
                FlowJs::multiflow_subtype_with_env(
                    cx,
                    env,
                    trace,
                    use_op.dupe(),
                    reason_callsite,
                    tins2,
                    &modified_funtype,
                )?;
            }
            // flow return type of function to the tvar holding the return type of the
            // call. clears the op stack because the result of the call is not the
            // call itself.
            let repos_t1 = helpers::reposition(
                cx,
                env,
                Some(trace),
                reason_callsite.loc().dupe(),
                None,
                None,
                t1.dupe(),
            )?;
            let open_t2 = Type::new(OpenT(t2.dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (&repos_t1, &open_t2))?;
        }
        (
            AnyT(_, _),
            CallT(box CallTData {
                call_action: box CallAction::ConcretizeCallee(tout),
                ..
            }),
        ) => {
            tout.add(l.dupe());
        }
        (
            AnyT(reason_fundef, src),
            CallT(box CallTData {
                use_op,
                reason: reason_op,
                call_action: box CallAction::Funcalltype(box calltype),
                return_hint: _,
            }),
        ) => {
            let call_this_t = &calltype.call_this_t;
            // An untyped receiver can't do anything with type args
            let call_args_tlist = &calltype.call_args_tlist;
            let call_tout = &calltype.call_tout;
            let call_specialized_callee = &calltype.call_specialized_callee;
            callee_recorder::add_callee(
                env,
                callee_recorder::Kind::All,
                l.dupe(),
                call_specialized_callee.as_ref(),
            );
            let src = flow_js_utils::any_mod_src_keep_placeholder(AnySource::Untyped, src);
            let any = any_t::why(src, reason_fundef.dupe());
            rec_flow_t(cx, env, trace, use_op.dupe(), (call_this_t, &any))?;
            call_args_iter(
                |t| {
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (t, &UseT::new(UseT(use_op.dupe(), any.dupe()))),
                    )?;
                    Ok::<(), FlowJsException>(())
                },
                call_args_tlist,
            )?;
            let any_op = any_t::why(src, reason_op.dupe());
            let open_tout = Type::new(OpenT(call_tout.dupe()));
            rec_flow_t(cx, env, trace, use_op.dupe(), (&any_op, &open_tout))?;
        }
        (
            _,
            ReactKitT(box ReactKitTData {
                use_op,
                reason: reason_op,
                tool,
            }),
        ) => {
            react_kit::run_with_env(cx, env, trace, use_op.dupe(), reason_op, l, tool)?;
        }

        // ****************************************
        // * You can cast an object to a function *
        // ****************************************
        (
            DefT(reason, def_t @ deref!(ObjT(_) | InstanceT(_))),
            CallT(box CallTData {
                use_op,
                reason: reason_op,
                ..
            }),
        ) => {
            let prop_name = Some(Name::new(FlowSmolStr::new_inline("$call")));
            let fun_t = match def_t.deref() {
                ObjT(
                    deref!(ObjType {
                        call_t: Some(id),
                        ..
                    }),
                ) => cx.find_call(*id),
                InstanceT(inst_t) if let Some(id) = inst_t.inst.inst_call_t.as_ref() => {
                    cx.find_call(*id)
                }
                _ => {
                    let reason_prop = reason_op
                        .dupe()
                        .replace_desc(VirtualReasonDesc::RProperty(prop_name.dupe()));
                    let error_message =
                        ErrorMessage::EPropNotFoundInLookup(Box::new(EPropNotFoundInLookupData {
                            prop_loc: reason_prop.loc().dupe(),
                            object: flow_js_utils::type_reference_with_reason_for_error(
                                l,
                                reason.dupe(),
                            ),
                            prop_name,
                            use_op: use_op.dupe(),
                            suggestion: None,
                        }));
                    flow_js_utils::add_output_with_env(cx, env, error_message)?;
                    any_t::error(reason_op.dupe())
                }
            };
            let repos_fun_t =
                helpers::reposition(cx, env, Some(trace), reason.loc().dupe(), None, None, fun_t)?;
            rec_flow(cx, env, trace, (&repos_fun_t, u))?;
        }
        (
            _,
            ConcretizeT(box ConcretizeTData {
                kind: ConcretizationKind::ConcretizeForConstructorReturn,
                collector,
                ..
            }),
        ) => {
            collector.add(l.dupe());
        }
        (AnyT(_, src), ObjTestProtoT(reason_op, u_inner)) => {
            let any = any_t::why(*src, reason_op.dupe());
            u_inner.add(any);
        }
        (DefT(_, deref!(NullT)), ObjTestProtoT(reason_op, u_inner)) => {
            let null_proto = Type::new(NullProtoT(
                reason_op.dupe().replace_desc(VirtualReasonDesc::RNull),
            ));
            u_inner.add(null_proto);
        }
        (_, ObjTestProtoT(reason_op, u_inner)) => {
            let proto = if flow_js_utils::object_like(l) {
                helpers::reposition(
                    cx,
                    env,
                    Some(trace),
                    reason_op.loc().dupe(),
                    None,
                    None,
                    l.dupe(),
                )?
            } else {
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    flow_js_utils::invalid_prototype_error(reason_op.loc().dupe(), l),
                )?;
                Type::new(ObjProtoT(reason_op.dupe().replace_desc(None)))
            };
            u_inner.add(proto);
        }

        // **************************************************
        // * instances of classes follow declared hierarchy *
        // **************************************************
        #[rustfmt::skip]
        (
            DefT(reason, deref!(InstanceT(inst_t))),
            ExtendsUseT(box ExtendsUseTData {
                use_op,
                reason: reason_op,
                targs: try_ts_on_failure,
                true_t: ext_l,
                false_t: ext_u @ deref!(DefT(reason_u, deref!(InstanceT(inst_super_t)))),
            }),
        ) => {
            let super_ = &inst_t.super_;
            let implements = &inst_t.implements;
            let inst = &inst_t.inst;
            let static_ = &inst_t.static_;
            let inst_super = &inst_super_t.inst;
            let static_super = &inst_super_t.static_;
            let super_super = &inst_super_t.super_;
            if flow_js_utils::is_same_instance_type(inst_t, inst_super_t) {
                let tmap1 = inst.type_args.dupe();
                let tmap2 = inst_super.type_args.dupe();
                let ureason = reason_op.dupe().update_desc_new(|desc| desc);
                flow_type_args(
                    cx,
                    env,
                    trace,
                    use_op.dupe(),
                    reason,
                    &ureason,
                    tmap1,
                    tmap2,
                    inst.strictness_kind,
                )?;
            } else if type_util::nominal_id_have_same_logical_module(
                &cx.file_options(),
                cx.projects_options(),
                (&inst.class_id, inst.class_name.as_deref()),
                (&inst_super.class_id, inst_super.class_name.as_deref()),
            ) && inst.type_args.len() == inst_super.type_args.len()
            {
                if type_util::is_in_common_interface_conformance_check(use_op) {
                    let implements_use_op =
                        VirtualUseOp::Op(Arc::new(VirtualRootUseOp::ClassImplementsCheck(
                            Box::new(ClassImplementsCheckData {
                                def: reason.dupe(),
                                name: reason.dupe(),
                                implements_loc: type_util::ref_loc_of_t(ext_u).dupe(),
                                implements: TypeOrTypeDescT::Type(ext_u.dupe()),
                            }),
                        )));
                    // We need to ensure that the shape of the class instances match.
                    let obj1 = inst_type_to_obj_type(
                        cx,
                        reason.dupe(),
                        inst.own_props.dupe(),
                        inst.proto_props.dupe(),
                        inst.inst_call_t,
                        &inst.inst_dict,
                        inst.strictness_kind,
                    );
                    let obj2 = inst_type_to_obj_type(
                        cx,
                        reason_u.dupe(),
                        inst_super.own_props.dupe(),
                        inst_super.proto_props.dupe(),
                        inst_super.inst_call_t,
                        &inst_super.inst_dict,
                        inst_super.strictness_kind,
                    );
                    rec_unify(
                        cx,
                        env,
                        trace,
                        implements_use_op.clone(),
                        UnifyCause::Uncategorized,
                        None,
                        &obj1,
                        &obj2,
                    )?;
                    // We need to ensure that the shape of the class statics match.
                    let spread_of = |reason: &Reason, t: &Type| -> Type {
                        let id = eval::Id::generate_id();
                        let destructor =
                            Rc::new(Destructor::SpreadType(Box::new(DestructorSpreadTypeData(
                                object::spread::Target::Annot {
                                    make_exact: false,
                                    strictness_kind: cx.type_strictness_kind(),
                                },
                                flow_data_structure_wrapper::list::FlowOcamlList::new(),
                                None,
                            ))));
                        Type::new(EvalT {
                            type_: t.dupe(),
                            defer_use_t: TypeDestructorT::new(TypeDestructorTInner(
                                use_op.dupe(),
                                reason.dupe(),
                                destructor,
                            )),
                            id,
                        })
                    };
                    let spread1 = spread_of(reason, static_);
                    let spread2 = spread_of(reason_u, static_super);
                    rec_unify(
                        cx,
                        env,
                        trace,
                        implements_use_op,
                        UnifyCause::Uncategorized,
                        None,
                        &spread1,
                        &spread2,
                    )?;
                    // We need to ensure that the classes have the same nominal hierarchy
                    rec_flow_t(cx, env, trace, use_op.dupe(), (super_, super_super))?;
                }
                // We need to ensure that the classes have the matching targs
                flow_type_args(
                    cx,
                    env,
                    trace,
                    use_op.dupe(),
                    reason,
                    reason_u,
                    inst.type_args.dupe(),
                    inst_super.type_args.dupe(),
                    inst.strictness_kind.join(inst_super.strictness_kind),
                )?;
            } else {
                // If this instance type has declared implementations, any structural
                // tests have already been performed at the declaration site. We can
                // then use the ExtendsUseT use type to search for a nominally matching
                // implementation, thereby short-circuiting a potentially expensive
                // structural test at the use site.
                let mut combined = try_ts_on_failure.to_vec();
                combined.extend(implements.iter().duped());
                let use_t = UseT::new(ExtendsUseT(Box::new(ExtendsUseTData {
                    use_op: use_op.dupe(),
                    reason: reason_op.dupe(),
                    targs: combined.into(),
                    true_t: ext_l.dupe(),
                    false_t: ext_u.dupe(),
                })));
                let repos_use = UseT::new(ReposLowerT {
                    reason: reason.dupe(),
                    use_desc: false,
                    use_t: Box::new(use_t),
                });
                return Ok(Some(TailCall::RecFlow(
                    super_.dupe(),
                    repos_use,
                    DepthTrace::rec_trace(trace),
                )));
            }
        }

        // *********************************************************
        // * class types derive instance types (with constructors) *
        // *********************************************************
        (DefT(reason, deref!(ClassT(this))), ConstructorT(box ctor_data)) => {
            let use_op = &ctor_data.use_op;
            let reason_op = &ctor_data.reason;
            let targs = &ctor_data.targs;
            let args = &ctor_data.args;
            let t = &ctor_data.tout;
            let return_hint = &ctor_data.return_hint;
            let specialized_ctor = &ctor_data.specialized_ctor;
            let reason_o = reason.dupe().replace_desc(None);
            let annot_loc = reason_op.loc().dupe();
            // Reject [new C(...)] where [C] is an abstract class. We do not
            // short-circuit — let the constructor call proceed so downstream
            // typing still produces sensible results.
            let concretize =
                |cx: &mut Context<'_>, t: &Type| -> Result<Vec<Type>, FlowJsException> {
                    helpers::possible_concrete_types_for_inspection(cx, env, reason_of_t(t), t)
                };
            if flow_js_utils::is_class_abstract(cx, &concretize, this)? {
                flow_js_utils::add_output_with_env(
                    cx,env,
                    ErrorMessage::EAbstractClass(Box::new(
                        flow_typing_errors::error_message::EAbstractClassData {
                            kind: flow_typing_errors::intermediate_error_types::AbstractErrorKind::AbstractClassInstantiation,
                            loc: annot_loc.dupe(),
                        },
                    )),
                )?;
            }
            // early error if type args passed to non-polymorphic class
            if targs.is_some() {
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::ECallTypeArity(Box::new(ECallTypeArityData {
                        call_loc: annot_loc.dupe(),
                        is_new: true,
                        callee: CallTypeArityCallee::Type(flow_js_utils::type_reference_for_error(
                            this,
                        )),
                        expected_arity: 0,
                    })),
                )?;
            }
            // call this.constructor(args)
            let ret = flow_typing_tvar::mk_no_wrap_where(
                cx,
                reason_op.dupe(),
                |cx, tvar_reason, tvar_id| {
                    let tout = Tvar::new(tvar_reason.dupe(), tvar_id as u32);
                    let funtype = MethodCallType {
                        meth_generic_this: None,
                        meth_targs: None,
                        meth_args_tlist: args.dupe(),
                        meth_tout: tout,
                        meth_strict_arity: true,
                    };
                    let propref = mk_named_prop(
                        reason_o.dupe(),
                        false,
                        Name::new(FlowSmolStr::new_inline("constructor")),
                    );
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (
                            this,
                            &UseT::new(MethodT(Box::new(MethodTData {
                                use_op: use_op.dupe(),
                                reason: reason_op.dupe(),
                                prop_reason: reason_o.dupe(),
                                propref: Box::new(propref),
                                method_action: Box::new(MethodAction::CallM(Box::new(CallMData {
                                    methodcalltype: funtype,
                                    return_hint: return_hint.clone(),
                                    specialized_callee: specialized_ctor.clone(),
                                }))),
                            }))),
                        ),
                    )?;
                    Ok::<(), FlowJsException>(())
                },
            )?;
            let annot_r = reason_op.dupe().reposition_and_annotate(annot_loc);
            // return this
            for ret_t in
                helpers::possible_concrete_types_for_constructor_return(cx, env, reason_op, &ret)?
            {
                if let AnyT(_, src) = ret_t.deref() {
                    let any = any_t::why(*src, annot_r.dupe());
                    rec_flow_t(cx, env, trace, unknown_use(), (&any, t))?;
                } else {
                    let repos_use = UseT::new(ReposLowerT {
                        reason: annot_r.dupe(),
                        use_desc: false,
                        use_t: Box::new(UseT::new(UseT(unknown_use(), t.dupe()))),
                    });
                    let ret_or_this = if flow_js_utils::object_like(&ret_t) {
                        &ret_t
                    } else {
                        this
                    };
                    rec_flow(cx, env, trace, (ret_or_this, &repos_use))?;
                }
            }
        }
        (AnyT(_, src), ConstructorT(box ctor_data)) => {
            let src = flow_js_utils::any_mod_src_keep_placeholder(AnySource::Untyped, src);
            let any_op = any_t::why(src, ctor_data.reason.dupe());
            // An untyped receiver can't do anything with type args
            call_args_iter(
                |arg_t| {
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (
                            arg_t,
                            &UseT::new(UseT(ctor_data.use_op.dupe(), any_op.dupe())),
                        ),
                    )?;
                    Ok::<(), FlowJsException>(())
                },
                &ctor_data.args,
            )?;
            rec_flow_t(cx, env, trace, unknown_use(), (&any_op, &ctor_data.tout))?;
        }
        // Interfaces with a construct signature can be constructed. The
        // sig is the [inst_construct_t] slot plus anything inherited via
        // [extends]; [collect_construct_ts] collects both in
        // derived-first order.
        (DefT(_, deref!(InstanceT(instance_t))), ConstructorT(box ctor_data)) => {
            let inst_abstract = instance_t.inst.inst_abstract;
            let use_op = &ctor_data.use_op;
            let reason_op = &ctor_data.reason;
            let targs = &ctor_data.targs;
            let args = &ctor_data.args;
            let t = &ctor_data.tout;
            let return_hint = &ctor_data.return_hint;
            let specialized_ctor = &ctor_data.specialized_ctor;
            if inst_abstract {
                flow_js_utils::add_output_with_env(
                    cx,env,
                    ErrorMessage::EAbstractClass(Box::new(
                        flow_typing_errors::error_message::EAbstractClassData {
                            kind: flow_typing_errors::intermediate_error_types::AbstractErrorKind::AbstractClassInstantiation,
                            loc: reason_op.loc().dupe(),
                        },
                    )),
                )?;
            }
            let concretize =
                |cx: &mut Context<'_>, t: &Type| -> Result<Vec<Type>, FlowJsException> {
                    helpers::possible_concrete_types_for_inspection(cx, env, reason_of_t(t), t)
                };
            match flow_js_utils::combine_construct_ts(flow_js_utils::collect_construct_ts(
                &concretize,
                cx,
                l,
            )?) {
                Some(construct_t) => {
                    let ret = flow_typing_tvar::mk_no_wrap_where(
                        cx,
                        reason_op.dupe(),
                        |cx, tvar_reason, tvar_id| {
                            let tout = Tvar::new(tvar_reason.dupe(), tvar_id as u32);
                            let calltype = flow_typing_type::type_::mk_functioncalltype(
                                reason_op.dupe(),
                                targs.clone(),
                                args.dupe(),
                                true,
                                tout,
                            );
                            let calltype = flow_typing_type::type_::FuncallType {
                                call_specialized_callee: specialized_ctor.clone(),
                                ..calltype
                            };
                            rec_flow(
                                cx,
                                env,
                                trace,
                                (
                                    &construct_t,
                                    &UseT::new(CallT(Box::new(CallTData {
                                        use_op: use_op.dupe(),
                                        reason: reason_op.dupe(),
                                        call_action: Box::new(CallAction::Funcalltype(Box::new(
                                            calltype,
                                        ))),
                                        return_hint: return_hint.clone(),
                                    }))),
                                ),
                            )?;
                            Ok::<(), FlowJsException>(())
                        },
                    )?;
                    rec_flow_t(cx, env, trace, use_op.dupe(), (&ret, t))?;
                }
                None => {
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        flow_js_utils::invalid_constructor_error(l),
                    )?;
                    let any_err = any_t::error(reason_op.dupe());
                    rec_flow_t(cx, env, trace, use_op.dupe(), (&any_err, t))?;
                }
            }
        }
        // Only classes (and `any`) can be constructed.
        (_, ConstructorT(box ctor_data)) => {
            flow_js_utils::add_output_with_env(
                cx,
                env,
                flow_js_utils::invalid_constructor_error(l),
            )?;
            let any_err = any_t::error(ctor_data.reason.dupe());
            rec_flow_t(
                cx,
                env,
                trace,
                ctor_data.use_op.dupe(),
                (&any_err, &ctor_data.tout),
            )?;
        }
        // Since we don't know the signature of a method on AnyT, assume every
        // parameter is an AnyT.
        (
            AnyT(_, src),
            MethodT(box MethodTData {
                use_op: _,
                reason: _,
                prop_reason: _,
                propref,
                method_action: box MethodAction::NoMethodAction(prop_t),
            }),
        ) => {
            let src = flow_js_utils::any_mod_src_keep_placeholder(AnySource::Untyped, src);
            let any = any_t::why(src, type_util::reason_of_propref(propref).dupe());
            prop_t.add(any);
        }
        (
            AnyT(_, src),
            PrivateMethodT(
                box pm_data @ PrivateMethodTData {
                    method_action: box MethodAction::NoMethodAction(prop_t),
                    ..
                },
            ),
        ) => {
            let src = flow_js_utils::any_mod_src_keep_placeholder(AnySource::Untyped, src);
            let any = any_t::why(src, pm_data.prop_reason.dupe());
            prop_t.add(any);
        }
        (
            AnyT(_, src),
            MethodT(box MethodTData {
                use_op,
                reason: reason_op,
                prop_reason: _,
                propref: _,
                method_action:
                    box MethodAction::CallM(box CallMData {
                        methodcalltype,
                        specialized_callee,
                        ..
                    }),
            }),
        ) => {
            let src = flow_js_utils::any_mod_src_keep_placeholder(AnySource::Untyped, src);
            let any = any_t::why(src, reason_op.dupe());
            call_args_iter(
                |t| {
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (t, &UseT::new(UseT(use_op.dupe(), any.dupe()))),
                    )?;
                    Ok::<(), FlowJsException>(())
                },
                &methodcalltype.meth_args_tlist,
            )?;
            callee_recorder::add_callee(
                env,
                callee_recorder::Kind::Tast,
                l.dupe(),
                specialized_callee.as_ref(),
            );
            let open_tout = Type::new(OpenT(methodcalltype.meth_tout.dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (&any, &open_tout))?;
        }
        (
            AnyT(_, src),
            PrivateMethodT(
                box pm_data @ PrivateMethodTData {
                    method_action:
                        box MethodAction::CallM(box CallMData {
                            methodcalltype,
                            specialized_callee,
                            ..
                        }),
                    ..
                },
            ),
        ) => {
            let src = flow_js_utils::any_mod_src_keep_placeholder(AnySource::Untyped, src);
            let any = any_t::why(src, pm_data.reason.dupe());
            call_args_iter(
                |t| {
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (t, &UseT::new(UseT(pm_data.use_op.dupe(), any.dupe()))),
                    )?;
                    Ok::<(), FlowJsException>(())
                },
                &methodcalltype.meth_args_tlist,
            )?;
            callee_recorder::add_callee(
                env,
                callee_recorder::Kind::Tast,
                l.dupe(),
                specialized_callee.as_ref(),
            );
            let open_tout = Type::new(OpenT(methodcalltype.meth_tout.dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (&any, &open_tout))?;
        }
        (
            AnyT(_, src),
            MethodT(box MethodTData {
                use_op,
                reason: reason_op,
                prop_reason: _,
                propref: _,
                method_action: box chain @ MethodAction::ChainM(..),
            }),
        ) => {
            let src = flow_js_utils::any_mod_src_keep_placeholder(AnySource::Untyped, src);
            let any = any_t::why(src, reason_op.dupe());
            apply_method_action(
                cx,
                env,
                trace,
                &any,
                use_op.dupe(),
                reason_op.dupe(),
                l.dupe(),
                chain,
            )?;
        }
        (
            AnyT(_, src),
            PrivateMethodT(
                box pm_data @ PrivateMethodTData {
                    method_action: box chain @ MethodAction::ChainM(..),
                    ..
                },
            ),
        ) => {
            let src = flow_js_utils::any_mod_src_keep_placeholder(AnySource::Untyped, src);
            let any = any_t::why(src, pm_data.reason.dupe());
            apply_method_action(
                cx,
                env,
                trace,
                &any,
                pm_data.use_op.dupe(),
                pm_data.reason.dupe(),
                l.dupe(),
                chain,
            )?;
        }
        // *************************
        // * statics can be read   *
        // *************************
        (DefT(_, deref!(InstanceT(inst_t))), GetStaticsT(reason_op, collector)) => {
            let static_ = helpers::reposition_reason(
                cx,
                env,
                Some(trace),
                reason_op,
                false,
                &inst_t.static_,
            )?;
            let mut statics = Vec::new();
            flow_js_utils::collect_lowers(
                true,
                cx,
                &mut BTreeSet::new(),
                &mut statics,
                vec![static_],
            );
            for static_ in statics {
                collector.add(static_);
            }
        }
        (AnyT(_, src), GetStaticsT(reason_op, collector)) => {
            collector.add(any_t::why(*src, reason_op.dupe()));
        }
        (ObjProtoT(_), GetStaticsT(reason_op, collector)) => {
            collector.add(helpers::reposition_reason(
                cx,
                env,
                Some(trace),
                reason_op,
                false,
                l,
            )?);
        }
        // ********************
        // * __proto__ getter *
        // ********************

        // TODO: Fix GetProtoT for InstanceT (and ClassT).
        // The __proto__ object of an instance is an ObjT having the properties in
        // insttype.methods_tmap, not the super instance.
        (DefT(_, deref!(InstanceT(inst_t))), GetProtoT(reason_op, t)) => {
            let super_ = &inst_t.super_;
            let proto = helpers::reposition(
                cx,
                env,
                Some(trace),
                reason_op.loc().dupe(),
                None,
                None,
                super_.dupe(),
            )?;
            let mut protos = Vec::new();
            flow_js_utils::collect_lowers(true, cx, &mut BTreeSet::new(), &mut protos, vec![proto]);
            for proto in protos {
                t.add(proto);
            }
        }
        (DefT(_, deref!(ObjT(obj))), GetProtoT(reason_op, t)) => {
            let proto_t = &obj.proto_t;
            let proto = helpers::reposition(
                cx,
                env,
                Some(trace),
                reason_op.loc().dupe(),
                None,
                None,
                proto_t.dupe(),
            )?;
            let mut protos = Vec::new();
            flow_js_utils::collect_lowers(true, cx, &mut BTreeSet::new(), &mut protos, vec![proto]);
            for proto in protos {
                t.add(proto);
            }
        }
        (ObjProtoT(_), GetProtoT(reason_op, t)) => {
            let proto = null::why(reason_op.dupe());
            t.add(proto);
        }
        (FunProtoT(reason), GetProtoT(reason_op, t)) => {
            let proto = Type::new(ObjProtoT(reason.dupe().reposition(reason_op.loc().dupe())));
            t.add(proto);
        }
        (AnyT(_, src), GetProtoT(reason_op, t)) => {
            let src = flow_js_utils::any_mod_src_keep_placeholder(AnySource::Untyped, src);
            let proto = any_t::why(src, reason_op.dupe());
            t.add(proto);
        }

        // ********************************************************
        // * instances of classes may have their fields looked up *
        // ********************************************************
        (
            DefT(_, deref!(InstanceT(inst_t))),
            LookupT(box LookupTData {
                reason: reason_op,
                lookup_kind: kind,
                indexer_fallback,
                try_ts_on_failure,
                propref,
                lookup_action: action,
                ids,
                method_accessible,
                ignore_dicts,
            }),
        ) => {
            let super_ = &inst_t.super_;
            let inst = &inst_t.inst;
            let use_op = use_op_of_lookup_action(action);
            let indexer_fallback_mode = match action.as_ref() {
                LookupAction::ReadProp(_) => IndexerFallbackMode::PropertyAndIndexedAccess,
                LookupAction::WriteProp(_) => IndexerFallbackMode::PropertyAccess,
                _ => IndexerFallbackMode::Disabled,
            };
            let mut indexer_fallback = indexer_fallback.clone();
            let mut missing = vec![];
            for (propref, up) in lookup_targets(propref, action) {
                let (property, candidate) =
                    flow_js_utils::get_prop_t_kit::get_instance_prop_for_lookup::<FlowJs>(
                        cx,
                        env,
                        &trace,
                        &use_op,
                        *ignore_dicts,
                        inst,
                        propref,
                        reason_op,
                        indexer_fallback_mode,
                        l,
                    )?;
                // A candidate found closer to the access wins: it is the one an
                // inherited declared property would have to beat.
                indexer_fallback = indexer_fallback.or(candidate);
                if (property.is_some() || indexer_fallback.is_some())
                    && let LookupKind::NonstrictReturning(box NonstrictReturningData(
                        _,
                        Some((id, _)),
                    )) = &**kind
                {
                    cx.test_prop_hit(*id);
                }
                match property {
                    Some((p, target_kind)) => {
                        let p = if !*method_accessible
                            || matches!(action.as_ref(), LookupAction::ReadProp(_))
                        {
                            flow_js_utils::method_property_for_read(*method_accessible, p)
                        } else {
                            p
                        };
                        let property_type = property::property_type(&p);
                        perform_lookup_action(
                            cx,
                            env,
                            trace,
                            propref,
                            &property_type,
                            Some(&p),
                            target_kind,
                            l,
                            reason_op,
                            action,
                            up,
                        )?;
                    }
                    None => missing.push((propref, up)),
                }
            }
            if let Some((propref, action)) = remaining_lookup(action, &missing) {
                let new_ids = ids.as_ref().map(|s| {
                    if s.contains(&inst.own_props) || s.contains(&inst.proto_props) {
                        s.dupe()
                    } else {
                        let mut new_set = s.dupe();
                        new_set.insert(inst.own_props.dupe());
                        new_set.insert(inst.proto_props.dupe());
                        new_set
                    }
                });
                rec_flow(
                    cx,
                    env,
                    trace,
                    (
                        super_,
                        &UseT::new(LookupT(Box::new(LookupTData {
                            reason: reason_op.dupe(),
                            lookup_kind: kind.clone(),
                            indexer_fallback,
                            try_ts_on_failure: try_ts_on_failure.clone(),
                            propref: Box::new(propref),
                            lookup_action: Box::new(action),
                            ids: new_ids,
                            method_accessible: *method_accessible,
                            ignore_dicts: *ignore_dicts,
                        }))),
                    ),
                )?;
            }
        }
        // ********************************
        // * ... and their fields written *
        // ********************************
        (DefT(_, deref!(InstanceT(inst_t))), SetPropT(use_op, _, propref, _, _, _, _))
            if let Some(ReactDro(dro_loc, dro_type)) = inst_t.inst.inst_react_dro.as_ref()
                && !flow_js_utils::is_exception_to_react_dro(propref) =>
        {
            let reason_prop = type_util::reason_of_propref(propref);
            let prop_name = name_of_propref(propref);
            let use_op = UseOp::Frame(
                std::sync::Arc::new(VirtualFrameUseOp::ReactDeepReadOnly(Box::new((
                    dro_loc.dupe(),
                    dro_type.clone(),
                )))),
                Arc::new(use_op.dupe()),
            );
            add_output_with_env(
                cx,
                env,
                ErrorMessage::EPropNotWritable(Box::new(EPropNotWritableData {
                    prop_loc: reason_prop.loc().dupe(),
                    prop_name,
                    use_op,
                })),
            )?;
        }
        (
            DefT(_, deref!(InstanceT(_))),
            SetPropT(use_op, reason_op, propref, mode, write_ctx, tin, prop_tout),
        ) => {
            let lookup_action = LookupAction::WriteProp(Box::new(WritePropData {
                use_op: use_op.dupe(),
                obj_t: l.dupe(),
                prop_tout: prop_tout.as_ref().map(|t| t.dupe()),
                tin: tin.dupe(),
                write_ctx: write_ctx.clone(),
                mode: mode.clone(),
            }));
            let method_accessible = true;
            let lookup_kind = instance_lookup_kind(
                cx,
                reason_op,
                method_accessible,
                l,
                propref,
                lookup_action.clone(),
            );
            rec_flow(
                cx,
                env,
                trace,
                (
                    l,
                    &UseT::new(LookupT(Box::new(LookupTData {
                        reason: reason_op.dupe(),
                        lookup_kind: Box::new(lookup_kind),
                        indexer_fallback: None,
                        try_ts_on_failure: Rc::from([]),
                        propref: propref.clone(),
                        lookup_action: Box::new(lookup_action),
                        ids: None,
                        method_accessible,
                        ignore_dicts: true,
                    }))),
                ),
            )?;
        }
        (DefT(reason_c, deref!(InstanceT(_))), SetPrivatePropT(box spp_data))
            if spp_data.class_bindings.is_empty() =>
        {
            add_output_with_env(
                cx,
                env,
                ErrorMessage::EPrivateLookupFailed(Box::new(EPrivateLookupFailedData {
                    loc: spp_data.reason.loc().dupe(),
                    object: flow_js_utils::type_reference_with_reason_for_error(l, reason_c.dupe()),
                    prop_name: Name::new(spp_data.name.dupe()),
                    use_op: spp_data.use_op.dupe(),
                })),
            )?;
        }
        (DefT(reason_c, deref!(InstanceT(inst_t))), SetPrivatePropT(box spp_data))
            if !spp_data.class_bindings.is_empty() =>
        {
            let inst = &inst_t.inst;
            let scope = &spp_data.class_bindings[0];
            let rest_scopes = &spp_data.class_bindings[1..];
            if scope.class_binding_id != inst.class_id {
                rec_flow(
                    cx,
                    env,
                    trace,
                    (
                        l,
                        &UseT::new(SetPrivatePropT(Box::new(SetPrivatePropTData {
                            use_op: spp_data.use_op.dupe(),
                            reason: spp_data.reason.dupe(),
                            name: spp_data.name.dupe(),
                            set_mode: spp_data.set_mode.clone(),
                            class_bindings: rest_scopes.to_vec().into(),
                            static_: spp_data.static_,
                            write_ctx: spp_data.write_ctx.clone(),
                            tin: spp_data.tin.dupe(),
                            tout: spp_data.tout.as_ref().map(|t| t.dupe()),
                        }))),
                    ),
                )?;
            } else {
                let map = if spp_data.static_ {
                    inst.class_private_static_fields.dupe()
                } else {
                    inst.class_private_fields.dupe()
                };
                let name = Name::new(spp_data.name.dupe());
                let props = cx.find_props(map);
                match props.get(&name) {
                    None => {
                        add_output_with_env(
                            cx,
                            env,
                            ErrorMessage::EPrivateLookupFailed(Box::new(
                                EPrivateLookupFailedData {
                                    loc: spp_data.reason.loc().dupe(),
                                    object: flow_js_utils::type_reference_with_reason_for_error(
                                        l,
                                        reason_c.dupe(),
                                    ),
                                    prop_name: name,
                                    use_op: spp_data.use_op.dupe(),
                                },
                            )),
                        )?;
                    }
                    Some(p) => {
                        let action = LookupAction::WriteProp(Box::new(WritePropData {
                            use_op: spp_data.use_op.dupe(),
                            obj_t: l.dupe(),
                            prop_tout: spp_data.tout.as_ref().map(|t| t.dupe()),
                            tin: spp_data.tin.dupe(),
                            write_ctx: spp_data.write_ctx.clone(),
                            mode: spp_data.set_mode.clone(),
                        }));
                        let propref = mk_named_prop(spp_data.reason.dupe(), false, name);
                        perform_lookup_action(
                            cx,
                            env,
                            trace,
                            &propref,
                            &property::property_type(p),
                            Some(p),
                            PropertySource::PropertyMapProperty,
                            l,
                            &spp_data.reason,
                            &action,
                            None,
                        )?;
                    }
                }
            }
        }

        // *****************************
        // * ... and their fields read *
        // *****************************
        (
            DefT(r, deref!(InstanceT(_))),
            GetPropT(
                box data @ GetPropTData {
                    propref: box PropRef::Named { name, .. },
                    ..
                },
            ),
        ) if name.matches_str("constructor") => {
            let t = type_util::class_type(l.dupe(), r.annot_loc().map(|a| a.dupe()));
            rec_flow_t(
                cx,
                env,
                trace,
                unknown_use(),
                (&t, &Type::new(OpenT((*data.tout).dupe()))),
            )?;
        }
        (DefT(_, deref!(InstanceT(inst_t))), GetPropT(box data)) => {
            let super_ = &inst_t.super_;
            let inst = &inst_t.inst;
            let method_accessible = data.from_annot;
            let lookup_action = LookupAction::ReadProp(Box::new(ReadPropData {
                use_op: data.use_op.dupe(),
                obj_t: l.dupe(),
                tout: (*data.tout).dupe(),
            }));
            let lookup_kind = instance_lookup_kind(
                cx,
                &data.reason,
                method_accessible,
                l,
                &data.propref,
                lookup_action.clone(),
            );
            flow_js_utils::get_prop_t_kit::read_instance_prop::<FlowJs>(
                cx,
                env,
                &trace,
                &data.use_op,
                l,
                data.id,
                method_accessible,
                super_.dupe(),
                lookup_kind,
                &data.hint,
                data.skip_optional,
                inst,
                &data.propref,
                &data.reason,
            )?(cx, (*data.tout).dupe())?;
        }
        (DefT(reason_c, deref!(InstanceT(inst_t))), GetPrivatePropT(box gpp_data)) => {
            let inst = &inst_t.inst;
            get_private_prop(
                cx,
                env,
                false,
                trace,
                l,
                reason_c,
                inst,
                &gpp_data.use_op,
                &gpp_data.reason,
                &gpp_data.name,
                &gpp_data.class_bindings,
                gpp_data.static_,
                &gpp_data.tout,
            )?;
        }
        // ********************************
        // * ... and their methods called *
        // ********************************
        (
            DefT(_, deref!(InstanceT(inst_t))),
            MethodT(box MethodTData {
                use_op,
                reason: reason_call,
                prop_reason: reason_lookup,
                propref,
                method_action: action,
            }),
        ) => {
            let super_ = &inst_t.super_;
            let inst = &inst_t.inst;
            let funt = flow_typing_tvar::mk_no_wrap_where(
                cx,
                reason_lookup.dupe(),
                |cx, reason_tout, tout| {
                    let tout_tvar = Tvar::new(reason_tout.dupe(), tout as u32);
                    let lookup_action = LookupAction::ReadProp(Box::new(ReadPropData {
                        use_op: use_op.dupe(),
                        obj_t: l.dupe(),
                        tout: tout_tvar.dupe(),
                    }));
                    let method_accessible = true;
                    let lookup_kind = instance_lookup_kind(
                        cx,
                        reason_lookup,
                        method_accessible,
                        l,
                        propref,
                        lookup_action.clone(),
                    );
                    flow_js_utils::get_prop_t_kit::read_instance_prop::<FlowJs>(
                        cx,
                        env,
                        &trace,
                        use_op,
                        l,
                        None,
                        true,
                        super_.dupe(),
                        lookup_kind,
                        &hint_unavailable(),
                        false,
                        inst,
                        propref,
                        reason_call,
                    )?(cx, tout_tvar.dupe())?;
                    Ok::<(), FlowJsException>(())
                },
            )?;
            // suppress ops while calling the function. if `funt` is a `FunT`, then
            // `CallT` will set its own ops during the call. if `funt` is something
            // else, then something like `VoidT ~> CallT` doesn't need the op either
            // because we want to point at the call and undefined thing.
            apply_method_action(
                cx,
                env,
                trace,
                &funt,
                use_op.dupe(),
                reason_call.dupe(),
                l.dupe(),
                action,
            )?;
        }
        (DefT(reason_c, deref!(InstanceT(inst_t))), PrivateMethodT(box pm_data)) => {
            let inst = &inst_t.inst;
            let tvar = flow_typing_tvar::mk_no_wrap(cx, &pm_data.prop_reason);
            let funt = Type::new(OpenT(Tvar::new(pm_data.prop_reason.dupe(), tvar as u32)));
            let l_for_private = if pm_data.static_ {
                type_util::class_type(l.dupe(), None)
            } else {
                l.dupe()
            };
            get_private_prop(
                cx,
                env,
                true,
                trace,
                &l_for_private,
                reason_c,
                inst,
                &pm_data.use_op,
                &pm_data.reason,
                &pm_data.name,
                &pm_data.class_bindings,
                pm_data.static_,
                &Tvar::new(pm_data.prop_reason.dupe(), tvar as u32),
            )?;
            apply_method_action(
                cx,
                env,
                trace,
                &funt,
                pm_data.use_op.dupe(),
                pm_data.reason.dupe(),
                l_for_private,
                &pm_data.method_action,
            )?;
        }

        // *****************************************************************
        // * Object.assign logic has been moved to type_operation_utils.rs *
        // *****************************************************************
        (
            _,
            ConcretizeT(box ConcretizeTData {
                kind: ConcretizationKind::ConcretizeForObjectAssign,
                collector,
                ..
            }),
        ) => {
            collector.add(l.dupe());
        }
        (
            _,
            ConcretizeT(box ConcretizeTData {
                kind: ConcretizationKind::ConcretizeForDestructuring,
                collector,
                ..
            }),
        ) => {
            collector.add(l.dupe());
        }
        // *************************
        // * objects can be copied *
        // *************************
        (DefT(reason_obj, deref!(ObjT(o))), ObjRestT(reason_op, xs, t, id, literal)) => {
            let props_tmap = o.props_tmap.dupe();
            let obj_kind = &o.flags.obj_kind;
            let reachable_targs = &o.reachable_targs;
            let guard_key = ObjRestGuardKey {
                reason: reason_obj.dupe(),
                props: cx.find_props(props_tmap.dupe()),
                reachable_targs: reachable_targs.dupe(),
                obj_kind: obj_kind.clone(),
            };
            const_fold_expansion::guard_obj_rest(cx, *id, guard_key, |cx, count| {
                match count {
                    0 => {
                        let xs_smol: Vec<flow_data_structure_wrapper::smol_str::FlowSmolStr> =
                            xs.iter().map(|s| s.as_str().into()).collect();
                        let rest_obj = flow_js_utils::objt_to_obj_rest(
                            cx,
                            env,
                            props_tmap.dupe(),
                            Some(reachable_targs.dupe()),
                            obj_kind.clone(),
                            reason_op,
                            reason_obj,
                            &xs_smol,
                            *literal,
                        )?;
                        rec_flow_t(cx, env, trace, unknown_use(), (&rest_obj, t))?;
                    }
                    _ => {}
                }
                Ok::<(), FlowJsException>(())
            })?;
        }
        (DefT(reason, deref!(InstanceT(inst_t))), ObjRestT(reason_op, xs, t, _, literal)) => {
            let super_ = &inst_t.super_;
            let inst = &inst_t.inst;
            // Spread fields from super into an object
            let obj_super = flow_typing_tvar::mk_where(cx, reason_op.dupe(), |cx, tvar| {
                let use_t = UseT::new(ObjRestT(
                    reason_op.dupe(),
                    xs.clone(),
                    tvar.dupe(),
                    flow_common::reason::mk_id() as i32,
                    *literal,
                ));
                rec_flow(
                    cx,
                    env,
                    trace,
                    (
                        super_,
                        &UseT::new(ReposLowerT {
                            reason: reason.dupe(),
                            use_desc: false,
                            use_t: Box::new(use_t),
                        }),
                    ),
                )?;
                Ok::<(), FlowJsException>(())
            })?;
            let xs_smol: Vec<flow_data_structure_wrapper::smol_str::FlowSmolStr> =
                xs.iter().map(|s| s.as_str().into()).collect();
            let o = flow_js_utils::objt_to_obj_rest(
                cx,
                env,
                inst.own_props.dupe(),
                Some(Rc::from([])),
                ObjKind::Exact,
                reason_op,
                reason,
                &xs_smol,
                *literal,
            );
            // Combine super and own props.
            let use_op = UseOp::Op(std::sync::Arc::new(VirtualRootUseOp::ObjectSpread {
                op: reason_op.dupe(),
            }));
            let spread_tool = object::ResolveTool::Resolve(object::Resolve::Next);
            let spread_target = object::spread::Target::Value {
                make_seal: obj_type::mk_seal(false, false),
                strictness_kind: cx.type_strictness_kind(),
                literal: *literal,
            };
            let spread_state = object::spread::State {
                todo_rev: flow_data_structure_wrapper::list::FlowOcamlList::unit(
                    object::spread::Operand::Type(o?),
                ),
                acc: flow_data_structure_wrapper::list::FlowOcamlList::new(),
                spread_id: flow_common::reason::mk_id() as i32,
                union_t: None,
                curr_resolve_idx: 0,
            };
            let o2 = flow_typing_tvar::mk_where(cx, reason_op.dupe(), |cx, tvar| {
                rec_flow(
                    cx,
                    env,
                    trace,
                    (
                        &obj_super,
                        &UseT::new(ObjKitT(
                            use_op.dupe(),
                            reason_op.dupe(),
                            Box::new(spread_tool.clone()),
                            Box::new(object::Tool::Spread(Box::new((
                                spread_target,
                                spread_state.clone(),
                            )))),
                            tvar.dupe(),
                        )),
                    ),
                )
            })?;
            rec_flow_t(cx, env, trace, use_op, (&o2, t))?;
        }
        (AnyT(_, src), ObjRestT(reason, _, t, _, _)) => {
            rec_flow_t(
                cx,
                env,
                trace,
                unknown_use(),
                (&any_t::why(*src, reason.dupe()), t),
            )?;
        }
        (ObjProtoT(_), ObjRestT(reason, _, t, _, literal)) => {
            let obj = obj_type::mk_with_proto(
                cx,
                reason.dupe(),
                ObjKind::Exact,
                None,
                None,
                None,
                None,
                l.dupe(),
                *literal,
            );
            rec_flow_t(cx, env, trace, unknown_use(), (&obj, t))?;
        }
        (DefT(_, deref!(NullT | VoidT)), ObjRestT(reason, _, t, _, literal)) => {
            let o = obj_type::mk(ObjKind::Exact, cx, reason.dupe(), *literal);
            rec_flow_t(cx, env, trace, unknown_use(), (&o, t))?;
        }
        // *******************************************
        // * objects may have their fields looked up *
        // *******************************************
        (
            DefT(_, deref!(ObjT(o))),
            LookupT(box LookupTData {
                reason: reason_op,
                lookup_kind,
                indexer_fallback,
                try_ts_on_failure,
                propref,
                lookup_action: action,
                ids,
                method_accessible,
                ignore_dicts,
            }),
        ) => {
            let mut missing = vec![];
            for (propref, up) in lookup_targets(propref, action) {
                match flow_js_utils::get_prop_t_kit::get_obj_prop::<FlowJs>(
                    cx,
                    env,
                    &trace,
                    &unknown_use(),
                    false,
                    true,
                    o,
                    propref,
                    reason_op,
                )? {
                    Some((p, target_kind)) => {
                        if let LookupKind::NonstrictReturning(box NonstrictReturningData(
                            _,
                            Some((id, _)),
                        )) = &**lookup_kind
                        {
                            cx.test_prop_hit(*id);
                        }
                        perform_lookup_action(
                            cx,
                            env,
                            trace,
                            propref,
                            &p,
                            None,
                            target_kind.clone(),
                            l,
                            reason_op,
                            action,
                            up,
                        )?;
                    }
                    None => missing.push((propref, up)),
                }
            }
            if let Some((propref, action)) = remaining_lookup(action, &missing) {
                let new_ids = ids.as_ref().map(|s| {
                    let mut new_set = s.dupe();
                    new_set.insert(o.props_tmap.dupe());
                    new_set
                });
                rec_flow(
                    cx,
                    env,
                    trace,
                    (
                        &o.proto_t,
                        &UseT::new(LookupT(Box::new(LookupTData {
                            reason: reason_op.dupe(),
                            lookup_kind: lookup_kind.clone(),
                            indexer_fallback: indexer_fallback.clone(),
                            try_ts_on_failure: try_ts_on_failure.clone(),
                            propref: Box::new(propref),
                            lookup_action: Box::new(action),
                            method_accessible: *method_accessible,
                            ids: new_ids,
                            ignore_dicts: *ignore_dicts,
                        }))),
                    ),
                )?;
            }
        }
        (
            AnyT(_, src),
            LookupT(box LookupTData {
                reason: reason_op,
                lookup_kind,
                propref,
                lookup_action: action,
                ..
            }),
        ) => {
            match action {
                box LookupAction::SuperProp(box (_, lp, _, _))
                    if property::write_t(lp).is_none() =>
                {
                    // Without this exception, we will call rec_flow_p where
                    // `write_t(lp)` is `None` and `write_t(up)` is `Some`, which is a polarity
                    // mismatch error. Instead of this, we could "read" `mixed` from
                    // covariant props, which would always flow into `any`.
                }
                _ => {
                    let src = flow_js_utils::any_mod_src_keep_placeholder(AnySource::Untyped, src);
                    let p = PropertyType::OrdinaryField {
                        type_: any_t::why(src, reason_op.dupe()),
                        polarity: Polarity::Neutral,
                    };
                    if let LookupKind::NonstrictReturning(box NonstrictReturningData(
                        _,
                        Some((id, _)),
                    )) = &**lookup_kind
                    {
                        cx.test_prop_hit(*id);
                    }
                    for (propref, up) in lookup_targets(propref, action) {
                        perform_lookup_action(
                            cx,
                            env,
                            trace,
                            propref,
                            &p,
                            None,
                            PropertySource::DynamicProperty,
                            l,
                            reason_op,
                            action,
                            up,
                        )?;
                    }
                }
            }
        }

        // ********************************
        // * ... and their fields written *
        // ********************************

        // o.x = ... has the additional effect of o[_] = ...
        (DefT(_, deref!(ObjT(o))), SetPropT(use_op, _, propref, _, _, _, _))
            if flow_js_utils::obj_is_readonlyish(o)
                && !flow_js_utils::is_exception_to_react_dro(propref) =>
        {
            let reason_prop = type_util::reason_of_propref(propref);
            let prop_name = name_of_propref(propref);
            let use_op = match &o.flags.react_dro {
                Some(ReactDro(loc, dro_type)) => UseOp::Frame(
                    std::sync::Arc::new(VirtualFrameUseOp::ReactDeepReadOnly(Box::new((
                        loc.dupe(),
                        dro_type.clone(),
                    )))),
                    Arc::new(use_op.dupe()),
                ),
                None => use_op.dupe(),
            };
            add_output_with_env(
                cx,
                env,
                ErrorMessage::EPropNotWritable(Box::new(EPropNotWritableData {
                    prop_loc: reason_prop.loc().dupe(),
                    prop_name,
                    use_op,
                })),
            )?;
        }
        (
            DefT(reason_obj, deref!(ObjT(o))),
            SetPropT(use_op, reason_op, propref, mode, _, tin, prop_t),
        ) => {
            write_obj_prop(
                cx, env, trace, use_op, mode, o, propref, reason_obj, reason_op, tin, prop_t,
            )?;
        }

        // Since we don't know the type of the prop, use AnyT.
        (AnyT(_, src), SetPropT(use_op, reason_op, _, _, _, t, prop_t)) => {
            let src = flow_js_utils::any_mod_src_keep_placeholder(AnySource::Untyped, src);
            if let Some(prop_t) = prop_t {
                rec_flow_t(
                    cx,
                    env,
                    trace,
                    unknown_use(),
                    (&any_t::why(src, reason_op.dupe()), prop_t),
                )?;
            }
            rec_flow(
                cx,
                env,
                trace,
                (
                    t,
                    &UseT::new(UseT(use_op.dupe(), any_t::why(src, reason_op.dupe()))),
                ),
            )?;
        }

        // *****************************
        // * ... and their fields read *
        // *****************************
        (
            DefT(_, deref!(ObjT(_))),
            GetPropT(
                box data @ GetPropTData {
                    propref: box PropRef::Named { name, .. },
                    ..
                },
            ),
        ) if name.matches_str("constructor") => {
            rec_flow_t(
                cx,
                env,
                trace,
                unknown_use(),
                (
                    &unsoundness::why(UnsoundnessKind::Constructor, data.reason.dupe()),
                    &Type::new(OpenT((*data.tout).dupe())),
                ),
            )?;
        }
        (DefT(reason_obj, deref!(ObjT(o))), GetPropT(box data)) => {
            let lookup_info = data.id.map(|id| {
                (
                    id,
                    LookupDefaultResult::Flow {
                        use_op: data.use_op.dupe(),
                        tout: Type::new(OpenT((*data.tout).dupe())),
                    },
                )
            });
            flow_js_utils::get_prop_t_kit::read_obj_prop::<FlowJs>(
                cx,
                env,
                &trace,
                data.use_op.dupe(),
                data.from_annot,
                data.skip_optional,
                o,
                &data.propref,
                reason_obj.dupe(),
                data.reason.dupe(),
                lookup_info,
            )?(cx, (*data.tout).dupe())?;
        }
        (AnyT(_, src), GetPropT(box data)) => {
            if let Some(id) = data.id {
                cx.test_prop_hit(id);
            }
            let src = flow_js_utils::any_mod_src_keep_placeholder(AnySource::Untyped, src);
            rec_flow_t(
                cx,
                env,
                trace,
                unknown_use(),
                (
                    &any_t::why(src, data.reason.dupe()),
                    &Type::new(OpenT((*data.tout).dupe())),
                ),
            )?;
        }
        (AnyT(_, src), GetPrivatePropT(box gpp_data)) => {
            let src = flow_js_utils::any_mod_src_keep_placeholder(AnySource::Untyped, src);
            rec_flow_t(
                cx,
                env,
                trace,
                unknown_use(),
                (
                    &any_t::why(src, gpp_data.reason.dupe()),
                    &Type::new(OpenT((*gpp_data.tout).dupe())),
                ),
            )?;
        }
        // ********************************
        // * ... and their methods called *
        // ********************************
        (
            DefT(_, deref!(ObjT(_))),
            MethodT(box MethodTData {
                use_op: _,
                reason: reason_call,
                prop_reason: _,
                propref: box PropRef::Named { name, .. },
                method_action: action,
            }),
        ) if name.matches_str("constructor") => {
            add_specialized_callee_method_action(env, &any_t::untyped(reason_call.dupe()), action)?;
        }
        (
            DefT(reason_obj, deref!(ObjT(o))),
            MethodT(box MethodTData {
                use_op,
                reason: reason_call,
                prop_reason: reason_lookup,
                propref,
                method_action: action,
            }),
        ) => {
            let t = flow_typing_tvar::mk_no_wrap_where(
                cx,
                reason_lookup.dupe(),
                |cx, reason_tout, tout| {
                    flow_js_utils::get_prop_t_kit::read_obj_prop::<FlowJs>(
                        cx,
                        env,
                        &trace,
                        use_op.dupe(),
                        false,
                        false,
                        o,
                        propref,
                        reason_obj.dupe(),
                        reason_lookup.dupe(),
                        None,
                    )?(cx, Tvar::new(reason_tout.dupe(), tout as u32))?;
                    Ok::<(), FlowJsException>(())
                },
            )?;
            apply_method_action(
                cx,
                env,
                trace,
                &t,
                use_op.dupe(),
                reason_call.dupe(),
                l.dupe(),
                action,
            )?;
        }
        // ******************************************
        // * strings may have their characters read *
        // ******************************************
        (
            DefT(reason_s, deref!(StrGeneralT(_) | SingletonStrT { .. })),
            GetElemT(box GetElemTData {
                use_op,
                reason: reason_op,
                key_t,
                tout,
                ..
            }),
        ) => {
            rec_flow(
                cx,
                env,
                trace,
                (
                    key_t,
                    &UseT::new(UseT(use_op.dupe(), num_module_t::why(reason_s.dupe()))),
                ),
            )?;
            rec_flow_t(
                cx,
                env,
                trace,
                unknown_use(),
                (
                    &str_module_t::why(reason_op.dupe()),
                    &Type::new(OpenT((**tout).dupe())),
                ),
            )?;
        }
        // Expressions may be used as keys to access objects and arrays. In
        // general, we cannot evaluate such expressions at compile time. However,
        // in some idiomatic special cases, we can; in such cases, we know exactly
        // which strings/numbers the keys may be, and thus, we can use precise
        // properties and indices to resolve the accesses.

        // **********************************************************************
        // * objects/arrays may have their properties/elements written and read *
        // **********************************************************************
        (
            DefT(_, deref!(ObjT(_) | ArrT(_) | InstanceT(_))),
            SetElemT(box SetElemTData {
                use_op,
                reason,
                key_t: key,
                set_mode: mode,
                tin,
                tout,
            }),
        ) => {
            let action = ElemAction::WriteElem(Box::new(WriteElemData {
                tin: tin.dupe(),
                tout: tout.as_ref().map(|t| t.dupe()),
                mode: mode.clone(),
            }));
            rec_flow(
                cx,
                env,
                trace,
                (
                    key,
                    &UseT::new(ElemT(Box::new(ElemTData {
                        use_op: use_op.dupe(),
                        reason: reason.dupe(),
                        obj: l.dupe(),
                        action: Box::new(action),
                    }))),
                ),
            )?;
        }
        (
            AnyT(_, _),
            SetElemT(box SetElemTData {
                use_op,
                reason,
                key_t: key,
                set_mode: mode,
                tin,
                tout,
            }),
        ) => {
            let action = ElemAction::WriteElem(Box::new(WriteElemData {
                tin: tin.dupe(),
                tout: tout.as_ref().map(|t| t.dupe()),
                mode: mode.clone(),
            }));
            rec_flow(
                cx,
                env,
                trace,
                (
                    key,
                    &UseT::new(ElemT(Box::new(ElemTData {
                        use_op: use_op.dupe(),
                        reason: reason.dupe(),
                        obj: l.dupe(),
                        action: Box::new(action),
                    }))),
                ),
            )?;
        }
        (
            DefT(_, deref!(ObjT(_) | ArrT(_) | InstanceT(_))),
            GetElemT(box GetElemTData {
                use_op,
                reason,
                id,
                from_annot,
                skip_optional,
                access_iterables,
                key_t,
                tout,
            }),
        ) => {
            let action = ElemAction::ReadElem(Box::new(ReadElemData {
                id: *id,
                from_annot: *from_annot,
                skip_optional: *skip_optional,
                access_iterables: *access_iterables,
                tout: (**tout).dupe(),
            }));
            rec_flow(
                cx,
                env,
                trace,
                (
                    key_t,
                    &UseT::new(ElemT(Box::new(ElemTData {
                        use_op: use_op.dupe(),
                        reason: reason.dupe(),
                        obj: l.dupe(),
                        action: Box::new(action),
                    }))),
                ),
            )?;
        }
        (
            AnyT(_, _),
            GetElemT(box GetElemTData {
                use_op,
                reason,
                id,
                from_annot,
                skip_optional,
                access_iterables,
                key_t,
                tout,
            }),
        ) => {
            let action = ElemAction::ReadElem(Box::new(ReadElemData {
                id: *id,
                from_annot: *from_annot,
                skip_optional: *skip_optional,
                access_iterables: *access_iterables,
                tout: (**tout).dupe(),
            }));
            rec_flow(
                cx,
                env,
                trace,
                (
                    key_t,
                    &UseT::new(ElemT(Box::new(ElemTData {
                        use_op: use_op.dupe(),
                        reason: reason.dupe(),
                        obj: l.dupe(),
                        action: Box::new(action),
                    }))),
                ),
            )?;
        }
        (
            DefT(_, deref!(ObjT(_) | ArrT(_) | InstanceT(_))),
            CallElemT(box CallElemTData {
                use_op,
                reason: reason_call,
                prop_reason: reason_lookup,
                key_t: key,
                method_action: action,
            }),
        ) => {
            let action = ElemAction::CallElem(reason_call.dupe(), action.clone());
            rec_flow(
                cx,
                env,
                trace,
                (
                    key,
                    &UseT::new(ElemT(Box::new(ElemTData {
                        use_op: use_op.dupe(),
                        reason: reason_lookup.dupe(),
                        obj: l.dupe(),
                        action: Box::new(action),
                    }))),
                ),
            )?;
        }
        (
            AnyT(_, _),
            CallElemT(box CallElemTData {
                use_op,
                reason: reason_call,
                prop_reason: reason_lookup,
                key_t: key,
                method_action: action,
            }),
        ) => {
            let action = ElemAction::CallElem(reason_call.dupe(), action.clone());
            rec_flow(
                cx,
                env,
                trace,
                (
                    key,
                    &UseT::new(ElemT(Box::new(ElemTData {
                        use_op: use_op.dupe(),
                        reason: reason_lookup.dupe(),
                        obj: l.dupe(),
                        action: Box::new(action),
                    }))),
                ),
            )?;
        }
        // If we are accessing `Iterable<T>` with a number, and have `access_iterables = true`,
        // then output `T`.
        (
            DefT(_, deref!(NumGeneralT(_) | SingletonNumT { .. })),
            ElemT(box ElemTData {
                use_op,
                obj: deref!(DefT(_, deref!(InstanceT(inst)))),
                action:
                    box ElemAction::ReadElem(box ReadElemData {
                        access_iterables: true,
                        tout,
                        ..
                    }),
                ..
            }),
        ) if !inst.inst.type_args.is_empty()
            && flow_js_utils::is_builtin_iterable_class_id(inst.inst.class_id.clone(), cx) =>
        {
            let (_, _, t, _) = &inst.inst.type_args[0];
            rec_flow_t(
                cx,
                env,
                trace,
                use_op.dupe(),
                (t, &Type::new(OpenT(tout.dupe()))),
            )?;
        }
        (
            _,
            ElemT(box ElemTData {
                use_op,
                reason,
                obj: obj @ deref!(DefT(_, deref!(ObjT(_) | InstanceT(_)))),
                action,
            }),
        ) => {
            let propref = flow_js_utils::propref_for_elem_t_with_env(cx, env, l);
            elem_action_on_obj(cx, env, trace, use_op, propref, obj, reason, action)?;
        }
        (
            _,
            ElemT(box ElemTData {
                use_op,
                reason,
                obj: obj @ deref!(AnyT(_, src)),
                action,
            }),
        ) => {
            let value = any_t::why(*src, reason.dupe());
            perform_elem_action(
                cx,
                env,
                trace,
                use_op.dupe(),
                false,
                reason,
                obj,
                &value,
                action,
            )?;
        }
        // It is not safe to write to an unknown index in a tuple. However, any is
        // a source of unsoundness, so that's ok. `tup[(0: any)] = 123` should not
        // error when `tup[0] = 123` does not.
        (
            AnyT(_, _),
            ElemT(box ElemTData {
                use_op,
                reason: reason_op,
                obj: obj @ deref!(DefT(_, deref!(ArrT(arrtype)))),
                action,
            }),
        ) => {
            let react_dro = match (action, arrtype.as_ref()) {
                (
                    box ElemAction::WriteElem(..),
                    ROArrayAT(box ROArrayATData { .. })
                    | TupleAT(box TupleATData {
                        react_dro: Some(_), ..
                    })
                    | ArrayAT(box ArrayATData {
                        react_dro: Some(_), ..
                    }),
                ) => {
                    let use_op_for_err = match arrtype.as_ref() {
                        TupleAT(box TupleATData {
                            react_dro: Some(dro),
                            ..
                        })
                        | ArrayAT(box ArrayATData {
                            react_dro: Some(dro),
                            ..
                        }) => UseOp::Frame(
                            std::sync::Arc::new(VirtualFrameUseOp::ReactDeepReadOnly(Box::new((
                                dro.0.dupe(),
                                dro.1.clone(),
                            )))),
                            Arc::new(use_op.dupe()),
                        ),
                        _ => use_op.dupe(),
                    };
                    add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EROArrayWrite(reason_op.loc().dupe(), use_op_for_err),
                    )?;
                    None
                }
                (
                    box ElemAction::ReadElem(..),
                    ROArrayAT(box ROArrayATData { react_dro, .. })
                    | TupleAT(box TupleATData { react_dro, .. })
                    | ArrayAT(box ArrayATData { react_dro, .. }),
                ) => react_dro.clone(),
                _ => None,
            };
            let value = elemt_of_arrtype(arrtype);
            let value = match react_dro {
                Some(dro) => mk_react_dro(cx, use_op.dupe(), dro, value),
                None => value,
            };
            perform_elem_action(
                cx,
                env,
                trace,
                use_op.dupe(),
                false,
                reason_op,
                obj,
                &value,
                action,
            )?;
        }
        (
            DefT(_, def_t_l),
            ElemT(box ElemTData {
                use_op,
                reason,
                obj: obj @ deref!(DefT(reason_tup, deref!(ArrT(arrtype)))),
                action,
            }),
        ) if flow_js_utils::is_array_index_key(def_t_l.deref()) => {
            // An int-like string key indexes the array just like the equivalent
            // numeric key: `arr['0']` reads the element type, and `tup['2']`
            // resolves the third element or reports out-of-bounds. So convert it
            // to a numeric singleton before the index check. The guard accepts a
            // string only when this conversion succeeds, so `None` here means the
            // key is already a number.
            let int_key = flow_js_utils::intlike_str_key_as_num(l);
            let l = int_key.as_ref().unwrap_or(l);

            let (write_action, read_action, never_union_void_on_computed_prop_access) = match action
            {
                box ElemAction::ReadElem(box ReadElemData { from_annot, .. }) => {
                    (false, true, *from_annot)
                }
                box ElemAction::CallElem(_, _) => (false, false, false),
                box ElemAction::WriteElem(..) => (true, false, true),
            };
            let (value, is_tuple, use_op_out, react_dro) = flow_js_utils::array_elem_check(
                cx,
                env,
                write_action,
                never_union_void_on_computed_prop_access,
                obj,
                l,
                use_op.dupe(),
                reason,
                reason_tup,
                arrtype,
            )?;
            let value = match react_dro {
                Some(dro) if read_action => mk_react_dro(cx, use_op_out.dupe(), dro, value),
                _ => value,
            };
            perform_elem_action(
                cx, env, trace, use_op_out, is_tuple, reason, obj, &value, action,
            )?;
        }
        // A string-literal key that is not an array index reads a named property
        // off the array's object shape, e.g. `arr['map']` or
        // `arr[Symbol.iterator]`, mirroring object and instance access. A general
        // `string`/`number` key resolves to a `Computed` propref, fails this
        // guard, and falls through to the "not an array index" error.
        (
            _,
            ElemT(box ElemTData {
                use_op,
                reason,
                obj: obj @ deref!(DefT(_, deref!(ArrT(_)))),
                action,
            }),
        ) if let propref @ PropRef::Named { .. } =
            flow_js_utils::propref_for_elem_t_with_env(cx, env, l) =>
        {
            elem_action_on_obj(cx, env, trace, use_op, propref, obj, reason, action)?;
        }
        (
            DefT(_, deref!(ArrT(_))),
            GetPropT(
                box data @ GetPropTData {
                    propref: box PropRef::Named { name, .. },
                    ..
                },
            ),
        ) if name.matches_str("constructor") => {
            rec_flow_t(
                cx,
                env,
                trace,
                unknown_use(),
                (
                    &unsoundness::why(UnsoundnessKind::Constructor, data.reason.dupe()),
                    &Type::new(OpenT((*data.tout).dupe())),
                ),
            )?;
        }
        (DefT(_, deref!(ArrT(_))), SetPropT(_, _, box PropRef::Named { name, .. }, _, _, _, _))
            if name.matches_str("constructor") => {}
        (
            DefT(_, deref!(ArrT(_))),
            MethodT(box MethodTData {
                use_op: _,
                reason: reason_call,
                prop_reason: _,
                propref: box PropRef::Named { name, .. },
                method_action: action,
            }),
        ) if name.matches_str("constructor") => {
            add_specialized_callee_method_action(env, &any_t::untyped(reason_call.dupe()), action)?;
        }
        // **************************************************
        // * array pattern can consume the rest of an array *
        // **************************************************
        (
            DefT(_, deref!(ArrT(arrtype))),
            ArrRestT(box ArrRestTData {
                literal,
                use_op: _,
                reason,
                index: i,
                tout,
                ..
            }),
        ) => {
            let i = *i as usize;
            let arrtype = match arrtype.deref() {
                ArrayAT(box ArrayATData {
                    tuple_view: None, ..
                })
                | ROArrayAT(box ROArrayATData { .. }) => arrtype.deref().clone(),
                ArrayAT(box ArrayATData {
                    elem_t,
                    tuple_view: Some(tuple_view),
                    react_dro,
                    strictness_kind,
                    ..
                }) => {
                    let elements = tuple_view.elements[i.min(tuple_view.elements.len())..].to_vec();
                    let (num_req, num_total) = tuple_view.arity;
                    let arity = (
                        (num_req as i64 - i as i64).max(0) as i32,
                        (num_total as i64 - i as i64).max(0) as i32,
                    );
                    // An array with only a rest element has no tuple view.
                    let tuple_view = match &tuple_view.rest {
                        Some(_) if elements.is_empty() => None,
                        _ => Some(TupleView {
                            kind: tuple_view.kind,
                            elements: elements.into(),
                            arity,
                            inexact: tuple_view.inexact,
                            rest: tuple_view.rest.dupe(),
                        }),
                    };
                    ArrayAT(Box::new(ArrayATData {
                        elem_t: elem_t.dupe(),
                        tuple_view,
                        react_dro: react_dro.clone(),
                        strictness_kind: *strictness_kind,
                        literal: *literal,
                        fresh: false,
                    }))
                }
                TupleAT(box TupleATData {
                    elem_t,
                    elements,
                    arity: (num_req, num_total),
                    inexact,
                    rest,
                    react_dro,
                    strictness_kind,
                    ..
                }) => {
                    let elements = &elements[i.min(elements.len())..];
                    // The element type of an exact tuple is the union of its elements.
                    let elem_t = if *inexact {
                        elem_t.dupe()
                    } else {
                        type_util::union_of_ts(
                            reason.dupe().replace_desc(None),
                            elements
                                .iter()
                                .chain(rest)
                                .map(|elem| elem.t.dupe())
                                .collect(),
                            None,
                        )
                    };
                    match rest {
                        // A tuple with only a rest element is an array.
                        Some(_) if elements.is_empty() => ArrayAT(Box::new(ArrayATData {
                            elem_t,
                            tuple_view: None,
                            react_dro: react_dro.clone(),
                            strictness_kind: *strictness_kind,
                            literal: *literal,
                            fresh: false,
                        })),
                        _ => TupleAT(Box::new(TupleATData {
                            elem_t,
                            elements: elements.to_vec().into(),
                            arity: (
                                (*num_req as i64 - i as i64).max(0) as i32,
                                (*num_total as i64 - i as i64).max(0) as i32,
                            ),
                            inexact: *inexact,
                            rest: rest.dupe(),
                            react_dro: react_dro.clone(),
                            strictness_kind: *strictness_kind,
                            literal: *literal,
                            fresh: false,
                        })),
                    }
                }
            };
            let a = Type::new(DefT(
                reason.dupe(),
                DefT::new(ArrT(Rc::new(arrtype.with_literal(*literal, false)))),
            ));
            rec_flow_t(cx, env, trace, unknown_use(), (&a, tout))?;
        }
        (
            AnyT(_, src),
            ArrRestT(box ArrRestTData {
                use_op: _,
                reason,
                index: _,
                tout,
                ..
            }),
        ) => {
            rec_flow_t(
                cx,
                env,
                trace,
                unknown_use(),
                (&any_t::why(*src, reason.dupe()), tout),
            )?;
        }
        // **************************************************
        // * function types can be mapped over a structure  *
        // **************************************************
        (
            AnyT(_, src),
            MapTypeT(box MapTypeTData {
                use_op: _,
                reason: reason_op,
                type_map: _,
                tout,
            }),
        ) => {
            let src = flow_js_utils::any_mod_src_keep_placeholder(AnySource::Untyped, src);
            rec_flow_t(
                cx,
                env,
                trace,
                unknown_use(),
                (&any_t::why(src, reason_op.dupe()), tout),
            )?;
        }
        (
            DefT(_, deref!(ObjT(o))),
            MapTypeT(box MapTypeTData {
                use_op: _,
                reason: reason_op,
                type_map: TypeMap::ObjectKeyMirror,
                tout,
            }),
        ) => {
            let mirrored = flow_js_utils::obj_key_mirror(cx, env, o, reason_op);
            rec_flow_t(cx, env, trace, unknown_use(), (&mirrored, tout))?;
        }

        // **************
        // * object kit *
        // **************
        // Reject name remapping (`as`) on array/tuple sources for v1
        (
            l_inner,
            ObjKitT(
                _use_op,
                reason_op,
                box object::ResolveTool::Resolve(object::Resolve::Next),
                box object::Tool::ObjectMap(box ObjectToolObjectMapData {
                    name_type: Some(_), ..
                }),
                deref!(OpenT(tout_tvar)),
            ),
        ) if {
            match l_inner {
                DefT(_, deref!(ArrT(_))) => true,
                GenericT(box GenericTData { reason, bound, .. }) => {
                    let roarray_bound = Type::new(DefT(
                        reason.dupe(),
                        DefT::new(ArrT(Rc::new(ROArrayAT(Box::new(ROArrayATData {
                            elem_t: Type::new(DefT(
                                reason.dupe(),
                                DefT::new(MixedT(MixedFlavor::MixedEverything)),
                            )),
                            react_dro: None,
                            literal: false,
                        }))))),
                    ));
                    speculative_subtyping_succeeds(cx, env, bound, &roarray_bound)?
                }
                _ => false,
            }
        } =>
        {
            flow_js_utils::add_output_with_env(
                cx,env,
                ErrorMessage::EUnsupportedSyntax(Box::new((
                    reason_op.loc().dupe(),
                    flow_typing_errors::intermediate_error_types::UnsupportedSyntax::TSLibSyntax(
                        flow_typing_errors::intermediate_error_types::TsLibSyntaxKind::MappedTypeKeyRemappingOnArraySource,
                    ),
                ))),
            )?;
            rec_flow_t(
                cx,
                env,
                trace,
                unknown_use(),
                (
                    &any_t::error(reason_op.dupe()),
                    &Type::new(OpenT(tout_tvar.dupe())),
                ),
            )?;
        }
        (
            GenericT(box GenericTData {
                reason,
                no_infer,
                bound,
                id,
                name,
            }),
            ObjKitT(
                use_op,
                reason_op,
                box object::ResolveTool::Resolve(object::Resolve::Next),
                box object::Tool::ObjectMap(box ObjectToolObjectMapData {
                    prop_type,
                    name_type: None,
                    mapped_type_flags,
                    selected_keys_opt: None,
                }),
                tout,
            ),
        ) if {
            let roarray_bound = Type::new(DefT(
                reason.dupe(),
                DefT::new(ArrT(Rc::new(ROArrayAT(Box::new(ROArrayATData {
                    elem_t: Type::new(DefT(
                        reason.dupe(),
                        DefT::new(MixedT(MixedFlavor::MixedEverything)),
                    )),
                    react_dro: None,
                    literal: false,
                }))))),
            ));
            speculative_subtyping_succeeds(cx, env, bound, &roarray_bound)?
        } =>
        {
            let (t_generic_id, t) = {
                fn loop_generic(
                    t: &Type,
                    ls: flow_typing_generics::SpreadId,
                ) -> (flow_typing_generics::SpreadId, Type) {
                    match t.deref() {
                        GenericT(box GenericTData {
                            id, bound, reason, ..
                        }) => {
                            let bound_with_reason =
                                type_util::mod_reason_of_t(&|_| reason.dupe(), bound);
                            let new_ls = flow_typing_generics::spread_append(
                                &flow_typing_generics::GenericId::make_spread(id),
                                &ls,
                            );
                            loop_generic(&bound_with_reason, new_ls)
                        }
                        _ => (ls, t.dupe()),
                    }
                }
                loop_generic(l, flow_typing_generics::spread_empty())
            };
            let mapped_bound =
                flow_typing_tvar::mk_where(cx, reason_op.dupe(), |cx, tout_inner| {
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (
                            &t,
                            &UseT::new(ObjKitT(
                                use_op.dupe(),
                                reason_op.dupe(),
                                Box::new(object::ResolveTool::Resolve(object::Resolve::Next)),
                                Box::new(object::Tool::ObjectMap(Box::new(
                                    ObjectToolObjectMapData {
                                        prop_type: prop_type.dupe(),
                                        name_type: None,
                                        mapped_type_flags: mapped_type_flags.clone(),
                                        selected_keys_opt: None,
                                    },
                                ))),
                                tout_inner.dupe(),
                            )),
                        ),
                    )?;
                    Ok::<(), FlowJsException>(())
                })?;
            let mapped_generic_t = match flow_typing_generics::GenericId::make_op_id(
                flow_common::subst_name::OpKind::MappedArray,
                t_generic_id,
            ) {
                Some(new_id) => Type::new(GenericT(Box::new(GenericTData {
                    bound: mapped_bound,
                    reason: reason.dupe(),
                    id: new_id.clone(),
                    name: new_id.subst_name(),
                    no_infer: *no_infer,
                }))),
                None => t,
            };
            rec_flow_t(cx, env, trace, unknown_use(), (&mapped_generic_t, tout))?;
        }
        (
            DefT(_, deref!(ArrT(arrtype))),
            ObjKitT(
                use_op,
                reason_op,
                box object::ResolveTool::Resolve(object::Resolve::Next),
                box object::Tool::ObjectMap(box ObjectToolObjectMapData {
                    prop_type: property_type,
                    name_type: None,
                    mapped_type_flags,
                    selected_keys_opt: None,
                }),
                deref!(OpenT(tout_tvar)),
            ),
        ) => {
            let mapped_type_variance = &mapped_type_flags.variance;
            let mapped_type_optionality = &mapped_type_flags.optional;
            let filter_optional_t = |cx: &mut Context<'cx>,
                                     t: Type|
             -> Result<Type, FlowJsException> {
                let filter_id = crate::flow_js::filter_optional_with_env(cx, env, reason_op, &t)?;
                Ok(Type::new(OpenT(Tvar::new(reason_op.dupe(), filter_id))))
            };
            let f = |cx: &mut Context<'cx>,
                     value_t: &Type,
                     index: Option<usize>,
                     optional: bool|
             -> Result<Type, FlowJsException> {
                let r = reason_of_t(value_t).dupe();
                let key_t = match index {
                    None => num_module_t::why(r),
                    Some(i) => {
                        // The key for tuple position [i] is the numeric literal [i]. Borrow only the
                        // location from the element type's reason; its description must describe the key
                        // (the number literal), not the value type, otherwise errors mentioning the key
                        // render with the element type's description (e.g. the self-contradictory
                        // "string is incompatible with string").
                        let key_reason =
                            r.replace_desc(VirtualReasonDesc::RNumberLit(i.to_string().into()));
                        Type::new(DefT(
                            key_reason,
                            DefT::new(SingletonNumT {
                                from_annot: true,
                                value: NumberLiteral(i as f64, i.to_string().into()),
                            }),
                        ))
                    }
                };
                slice_utils::mk_mapped_prop_type(
                    cx,
                    &filter_optional_t,
                    use_op,
                    mapped_type_optionality,
                    property_type,
                    key_t,
                    optional,
                )
            };
            let new_element_optional = |optional: bool| -> bool {
                match mapped_type_optionality {
                    flow_typing_type::type_::MappedTypeOptionality::RemoveOptional => false,
                    flow_typing_type::type_::MappedTypeOptionality::MakeOptional
                    | flow_typing_type::type_::MappedTypeOptionality::KeepOptionality => optional,
                }
            };
            let new_arity = |arity: (i32, i32)| -> (i32, i32) {
                match mapped_type_optionality {
                    flow_typing_type::type_::MappedTypeOptionality::RemoveOptional => {
                        let (_, max) = arity;
                        (max, max)
                    }
                    flow_typing_type::type_::MappedTypeOptionality::MakeOptional
                    | flow_typing_type::type_::MappedTypeOptionality::KeepOptionality => arity,
                }
            };
            let mapped_polarity =
                |polarity: Polarity| slice_utils::mk_variance(*mapped_type_variance, polarity);
            let array_polarity = match arrtype.deref() {
                ArrayAT(_) => Some(mapped_polarity(Polarity::Neutral)),
                ROArrayAT(_) => Some(mapped_polarity(Polarity::Positive)),
                TupleAT(_) => None,
            };
            if array_polarity == Some(Polarity::Negative) {
                add_output_with_env(
                    cx, env,
                ErrorMessage::EInvalidMappedType {
                        loc: reason_op.loc().dupe(),
                        kind: flow_typing_errors::error_message::InvalidMappedTypeErrorKind::VarianceOnArrayInput,
                    },
                )?;
            }
            let array_elem = |cx: &mut Context<'cx>, t: Type| -> Result<Type, FlowJsException> {
                // For real arrays (no tuple shape) TS strips `undefined` from the element
                // type under `-?`, since indexed access on an array is implicitly optional.
                match mapped_type_optionality {
                    flow_typing_type::type_::MappedTypeOptionality::RemoveOptional => {
                        filter_optional_t(cx, t)
                    }
                    flow_typing_type::type_::MappedTypeOptionality::MakeOptional
                    | flow_typing_type::type_::MappedTypeOptionality::KeepOptionality => Ok(t),
                }
            };
            let new_arrtype = match arrtype.deref() {
                ArrayAT(box ArrayATData {
                    elem_t, react_dro, ..
                }) if array_polarity == Some(Polarity::Positive) => {
                    let mapped_elem_t = f(cx, elem_t, None, false)?;
                    ROArrayAT(Box::new(ROArrayATData {
                        elem_t: array_elem(cx, mapped_elem_t)?,
                        react_dro: react_dro.clone(),
                        literal: false,
                    }))
                }
                ArrayAT(box ArrayATData {
                    elem_t,
                    tuple_view,
                    react_dro,
                    strictness_kind,
                    ..
                }) => {
                    let mapped_elem_t = f(cx, elem_t, None, false)?;
                    let mapped_tuple_view = match tuple_view.as_ref() {
                        None => None,
                        Some(tv) => {
                            let elements = tv
                                .elements
                                .iter()
                                .enumerate()
                                .map(|(i, elem)| {
                                    Ok::<TupleElement, FlowJsException>(TupleElement {
                                        name: elem.name.dupe(),
                                        t: f(cx, &elem.t, Some(i), elem.optional)?,
                                        polarity: elem.polarity,
                                        optional: new_element_optional(elem.optional),
                                        reason: elem.reason.dupe(),
                                    })
                                })
                                .collect::<Result<Vec<_>, _>>()?
                                .into();
                            let rest = tv
                                .rest
                                .as_ref()
                                .map(|elem| {
                                    Ok::<TupleElement, FlowJsException>(TupleElement {
                                        t: {
                                            let mapped_t = f(cx, &elem.t, None, false)?;
                                            array_elem(cx, mapped_t)?
                                        },
                                        ..elem.clone()
                                    })
                                })
                                .transpose()?;
                            Some(TupleView {
                                kind: tv.kind,
                                elements,
                                arity: new_arity(tv.arity),
                                inexact: tv.inexact,
                                rest,
                            })
                        }
                    };
                    ArrayAT(Box::new(ArrayATData {
                        elem_t: array_elem(cx, mapped_elem_t)?,
                        react_dro: react_dro.clone(),
                        tuple_view: mapped_tuple_view,
                        strictness_kind: *strictness_kind,
                        literal: false,
                        fresh: false,
                    }))
                }
                TupleAT(box TupleATData {
                    elem_t,
                    elements,
                    arity,
                    inexact,
                    rest,
                    react_dro,
                    strictness_kind,
                    ..
                }) => {
                    let mapped_elem_t = f(cx, elem_t, None, false)?;
                    TupleAT(Box::new(TupleATData {
                        elem_t: array_elem(cx, mapped_elem_t)?,
                        react_dro: react_dro.clone(),
                        elements: elements
                            .iter()
                            .enumerate()
                            .map(|(i, elem)| {
                                Ok::<TupleElement, FlowJsException>(TupleElement {
                                    name: elem.name.dupe(),
                                    t: f(cx, &elem.t, Some(i), elem.optional)?,
                                    polarity: mapped_polarity(elem.polarity),
                                    optional: new_element_optional(elem.optional),
                                    reason: elem.reason.dupe(),
                                })
                            })
                            .collect::<Result<Vec<_>, _>>()?
                            .into(),
                        arity: new_arity(*arity),
                        inexact: *inexact,
                        rest: rest
                            .as_ref()
                            .map(|elem| {
                                let mapped_t = f(cx, &elem.t, None, false)?;
                                Ok::<TupleElement, FlowJsException>(TupleElement {
                                    t: array_elem(cx, mapped_t)?,
                                    polarity: mapped_polarity(elem.polarity),
                                    ..elem.clone()
                                })
                            })
                            .transpose()?,
                        strictness_kind: *strictness_kind,
                        literal: false,
                        fresh: false,
                    }))
                }
                ROArrayAT(box ROArrayATData {
                    elem_t: elemt,
                    react_dro: dro,
                    ..
                }) if array_polarity == Some(Polarity::Neutral) => {
                    let mapped_elem_t = f(cx, elemt, None, false)?;
                    ArrayAT(Box::new(ArrayATData {
                        elem_t: array_elem(cx, mapped_elem_t)?,
                        react_dro: dro.clone(),
                        tuple_view: None,
                        strictness_kind: TypeStrictnessKind::Flow,
                        literal: false,
                        fresh: false,
                    }))
                }
                ROArrayAT(box ROArrayATData {
                    elem_t: elemt,
                    react_dro: dro,
                    ..
                }) => {
                    let mapped_elem_t = f(cx, elemt, None, false)?;
                    ROArrayAT(Box::new(ROArrayATData {
                        elem_t: array_elem(cx, mapped_elem_t)?,
                        react_dro: dro.clone(),
                        literal: false,
                    }))
                }
            };
            let t = {
                let arr_reason = reason_op
                    .dupe()
                    .replace_desc(flow_common::reason::VirtualReasonDesc::RArrayType);
                Type::new(DefT(arr_reason, DefT::new(ArrT(Rc::new(new_arrtype)))))
            };
            rec_flow_t(
                cx,
                env,
                trace,
                unknown_use(),
                (&t, &Type::new(OpenT(tout_tvar.dupe()))),
            )?;
        }
        (_, ObjKitT(use_op, reason, resolve_tool, tool, tout)) => {
            object_kit::run_with_env(
                trace,
                cx,
                env,
                use_op.dupe(),
                reason,
                resolve_tool,
                tool,
                l,
                tout,
            )?;
        }
        // ***************************************************************
        // * Enable structural subtyping for upperbounds like interfaces *
        // ***************************************************************
        (ObjProtoT(_), ImplementsT(_, _)) | (FunProtoT(_), ImplementsT(_, _)) => {}
        (DefT(_, deref!(NullT)), ImplementsT(_, _)) => {}
        (DefT(reason_inst, deref!(InstanceT(inst_t))), ImplementsT(use_op, t))
            if matches!(&inst_t.inst.inst_kind, InstanceKind::InterfaceKind { .. }) =>
        {
            let inst = &inst_t.inst;
            let upper_inst_abstract = inst.inst_abstract;
            let super_ = &inst_t.super_;
            let implementor_strictness_kind = match t.deref() {
                DefT(_, deref!(InstanceT(instance))) => instance.inst.strictness_kind,
                DefT(_, deref!(ObjT(obj))) => obj.strictness_kind,
                _ => TypeStrictnessKind::Flow,
            };
            let strictness_kind = inst.strictness_kind.join(implementor_strictness_kind);
            structural_subtype(
                cx,
                env,
                trace,
                use_op.dupe(),
                upper_inst_abstract,
                strictness_kind,
                t,
                l,
                (
                    inst.own_props.dupe(),
                    inst.proto_props.dupe(),
                    inst.inst_call_t,
                    inst.inst_construct_t,
                    &inst.inst_dict,
                ),
            )?;
            // An interface can extend a class, as TypeScript's `interface
            // RegExpMatchArray extends Array<string>` does, and then has the
            // class's members. Sending [ImplementsT] on to such a super would
            // report the class as "not an interface", at the class's location, so
            // class supers are checked here instead.
            // Each super carries the member names already checked by more derived
            // declarations on its own chain.
            let checked_names: BTreeSet<Name> =
                std::iter::once(Name::new(FlowSmolStr::new_inline("constructor")))
                    .chain(cx.find_props(inst.own_props.dupe()).keys().duped())
                    .chain(cx.find_props(inst.proto_props.dupe()).keys().duped())
                    .collect();
            let mut supers = vec![(super_.dupe(), checked_names)];
            let mut seen_classes = BTreeSet::new();
            while let Some((super_, checked_names)) = supers.pop() {
                for super_t in helpers::possible_concrete_types_for_inspection(
                    cx,
                    env,
                    reason_of_t(&super_),
                    &super_,
                )? {
                    match super_t.deref() {
                        IntersectionT(_, rep) => {
                            supers.extend(
                                rep.members_iter()
                                    .map(|member| (member.dupe(), checked_names.clone())),
                            );
                        }
                        DefT(_, deref!(InstanceT(super_inst)))
                            if !matches!(
                                super_inst.inst.inst_kind,
                                InstanceKind::InterfaceKind { .. }
                            ) =>
                        {
                            if !seen_classes.insert((super_t.dupe(), checked_names.clone())) {
                                continue;
                            }
                            match t.deref() {
                                // Private (`#`) members are not checked.
                                DefT(_, deref!(InstanceT(_) | ObjT(_))) => {
                                    let unchecked = |props: properties::Id| {
                                        properties::PropertiesMap::from_btree_map(
                                            cx.find_props(props)
                                                .iter()
                                                .filter(|(name, _)| !checked_names.contains(*name))
                                                .map(|(name, prop)| (name.dupe(), prop.dupe()))
                                                .collect(),
                                        )
                                    };
                                    let own_props = unchecked(super_inst.inst.own_props.dupe());
                                    let proto_props = unchecked(super_inst.inst.proto_props.dupe());
                                    let mut super_checked_names = checked_names.clone();
                                    super_checked_names.extend(own_props.keys().duped());
                                    super_checked_names.extend(proto_props.keys().duped());
                                    let own_props = cx.generate_property_map(own_props);
                                    let proto_props = cx.generate_property_map(proto_props);
                                    structural_subtype(
                                        cx,
                                        env,
                                        trace,
                                        use_op.dupe(),
                                        super_inst.inst.inst_abstract,
                                        super_inst
                                            .inst
                                            .strictness_kind
                                            .join(implementor_strictness_kind),
                                        t,
                                        &super_t,
                                        (
                                            own_props,
                                            proto_props,
                                            super_inst.inst.inst_call_t,
                                            super_inst.inst.inst_construct_t,
                                            &super_inst.inst.inst_dict,
                                        ),
                                    )?;
                                    supers.push((super_inst.super_.dupe(), super_checked_names));
                                }
                                // An array's members come from a builtin instance, so
                                // compare that instance with the class. A tuple's
                                // members are those of `ReadonlyArray`.
                                DefT(r, deref!(ArrT(arr))) => {
                                    let (name, elem_t) = match arr.as_ref() {
                                        ArrayAT(box ArrayATData { elem_t, .. }) => {
                                            ("Array", elem_t)
                                        }
                                        TupleAT(box TupleATData { elem_t, .. })
                                        | ROArrayAT(box ROArrayATData { elem_t, .. }) => {
                                            ("ReadonlyArray", elem_t)
                                        }
                                    };
                                    let instance = get_builtin_typeapp(
                                        cx,
                                        env,
                                        r,
                                        None,
                                        name,
                                        vec![elem_t.dupe()],
                                    );
                                    rec_flow_t(
                                        cx,
                                        env,
                                        trace,
                                        use_op.dupe(),
                                        (&instance, &super_t),
                                    )?;
                                }
                                _ => rec_flow_t(cx, env, trace, use_op.dupe(), (t, &super_t))?,
                            }
                        }
                        _ => rec_flow(
                            cx,
                            env,
                            trace,
                            (
                                &super_t,
                                &UseT::new(ReposLowerT {
                                    reason: reason_inst.dupe(),
                                    use_desc: false,
                                    use_t: Box::new(UseT::new(ImplementsT(
                                        use_op.dupe(),
                                        t.dupe(),
                                    ))),
                                }),
                            ),
                        )?,
                    }
                }
            }
        }
        (DefT(_, deref!(ObjT(obj))), ImplementsT(use_op, implementor))
            if obj.strictness_kind.is_typescript_loose()
                && obj_type::get_dict_opt(&obj.flags.obj_kind).is_none() =>
        {
            // TS-mode: a class may `implements` a resolved object type
            // (e.g. `class C implements Omit<HTMLAttrs, "k">`). Run the same
            // structural check as the InterfaceKind arm above. Indexed ObjTs
            // (e.g. `Record<string, number>`) are excluded: structural_subtype
            // would silently accept indexer-free classes, since
            // inst_structural_subtype only enforces the upper indexer when the
            // lower instance hierarchy carries inst_dict.
            let ObjType {
                props_tmap,
                flags: _,
                call_t,
                proto_t: _,
                reachable_targs: _,
                strictness_kind: _,
                literal: _,
            } = obj.as_ref();
            let proto_props = cx.generate_property_map(properties::PropertiesMap::new());
            structural_subtype(
                cx,
                env,
                trace,
                use_op.dupe(),
                false,
                obj.strictness_kind,
                implementor,
                l,
                (props_tmap.dupe(), proto_props, *call_t, None, &None),
            )?;
        }
        (_, ImplementsT(_, _)) => {
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::EUnsupportedImplements(flow_js_utils::type_reference_for_error(l)),
            )?;
        }

        // *********************************************************************
        // * class A is a base class of class B iff                            *
        // * properties in B that override properties in A or its base classes *
        // * have the same signatures                                          *
        // *********************************************************************

        // The purpose of SuperT is to establish consistency between overriding
        // properties with overridden properties. As such, the lookups performed
        // for the inherited properties are non-strict: they are not required to
        // exist.
        (
            DefT(ureason, deref!(InstanceT(inst_t))),
            SuperT(box SuperTData {
                use_op,
                reason,
                derived_type: derived,
            }),
        ) if matches!(
            &inst_t.inst.inst_kind,
            InstanceKind::ClassKind | InstanceKind::InterfaceKind { .. }
        ) =>
        {
            let st = &inst_t.static_;
            let DerivedType {
                own,
                proto,
                static_: static_props,
                strictness_kind,
                reference,
                instance,
            } = derived;
            for (x, p) in own.iter() {
                check_super(
                    cx,
                    env,
                    trace,
                    use_op.dupe(),
                    reason,
                    ureason,
                    reference,
                    instance,
                    l,
                    x,
                    p,
                    strictness_kind.join(inst_t.inst.strictness_kind),
                )?;
            }
            for (x, p) in proto.iter() {
                if inherited_method(x) {
                    check_super(
                        cx,
                        env,
                        trace,
                        use_op.dupe(),
                        reason,
                        ureason,
                        reference,
                        instance,
                        l,
                        x,
                        p,
                        strictness_kind.join(inst_t.inst.strictness_kind),
                    )?;
                }
            }
            // TODO: inherited_method logic no longer applies for statics. It used to
            // when call properties were included in the props, but that is no longer
            // the case. All that remains is the "constructor" prop, which has no
            // special meaning on the static object.
            for (x, p) in static_props.iter() {
                if inherited_method(x) {
                    check_super(
                        cx,
                        env,
                        trace,
                        use_op.dupe(),
                        reason,
                        ureason,
                        reference,
                        instance,
                        st,
                        x,
                        p,
                        strictness_kind.join(inst_t.inst.strictness_kind),
                    )?;
                }
            }
        }
        (
            DefT(ureason, deref!(ObjT(obj))),
            SuperT(box SuperTData {
                use_op,
                reason,
                derived_type: derived,
            }),
        ) if obj
            .strictness_kind
            .join(derived.strictness_kind)
            .is_typescript_loose() =>
        {
            // TS-mode: an interface may extend a resolved object type
            // (e.g. `interface X extends Omit<Y, K>`). Run the same
            // override-consistency check as the InstanceT arm above; ObjT has
            // no static slot so the static map is dropped.
            let DerivedType {
                own,
                proto,
                static_: _,
                strictness_kind,
                reference,
                instance,
            } = derived;
            for (x, p) in own.iter() {
                check_super(
                    cx,
                    env,
                    trace,
                    use_op.dupe(),
                    reason,
                    ureason,
                    reference,
                    instance,
                    l,
                    x,
                    p,
                    strictness_kind.join(obj.strictness_kind),
                )?;
            }
            for (x, p) in proto.iter() {
                if inherited_method(x) {
                    check_super(
                        cx,
                        env,
                        trace,
                        use_op.dupe(),
                        reason,
                        ureason,
                        reference,
                        instance,
                        l,
                        x,
                        p,
                        strictness_kind.join(obj.strictness_kind),
                    )?;
                }
            }
        }
        // Keep opaque types in computed object keys
        (
            _,
            ConcretizeT(box ConcretizeTData {
                reason: _,
                kind: ConcretizationKind::ConcretizeForComputedObjectKeys,
                seen: _,
                collector,
            }),
        ) => {
            collector.add(l.dupe());
        }

        // ***********************
        // * opaque types part 2 *
        // ***********************

        // Predicate_kit should not see unwrapped opaque type
        (
            NominalT { .. },
            ConcretizeT(box ConcretizeTData {
                reason: _,
                kind: ConcretizationKind::ConcretizeForPredicate(_),
                seen: _,
                collector,
            }),
        ) => {
            collector.add(l.dupe());
        }
        (
            NominalT { .. },
            SealGenericT(box SealGenericTData {
                loc: _,
                id,
                name,
                cont,
                no_infer,
            }),
        ) => {
            let reason = reason_of_t(l);
            let generic = Type::new(GenericT(Box::new(GenericTData {
                reason: reason.dupe(),
                id: id.clone(),
                name: name.dupe(),
                bound: l.dupe(),
                no_infer: *no_infer,
            })));
            continue_(cx, env, trace, &generic, cont)?;
        }
        // Preserve NominalT as consequent, but branch based on the bound
        (
            NominalT {
                nominal_type: opaque,
                ..
            },
            CondT(box CondTData {
                loc,
                opt_type: then_t_opt,
                true_t: else_t,
                collector,
            }),
        ) if opaque.upper_t.is_some() => {
            let t = opaque.upper_t.as_ref().unwrap();
            let new_then_t_opt = match then_t_opt {
                Some(_) => then_t_opt.dupe(),
                None => Some(l.dupe()),
            };
            rec_flow(
                cx,
                env,
                trace,
                (
                    t,
                    &UseT::new(CondT(Box::new(CondTData {
                        loc: loc.dupe(),
                        opt_type: new_then_t_opt,
                        true_t: else_t.dupe(),
                        collector: collector.dupe(),
                    }))),
                ),
            )?;
        }
        // Opaque types may be treated as their supertype when they are a lower bound for a use
        (
            NominalT {
                reason: opaque_t_reason,
                nominal_type: opaque,
            },
            _,
        ) if opaque.upper_t.is_some() => {
            let t = opaque.upper_t.as_ref().unwrap();
            let u_mod = type_util::mod_use_op_of_use_t(
                |use_op: &UseOp| {
                    VirtualUseOp::Frame(
                        Arc::new(FrameUseOp::OpaqueTypeUpperBound {
                            opaque_t_reason: opaque_t_reason.dupe(),
                        }),
                        Arc::new(use_op.dupe()),
                    )
                },
                u,
            );
            rec_flow(cx, env, trace, (t, &u_mod))?;
        }
        // Concretize types for type operation purpose up to this point. The rest are
        // recorded as lower bound to the target tvar.
        (
            _,
            ConcretizeT(box ConcretizeTData {
                reason: _,
                kind: ConcretizationKind::ConcretizeForOperatorsChecking,
                seen: _,
                collector,
            }),
        ) => {
            collector.add(l.dupe());
        }
        // **************************************************************************
        // * final shared concretization point for predicate and sentinel prop test *
        // **************************************************************************
        (
            _,
            ConcretizeT(box ConcretizeTData {
                reason: _,
                kind:
                    ConcretizationKind::ConcretizeForPredicate(_)
                    | ConcretizationKind::ConcretizeForSentinelPropTest,
                seen: _,
                collector,
            }),
        ) => {
            collector.add(l.dupe());
        }
        // ******************************
        // * functions statics - part B *
        // ******************************
        (DefT(reason, deref!(FunT(static_, _))), _) if flow_js_utils::object_like_op(u) => {
            rec_flow(
                cx,
                env,
                trace,
                (
                    static_,
                    &UseT::new(ReposLowerT {
                        reason: reason.dupe(),
                        use_desc: false,
                        use_t: Box::new(u.dupe()),
                    }),
                ),
            )?;
        }
        // *****************************************
        // * classes can have their prototype read *
        // *****************************************
        (
            DefT(reason, deref!(ClassT(instance))),
            GetPropT(
                box data @ GetPropTData {
                    propref: box PropRef::Named { name, .. },
                    ..
                },
            ),
        ) if name == &Name::new("prototype") => {
            let instance = helpers::reposition(
                cx,
                env,
                Some(trace),
                reason.loc().dupe(),
                None,
                None,
                instance.dupe(),
            )?;
            let open_tout = Type::new(OpenT((*data.tout).dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (&instance, &open_tout))?;
        }

        // *****************
        // * class statics *
        // *****************

        // For Get/SetPrivatePropT or PrivateMethodT, the instance id is needed to determine whether
        // or not the private static field exists on that class. Since we look through the scopes for
        // the type of the field, there is no need to look at the static member of the instance.
        // Instead, we just flip the boolean flag to true, indicating that when the
        // InstanceT ~> Set/GetPrivatePropT or PrivateMethodT constraint is processed that we should
        // look at the private static fields instead of the private instance fields.
        (DefT(reason, deref!(ClassT(instance))), GetPrivatePropT(box gpp_data)) => {
            let new_u = UseT::new(GetPrivatePropT(Box::new(GetPrivatePropTData {
                use_op: gpp_data.use_op.dupe(),
                reason: gpp_data.reason.dupe(),
                name: gpp_data.name.dupe(),
                class_bindings: gpp_data.class_bindings.dupe(),
                static_: true,
                tout: gpp_data.tout.clone(),
            })));
            rec_flow(
                cx,
                env,
                trace,
                (
                    instance,
                    &UseT::new(ReposLowerT {
                        reason: reason.dupe(),
                        use_desc: false,
                        use_t: Box::new(new_u),
                    }),
                ),
            )?;
        }
        (DefT(reason, deref!(ClassT(instance))), SetPrivatePropT(box spp_data)) => {
            let new_u = UseT::new(SetPrivatePropT(Box::new(SetPrivatePropTData {
                use_op: spp_data.use_op.dupe(),
                reason: spp_data.reason.dupe(),
                name: spp_data.name.dupe(),
                set_mode: spp_data.set_mode.clone(),
                class_bindings: spp_data.class_bindings.dupe(),
                static_: true,
                write_ctx: spp_data.write_ctx.clone(),
                tin: spp_data.tin.dupe(),
                tout: spp_data.tout.clone(),
            })));
            rec_flow(
                cx,
                env,
                trace,
                (
                    instance,
                    &UseT::new(ReposLowerT {
                        reason: reason.dupe(),
                        use_desc: false,
                        use_t: Box::new(new_u),
                    }),
                ),
            )?;
        }
        (DefT(reason, deref!(ClassT(instance))), PrivateMethodT(box pm_data)) => {
            let new_u = UseT::new(PrivateMethodT(Box::new(PrivateMethodTData {
                use_op: pm_data.use_op.dupe(),
                reason: pm_data.reason.dupe(),
                prop_reason: pm_data.prop_reason.dupe(),
                name: pm_data.name.dupe(),
                class_bindings: pm_data.class_bindings.dupe(),
                static_: true,
                method_action: pm_data.method_action.clone(),
            })));
            rec_flow(
                cx,
                env,
                trace,
                (
                    instance,
                    &UseT::new(ReposLowerT {
                        reason: reason.dupe(),
                        use_desc: false,
                        use_t: Box::new(new_u),
                    }),
                ),
            )?;
        }
        (
            DefT(reason, deref!(ClassT(instance))),
            MethodT(box MethodTData {
                use_op,
                reason: reason_call,
                prop_reason: reason_lookup,
                propref,
                method_action: action,
            }),
        ) => {
            let collector = type_collector::TypeCollector::create();
            rec_flow(
                cx,
                env,
                trace,
                (
                    instance,
                    &UseT::new(GetStaticsT(reason.dupe(), collector.dupe())),
                ),
            )?;
            let method_type = flow_typing_tvar::mk_no_wrap_where(
                cx,
                reason_lookup.dupe(),
                |cx, tout_reason, tout_id| {
                    let tout = Tvar::new(tout_reason.dupe(), tout_id as u32);
                    let get_prop_u = UseT::new(GetPropT(Box::new(GetPropTData {
                        use_op: use_op.dupe(),
                        reason: reason_lookup.dupe(),
                        id: None,
                        from_annot: false,
                        skip_optional: false,
                        propref: propref.clone(),
                        tout: Box::new(tout),
                        hint: hint_unavailable(),
                    })));
                    let repos_use = UseT::new(ReposLowerT {
                        reason: reason.dupe(),
                        use_desc: false,
                        use_t: Box::new(get_prop_u),
                    });
                    for static_ in collector.collect_to_vec() {
                        rec_flow(cx, env, trace, (&static_, &repos_use))?;
                    }
                    Ok::<(), FlowJsException>(())
                },
            )?;
            apply_method_action(
                cx,
                env,
                trace,
                &method_type,
                use_op.dupe(),
                reason_call.dupe(),
                l.dupe(),
                action,
            )?;
        }
        (DefT(reason, deref!(ClassT(instance))), _) if flow_js_utils::object_like_op(u) => {
            let collector = type_collector::TypeCollector::create();
            rec_flow(
                cx,
                env,
                trace,
                (
                    instance,
                    &UseT::new(GetStaticsT(reason.dupe(), collector.dupe())),
                ),
            )?;
            for static_ in collector.collect_to_vec() {
                rec_flow(cx, env, trace, (&static_, u))?;
            }
        }
        // ************************
        // * classes as functions *
        // ************************

        /*
          When a class value flows to a function annotation or call site, check for
          the presence of a call property in the former (as a static) compatible
          with the latter.

          TODO: Call properties are excluded from the subclass compatibility
          checks, which makes it unsafe to call a Class<T> type like this.
          For example:

              declare class A { static (): string };
              declare class B extends A { static (): number }
              var klass: Class<A> = B;
              var foo: string = klass(); // passes, but `foo` is a number

          The same issue is also true for constructors, which are similarly
          excluded from subclass compatibility checks, but are allowed on ClassT
          types.
        */
        (DefT(reason, deref!(ClassT(instance))), CallT(..)) => {
            let collector = type_collector::TypeCollector::create();
            rec_flow(
                cx,
                env,
                trace,
                (
                    instance,
                    &UseT::new(GetStaticsT(reason.dupe(), collector.dupe())),
                ),
            )?;
            for static_ in collector.collect_to_vec() {
                rec_flow(cx, env, trace, (&static_, u))?;
            }
        }
        // *********
        // * enums *
        // *********
        (
            DefT(
                _,
                deref!(EnumObjectT {
                    enum_value_t,
                    enum_info
                }),
            ),
            GetPropT(
                box data @ GetPropTData {
                    propref:
                        box PropRef::Named {
                            reason: prop_reason,
                            name: member_name,
                            ..
                        },
                    ..
                },
            ),
        ) if let EnumInfoInner::ConcreteEnum(enum_info) = EnumInfo::deref(enum_info) => {
            flow_js_utils::get_prop_t_kit::on_enum_object_t::<FlowJs>(
                cx,
                env,
                &trace,
                l.dupe(),
                enum_value_t.dupe(),
                enum_info,
                &(
                    data.use_op.dupe(),
                    data.reason.dupe(),
                    None,
                    (prop_reason.dupe(), member_name.dupe()),
                ),
            )?(cx, (*data.tout).dupe())?;
        }
        (
            DefT(_, deref!(EnumObjectT { .. })),
            TestPropT(box TestPropTData {
                reason,
                propref,
                tout,
                hint,
                ..
            }),
        ) => {
            rec_flow(
                cx,
                env,
                trace,
                (
                    l,
                    &UseT::new(GetPropT(Box::new(GetPropTData {
                        use_op: VirtualUseOp::Op(Arc::new(VirtualRootUseOp::GetProperty(
                            reason.dupe(),
                        ))),
                        reason: reason.dupe(),
                        id: None,
                        from_annot: false,
                        skip_optional: false,
                        propref: propref.clone(),
                        tout: tout.clone(),
                        hint: hint.clone(),
                    }))),
                ),
            )?;
        }
        (
            DefT(
                _,
                deref!(EnumObjectT {
                    enum_value_t,
                    enum_info
                }),
            ),
            MethodT(box MethodTData {
                use_op,
                reason: call_reason,
                prop_reason: lookup_reason,
                propref: propref @ box PropRef::Named { .. },
                method_action: action,
            }),
        ) => {
            let t = flow_typing_tvar::mk_no_wrap_where(
                cx,
                lookup_reason.dupe(),
                |cx, tout_reason, tout_id| {
                    let tout = Tvar::new(tout_reason.dupe(), tout_id as u32);
                    let representation_t = match EnumInfo::deref(enum_info) {
                        EnumInfoInner::ConcreteEnum(ce) => &ce.representation_t,
                        EnumInfoInner::AbstractEnum { representation_t } => representation_t,
                    };
                    let proto = enum_proto(
                        cx,
                        env,
                        lookup_reason.dupe(),
                        l.dupe(),
                        enum_value_t.dupe(),
                        representation_t.dupe(),
                    );
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (
                            &proto,
                            &UseT::new(GetPropT(Box::new(GetPropTData {
                                use_op: use_op.dupe(),
                                reason: lookup_reason.dupe(),
                                id: None,
                                from_annot: false,
                                skip_optional: false,
                                propref: propref.clone(),
                                tout: Box::new(tout),
                                hint: hint_unavailable(),
                            }))),
                        ),
                    )?;
                    Ok::<(), FlowJsException>(())
                },
            )?;
            apply_method_action(
                cx,
                env,
                trace,
                &t,
                use_op.dupe(),
                call_reason.dupe(),
                l.dupe(),
                action,
            )?;
        }
        (
            DefT(_, deref!(EnumObjectT { enum_info, .. })),
            GetElemT(box GetElemTData { key_t, tout, .. }),
        ) => {
            let reason = reason_of_t(key_t);
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidMemberAccess(Box::new(
                    EnumInvalidMemberAccessData {
                        member: EnumMemberAccess::Computed(
                            flow_js_utils::type_reference_for_error(key_t),
                        ),
                        member_loc: reason.loc().dupe(),
                        enum_: EnumReferenceData {
                            loc: type_util::ref_loc_of_t(l).dupe(),
                            name: enum_info.enum_name().map(Dupe::dupe),
                        },
                    },
                ))),
            )?;
            let any = any_t::error(reason.dupe());
            let open_tout = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (&any, &open_tout))?;
        }
        (
            DefT(_, deref!(EnumObjectT { enum_info, .. })),
            SetPropT(_, op_reason, _, _, _, _, tout),
        ) => {
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::EEnumError(EnumErrorKind::EnumModification(Box::new(
                    EnumModificationData {
                        loc: op_reason.loc().dupe(),
                        enum_: EnumReferenceData {
                            loc: type_util::ref_loc_of_t(l).dupe(),
                            name: enum_info.enum_name().map(Dupe::dupe),
                        },
                    },
                ))),
            )?;
            if let Some(tout) = tout {
                let any = any_t::error(op_reason.dupe());
                rec_flow_t(cx, env, trace, unknown_use(), (&any, tout))?;
            }
        }
        (
            DefT(_, deref!(EnumObjectT { enum_info, .. })),
            SetElemT(box SetElemTData {
                use_op: _,
                reason: op_reason,
                key_t: _,
                set_mode: _,
                tin: _,
                tout,
            }),
        ) => {
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::EEnumError(EnumErrorKind::EnumModification(Box::new(
                    EnumModificationData {
                        loc: op_reason.loc().dupe(),
                        enum_: EnumReferenceData {
                            loc: type_util::ref_loc_of_t(l).dupe(),
                            name: enum_info.enum_name().map(Dupe::dupe),
                        },
                    },
                ))),
            )?;
            if let Some(tout) = tout {
                let any = any_t::error(op_reason.dupe());
                rec_flow_t(cx, env, trace, unknown_use(), (&any, tout))?;
            }
        }
        (DefT(_, deref!(EnumObjectT { enum_info, .. })), GetValuesT(op_reason, tout)) => {
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidObjectUtilType(Box::new(
                    EnumInvalidObjectUtilTypeData {
                        operation_loc: op_reason.loc().dupe(),
                        operation_name: "Values".into(),
                        enum_: EnumReferenceData {
                            loc: type_util::ref_loc_of_t(l).dupe(),
                            name: enum_info.enum_name().map(Dupe::dupe),
                        },
                    },
                ))),
            )?;
            let any = any_t::error(op_reason.dupe());
            rec_flow_t(cx, env, trace, unknown_use(), (&any, tout))?;
        }
        (DefT(_, deref!(EnumObjectT { enum_info, .. })), GetDictValuesT(reason, result)) => {
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidObjectFunction(Box::new(
                    EnumInvalidObjectFunctionData {
                        operation_loc: reason.loc().dupe(),
                        function_name: "Object.values".into(),
                        enum_: EnumReferenceData {
                            loc: type_util::ref_loc_of_t(l).dupe(),
                            name: enum_info.enum_name().map(Dupe::dupe),
                        },
                    },
                ))),
            )?;
            let any = any_t::error(reason.dupe());
            rec_flow(cx, env, trace, (&any, result))?;
        }
        (
            DefT(_, deref!(EnumValueT(enum_info))),
            MethodT(box MethodTData {
                use_op,
                reason: call_reason,
                prop_reason: lookup_reason,
                propref: propref @ box PropRef::Named { .. },
                method_action: action,
            }),
        ) => {
            let representation_t = match EnumInfo::deref(enum_info) {
                EnumInfoInner::ConcreteEnum(ce) => &ce.representation_t,
                EnumInfoInner::AbstractEnum { representation_t } => representation_t,
            };
            let enum_value_proto = get_builtin_typeapp(
                cx,
                env,
                lookup_reason,
                None,
                "$EnumValueProto",
                vec![l.dupe(), representation_t.dupe()],
            );
            let t = flow_typing_tvar::mk_no_wrap_where(
                cx,
                lookup_reason.dupe(),
                |cx, tout_reason, tout_id| {
                    let tout = Tvar::new(tout_reason.dupe(), tout_id as u32);
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (
                            &enum_value_proto,
                            &UseT::new(GetPropT(Box::new(GetPropTData {
                                use_op: use_op.dupe(),
                                reason: lookup_reason.dupe(),
                                id: None,
                                from_annot: false,
                                skip_optional: false,
                                propref: propref.clone(),
                                tout: Box::new(tout),
                                hint: hint_unavailable(),
                            }))),
                        ),
                    )?;
                    Ok::<(), FlowJsException>(())
                },
            )?;
            apply_method_action(
                cx,
                env,
                trace,
                &t,
                use_op.dupe(),
                call_reason.dupe(),
                l.dupe(),
                action,
            )?;
        }
        // **************************************************************************
        // TestPropT is emitted for property reads in the context of branch tests.
        // Such tests are always non-strict, in that we don't immediately report an
        // error if the property is not found not in the object type. Instead, if
        // the property is not found, we control the result type of the read based
        // on the flags on the object type. For exact object types, the
        // result type is `void`; otherwise, it is "unknown". Indeed, if the
        // property is not found in an exact object type, we can be sure it
        // won't exist at run time, so the read will return undefined; but for other
        // object types, the property *might* exist at run time, and since we don't
        // know what the type of the property would be, we set things up so that the
        // result of the read cannot be used in any interesting way.
        // **************************************************************************
        (
            DefT(_, deref!(NullT | VoidT)),
            TestPropT(box TestPropTData {
                use_op,
                reason,
                id,
                propref,
                tout,
                hint,
            }),
        ) => {
            // The wildcard TestPropT implementation forwards the lower bound to
            // LookupT. This is unfortunate, because LookupT is designed to terminate
            // (successfully) on NullT, but property accesses on null should be type
            // errors. Ideally, we should prevent LookupT constraints from being
            // syntax-driven, in order to preserve the delicate invariants that
            // surround it.
            rec_flow(
                cx,
                env,
                trace,
                (
                    l,
                    &UseT::new(GetPropT(Box::new(GetPropTData {
                        use_op: use_op.dupe(),
                        reason: reason.dupe(),
                        id: Some(*id),
                        from_annot: false,
                        skip_optional: false,
                        propref: propref.clone(),
                        tout: tout.clone(),
                        hint: hint.clone(),
                    }))),
                ),
            )?;
        }
        (
            DefT(
                r,
                deref!(MixedT(
                    MixedFlavor::MixedTruthy | MixedFlavor::MixedNonMaybe
                )),
            ),
            TestPropT(box TestPropTData {
                use_op, id, tout, ..
            }),
        ) => {
            // Special-case property tests of definitely non-null/non-void values to
            // return mixed and treat them as a hit.
            cx.test_prop_hit(*id);
            let mixed = Type::new(DefT(
                r.dupe(),
                DefT::new(MixedT(MixedFlavor::MixedEverything)),
            ));
            let open_tout = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, use_op.dupe(), (&mixed, &open_tout))?;
        }
        (
            _,
            TestPropT(box TestPropTData {
                use_op,
                reason,
                id,
                propref: propref @ box PropRef::Named { name, .. },
                tout,
                hint,
            }),
        ) if name == &Name::new("constructor") => {
            rec_flow(
                cx,
                env,
                trace,
                (
                    l,
                    &UseT::new(GetPropT(Box::new(GetPropTData {
                        use_op: use_op.dupe(),
                        reason: reason.dupe(),
                        id: Some(*id),
                        from_annot: false,
                        skip_optional: false,
                        propref: propref.clone(),
                        tout: tout.clone(),
                        hint: hint.clone(),
                    }))),
                ),
            )?;
        }
        (
            _,
            TestPropT(box TestPropTData {
                use_op,
                reason: reason_op,
                id,
                propref,
                tout,
                hint: _,
            }),
        ) => {
            let lookup_default = LookupDefaultResult::Flow {
                use_op: use_op.dupe(),
                tout: Type::new(OpenT((**tout).dupe())),
            };
            let reason_prop = match &**propref {
                PropRef::Named { reason, .. } => reason,
                PropRef::Computed(_) => reason_op,
            };
            let test_info = Some((*id, (reason_prop.dupe(), l.dupe())));
            let lookup_default = match l.deref() {
                DefT(_, deref!(ObjT(obj))) if obj_type::is_exact(&obj.flags.obj_kind) => {
                    let r = reason_op.dupe().replace_desc(VirtualReasonDesc::RVoid);
                    Some((Type::new(DefT(r, DefT::new(VoidT))), lookup_default))
                }
                _ => {
                    // Note: a lot of other types could in principle be considered
                    // "exact". For example, new instances of classes could have exact
                    // types; so could `super` references (since they are statically
                    // rather than dynamically bound). However, currently we don't support
                    // any other exact types. Considering exact types inexact is sound, so
                    // there is no problem falling back to the same conservative
                    // approximation we use for inexact types in those cases.
                    let r = reason_op.dupe().replace_desc(None);
                    Some((
                        Type::new(DefT(r, DefT::new(MixedT(MixedFlavor::MixedEverything)))),
                        lookup_default,
                    ))
                }
            };
            let lookup_kind = LookupKind::NonstrictReturning(Box::new(NonstrictReturningData(
                lookup_default,
                test_info,
            )));
            let method_accessible = match l.deref() {
                DefT(_, deref!(InstanceT(_))) => false,
                _ => true,
            };
            rec_flow(
                cx,
                env,
                trace,
                (
                    l,
                    &UseT::new(LookupT(Box::new(LookupTData {
                        reason: reason_op.dupe(),
                        lookup_kind: Box::new(lookup_kind),
                        indexer_fallback: None,
                        try_ts_on_failure: vec![].into(),
                        propref: propref.clone(),
                        lookup_action: Box::new(LookupAction::ReadProp(Box::new(ReadPropData {
                            use_op: use_op.dupe(),
                            obj_t: l.dupe(),
                            tout: (**tout).dupe(),
                        }))),
                        method_accessible,
                        ids: Some(Default::default()),
                        ignore_dicts: false,
                    }))),
                ),
            )?;
        }

        // ***************************
        // * conditional type switch *
        // ***************************
        (
            _,
            CondT(box CondTData {
                loc: _,
                opt_type: then_t_opt,
                true_t: else_t,
                collector,
            }),
        ) => {
            let t = match l.deref() {
                DefT(_, deref!(EmptyT)) => else_t,
                _ => then_t_opt.as_ref().unwrap_or(l),
            };
            collector.add(t.dupe());
        }
        // *****************
        // * repositioning *
        // *****************

        // waits for a lower bound to become concrete, and then repositions it to
        // the location stored in the ReposLowerT, which is usually the location
        // where that lower bound was used; the lower bound's location (which is
        // being overwritten) is where it was defined.
        (
            _,
            ReposLowerT {
                reason,
                use_desc,
                use_t: u_inner,
            },
        ) => {
            let repos_l = helpers::reposition_reason(cx, env, Some(trace), reason, *use_desc, l)?;
            return Ok(Some(TailCall::RecFlow(
                repos_l,
                u_inner.as_ref().dupe(),
                DepthTrace::rec_trace(trace),
            )));
        }

        // ***********************************************************
        // * generics                                                *
        // ***********************************************************
        (
            _,
            SealGenericT(box SealGenericTData {
                loc: _,
                id,
                name,
                cont,
                no_infer,
            }),
        ) => {
            let reason = reason_of_t(l);
            let generic = Type::new(GenericT(Box::new(GenericTData {
                reason: reason.dupe(),
                id: id.clone(),
                name: name.dupe(),
                bound: l.dupe(),
                no_infer: *no_infer,
            })));
            continue_(cx, env, trace, &generic, cont)?;
        }
        (GenericT(box GenericTData { reason, bound, .. }), _) => {
            let repos = helpers::reposition_reason(cx, env, None, reason, false, bound)?;
            rec_flow(cx, env, trace, (&repos, u))?;
        }
        (ImplicitInstantiationTvar(data), _) => {
            rec_flow(cx, env, trace, (&env.frozen_tvar_bound(data), u))?;
        }
        (
            _,
            ConcretizeT(box ConcretizeTData {
                kind: ConcretizationKind::ConcretizeForEnumExhaustiveCheck,
                collector,
                ..
            }),
        ) => {
            collector.add(l.dupe());
        }
        // ************
        // * GetEnumT *
        // ************
        (
            DefT(enum_reason, deref!(EnumValueT(enum_info))),
            GetEnumT(box GetEnumTData {
                orig_t,
                kind: GetEnumKind::GetEnumObject,
                collector,
                ..
            }),
        ) => {
            let enum_value_t = match orig_t {
                Some(t) => t.dupe(),
                None => l.dupe(),
            };
            let enum_obj = Type::new(DefT(
                enum_reason.dupe(),
                DefT::new(EnumObjectT {
                    enum_value_t,
                    enum_info: enum_info.dupe(),
                }),
            ));
            collector.add(enum_obj);
        }
        (
            _,
            GetEnumT(box GetEnumTData {
                kind: GetEnumKind::GetEnumObject,
                collector,
                ..
            }),
        ) => {
            collector.add(l.dupe());
        }
        (
            DefT(_, deref!(EnumObjectT { enum_value_t, .. })),
            GetEnumT(box GetEnumTData {
                kind: GetEnumKind::GetEnumValue,
                collector,
                ..
            }),
        ) => {
            collector.add(enum_value_t.dupe());
        }
        // **********************************
        // * Flow Enums exhaustive checking *
        // **********************************
        (
            _,
            GetEnumT(box GetEnumTData {
                kind: GetEnumKind::GetEnumValue,
                collector,
                ..
            }),
        ) => {
            collector.add(l.dupe());
        }
        // ***************
        // * unsupported *
        // ***************

        // Lookups can be strict or non-strict, as denoted by the presence or
        // absence of strict_obj in the following two pattern matches.
        // Strictness derives from whether the object is sealed and was
        // created in the same scope in which the lookup occurs - see
        // mk_strict_lookup_reason below. The failure of a strict lookup
        // to find the desired property causes an error; a non-strict one
        // does not.
        (
            _,
            LookupT(box LookupTData {
                reason,
                lookup_kind,
                indexer_fallback,
                try_ts_on_failure,
                propref,
                lookup_action,
                method_accessible,
                ids,
                ignore_dicts,
            }),
        ) if is_extends_use_t_root(l.deref()) && !try_ts_on_failure.is_empty() => {
            // When the property is not found, we always try to look it up in the next
            // element in the list try_ts_on_failure. (`__proto__` is a getter/setter on
            // Object.prototype.)
            let next = &try_ts_on_failure[0];
            let rest = try_ts_on_failure[1..].to_vec();
            rec_flow(
                cx,
                env,
                trace,
                (
                    next,
                    &UseT::new(LookupT(Box::new(LookupTData {
                        reason: reason.dupe(),
                        lookup_kind: lookup_kind.clone(),
                        indexer_fallback: indexer_fallback.clone(),
                        try_ts_on_failure: rest.into(),
                        propref: propref.clone(),
                        lookup_action: lookup_action.clone(),
                        method_accessible: *method_accessible,
                        ids: ids.dupe(),
                        ignore_dicts: *ignore_dicts,
                    }))),
                ),
            )?;
        }
        // A batched lookup reaches the end of the prototype chain with a mix of
        // properties the prototype itself provides and properties that are missing
        // everywhere. Peel the former off into their own lookups so that the arms
        // below see a batch whose properties are all missing and can report them
        // together.
        (
            _,
            LookupT(box LookupTData {
                reason,
                lookup_kind,
                indexer_fallback,
                try_ts_on_failure,
                propref: _,
                lookup_action: box LookupAction::LookupPropsForSubtyping(box lookup_props_data),
                method_accessible,
                ids,
                ignore_dicts,
            }),
        ) if is_extends_use_t_root(l.deref())
            && try_ts_on_failure.is_empty()
            && lookup_props_data.props.len() > 1
            && lookup_props_data
                .props
                .iter()
                .any(|(propref, _)| !prop_missing_from_lookup_terminal(l, propref)) =>
        {
            let (missing, provided): (Vec<_>, Vec<_>) = lookup_props_data
                .props
                .iter()
                .partition(|(propref, _)| prop_missing_from_lookup_terminal(l, propref));
            let mut relookup = |props: Rc<[(PropRef, Property)]>| -> Result<(), FlowJsException> {
                let Some((propref, _)) = props.first() else {
                    return Ok(());
                };
                rec_flow(
                    cx,
                    env,
                    trace,
                    (
                        l,
                        &UseT::new(LookupT(Box::new(LookupTData {
                            reason: reason.dupe(),
                            lookup_kind: lookup_kind.clone(),
                            indexer_fallback: indexer_fallback.clone(),
                            try_ts_on_failure: Rc::from([]),
                            propref: Box::new(propref.clone()),
                            lookup_action: Box::new(LookupAction::LookupPropsForSubtyping(
                                Box::new(LookupPropsForSubtypingData {
                                    props: props.dupe(),
                                    ..lookup_props_data.clone()
                                }),
                            )),
                            ids: ids.dupe(),
                            method_accessible: *method_accessible,
                            ignore_dicts: *ignore_dicts,
                        }))),
                    ),
                )
            };
            for prop in provided {
                relookup(Rc::from([prop.clone()]))?;
            }
            relookup(missing.into_iter().cloned().collect())?;
        }
        (
            ObjProtoT(_) | FunProtoT(_),
            LookupT(box LookupTData {
                reason: reason_op,
                lookup_kind: _,
                indexer_fallback: _,
                try_ts_on_failure,
                propref: box PropRef::Named { name, .. },
                lookup_action:
                    box LookupAction::ReadProp(box ReadPropData {
                        use_op: _,
                        obj_t: lookup_l,
                        tout,
                    }),
                ids: _,
                method_accessible: _,
                ignore_dicts: _,
            }),
        ) if try_ts_on_failure.is_empty() && name == &Name::new("__proto__") => {
            // __proto__ is a getter/setter on Object.prototype
            let collector = type_collector::TypeCollector::create();
            rec_flow(
                cx,
                env,
                trace,
                (
                    lookup_l,
                    &UseT::new(GetProtoT(reason_op.dupe(), collector.dupe())),
                ),
            )?;
            let open_t = Type::new(OpenT(tout.dupe()));
            for proto in collector.collect_to_vec() {
                rec_flow_t(cx, env, trace, unknown_use(), (&proto, &open_t))?;
            }
        }
        (
            ObjProtoT(_) | FunProtoT(_),
            LookupT(box LookupTData {
                reason: reason_op,
                lookup_kind: _,
                indexer_fallback: _,
                try_ts_on_failure,
                propref: box PropRef::Named { name, .. },
                lookup_action: box LookupAction::WriteProp(_),
                method_accessible: _,
                ids: _,
                ignore_dicts: _,
            }),
        ) if try_ts_on_failure.is_empty() && name == &Name::new("__proto__") => {
            // __proto__ is a getter/setter on Object.prototype
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::EUnsupportedSetProto(reason_op.loc().dupe()),
            )?;
        }
        (
            ObjProtoT(_),
            LookupT(box LookupTData {
                reason: reason_op,
                try_ts_on_failure,
                propref: box PropRef::Named { name, .. },
                ..
            }),
        ) if try_ts_on_failure.is_empty() && flow_js_utils::is_object_prototype_method(name) => {
            // TODO: These properties should go in Object.prototype. Currently we
            // model Object.prototype as a ObjProtoT, as an optimization against a
            // possible deluge of shadow properties on Object.prototype, since it
            // is shared by every object.
            let obj = helpers::get_builtin_type(cx, env, Some(trace), reason_op, None, "Object")?;
            rec_flow(cx, env, trace, (&obj, u))?;
        }
        (
            FunProtoT(_),
            LookupT(box LookupTData {
                reason: reason_op,
                propref:
                    box PropRef::Named {
                        reason: reason_prop,
                        name,
                        ..
                    },
                ..
            }),
        ) if flow_js_utils::is_function_prototype(name) => {
            // Reaching here means the access resolved on `Function.prototype`
            // rather than on the receiver itself. `statement.rs` reads this back
            // to decide whether its `.call`/`.apply`/`.bind` receiver check applies.
            if matches!(name.as_str_opt(), Some("apply" | "bind" | "call")) {
                cx.record_fun_proto_method_lookup(reason_prop.loc().dupe());
            }
            // TODO: Ditto above comment for Function.prototype
            let fun = helpers::get_builtin_type(cx, env, Some(trace), reason_op, None, "Function")?;
            rec_flow(cx, env, trace, (&fun, u))?;
        }
        (
            DefT(_, deref!(NullT)),
            LookupT(box LookupTData {
                reason: reason_op,
                lookup_kind: box LookupKind::Strict(strict_obj),
                indexer_fallback,
                try_ts_on_failure,
                propref: propref @ box PropRef::Named { .. },
                lookup_action: action,
                method_accessible: _,
                ids,
                ignore_dicts: _,
            }),
        ) if try_ts_on_failure.is_empty() => {
            strict_lookup_failed(
                cx,
                env,
                trace,
                l,
                reason_op,
                strict_obj,
                propref,
                action,
                ids.as_ref(),
                indexer_fallback.as_deref(),
            )?;
        }

        (
            ObjProtoT(_) | FunProtoT(_),
            LookupT(box LookupTData {
                reason: reason_op,
                lookup_kind: box LookupKind::Strict(strict_obj),
                indexer_fallback,
                try_ts_on_failure,
                propref: propref @ box PropRef::Named { .. },
                lookup_action: action,
                method_accessible: _,
                ids,
                ignore_dicts: _,
            }),
        ) if try_ts_on_failure.is_empty() => {
            strict_lookup_failed(
                cx,
                env,
                trace,
                l,
                reason_op,
                strict_obj,
                propref,
                action,
                ids.as_ref(),
                indexer_fallback.as_deref(),
            )?;
        }
        // A computed lookup never carries an `indexer_fallback`: an indexer answers a
        // computed access on the spot, so there is nothing to hold back.
        (
            DefT(_, deref!(NullT)),
            LookupT(box LookupTData {
                reason: reason_op,
                lookup_kind: box LookupKind::Strict(strict_obj),
                try_ts_on_failure,
                propref: propref @ box PropRef::Computed(elem_t),
                lookup_action: action,
                method_accessible: _,
                ids: _,
                ignore_dicts: _,
                indexer_fallback: _,
            }),
        ) if try_ts_on_failure.is_empty() => {
            strict_computed_lookup_failed(
                cx, env, trace, l, reason_op, strict_obj, propref, elem_t, action,
            )?;
        }
        (
            ObjProtoT(_) | FunProtoT(_),
            LookupT(box LookupTData {
                reason: reason_op,
                lookup_kind: box LookupKind::Strict(strict_obj),
                indexer_fallback: _,
                try_ts_on_failure,
                propref: propref @ box PropRef::Computed(elem_t),
                lookup_action: action,
                method_accessible: _,
                ids: _,
                ignore_dicts: _,
            }),
        ) if try_ts_on_failure.is_empty() => {
            strict_computed_lookup_failed(
                cx, env, trace, l, reason_op, strict_obj, propref, elem_t, action,
            )?;
        }
        (
            ObjProtoT(_) | FunProtoT(_) | DefT(_, deref!(NullT)),
            LookupT(box LookupTData {
                reason: reason_op,
                lookup_kind:
                    box LookupKind::NonstrictReturning(box NonstrictReturningData(t_opt, test_opt)),
                indexer_fallback,
                try_ts_on_failure,
                propref,
                lookup_action: action,
                ids,
                ..
            }),
        ) if try_ts_on_failure.is_empty() => {
            // An indexer matched the access on the way up, so the lookup did not fail and
            // the not-found default does not apply.
            if let Some(fallback) = indexer_fallback {
                perform_lookup_action(
                    cx,
                    env,
                    trace,
                    propref,
                    &property::property_type(&fallback.property),
                    Some(&fallback.property),
                    PropertySource::IndexerProperty,
                    &fallback.obj,
                    reason_op,
                    action,
                    None,
                )?;
            } else {
                // don't fire
                //
                //  ...unless a default return value is given. Two examples:
                //
                //  1. A failure could arise when an unchecked module was looked up and
                //  not found declared, in which case we consider that module's exports to
                //  be `any`.
                //
                //  2. A failure could arise also when an object property is looked up in
                //  a condition, in which case we consider the object's property to be
                //  `mixed`.
                let use_op = flow_js_utils::use_op_of_lookup_action(action);
                if let Some((id, prop_reason_and_object)) = test_opt {
                    let suggestion: Option<FlowSmolStr> = match &**propref {
                        PropRef::Named { name, .. } => ids.as_ref().and_then(|ids| {
                            let ids_vec: Vec<_> = ids.iter().duped().collect();
                            prop_typo_suggestion_for_name(cx, &ids_vec, name)
                        }),
                        _ => None,
                    };
                    if !matches!(
                        *cx.typing_mode(),
                        flow_typing_context::TypingMode::HintEvaluationMode
                    ) {
                        cx.test_prop_miss(
                            *id,
                            name_of_propref(propref),
                            prop_reason_and_object.dupe(),
                            use_op.dupe(),
                            suggestion,
                        );
                    }
                }
                match t_opt {
                    Some((not_found, LookupDefaultResult::Unify(t))) => {
                        FlowJs::rec_unify_with_env(
                            cx,
                            env,
                            trace,
                            use_op,
                            UnifyCause::Uncategorized,
                            Some(true),
                            t,
                            not_found,
                        )?;
                    }
                    Some((not_found, LookupDefaultResult::Flow { use_op, tout })) => {
                        rec_flow_t(cx, env, trace, use_op.dupe(), (not_found, tout))?;
                    }
                    Some((not_found, LookupDefaultResult::Lookup(lookup))) => {
                        rec_flow(
                            cx,
                            env,
                            trace,
                            (not_found, &UseT::new(LookupT(lookup.clone()))),
                        )?;
                    }
                    None => {}
                }
            }
        }
        // SuperT only involves non-strict lookups
        (DefT(_, deref!(NullT)), SuperT(..)) => {}
        (ObjProtoT(_), SuperT(..)) => {}
        (FunProtoT(_), SuperT(..)) => {}
        // ExtendsUseT searches for a nominal superclass. The search terminates with
        // either failure at the root or a structural subtype check.
        (
            AnyT(_, src),
            ExtendsUseT(box ExtendsUseTData {
                use_op,
                reason: reason_op,
                targs: ts,
                true_t: t1,
                false_t: t2,
            }),
        ) => {
            for t in ts.iter() {
                let any = any_t::why(*src, reason_op.dupe());
                rec_flow_t(cx, env, trace, use_op.dupe(), (&any, t))?;
            }
            let any1 = any_t::why(*src, reason_op.dupe());
            rec_flow_t(cx, env, trace, use_op.dupe(), (&any1, t1))?;
            let any2 = any_t::why(*src, reason_op.dupe());
            rec_flow_t(cx, env, trace, use_op.dupe(), (&any2, t2))?;
        }
        (DefT(lreason, deref!(ObjT(obj))), ExtendsUseT(..)) => {
            let proto_t = &obj.proto_t;
            let repositioned = helpers::reposition(
                cx,
                env,
                Some(trace),
                lreason.loc().dupe(),
                None,
                None,
                proto_t.dupe(),
            )?;
            rec_flow(cx, env, trace, (&repositioned, u))?;
        }
        (DefT(reason, deref!(ClassT(instance))), ExtendsUseT(..)) => {
            let collector = type_collector::TypeCollector::create();
            rec_flow(
                cx,
                env,
                trace,
                (
                    instance,
                    &UseT::new(GetStaticsT(reason.dupe(), collector.dupe())),
                ),
            )?;
            for static_ in collector.collect_to_vec() {
                rec_flow(cx, env, trace, (&static_, u))?;
            }
        }
        (
            root,
            ExtendsUseT(box ExtendsUseTData {
                use_op,
                reason: reason_ext,
                targs: try_ts,
                true_t: ext_l,
                false_t: ext_u,
            }),
        ) if is_extends_use_t_root(root) && !try_ts.is_empty() => {
            // When seaching for a nominal superclass fails, we always try to look it
            // up in the next element in the list try_ts_on_failure.
            let next = &try_ts[0];
            let rest = try_ts[1..].to_vec();
            rec_flow(
                cx,
                env,
                trace,
                (
                    next,
                    &UseT::new(ExtendsUseT(Box::new(ExtendsUseTData {
                        use_op: use_op.dupe(),
                        reason: reason_ext.dupe(),
                        targs: rest.into(),
                        true_t: ext_l.dupe(),
                        false_t: ext_u.dupe(),
                    }))),
                ),
            )?;
        }
        (
            root,
            ExtendsUseT(box ExtendsUseTData {
                use_op,
                reason: _,
                targs: try_ts,
                true_t: ext_l,
                false_t: ext_u @ deref!(DefT(_, deref!(InstanceT(inst_t)))),
            }),
        ) if is_extends_use_t_root(root) && try_ts.is_empty() && {
            match &inst_t.inst.inst_kind {
                InstanceKind::InterfaceKind { .. } => true,
                InstanceKind::ClassKind => inst_t.inst.strictness_kind.is_typescript_loose(),
                InstanceKind::RecordKind { .. } => false,
            }
        } =>
        {
            let super_ = &inst_t.super_;
            let own_props = inst_t.inst.own_props.dupe();
            let inst_kind = &inst_t.inst.inst_kind;
            let upper_inst_abstract = inst_t.inst.inst_abstract;
            let lower_strictness_kind = match ext_l.deref() {
                DefT(_, deref!(InstanceT(instance))) => instance.inst.strictness_kind,
                DefT(_, deref!(ObjT(obj))) => obj.strictness_kind,
                _ => TypeStrictnessKind::Flow,
            };
            let strictness_kind = inst_t.inst.strictness_kind.join(lower_strictness_kind);
            // In TS mode, classes are structural and `constructor` is not part of
            // the structural shape, so drop it from proto_props before checking.
            // Matches TS, where `const c: C = { x: 1 }` is accepted.
            //
            // KNOWN LIMITATION: ECMAScript `#private` fields and methods live on
            // `class_private_fields` / `class_private_methods` on `insttype` and
            // are not threaded into `structural_subtype`, so a `.ts` consumer can
            // satisfy a class with private members via a structurally-matching
            // object literal. TS treats `#private` nominally; matching that here
            // would require also passing the private maps through the structural
            // check and is left as future work.
            let proto_props = {
                // The `if` guard above guarantees `ClassKind` only matches in TS mode,
                // so no extra gating is needed here.
                match inst_kind {
                    InstanceKind::ClassKind => {
                        let mut filtered = cx.find_props(inst_t.inst.proto_props.dupe());
                        filtered.remove(&Name::new(FlowSmolStr::new_inline("constructor")));
                        let id = properties::Id::generate_id();
                        cx.add_property_map(id.dupe(), filtered);
                        id
                    }
                    InstanceKind::InterfaceKind { .. } | InstanceKind::RecordKind { .. } => {
                        inst_t.inst.proto_props.dupe()
                    }
                }
            };
            let inst_call_t = inst_t.inst.inst_call_t;
            let inst_construct_t = inst_t.inst.inst_construct_t;
            let inst_dict = &inst_t.inst.inst_dict;
            let check_structural_interface = |cx: &mut Context<'cx>, env: &FlowJsEnv| {
                if matches!(inst_kind, InstanceKind::InterfaceKind { .. })
                    && matches!(ext_l, deref!(DefT(_, deref!(ObjT(_)))))
                {
                    return rec_flow(
                        cx,
                        env,
                        trace,
                        (ext_u, &UseT::new(ImplementsT(use_op.dupe(), ext_l.dupe()))),
                    );
                }
                structural_subtype(
                    cx,
                    env,
                    trace,
                    use_op.dupe(),
                    upper_inst_abstract,
                    strictness_kind,
                    ext_l,
                    ext_u,
                    (
                        own_props,
                        proto_props,
                        inst_call_t,
                        inst_construct_t,
                        inst_dict,
                    ),
                )?;
                match super_.deref() {
                    ObjProtoT(_) | FunProtoT(_) | NullProtoT(_) => {}
                    _ => {
                        rec_flow(
                            cx,
                            env,
                            trace,
                            (ext_l, &UseT::new(UseT(use_op.dupe(), super_.dupe()))),
                        )?;
                    }
                }
                Ok(())
            };
            if strictness_kind.is_typescript_loose()
                && !env.speculating()
                && !flow_js_utils::tvar_visitors::has_unresolved_tvars(cx, ext_l)
                && !flow_js_utils::tvar_visitors::has_unresolved_tvars(cx, ext_u)
            {
                match speculation_kit::try_singleton_custom_with_error(
                    cx,
                    env,
                    Box::new(check_structural_interface),
                )? {
                    None => {}
                    Some(error) => {
                        let type_or_explanatory_desc = |t: &Type| {
                            if let Some(desc) = reason_of_t(t).desc(true)
                                && desc.is_explanatory()
                            {
                                TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::ReasonDesc(desc.clone()))
                            } else {
                                TypeOrTypeDescT::Type(t.dupe())
                            }
                        };
                        add_output_with_env(
                            cx,
                            env,
                            ErrorMessage::EIncompatibleTypesWithUseOp(Box::new(
                                EIncompatibleTypesWithUseOpData {
                                    use_op: use_op.dupe(),
                                    lower_loc: type_util::loc_of_t(ext_l).dupe(),
                                    lower_def_loc: type_util::ref_loc_of_t(ext_l).dupe(),
                                    upper_loc: type_util::loc_of_t(ext_u).dupe(),
                                    upper_def_loc: type_util::ref_loc_of_t(ext_u).dupe(),
                                    lower_desc: type_or_explanatory_desc(ext_l),
                                    upper_desc: type_or_explanatory_desc(ext_u),
                                    explanation: None,
                                    example: Some(error.0),
                                    branches: vec![],
                                },
                            )),
                        )?;
                    }
                }
            } else {
                check_structural_interface(cx, env)?;
            }
        }
        // Unwrap deep readonly
        (_, DeepReadOnlyT(tout, _)) | (_, HooklikeT(tout)) => {
            let open_tout = Type::new(OpenT((**tout).dupe()));
            rec_flow_t(cx, env, trace, unknown_use(), (l, &open_tout))?;
        }
        // Render Type Misc Uses
        #[rustfmt::skip]
        (
            DefT(_, deref!(RendersT(deref!(
                CanonicalRendersForm::IntrinsicRenders(_)
                | CanonicalRendersForm::NominalRenders { .. }
            )))),
            ExitRendersT {
                renders_reason,
                u: exit_u,
            },
        ) => {
            let mixed_element = FlowJs::get_builtin_react_type_with_env(
                cx, env,
                Some(trace),
                renders_reason,
                None,
                flow_typing_errors::intermediate_error_types::ExpectedModulePurpose::ReactModuleForReactMixedElementType,
            )?;
            rec_flow(cx, env, trace, (&mixed_element, exit_u))?;
        }
        #[rustfmt::skip]
        (
            DefT(renders_reason, deref!(RendersT(deref!(
                CanonicalRendersForm::IntrinsicRenders(_)
                | CanonicalRendersForm::NominalRenders { .. }
            )))),
            _,
        ) => {
            let mixed_element = FlowJs::get_builtin_react_type_with_env(
                cx, env,
                Some(trace),
                renders_reason,
                None,
                flow_typing_errors::intermediate_error_types::ExpectedModulePurpose::ReactModuleForReactMixedElementType,
            )?;
            rec_flow(cx, env, trace, (&mixed_element, u))?;
        }
        #[rustfmt::skip]
        (
            DefT(r, deref!(RendersT(deref!(CanonicalRendersForm::StructuralRenders {
                renders_variant: RendersVariant::RendersNormal,
                renders_structural_type,
            })))),
            _,
        ) => {
            let t = renders_structural_type;
            let exit_u = UseT::new(ExitRendersT {
                renders_reason: r.dupe(),
                u: Box::new(u.dupe()),
            });
            rec_flow(cx, env, trace, (t, &exit_u))?;
        }
        (
            _,
            ExitRendersT {
                renders_reason,
                u: exit_u,
            },
        ) => {
            let node = FlowJs::get_builtin_react_type_with_env(
                cx, env,
                Some(trace),
                renders_reason,
                None,
                flow_typing_errors::intermediate_error_types::ExpectedModulePurpose::ReactModuleForReactNodeType,
            )?;
            rec_flow(cx, env, trace, (&node, exit_u))?;
        }
        // ***********************
        // * Object library call *
        // ***********************
        (ObjProtoT(reason), _) => {
            let obj_proto =
                helpers::get_builtin_type(cx, env, Some(trace), reason, Some(true), "Object")?;
            return Ok(Some(TailCall::RecFlow(
                obj_proto,
                u.dupe(),
                DepthTrace::rec_trace(trace),
            )));
        }
        // *************************
        // * Function library call *
        // *************************
        (FunProtoT(reason), _) => {
            let fun_proto =
                helpers::get_builtin_type(cx, env, Some(trace), reason, Some(true), "Function")?;
            return Ok(Some(TailCall::RecFlow(
                fun_proto,
                u.dupe(),
                DepthTrace::rec_trace(trace),
            )));
        }
        (
            _,
            ExtendsUseT(box ExtendsUseTData {
                use_op,
                reason: _,
                targs: try_ts,
                true_t: t,
                false_t: tc,
            }),
        ) if try_ts.is_empty() => {
            flow_js_utils::add_output_with_env(
                cx,
                env,
                flow_js_utils::incompatible_types_error(t, tc, use_op.dupe(), None),
            )?;
        }
        // *********
        // * Match *
        // *********
        (
            _,
            ConcretizeT(box ConcretizeTData {
                reason: _,
                kind: ConcretizationKind::ConcretizeForMatchArg { .. },
                seen: _,
                collector,
            }),
        ) => {
            collector.add(l.dupe());
        }

        // A template literal indexer key is already a string, so it joins the key set unchanged.
        (TemplateLiteralT { .. }, GetKeysDictKeyT { t_out, .. }) => {
            rec_flow(cx, env, trace, (l, t_out.as_ref()))?;
        }
        (TemplateLiteralT { reason, quasis, .. }, _) => {
            let reason = reason.dupe().replace_desc(VirtualReasonDesc::RString);
            let literal_kind = if quasis.iter().any(|q| !q.is_empty()) {
                Literal::Truthy
            } else {
                Literal::AnyLiteral
            };
            let str_t = Type::new(DefT(reason, DefT::new(StrGeneralT(literal_kind))));
            rec_flow(cx, env, trace, (&str_t, u))?;
        }
        // *******************************
        // * ToString abstract operation *
        // *******************************

        // ToStringT passes through strings unchanged, and flows a generic StrT otherwise
        (
            DefT(_, deref!(StrGeneralT(_) | SingletonStrT { .. })),
            ToStringT {
                orig_t: None,
                t_out,
                ..
            },
        ) => {
            rec_flow(cx, env, trace, (l, t_out))?;
        }
        (
            DefT(_, deref!(StrGeneralT(_) | SingletonStrT { .. })),
            ToStringT {
                orig_t: Some(t),
                t_out,
                ..
            },
        ) => {
            rec_flow(cx, env, trace, (t, t_out))?;
        }
        (_, ToStringT { reason, t_out, .. }) => {
            let str_mod = str_module_t::why(reason.dupe());
            rec_flow(cx, env, trace, (&str_mod, t_out))?;
        }
        // ***********************************
        // * The indexer half of a key set   *
        // ***********************************

        // A symbol-valued indexer key contributes to type-level `keyof` as that
        // same symbol type, and is left out of runtime key enumeration
        // (`Object.keys`) entirely, matching JavaScript.
        (
            DefT(_, deref!(UniqueSymbolT(_) | SymbolT)),
            GetKeysDictKeyT {
                t_out,
                include_symbols,
                ..
            },
        ) => {
            if *include_symbols {
                rec_flow(cx, env, trace, (l, t_out.as_ref()))?;
            }
        }
        // Every other key stringifies.
        (_, GetKeysDictKeyT { reason, t_out, .. }) => {
            rec_flow(
                cx,
                env,
                trace,
                (
                    l,
                    &UseT::new(ToStringT {
                        orig_t: None,
                        reason: reason.dupe(),
                        t_out: t_out.clone(),
                    }),
                ),
            )?;
        }
        // **********************
        // * Array library call *
        // **********************
        #[rustfmt::skip]
        (
            DefT(reason, deref!(ArrT(deref!(ArrayAT(box ArrayATData {
                elem_t,
                react_dro: Some(ReactDro(dro_loc, dro_type)),
                tuple_view,
                strictness_kind,
                literal,
                fresh,
            }))))),
            GetPropT(..) | SetPropT(..) | MethodT(..) | LookupT(..),
        ) => {
            match u.deref() {
                MethodT(box MethodTData {
                    use_op,
                    reason: _,
                    prop_reason: method_reason,
                    propref: box PropRef::Named { name, .. },
                    method_action: _,
                }) if matches!(
                    name.as_str_opt(),
                    Some(
                        "fill"
                            | "pop"
                            | "push"
                            | "reverse"
                            | "shift"
                            | "sort"
                            | "splice"
                            | "unshift"
                    )
                ) =>
                {
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EPropNotReadable(Box::new(EPropNotReadableData {
                            prop_loc: method_reason.loc().dupe(),
                            prop_name: Some(name.dupe()),
                            use_op: VirtualUseOp::Frame(
                                Arc::new(FrameUseOp::ReactDeepReadOnly(Box::new((
                                    dro_loc.dupe(),
                                    dro_type.clone(),
                                )))),
                                Arc::new(use_op.dupe()),
                            ),
                        })),
                    )?;
                    let arr_no_dro = Type::new(DefT(
                        reason.dupe(),
                        DefT::new(ArrT(Rc::new(ArrayAT(Box::new(ArrayATData {
                            elem_t: elem_t.dupe(),
                            react_dro: None,
                            tuple_view: tuple_view.clone(),
                            strictness_kind: *strictness_kind,
                            literal: *literal,
                            fresh: *fresh,
                        }))))),
                    ));
                    rec_flow(cx, env, trace, (&arr_no_dro, u))?;
                }
                GetPropT(box data @ GetPropTData {
                    propref: box PropRef::Named { name, .. }, ..
                }) if matches!(
                            name.as_str_opt(),
                            Some(
                                "fill"
                                    | "pop"
                                    | "push"
                                    | "reverse"
                                    | "shift"
                                    | "sort"
                                    | "splice"
                                    | "unshift"
                            )
                        ) =>
                {
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EPropNotReadable(Box::new(EPropNotReadableData {
                            prop_loc: data.reason.loc().dupe(),
                            prop_name: Some(name.dupe()),
                            use_op: VirtualUseOp::Frame(
                                Arc::new(FrameUseOp::ReactDeepReadOnly(Box::new((
                                    dro_loc.dupe(),
                                    dro_type.clone(),
                                )))),
                                Arc::new(data.use_op.dupe()),
                            ),
                        })),
                    )?;
                    let arr_no_dro = Type::new(DefT(
                        reason.dupe(),
                        DefT::new(ArrT(Rc::new(ArrayAT(Box::new(ArrayATData {
                            elem_t: elem_t.dupe(),
                            react_dro: None,
                            tuple_view: tuple_view.clone(),
                            strictness_kind: *strictness_kind,
                            literal: *literal,
                            fresh: *fresh,
                        }))))),
                    ));
                    rec_flow(cx, env, trace, (&arr_no_dro, u))?;
                }
                _ => {
                    let dro_elem = mk_react_dro(
                        cx,
                        unknown_use(),
                        ReactDro(dro_loc.dupe(), dro_type.clone()),
                        elem_t.dupe(),
                    );
                    let ro_arr = get_builtin_typeapp(
                        cx,
                        env,
                        reason,
                        Some(true),
                        "ReadonlyArray",
                        vec![dro_elem],
                    );
                    let u_mod = type_util::mod_use_op_of_use_t(
                        |use_op: &UseOp| {
                            VirtualUseOp::Frame(
                                Arc::new(FrameUseOp::ReactDeepReadOnly(Box::new((
                                    dro_loc.dupe(),
                                    dro_type.clone(),
                                )))),
                                Arc::new(use_op.dupe()),
                            )
                        },
                        u,
                    );
                    rec_flow(cx, env, trace, (&ro_arr, &u_mod))?;
                }
            }
        },
        #[rustfmt::skip]
        (
            DefT(reason, deref!(ArrT(deref!(ArrayAT(box ArrayATData {
                elem_t, strictness_kind, ..
            }))))),
            GetPropT(..) | SetPropT(..) | MethodT(..) | LookupT(..),
        ) => {
            // The members are those of the builtin `Array` instance, with `this` being the array
            // type `T[]` of the receiver.
            let this_t = Type::new(DefT(
                reason.dupe(),
                DefT::new(ArrT(Rc::new(ArrayAT(Box::new(
                    ArrayATData {
                        react_dro: None,
                        elem_t: elem_t.dupe(),
                        tuple_view: None,
                        strictness_kind: *strictness_kind,
                        literal: false,
                        fresh: false,
                    },
                ))))),
            ));
            let arr_t = Type::new(ThisTypeAppT(Box::new(ThisTypeAppTData {
                reason: reason.dupe(),
                this_t,
                type_: flow_js_utils::lookup_builtin_type_with_env(cx, env, "Array", reason.dupe()),
                targs: Some(Rc::from([elem_t.dupe()])),
            })));
            rec_flow(cx, env, trace, (&arr_t, u))?;
        }
        // *************************
        // * Tuple "length" access *
        // *************************
        #[rustfmt::skip]
        (
            DefT(reason, deref!(ArrT(deref!(TupleAT(box TupleATData {
                arity, inexact, rest, ..
            }))))),
            GetPropT(box data @ GetPropTData {
                propref: box PropRef::Named { name, .. }, ..
            }),
        ) if name == &Name::new("length") => {
            flow_js_utils::get_prop_t_kit::on_array_length::<FlowJs>(
                cx,
                env,
                &trace,
                reason.dupe(),
                *inexact || rest.is_some(),
                *arity,
                data.reason.loc().dupe(),
            )?(cx, (*data.tout).dupe())?;
        }
        #[rustfmt::skip]
        (
            DefT(reason, deref!(ArrT(deref!(
                TupleAT(box TupleATData {
                    elem_t, react_dro: Some(ReactDro(dro_loc, dro_type)), ..
                })
                | ROArrayAT(box ROArrayATData {
                    elem_t, react_dro: Some(ReactDro(dro_loc, dro_type)), ..
                })
            )))),
            GetPropT(..) | SetPropT(..) | MethodT(..) | LookupT(..),
        ) => {
            let u_mod = type_util::mod_use_op_of_use_t(
                |use_op: &UseOp| {
                    VirtualUseOp::Frame(
                        Arc::new(FrameUseOp::ReactDeepReadOnly(Box::new((
                            dro_loc.dupe(),
                            dro_type.clone(),
                        )))),
                        Arc::new(use_op.dupe()),
                    )
                },
                u,
            );
            let dro_elem = mk_react_dro(
                cx,
                unknown_use(),
                ReactDro(dro_loc.dupe(), dro_type.clone()),
                elem_t.dupe(),
            );
            let ro_arr =
                get_builtin_typeapp(cx, env, reason, Some(true), "ReadonlyArray", vec![dro_elem]);
            rec_flow(cx, env, trace, (&ro_arr, &u_mod))?;
        }
        (
            DefT(reason, deref!(ArrT(arr @ deref!(TupleAT(..) | ROArrayAT(..))))),
            GetPropT(..) | SetPropT(..) | MethodT(..) | LookupT(..),
        ) => {
            let t = elemt_of_arrtype(arr);
            let ro_arr = get_builtin_typeapp(cx, env, reason, None, "ReadonlyArray", vec![t]);
            rec_flow(cx, env, trace, (&ro_arr, u))?;
        }
        // ***********************
        // * String library call *
        // ***********************
        (DefT(reason, deref!(StrGeneralT(_) | SingletonStrT { .. })), _)
            if primitive_promoting_use_t(u) =>
        {
            let string_t = helpers::get_builtin_type(cx, env, Some(trace), reason, None, "String")?;
            rec_flow(cx, env, trace, (&string_t, u))?;
        }
        // ***********************
        // * Number library call *
        // ***********************
        (DefT(reason, deref!(NumGeneralT(_) | SingletonNumT { .. })), _)
            if primitive_promoting_use_t(u) =>
        {
            let number_t = helpers::get_builtin_type(cx, env, Some(trace), reason, None, "Number")?;
            rec_flow(cx, env, trace, (&number_t, u))?;
        }
        // ************************
        // * Boolean library call *
        // ************************
        (DefT(reason, deref!(BoolGeneralT | SingletonBoolT { .. })), _)
            if primitive_promoting_use_t(u) =>
        {
            let bool_t = helpers::get_builtin_type(cx, env, Some(trace), reason, None, "Boolean")?;
            rec_flow(cx, env, trace, (&bool_t, u))?;
        }
        // ***********************
        // * BigInt library call *
        // ***********************
        (DefT(reason, deref!(BigIntGeneralT(_) | SingletonBigIntT { .. })), _)
            if primitive_promoting_use_t(u) =>
        {
            let bigint_t = helpers::get_builtin_type(cx, env, Some(trace), reason, None, "BigInt")?;
            rec_flow(cx, env, trace, (&bigint_t, u))?;
        }
        // ***********************
        // * Symbol library call *
        // ***********************
        (DefT(reason, deref!(SymbolT | UniqueSymbolT(_))), _) if primitive_promoting_use_t(u) => {
            let symbol_t = helpers::get_builtin_type(cx, env, Some(trace), reason, None, "Symbol")?;
            rec_flow(cx, env, trace, (&symbol_t, u))?;
        }
        // *****************************************************
        // * Nice error messages for mixed function refinement *
        // *****************************************************
        (
            DefT(lreason, deref!(MixedT(MixedFlavor::MixedFunction))),
            MethodT(..) | SetPropT(..) | GetPropT(..) | LookupT(..),
        ) => {
            let fun_proto = Type::new(FunProtoT(lreason.dupe()));
            rec_flow(cx, env, trace, (&fun_proto, u))?;
        }
        (
            DefT(_, deref!(MixedT(MixedFlavor::MixedFunction))),
            CallT(box CallTData {
                call_action: box CallAction::ConcretizeCallee(tout),
                ..
            }),
        ) => {
            tout.add(l.dupe());
        }
        (
            DefT(lreason, deref!(MixedT(MixedFlavor::MixedFunction))),
            CallT(box CallTData {
                use_op,
                reason: ureason,
                ..
            }),
        ) => {
            flow_js_utils::add_output_with_env(
                cx,
                env,
                flow_js_utils::incompatible_type_error(
                    l,
                    IncompatibleUpperData {
                        loc: ureason.loc().dupe(),
                        kind: UpperKind::IncompatibleMixedCallT,
                    },
                    Some(use_op.dupe()),
                ),
            )?;
            let any = any_t::make(AnySource::AnyError(None), lreason.dupe());
            rec_flow(cx, env, trace, (&any, u))?;
        }
        (
            _,
            LookupT(box LookupTData {
                propref,
                lookup_action,
                ..
            }),
        ) => {
            default_resolve::default_resolve_touts(
                cx,
                &|cx: &mut Context<'cx>, l_inner, u_inner| {
                    rec_flow(cx, env, trace, (l_inner, u_inner))
                },
                None,
                env,
                reason_of_t(l).loc().dupe(),
                u,
            )?;
            let use_op = Some(use_op_of_lookup_action(lookup_action));
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::EIncompatibleProp(Box::new(EIncompatiblePropData {
                    prop: name_of_propref(propref),
                    prop_loc: reason_of_propref(propref).loc().dupe(),
                    object: flow_js_utils::type_reference_with_reason_for_error(
                        l,
                        reason_of_t(l).dupe(),
                    ),
                    special: flow_js_utils::error_message_kind_of_lower(l),
                    use_op,
                })),
            )?;
        }
        // Super classes are collected too, so that subclasses of `Promise` are found
        (
            DefT(_, deref!(InstanceT(inst_t))),
            ConcretizeT(box ConcretizeTData {
                kind: ConcretizationKind::ConcretizeForUnusedPromiseCheck,
                collector,
                ..
            }),
        ) => {
            collector.add(l.dupe());
            if !flow_js_utils::is_builtin_class_id("Promise", inst_t.inst.class_id.dupe(), cx) {
                rec_flow(cx, env, trace, (&inst_t.super_, u))?;
            }
        }
        (
            _,
            ConcretizeT(box ConcretizeTData {
                reason: _,
                kind:
                    ConcretizationKind::ConcretizeForUnusedPromiseCheck
                    | ConcretizationKind::ConcretizeAll,
                seen: _,
                collector,
            }),
        ) => {
            collector.add(l.dupe());
        }
        _ => {
            let use_op = use_op_of_use_t(u);
            flow_js_utils::add_output_with_env(
                cx,
                env,
                flow_js_utils::incompatible_type_error(
                    l,
                    IncompatibleUpperData {
                        loc: flow_js_utils::error_message_loc_of_upper(u),
                        kind: flow_js_utils::error_message_kind_of_upper(u),
                    },
                    use_op,
                ),
            )?;
            let resolve_callee = match u.deref() {
                CallT(..) => Some((reason_of_t(l).dupe(), vec![l.dupe()])),
                _ => None,
            };
            default_resolve::default_resolve_touts(
                cx,
                &|cx: &mut Context<'cx>, l_inner, u_inner| {
                    rec_flow(cx, env, trace, (l_inner, u_inner))
                },
                resolve_callee.as_ref(),
                env,
                reason_of_t(l).loc().dupe(),
                u,
            )?;
        }
    }
    Ok(None)
}
