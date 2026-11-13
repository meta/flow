/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::Cell;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

use flow_common::reason::Name;
use flow_common_utils::option_utils;
use flow_typing_errors::error_message::EPropNotReadableData;
use flow_typing_errors::error_message::EPropNotWritableData;
use flow_typing_errors::error_message::ETooFewTypeArgsData;
use flow_typing_errors::error_message::ETooManyTypeArgsData;
use flow_typing_errors::intermediate_error_types::NamedReferenceData;
use flow_typing_errors::intermediate_error_types::PropPolarityMismatchObject;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_type::type_::CallMData;
use flow_typing_type::type_::CallTData;
use flow_typing_type::type_::ChainMData;
use flow_typing_type::type_::ClassImplementsCheckData;
use flow_typing_type::type_::ConcretizeTData;
use flow_typing_type::type_::DerivedReference;
use flow_typing_type::type_::EvalTypeDestructorTData;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::ImplicitInstantiationTvarData;
use flow_typing_type::type_::ImplicitInstantiationTvarKind;
use flow_typing_type::type_::LookupActionMatchPropData;
use flow_typing_type::type_::LookupDefaultResult;
use flow_typing_type::type_::LookupPropsForSubtypingData;
use flow_typing_type::type_::LookupTData;
use flow_typing_type::type_::MethodTData;
use flow_typing_type::type_::NonstrictReturningData;
use flow_typing_type::type_::Property;
use flow_typing_type::type_::PropertyCompatibilityData;
use flow_typing_type::type_::ReadElemData;
use flow_typing_type::type_::ReadPropData;
use flow_typing_type::type_::ResolveUnionTData;
use flow_typing_type::type_::SealGenericTData;
use flow_typing_type::type_::SpecializeTData;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeArgCompatibilityData;
use flow_typing_type::type_::ValueToTypeReferenceTData;
use flow_typing_type::type_::WriteElemData;
use flow_typing_type::type_::WritePropData;

use super::any_helpers::any_propagated;
use super::constraint_helpers::constraint_node_constraints;
use super::constraint_helpers::constraint_node_id;
use super::constraint_helpers::resolve_id;
use super::dispatch::__flow;
use super::unification_helpers::__unify;
use super::*;
use crate::speculation_kit;
use crate::tvar_resolver;

pub(super) fn not_linked<CX: ContextFamily>(
    (id1, _bounds1): (i32, &constraint::BoundsRef<CX>),
    (_id2, bounds2): (i32, &constraint::BoundsRef<CX>),
) -> bool {
    // It suffices to check that id1 is not already in the lower bounds of
    // id2. Equivalently, we could check that id2 is not already in the upper
    // bounds of id1.
    !bounds2.borrow().lowertvars.contains_key(&id1)
}

thread_local! {
    static CHECK_CANCELED_COUNT: Cell<i32> = const { Cell::new(0) };
}

pub(super) fn check_canceled<'cx>(
    cx: &mut Context<'cx>,
) -> Result<(), flow_utils_concurrency::job_error::JobError> {
    CHECK_CANCELED_COUNT.with(|count| {
        let n = (count.get() + 1) % 128;
        count.set(n);
        if n == 0 { cx.check_budget() } else { Ok(()) }
    })
}

pub(super) fn inherited_method(name: &Name) -> bool {
    !name.matches_str("constructor")
}

/// The nearest field named `constructor` on the instance `t` or its supers, as in TypeScript's
/// `['constructor']: KlassConstructor<typeof Node>`.
pub(super) fn constructor_field<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    t: &Type,
) -> Result<Option<Property>, FlowJsException> {
    let name = Name::new(FlowSmolStr::new_inline("constructor"));
    let mut seen_classes = BTreeSet::new();
    let mut ts = VecDeque::from([t.dupe()]);
    while let Some(t) = ts.pop_front() {
        for t in possible_concrete_types_for_inspection(cx, env, reason_of_t(&t), &t)? {
            match t.deref() {
                TypeInner::IntersectionT(_, rep) => ts.extend(rep.members_iter().duped()),
                TypeInner::DefT(_, def_t)
                    if let DefTInner::InstanceT(inst_t) = def_t.deref()
                        && seen_classes.insert(inst_t.inst.class_id.dupe()) =>
                {
                    if let Some(p) = cx.get_prop(inst_t.inst.own_props.dupe(), &name) {
                        return Ok(Some(p));
                    }
                    ts.push_back(inst_t.super_.dupe());
                }
                _ => {}
            }
        }
    }
    Ok(None)
}

pub(super) fn speculative_subtyping_succeeds<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    l: &Type,
    u: &Type,
) -> Result<bool, FlowJsException> {
    match speculation_kit::try_singleton_throw_on_failure(
        cx,
        env,
        DepthTrace::dummy_trace(),
        l.dupe(),
        UseT::new(UseTInner::UseT(unknown_use(), u.dupe())),
    ) {
        Ok(()) => Ok(true),
        Err(FlowJsException::SpeculationSingletonError) => Ok(false),
        Err(e) => Err(e),
    }
}

pub(super) fn speculative_subtyping_succeeds_non_speculating<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    l: &Type,
    u: &Type,
) -> Result<bool, flow_utils_concurrency::job_error::JobError> {
    match speculative_subtyping_succeeds(cx, env, l, u) {
        Ok(result) => Ok(result),
        Err(FlowJsException::SpeculationSingletonError) => Ok(false),
        Err(FlowJsException::WorkerCanceled(c)) => {
            Err(flow_utils_concurrency::job_error::JobError::Canceled(c))
        }
        Err(FlowJsException::TimedOut(t)) => {
            Err(flow_utils_concurrency::job_error::JobError::TimedOut(t))
        }
        Err(FlowJsException::DebugThrow { loc }) => {
            Err(flow_utils_concurrency::job_error::JobError::DebugThrow { loc })
        }
        // JobError intentionally only carries cancellation-style failures. Preserve the existing
        // compatibility behavior for callers that cannot propagate FlowJsException.
        Err(FlowJsException::Speculative(_)) | Err(FlowJsException::LimitExceeded) => Ok(false),
    }
}

// get prop

/// `prop` is the property the lookup found, with the language of the type that
/// declares it.
///
/// `up` is the upper property `propref` was looked up for. Only
/// [`LookupAction::LookupPropsForSubtyping`] needs it, since it carries several
/// upper properties at once; `dispatch::lookup_targets` pairs each of them with
/// the reference it is looked up under.
pub(super) fn perform_lookup_action<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    propref: &PropRef,
    p: &PropertyType,
    prop: Option<(&Property, TypeStrictnessKind)>,
    target_kind: PropertySource,
    l: &Type,
    ureason: &Reason,
    action: &LookupAction,
    up: Option<&Property>,
) -> Result<(), FlowJsException> {
    match action {
        LookupAction::LookupPropForTvarPopulation { tout, polarity } => {
            subtyping_kit::rec_flow_p(
                cx,
                env,
                Some(trace),
                unknown_use(),
                true,
                PropPolarityMismatchObject::Type(l),
                PropPolarityMismatchObject::Type(tout),
                propref,
                p,
                &PropertyType::OrdinaryField {
                    type_: tout.dupe(),
                    polarity: *polarity,
                },
            )?;
        }
        LookupAction::LookupPropsForSubtyping(box LookupPropsForSubtypingData {
            use_op,
            props: _,
            strictness_kind,
            lower,
            upper,
        }) => {
            if let Some(up) = up {
                let use_op = UseOp::Frame(
                    Arc::new(VirtualFrameUseOp::PropertyCompatibility(Box::new(
                        PropertyCompatibilityData {
                            prop: name_of_propref(propref),
                            lower: reason_of_t(lower).dupe(),
                            upper: reason_of_t(upper).dupe(),
                            lower_t: flow_js_utils::type_or_type_desc_for_error(lower),
                            upper_t: flow_js_utils::type_or_type_desc_for_error(upper),
                        },
                    ))),
                    Arc::new(use_op.dupe()),
                );
                let up_type = property::property_type(up);
                subtyping_kit::rec_flow_p_with_lower_upper_property(
                    cx,
                    env,
                    Some(trace),
                    use_op,
                    prop.map(|(lp, _)| (lp, up)),
                    *strictness_kind,
                    true,
                    PropPolarityMismatchObject::Type(l),
                    PropPolarityMismatchObject::Type(upper),
                    propref,
                    p,
                    &up_type,
                )?;
            }
        }
        LookupAction::SuperProp(box (use_op, lp, strictness_kind, derived)) => {
            let lp_type = property::property_type(lp);
            let strictness_kind = match prop {
                Some((_, declared_strictness_kind)) => {
                    strictness_kind.join(declared_strictness_kind)
                }
                None => *strictness_kind,
            };
            let derived = match derived {
                DerivedReference::Named(name) => {
                    PropPolarityMismatchObject::Class(NamedReferenceData {
                        loc: ureason.loc().dupe(),
                        name: name.dupe(),
                    })
                }
                DerivedReference::Unnamed(t) => PropPolarityMismatchObject::Type(t),
            };
            subtyping_kit::rec_flow_p_with_lower_upper_property(
                cx,
                env,
                Some(trace),
                use_op.dupe(),
                prop.map(|(up, _)| (lp, up)),
                strictness_kind,
                true,
                derived,
                PropPolarityMismatchObject::Type(l),
                propref,
                &lp_type,
                p,
            )?;
        }
        LookupAction::ReadProp(box ReadPropData {
            use_op,
            obj_t,
            tout,
        }) => {
            let react_dro = match obj_t.deref() {
                _ if constraint_node_id(obj_t).is_some() => panic!("Expected concrete type"),
                TypeInner::DefT(_, def_t) if let DefTInner::InstanceT(inst) = def_t.deref() => {
                    inst.inst.inst_react_dro.clone()
                }
                TypeInner::DefT(_, def_t) if let DefTInner::ObjT(o) = def_t.deref() => {
                    o.flags.react_dro.clone()
                }
                _ => None,
            };
            let dro = react_dro.map(|ReactDro(loc, dro_type)| (loc, dro_type));
            FlowJs::perform_read_prop_action(
                cx,
                env,
                trace,
                use_op.dupe(),
                propref,
                p,
                ureason,
                dro,
                tout,
            )?;
        }
        LookupAction::WriteProp(box WritePropData {
            use_op,
            obj_t: _,
            tin,
            write_ctx,
            prop_tout,
            mode,
        }) => {
            match (
                property::write_t_of_property_type(p, Some(*write_ctx)),
                target_kind,
                mode,
            ) {
                (Some(t), PropertySource::IndexerProperty, SetMode::Delete) => {
                    // Always OK to delete a property we found via an indexer
                    let void = void::why(reason_of_t(&t).dupe());
                    if let Some(prop_tout) = prop_tout {
                        rec_flow_t(cx, env, trace, unknown_use(), (&void, prop_tout))?;
                    }
                }
                (Some(t), _, _) => {
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (tin, &UseT::new(UseTInner::UseT(use_op.dupe(), t.dupe()))),
                    )?;
                    if let Some(prop_tout) = prop_tout {
                        rec_flow_t(cx, env, trace, unknown_use(), (&t, prop_tout))?;
                    }
                }
                (None, _, _) => {
                    let reason_prop = reason_of_propref(propref);
                    let prop_name = name_of_propref(propref);
                    let msg = ErrorMessage::EPropNotWritable(Box::new(EPropNotWritableData {
                        prop_loc: reason_prop.loc().dupe(),
                        prop_name,
                        use_op: use_op.dupe(),
                    }));
                    flow_js_utils::add_output_with_env(cx, env, msg)?;
                }
            }
        }
        LookupAction::MatchProp(box LookupActionMatchPropData {
            use_op,
            drop_generic: drop_generic_,
            prop_t: tin,
        }) => match property::read_t_of_property_type(p) {
            Some(t) => {
                let t = if *drop_generic_ { drop_generic(t) } else { t };
                rec_flow(
                    cx,
                    env,
                    trace,
                    (tin, &UseT::new(UseTInner::UseT(use_op.dupe(), t))),
                )?;
            }
            None => {
                let reason_prop = reason_of_propref(propref);
                let prop_name = name_of_propref(propref);
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::EPropNotReadable(Box::new(EPropNotReadableData {
                        prop_loc: reason_prop.loc().dupe(),
                        prop_name,
                        use_op: use_op.dupe(),
                    })),
                )?;
            }
        },
    }
    Ok(())
}

pub(super) fn mk_react_dro<'cx>(
    _cx: &mut Context<'cx>,
    use_op: UseOp,
    dro: ReactDro,
    t: Type,
) -> Type {
    let id = eval::Id::generate_id();
    let reason = type_util::reason_of_t(&t).dupe();
    Type::new(TypeInner::EvalT {
        type_: t,
        defer_use_t: TypeDestructorT::new(TypeDestructorTInner(
            use_op,
            reason,
            Rc::new(Destructor::ReactDRO(Box::new(dro))),
        )),
        id,
    })
}

/// Look up a key's value type directly in a property map. Returns Some type_ if the
/// key is a string literal, the property exists, and is readable; None otherwise.
/// Applies react_dro wrapping if needed.
pub(super) fn lookup_prop_type_direct<'cx>(
    cx: &mut Context<'cx>,
    use_op: &UseOp,
    react_dro: &Option<ReactDro>,
    props: &properties::PropertiesMap,
    key_type: &Type,
) -> Option<Type> {
    let name = type_util::string_of_singleton_string_type(key_type)?;
    let prop = props.get(&Name::new(name.dupe()))?;
    match property::property_type(prop) {
        PropertyType::OrdinaryField { type_, polarity }
            if Polarity::compat(polarity, Polarity::Positive) =>
        {
            let type_ = match react_dro {
                Some(dro) => {
                    let frame_use_op = VirtualUseOp::Frame(
                        Arc::new(FrameUseOp::ReactDeepReadOnly(Box::new((
                            dro.0.dupe(),
                            dro.1.clone(),
                        )))),
                        Arc::new(use_op.dupe()),
                    );
                    mk_react_dro(cx, frame_use_op, dro.clone(), type_)
                }
                None => type_,
            };
            Some(type_)
        }
        _ => None,
    }
}

/// Returns true when __flow should succeed immediately if EmptyT flows into u.
pub(super) fn empty_success(u: &UseT<Cx>) -> bool {
    match u.deref() {
        // Work has to happen when Empty flows to these types
        UseTInner::ConcretizeT(box ConcretizeTData {
            kind: ConcretizationKind::ConcretizeForLowerBounds,
            ..
        }) => false,
        UseTInner::UseT(_, t) if type_util::constraint_node_id(t).is_some() => false,
        UseTInner::EvalTypeDestructorT(..) => false,
        UseTInner::UseT(_, t)
            if matches!(
                t.deref(),
                TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::TypeT(..))
            ) =>
        {
            false
        }
        UseTInner::CondT(..) => false,
        UseTInner::ConditionalT(..) => false,
        UseTInner::FilterMaybeT(..) => false,
        UseTInner::ObjKitT(..) => false,
        UseTInner::OptionalIndexedAccessT(..) => false,
        UseTInner::ReposLowerT { .. } => false,
        UseTInner::ReposUseT(..) => false,
        UseTInner::SealGenericT(..) => false,
        UseTInner::ResolveUnionT(..) => false,
        UseTInner::ConvertEmptyPropsToMixedT(..) => false,
        UseTInner::HooklikeT(..) => false,
        UseTInner::SpecializeT(..) => false,
        UseTInner::ValueToTypeReferenceT(..) => false,
        _ => true,
    }
}

fn is_concrete(t: &Type) -> bool {
    if type_util::constraint_node_id(t).is_some() {
        return false;
    }

    match t.deref() {
        TypeInner::EvalT { .. }
        | TypeInner::AnnotT(..)
        | TypeInner::MaybeT(..)
        | TypeInner::OptionalT { .. }
        | TypeInner::TypeAppT(..)
        | TypeInner::ThisTypeAppT(..) => false,
        _ => true,
    }
}

pub(super) fn handle_generic<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    no_infer: bool,
    bound: &Type,
    reason: &Reason,
    id: &GenericId,
    name: &SubstName,
    u: &UseT<Cx>,
) -> Result<bool, FlowJsException> {
    let make_generic = |t: Type| -> Type {
        Type::new(TypeInner::GenericT(Box::new(GenericTData {
            reason: reason.dupe(),
            id: id.clone(),
            name: name.dupe(),
            bound: t,
            no_infer,
        })))
    };
    let narrow_generic_with_continuation = |cx: &mut Context<'cx>,
                                            mk_use_t: &dyn Fn(Tvar) -> UseT<Cx>,
                                            cont: Cont<Cx>|
     -> Result<(), FlowJsException> {
        let tvar_id = flow_typing_tvar::mk_no_wrap(cx, reason);
        let t_out_prime = Tvar::new(reason.dupe(), tvar_id as u32);
        let use_t = mk_use_t(t_out_prime.dupe());
        let repos_bound = reposition_reason(cx, env, Some(trace), reason, false, bound)?;
        rec_flow(cx, env, trace, (&repos_bound, &use_t))?;
        let open_t = Type::new(TypeInner::OpenT(t_out_prime));
        let seal = UseT::new(UseTInner::SealGenericT(Box::new(SealGenericTData {
            loc: reason.loc().dupe(),
            id: id.clone(),
            name: name.dupe(),
            cont,
            no_infer,
        })));
        rec_flow(cx, env, trace, (&open_t, &seal))
    };
    let narrow_generic_use = |cx: &mut Context<'cx>,
                              mk_use_t: &dyn Fn(Tvar) -> UseT<Cx>,
                              use_t_out: UseT<Cx>|
     -> Result<(), FlowJsException> {
        narrow_generic_with_continuation(cx, mk_use_t, Cont::Upper(Box::new(use_t_out)))
    };
    let narrow_generic = |cx: &mut Context<'cx>,
                          use_op: Option<UseOp>,
                          mk_use_t: &dyn Fn(Type) -> UseT<Cx>,
                          t_out: &Type|
     -> Result<(), FlowJsException> {
        let use_op = use_op.unwrap_or_else(unknown_use);
        let mk_use_t_wrapper = |v: Tvar| -> UseT<Cx> { mk_use_t(Type::new(TypeInner::OpenT(v))) };
        narrow_generic_use(
            cx,
            &mk_use_t_wrapper,
            UseT::new(UseTInner::UseT(use_op, t_out.dupe())),
        )
    };
    let narrow_generic_tvar = |cx: &mut Context<'cx>,
                               use_op: Option<UseOp>,
                               mk_use_t: &dyn Fn(Tvar) -> UseT<Cx>,
                               t_out: &Tvar|
     -> Result<(), FlowJsException> {
        let use_op = use_op.unwrap_or_else(unknown_use);
        narrow_generic_use(
            cx,
            mk_use_t,
            UseT::new(UseTInner::UseT(
                use_op,
                Type::new(TypeInner::OpenT(t_out.dupe())),
            )),
        )
    };
    let wait_for_concrete_bound =
        |cx: &mut Context<'cx>, upper: Option<&UseT<Cx>>| -> Result<bool, FlowJsException> {
            let upper = upper.unwrap_or(u);
            let repos_bound = reposition_reason(cx, env, Some(trace), reason, false, bound)?;
            let seal = UseT::new(UseTInner::SealGenericT(Box::new(SealGenericTData {
                loc: reason.loc().dupe(),
                id: id.clone(),
                name: name.dupe(),
                cont: Cont::Upper(Box::new(upper.dupe())),
                no_infer,
            })));
            rec_flow(cx, env, trace, (&repos_bound, &seal))?;
            Ok(true)
        };
    let distribute_union_intersection = |cx: &mut Context<'cx>,
                                         upper: Option<&UseT<Cx>>|
     -> Result<bool, FlowJsException> {
        let upper = upper.unwrap_or(u);
        match bound.deref() {
            TypeInner::UnionT(_, rep) => {
                let mut members = rep.members_iter();
                let t1 = members.next().unwrap();
                let t2 = members.next().unwrap();
                let ts: Vec<Type> = members.duped().collect();
                let union_of_generics = union_rep::make(
                    None,
                    rep.union_kind(),
                    make_generic(t1.dupe()),
                    make_generic(t2.dupe()),
                    ts.into_iter().map(make_generic).collect::<Rc<[_]>>(),
                );
                let union_t = Type::new(TypeInner::UnionT(reason.dupe(), union_of_generics));
                rec_flow(cx, env, trace, (&union_t, upper))?;
                Ok(true)
            }
            TypeInner::IntersectionT(_, rep) => {
                let mut members = rep.members_iter();
                let t1 = members.next().unwrap();
                let t2 = members.next().unwrap();
                let ts: Vec<Type> = members.duped().collect();
                let inter_of_generics = inter_rep::make(
                    make_generic(t1.dupe()),
                    make_generic(t2.dupe()),
                    ts.into_iter().map(make_generic).collect::<Rc<[_]>>(),
                );
                let inter_t = Type::new(TypeInner::IntersectionT(reason.dupe(), inter_of_generics));
                rec_flow(cx, env, trace, (&inter_t, upper))?;
                Ok(true)
            }
            _ => Ok(false),
        }
    };

    let update_action_meth_generic_this =
        |l: Type, action: &MethodAction<Cx>| -> MethodAction<Cx> {
            match action {
                MethodAction::CallM(box CallMData {
                    methodcalltype,
                    return_hint,
                    specialized_callee,
                }) => MethodAction::CallM(Box::new(CallMData {
                    methodcalltype: MethodCallType {
                        meth_generic_this: Some(l),
                        ..methodcalltype.clone()
                    },
                    return_hint: return_hint.clone(),
                    specialized_callee: specialized_callee.clone(),
                })),
                MethodAction::ChainM(box ChainMData {
                    exp_reason,
                    lhs_reason,
                    lhs_expression,
                    methodcalltype,
                    voided_out_collector,
                    return_hint,
                    specialized_callee,
                }) => MethodAction::ChainM(Box::new(ChainMData {
                    exp_reason: exp_reason.dupe(),
                    lhs_reason: lhs_reason.dupe(),
                    lhs_expression: lhs_expression.dupe(),
                    methodcalltype: MethodCallType {
                        meth_generic_this: Some(l),
                        ..methodcalltype.clone()
                    },
                    voided_out_collector: voided_out_collector.dupe(),
                    return_hint: return_hint.clone(),
                    specialized_callee: specialized_callee.clone(),
                })),
                MethodAction::NoMethodAction(t) => MethodAction::NoMethodAction(t.dupe()),
            }
        };

    if match bound.deref() {
        TypeInner::GenericT(box GenericTData {
            bound: inner_bound,
            id: id_prime,
            no_infer: inner_no_infer,
            ..
        }) => match id.collapse(id_prime) {
            Some(collapsed_id) => {
                let generic_t = Type::new(TypeInner::GenericT(Box::new(GenericTData {
                    reason: reason.dupe(),
                    name: name.dupe(),
                    bound: inner_bound.dupe(),
                    id: collapsed_id,
                    no_infer: *inner_no_infer,
                })));
                rec_flow(cx, env, trace, (&generic_t, u))?;
                true
            }
            None => false,
        },
        // The ClassT operation should commute with GenericT;
        // that is, GenericT(ClassT(x)) = ClassT(GenericT(x))
        TypeInner::DefT(r, def_t) if let DefTInner::ClassT(class_bound) = def_t.deref() => {
            let inner_generic = Type::new(TypeInner::GenericT(Box::new(GenericTData {
                reason: reason_of_t(class_bound).dupe(),
                name: name.dupe(),
                bound: class_bound.dupe(),
                id: id.clone(),
                no_infer,
            })));
            let class_t = Type::new(TypeInner::DefT(
                r.dupe(),
                DefT::new(DefTInner::ClassT(inner_generic)),
            ));
            rec_flow(cx, env, trace, (&class_t, u))?;
            true
        }
        TypeInner::KeysT(..) => {
            let repos_bound = reposition_reason(cx, env, Some(trace), reason, false, bound)?;
            let seal = UseT::new(UseTInner::SealGenericT(Box::new(SealGenericTData {
                loc: reason.loc().dupe(),
                id: id.clone(),
                name: name.dupe(),
                no_infer,
                cont: Cont::Upper(Box::new(u.dupe())),
            })));
            rec_flow(cx, env, trace, (&repos_bound, &seal))?;
            true
        }
        TypeInner::DefT(_, def_t) if matches!(&**def_t, DefTInner::EmptyT) => empty_success(u),
        _ => false,
    } {
        return Ok(true);
    }

    match u.deref() {
        // In this set of cases, we flow the generic's upper bound to u. This is what we normally would do
        // in the catch-all generic case anyways, but these rules are to avoid wildcards elsewhere in __flow.
        UseTInner::ConcretizeT(box ConcretizeTData {
            kind: ConcretizationKind::ConcretizeForOperatorsChecking,
            ..
        })
        | UseTInner::ConcretizeT(box ConcretizeTData {
            kind: ConcretizationKind::ConcretizeForComputedObjectKeys,
            ..
        })
        | UseTInner::ConcretizeT(box ConcretizeTData {
            kind: ConcretizationKind::ConcretizeForOptionalChain,
            ..
        })
        | UseTInner::TestPropT(..)
        | UseTInner::OptionalIndexedAccessT(..) => {
            let repos_bound = reposition_reason(cx, env, Some(trace), reason, false, bound)?;
            rec_flow(cx, env, trace, (&repos_bound, u))?;
            Ok(true)
        }
        UseTInner::UseT(use_op, t)
            if matches!(
                use_op,
                VirtualUseOp::Op(root) if matches!(&**root, RootUseOp::Coercion { .. })
            ) && matches!(
                t.deref(),
                TypeInner::DefT(_, def_t) if matches!(&**def_t, DefTInner::StrGeneralT(_) | DefTInner::SingletonStrT { .. })
            ) =>
        {
            let repos_bound = reposition_reason(cx, env, Some(trace), reason, false, bound)?;
            rec_flow(cx, env, trace, (&repos_bound, u))?;
            Ok(true)
        }
        UseTInner::ReactKitT(..) => {
            if is_concrete(bound) {
                distribute_union_intersection(cx, None)
            } else {
                wait_for_concrete_bound(cx, None)
            }
        }
        UseTInner::ToStringT {
            orig_t,
            reason: to_string_reason,
            t_out,
        } => {
            let orig_t = orig_t.dupe();
            let to_string_reason = to_string_reason.dupe();
            narrow_generic_use(
                cx,
                &|t_out_prime: Tvar| -> UseT<Cx> {
                    UseT::new(UseTInner::ToStringT {
                        orig_t: orig_t.dupe(),
                        reason: to_string_reason.dupe(),
                        t_out: Box::new(UseT::new(UseTInner::UseT(
                            unknown_use(),
                            Type::new(TypeInner::OpenT(t_out_prime)),
                        ))),
                    })
                },
                t_out.as_ref().dupe(),
            )?;
            Ok(true)
        }
        // A generic indexer key stays generic in the key set, the same way it
        // does under the `ToStringT` that a non-symbol key ends up in.
        UseTInner::GetKeysDictKeyT {
            reason: keys_reason,
            t_out,
            include_symbols,
        } => {
            let keys_reason = keys_reason.dupe();
            let include_symbols = *include_symbols;
            narrow_generic_use(
                cx,
                &|t_out_prime: Tvar| -> UseT<Cx> {
                    UseT::new(UseTInner::GetKeysDictKeyT {
                        reason: keys_reason.dupe(),
                        t_out: Box::new(UseT::new(UseTInner::UseT(
                            unknown_use(),
                            Type::new(TypeInner::OpenT(t_out_prime)),
                        ))),
                        include_symbols,
                    })
                },
                t_out.as_ref().dupe(),
            )?;
            Ok(true)
        }
        UseTInner::UseT(use_op, t) if let TypeInner::MaybeT(r, t_out) = t.deref() => {
            let use_op_cloned = use_op.dupe();
            let r = r.dupe();
            narrow_generic(
                cx,
                Some(use_op.dupe()),
                &|t_out_prime: Type| -> UseT<Cx> {
                    UseT::new(UseTInner::UseT(
                        use_op_cloned.dupe(),
                        Type::new(TypeInner::MaybeT(r.dupe(), t_out_prime)),
                    ))
                },
                t_out,
            )?;
            Ok(true)
        }
        UseTInner::UseT(use_op, t)
            if let TypeInner::OptionalT {
                reason: opt_reason,
                type_: t_out,
                use_desc: opt_use_desc,
            } = t.deref() =>
        {
            let use_op_cloned = use_op.dupe();
            let opt_reason = opt_reason.dupe();
            let opt_use_desc = *opt_use_desc;
            narrow_generic(
                cx,
                Some(use_op.dupe()),
                &|t_out_prime: Type| -> UseT<Cx> {
                    UseT::new(UseTInner::UseT(
                        use_op_cloned.dupe(),
                        Type::new(TypeInner::OptionalT {
                            reason: opt_reason.dupe(),
                            type_: t_out_prime,
                            use_desc: opt_use_desc,
                        }),
                    ))
                },
                t_out,
            )?;
            Ok(true)
        }
        UseTInner::DeepReadOnlyT(t_out, dro) => {
            let dro = dro.clone();
            narrow_generic_tvar(
                cx,
                None,
                &|t_out_prime: Tvar| -> UseT<Cx> {
                    UseT::new(UseTInner::DeepReadOnlyT(Box::new(t_out_prime), dro.clone()))
                },
                t_out,
            )?;
            Ok(true)
        }
        UseTInner::HooklikeT(t_out) => {
            narrow_generic_tvar(
                cx,
                None,
                &|t_out_prime: Tvar| -> UseT<Cx> {
                    UseT::new(UseTInner::HooklikeT(Box::new(t_out_prime)))
                },
                t_out,
            )?;
            Ok(true)
        }
        UseTInner::FilterMaybeT(use_op, t_out) => {
            let use_op = use_op.dupe();
            narrow_generic(
                cx,
                None,
                &|t_out_prime: Type| -> UseT<Cx> {
                    UseT::new(UseTInner::FilterMaybeT(use_op.dupe(), t_out_prime))
                },
                t_out,
            )?;
            Ok(true)
        }
        UseTInner::FilterOptionalT(use_op, t_out) => {
            let use_op = use_op.dupe();
            narrow_generic(
                cx,
                None,
                &|t_out_prime: Type| -> UseT<Cx> {
                    UseT::new(UseTInner::FilterOptionalT(use_op.dupe(), t_out_prime))
                },
                t_out,
            )?;
            Ok(true)
        }
        UseTInner::ObjRestT(r, xs, t_out, obj_rest_id, literal) => {
            let r = r.dupe();
            let xs = xs.clone();
            let obj_rest_id = *obj_rest_id;
            let literal = *literal;
            narrow_generic(
                cx,
                None,
                &|t_out_prime: Type| -> UseT<Cx> {
                    UseT::new(UseTInner::ObjRestT(
                        r.dupe(),
                        xs.clone(),
                        t_out_prime,
                        obj_rest_id,
                        literal,
                    ))
                },
                t_out,
            )?;
            Ok(true)
        }
        // Support `new this.constructor ()`
        UseTInner::GetPropT(data)
            if let PropRef::Named {
                reason: prop_reason,
                name: prop_name,
                ..
            } = data.propref.as_ref()
                && prop_name.matches_str("constructor") =>
        {
            if is_concrete(bound) {
                match bound.deref() {
                    TypeInner::DefT(_, def_t) if matches!(&**def_t, DefTInner::InstanceT(_)) => {
                        let get_use_op = data.use_op.dupe();
                        let reason_op = data.reason.dupe();
                        let get_id = data.id;
                        let from_annot = data.from_annot;
                        let skip_optional = data.skip_optional;
                        let prop_reason = prop_reason.dupe();
                        let prop_name = prop_name.dupe();
                        let hint = data.hint.clone();
                        narrow_generic_tvar(
                            cx,
                            None,
                            &|tout_prime: Tvar| -> UseT<Cx> {
                                UseT::new(UseTInner::GetPropT(Box::new(GetPropTData {
                                    use_op: get_use_op.dupe(),
                                    reason: reason_op.dupe(),
                                    id: get_id,
                                    from_annot,
                                    skip_optional,
                                    propref: Box::new(type_util::mk_named_prop(
                                        prop_reason.dupe(),
                                        false,
                                        prop_name.dupe(),
                                    )),
                                    tout: Box::new(tout_prime),
                                    hint: hint.clone(),
                                })))
                            },
                            &data.tout,
                        )?;
                        Ok(true)
                    }
                    _ => Ok(false),
                }
            } else {
                wait_for_concrete_bound(cx, None)
            }
        }
        UseTInner::ConstructorT(data) => {
            if is_concrete(bound) {
                let has_construct_sig = match bound.deref() {
                    TypeInner::DefT(_, def_t)
                        if matches!(&**def_t, DefTInner::ClassT(_) | DefTInner::InstanceT(_)) =>
                    {
                        true
                    }
                    _ => false,
                };
                if has_construct_sig {
                    let ctor_use_op = data.use_op.dupe();
                    let reason_op = data.reason.dupe();
                    let targs = data.targs.clone();
                    let args = data.args.clone();
                    let return_hint = data.return_hint.clone();
                    let specialized_ctor = data.specialized_ctor.clone();
                    narrow_generic(
                        cx,
                        None,
                        &|tout_prime: Type| -> UseT<Cx> {
                            UseT::new(UseTInner::ConstructorT(Box::new(ConstructorTData {
                                use_op: ctor_use_op.dupe(),
                                reason: reason_op.dupe(),
                                targs: targs.clone(),
                                args: args.clone(),
                                tout: tout_prime,
                                return_hint: return_hint.clone(),
                                specialized_ctor: specialized_ctor.clone(),
                            })))
                        },
                        &data.tout,
                    )?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            } else {
                wait_for_concrete_bound(cx, None)
            }
        }
        UseTInner::ElemT(..) => {
            if is_concrete(bound) {
                distribute_union_intersection(cx, None)
            } else {
                wait_for_concrete_bound(cx, None)
            }
        }
        UseTInner::MethodT(box MethodTData {
            use_op: op,
            reason: r1,
            prop_reason: r2,
            propref: prop,
            method_action: action,
        }) => {
            let l = make_generic(bound.dupe());
            let action_prime = update_action_meth_generic_this(l, action);
            let u_prime = UseT::new(UseTInner::MethodT(Box::new(MethodTData {
                use_op: op.dupe(),
                reason: r1.dupe(),
                prop_reason: r2.dupe(),
                propref: prop.clone(),
                method_action: Box::new(action_prime),
            })));
            let consumed = {
                if is_concrete(bound) {
                    distribute_union_intersection(cx, Some(&u_prime))?
                } else {
                    wait_for_concrete_bound(cx, Some(&u_prime))?
                }
            };
            if !consumed {
                let repos_bound = reposition_reason(cx, env, Some(trace), reason, false, bound)?;
                rec_flow(cx, env, trace, (&repos_bound, &u_prime))?;
            }
            Ok(true)
        }
        UseTInner::PrivateMethodT(data) => {
            let l = make_generic(bound.dupe());
            let action_prime = update_action_meth_generic_this(l, &data.method_action);
            let u_prime = UseT::new(UseTInner::PrivateMethodT(Box::new(PrivateMethodTData {
                use_op: data.use_op.dupe(),
                reason: data.reason.dupe(),
                prop_reason: data.prop_reason.dupe(),
                name: data.name.dupe(),
                class_bindings: data.class_bindings.dupe(),
                static_: data.static_,
                method_action: Box::new(action_prime),
            })));
            let consumed = {
                if is_concrete(bound) {
                    distribute_union_intersection(cx, Some(&u_prime))?
                } else {
                    wait_for_concrete_bound(cx, Some(&u_prime))?
                }
            };
            if !consumed {
                let repos_bound = reposition_reason(cx, env, Some(trace), reason, false, bound)?;
                rec_flow(cx, env, trace, (&repos_bound, &u_prime))?;
            }
            Ok(true)
        }
        UseTInner::ObjKitT(..) => {
            if is_concrete(bound) {
                distribute_union_intersection(cx, None)
            } else {
                wait_for_concrete_bound(cx, None)
            }
        }
        UseTInner::UseT(_, t) if matches!(t.deref(), TypeInner::IntersectionT(..)) => {
            if is_concrete(bound) {
                distribute_union_intersection(cx, None)
            } else {
                wait_for_concrete_bound(cx, None)
            }
        }
        UseTInner::UseT(_, union_t) if matches!(union_t.deref(), TypeInner::UnionT(..)) => {
            if matches!(
                flow_js_utils::union_optimization_guard(
                    cx,
                    |t1, t2| type_util::quick_subtype(None::<&mut fn(&Type)>, t1, t2),
                    bound,
                    union_t,
                ),
                UnionOptimizationGuardResult::True
            ) {
                if cx.is_verbose() {
                    eprintln!("UnionT ~> UnionT fast path (via a generic)");
                }
                Ok(true)
            } else if is_concrete(bound) {
                distribute_union_intersection(cx, None)
            } else {
                wait_for_concrete_bound(cx, None)
            }
        }
        UseTInner::UseT(_, t) if matches!(t.deref(), TypeInner::KeysT(..)) => {
            if is_concrete(bound) {
                Ok(false)
            } else {
                wait_for_concrete_bound(cx, None)
            }
        }
        UseTInner::EvalTypeDestructorT(..) => {
            if is_concrete(bound) {
                Ok(false)
            } else {
                wait_for_concrete_bound(cx, None)
            }
        }
        UseTInner::ResolveSpreadT(..) if !is_concrete(bound) => wait_for_concrete_bound(cx, None),
        _ => Ok(false),
    }
}

/// A tvar of an enclosing implicit instantiation is frozen in a nested one: it
/// cannot be solved here, but the enclosing solve, where it is active again,
/// must still see it in what is computed here. So it is rigid like a generic:
/// a use sees it through its bound, and what a type destructor or the object
/// kit computes from it is a frozen tvar derived from it.
///
/// A placeholder is frozen in every solve. What a type destructor or the object
/// kit computes from it is a placeholder too, and every other use sees it as its
/// permissive instantiation, as it would see `any`.
pub(super) fn handle_frozen_tvar<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    l: &Type,
    tvar: &ImplicitInstantiationTvarData,
    u: &UseT<Cx>,
) -> Result<bool, FlowJsException> {
    let ImplicitInstantiationTvarData {
        reason,
        bound,
        id,
        kind,
        ..
    } = tvar;
    let derive = |cx: &mut Context<'cx>,
                  use_reason: &Reason,
                  mk_use_t: &dyn Fn(Tvar) -> UseT<Cx>,
                  use_op: UseOp,
                  tout: &Type|
     -> Result<bool, FlowJsException> {
        match kind {
            // The derived placeholder is bounded by what the use computes from the bound.
            // Whether the use holds for the placeholder is left to the final check, so a use
            // that fails on the bound computes a placeholder bounded by `mixed`.
            ImplicitInstantiationTvarKind::Placeholder => {
                let rigid = Tvar::new(
                    use_reason.dupe(),
                    flow_typing_tvar::mk_no_wrap(cx, use_reason) as u32,
                );
                let repos_bound = reposition_reason(cx, env, Some(trace), reason, false, bound)?;
                let bound = match speculation_kit::try_singleton_throw_on_failure(
                    cx,
                    env,
                    trace,
                    repos_bound,
                    mk_use_t(rigid.dupe()),
                ) {
                    Ok(()) => Type::new(TypeInner::OpenT(rigid)),
                    Err(FlowJsException::SpeculationSingletonError) => {
                        mixed_t::make(use_reason.dupe())
                    }
                    Err(e) => return Err(e),
                };
                let derived = cx.mk_placeholder(use_reason.dupe(), bound);
                rec_flow_t(cx, env, trace, use_op, (&derived, tout))?;
                Ok(true)
            }
            // The derived tvar is owned by the solve owning this one. Here it is bounded
            // by what the use computes from the bound. In the owning solve it is what
            // the use computes from this tvar, so that solve infers through it.
            ImplicitInstantiationTvarKind::Inferred => {
                let root_id = cx.find_constraints(*id).0;
                let mut is_tvar = |frozen_id: i32| cx.find_constraints(frozen_id).0 == root_id;
                let owner_env = env.implicit_instantiation_owner(&mut is_tvar);
                let frozen = env.frozen_implicit_instantiation_tvar_with_default(&mut is_tvar);
                let (Some(owner_env), Some((_, has_default))) = (owner_env, frozen) else {
                    return Ok(false);
                };
                let rigid = Tvar::new(
                    use_reason.dupe(),
                    flow_typing_tvar::mk_no_wrap(cx, use_reason) as u32,
                );
                let repos_bound = reposition_reason(cx, env, Some(trace), reason, false, bound)?;
                rec_flow(cx, env, trace, (&repos_bound, &mk_use_t(rigid.dupe())))?;
                let thawed = Tvar::new(
                    use_reason.dupe(),
                    flow_typing_tvar::mk_no_wrap(cx, use_reason) as u32,
                );
                rec_flow(cx, &owner_env, trace, (l, &mk_use_t(thawed.dupe())))?;
                let derived_id = flow_common::reason::mk_id() as i32;
                cx.add_inference_node(derived_id);
                let derived = Type::new(TypeInner::ImplicitInstantiationTvar(Box::new(
                    ImplicitInstantiationTvarData {
                        reason: use_reason.dupe(),
                        name: SubstName::name(
                            use_reason
                                .desc(true)
                                .map(flow_common::reason::string_of_desc)
                                .unwrap_or_default()
                                .into(),
                        ),
                        bound: Type::new(TypeInner::OpenT(rigid)),
                        id: derived_id,
                        kind: ImplicitInstantiationTvarKind::Inferred,
                    },
                )));
                rec_unify(
                    cx,
                    &owner_env,
                    trace,
                    unknown_use(),
                    UnifyCause::Uncategorized,
                    None,
                    &derived,
                    &Type::new(TypeInner::OpenT(thawed)),
                )?;
                owner_env.add_implicit_instantiation_tvar(derived_id, derived.dupe(), has_default);
                rec_flow_t(cx, env, trace, use_op, (&derived, tout))?;
                Ok(true)
            }
        }
    };
    match (kind, u.deref()) {
        // Whether a conditional holds for the bound says nothing about the tvar.
        // `run_conditional` decides one over a frozen tvar itself.
        (
            _,
            UseTInner::EvalTypeDestructorT(box EvalTypeDestructorTData {
                destructor_use_op,
                reason: use_reason,
                repos,
                destructor,
                tout,
            }),
        ) if !matches!(**destructor, Destructor::ConditionalType(_)) => derive(
            cx,
            use_reason,
            &|tout| {
                UseT::new(UseTInner::EvalTypeDestructorT(Box::new(
                    EvalTypeDestructorTData {
                        destructor_use_op: destructor_use_op.dupe(),
                        reason: use_reason.dupe(),
                        repos: repos.clone(),
                        destructor: destructor.clone(),
                        tout: Box::new(tout),
                    },
                )))
            },
            destructor_use_op.dupe(),
            &Type::new(TypeInner::OpenT((**tout).dupe())),
        ),
        (_, UseTInner::ObjKitT(use_op, reason_op, resolve_tool, tool, tout)) => derive(
            cx,
            reason_op,
            &|tout| {
                UseT::new(UseTInner::ObjKitT(
                    use_op.dupe(),
                    reason_op.dupe(),
                    resolve_tool.clone(),
                    tool.clone(),
                    Type::new(TypeInner::OpenT(tout)),
                ))
            },
            use_op.dupe(),
            tout,
        ),
        (ImplicitInstantiationTvarKind::Placeholder, _) if !env.restrictive_frozen_tvars() => {
            any_propagated(cx, env, trace, l, u)
        }
        // A restrictive check sees a placeholder as it sees a frozen tvar. These uses would
        // otherwise be caught by the cases for `any` in __flow.
        (
            ImplicitInstantiationTvarKind::Placeholder,
            UseTInner::HasOwnPropT(..)
            | UseTInner::GetKeysT { .. }
            | UseTInner::GetValuesT(..)
            | UseTInner::GetDictValuesT(..)
            | UseTInner::ResolveSpreadT(..)
            | UseTInner::MixinT(..)
            | UseTInner::SpecializeT(..)
            | UseTInner::ThisSpecializeT(..)
            | UseTInner::CallT(..)
            | UseTInner::ObjTestProtoT(..)
            | UseTInner::MethodT(..)
            | UseTInner::PrivateMethodT(..)
            | UseTInner::GetStaticsT(..)
            | UseTInner::GetProtoT(..)
            | UseTInner::ObjRestT(..)
            | UseTInner::LookupT(..)
            | UseTInner::SetPropT(..)
            | UseTInner::GetPropT(..)
            | UseTInner::GetPrivatePropT(..)
            | UseTInner::SetElemT(..)
            | UseTInner::GetElemT(..)
            | UseTInner::CallElemT(..)
            | UseTInner::ElemT(..)
            | UseTInner::ArrRestT(..)
            | UseTInner::MapTypeT(..)
            | UseTInner::ExtendsUseT(..),
        )
        // These would otherwise be caught by wildcard cases in __flow. A generic
        // is seen through its bound here too, either by `handle_generic` or by
        // the react kit itself.
        | (
            _,
            UseTInner::ReactKitT(..)
            | UseTInner::ConstructorT(..)
            | UseTInner::ConcretizeT(box ConcretizeTData {
                kind:
                    ConcretizationKind::ConcretizeForOperatorsChecking
                    | ConcretizationKind::ConcretizeForComputedObjectKeys
                    | ConcretizationKind::ConcretizeForOptionalChain,
                ..
            })
            | UseTInner::TestPropT(..)
            | UseTInner::OptionalIndexedAccessT(..),
        ) => {
            let bound = env.frozen_tvar_bound(tvar);
            let repos_bound = reposition_reason(cx, env, Some(trace), reason, false, &bound)?;
            rec_flow(cx, env, trace, (&repos_bound, u))?;
            Ok(true)
        }
        (_, _) => Ok(false),
    }
}

/// Resolves one member of a union and either finishes the union flow or returns
/// the next continuation for the `__flow` trampoline.
///
/// `Ok(None)` means union resolution is complete: the resolved members have
/// been rebuilt into a union and flowed to `upper`.
///
/// `Ok(Some((next_env, next_l, next_u, reason)))` means there are still
/// unresolved members. `next_l` is the next union member to resolve, in
/// `next_env`. `next_u` is a `ResolveUnionT` continuation carrying the resolved
/// prefix, the remaining unresolved members, and the original `upper`. The
/// returned `reason` is the reason of the union being rebuilt; the trampoline
/// uses it as the diagnostic reason for this continuation if resolving the next
/// member hits the recursion limit.
pub(super) fn resolve_union<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    reason: &Reason,
    id: i32,
    resolved: &flow_data_structure_wrapper::list::FlowOcamlList<Type>,
    unresolved: &flow_data_structure_wrapper::list::FlowOcamlList<Type>,
    l: &Type,
    upper: &UseT<Cx>,
) -> Result<Option<(FlowJsEnv, Type, UseT<Cx>, Reason)>, FlowJsException> {
    let env = &env.pop_to_parts_marker(id);
    let do_continue = |cx: &mut Context<'cx>,
                       resolved: flow_data_structure_wrapper::list::FlowOcamlList<Type>|
     -> Result<Option<(FlowJsEnv, Type, UseT<Cx>, Reason)>, FlowJsException> {
        match unresolved.first() {
            None => {
                let ts: Vec<Type> = resolved.iter().duped().collect();
                let union_t = type_util::union_of_ts(reason.dupe(), ts, None);
                rec_flow(cx, env, trace, (&union_t, upper))?;
                Ok(None)
            }
            Some(next) => {
                // We intentionally do not rec_flow here. Unions can be very large, and resolving each
                // member under the same trace can cause a recursion limit error. To avoid that, we resolve
                // each member under their own trace
                let next = next.dupe();
                let mut rest = unresolved.dupe();
                rest.drop_first();
                Ok(Some((
                    env.dupe(),
                    next,
                    UseT::new(UseTInner::ResolveUnionT(Box::new(ResolveUnionTData {
                        reason: reason.dupe(),
                        resolved,
                        unresolved: rest,
                        upper: Box::new(upper.dupe()),
                        id,
                    }))),
                    reason.dupe(),
                )))
            }
        }
    };
    match l.deref() {
        TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::EmptyT) => {
            do_continue(cx, resolved.dupe())
        }
        _ => {
            let reason_elemt = reason_of_t(l).dupe();
            let pos = resolved.len() as i32;
            // Union resolution can fall prey to the same sort of infinite recursion that array
            // spreads can, so we can use the same constant folding guard logic that arrays do.
            // To more fully understand how that works, see the comment there
            const_fold_expansion::guard(cx, id, (reason_elemt, pos), |cx, count| {
                match count {
                    0 => {
                        let mut new_resolved = resolved.dupe();
                        new_resolved.push_front(l.dupe());
                        do_continue(cx, new_resolved)
                    }
                    // Unions are idempotent, so we can just skip any duplicated elements
                    1 => do_continue(cx, resolved.dupe()),
                    _ => Ok(None),
                }
            })
        }
    }
}

// filter out undefined from a type
pub(super) fn filter_optional<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    reason: &Reason,
    opt_t: &Type,
) -> Result<u32, FlowJsException> {
    let tvar = flow_typing_tvar::mk_no_wrap(cx, reason);
    flow_opt(
        cx,
        env,
        trace,
        (
            opt_t,
            &UseT::new(UseTInner::FilterOptionalT(
                unknown_use(),
                Type::new(TypeInner::OpenT(Tvar::new(reason.dupe(), tvar as u32))),
            )),
        ),
    )?;
    Ok(tvar as u32)
}

pub(super) fn pick_use_op(env: &FlowJsEnv, op1: &UseOp, op2: &UseOp) -> UseOp {
    use flow_typing_type::type_::FrameUseOp;
    use flow_typing_type::type_::FunCallData;
    use flow_typing_type::type_::FunCallMethodData;
    use flow_typing_type::type_::RootUseOp;
    use flow_typing_type::type_::VirtualFrameUseOp;
    use flow_typing_type::type_::VirtualRootUseOp;
    use flow_typing_type::type_::fold_use_op;
    use flow_typing_type::type_::root_of_use_op;

    let ignore_root_virtual = |root: &RootUseOp| -> bool {
        match root {
            VirtualRootUseOp::UnknownUse => true,
            VirtualRootUseOp::Speculation { .. } => {
                // If we are speculating then a Speculation use_op should be considered
                // "opaque". If we are not speculating then Speculation use_ops that escaped
                // (through benign tvars) should be ignored.
                //
                // Ideally we could replace the Speculation use_ops on benign tvars with their
                // underlying use_op after speculation ends.
                !env.speculating()
            }
            _ => false,
        }
    };
    let ignore_root = |op: &UseOp| -> bool { ignore_root_virtual(root_of_use_op(op)) };
    if ignore_root(op1) {
        return op2.dupe();
    }

    let root_of_op2 = root_of_use_op(op2);
    let should_replace = fold_use_op(
        op2,
        // If the root of the previous use_op is UnknownUse and our alternate
        // use_op does not have an UnknownUse root then we use our
        // alternate use_op.
        |root| ignore_root_virtual(root),
        &|should_replace: bool, frame: &FrameUseOp| -> bool {
            match frame {
                // If the use was added to an implicit type param then we want to use
                // our alternate if the implicit type param use_op chain is inside
                // the implicit type param instantiation. Since we can't directly compare
                // abstract locations, we determine whether to do this using a heuristic
                // based on the 'locality' of the use_op root.
                VirtualFrameUseOp::ImplicitTypeParam if !should_replace => match root_of_op2 {
                    VirtualRootUseOp::FunCall(box FunCallData { local, .. })
                    | VirtualRootUseOp::FunCallMethod(box FunCallMethodData { local, .. }) => {
                        *local
                    }
                    VirtualRootUseOp::AssignVar { .. }
                    | VirtualRootUseOp::Coercion { .. }
                    | VirtualRootUseOp::DeleteVar { .. }
                    | VirtualRootUseOp::DeleteProperty { .. }
                    | VirtualRootUseOp::FunImplicitReturn(..)
                    | VirtualRootUseOp::FunReturnStatement { .. }
                    | VirtualRootUseOp::GetExport(..)
                    | VirtualRootUseOp::GetProperty(..)
                    | VirtualRootUseOp::IndexedTypeAccess(..)
                    | VirtualRootUseOp::InferBoundCompatibilityCheck { .. }
                    | VirtualRootUseOp::EvalMappedType(..)
                    | VirtualRootUseOp::SetProperty(..)
                    | VirtualRootUseOp::UpdateProperty { .. }
                    | VirtualRootUseOp::JSXCreateElement(..)
                    | VirtualRootUseOp::ObjectAddComputedProperty { .. }
                    | VirtualRootUseOp::ObjectSpread { .. }
                    | VirtualRootUseOp::ObjectRest { .. }
                    | VirtualRootUseOp::ObjectChain { .. }
                    | VirtualRootUseOp::TypeApplication { .. }
                    | VirtualRootUseOp::MakeReadOnly { .. }
                    | VirtualRootUseOp::Speculation(..)
                    | VirtualRootUseOp::InitField { .. } => true,
                    VirtualRootUseOp::Cast { .. }
                    | VirtualRootUseOp::RefinementCheck { .. }
                    | VirtualRootUseOp::SwitchRefinementCheck(..)
                    | VirtualRootUseOp::ClassExtendsCheck(..)
                    | VirtualRootUseOp::ClassMethodDefinition { .. }
                    | VirtualRootUseOp::ClassImplementsCheck(box ClassImplementsCheckData {
                        ..
                    })
                    | VirtualRootUseOp::ClassOwnProtoCheck(..)
                    | VirtualRootUseOp::ConformToCommonInterface(..)
                    | VirtualRootUseOp::MergedDeclaration(..)
                    | VirtualRootUseOp::DeclareComponentRef { .. }
                    | VirtualRootUseOp::GeneratorYield { .. }
                    | VirtualRootUseOp::ReactCreateElementCall(..)
                    | VirtualRootUseOp::ReactGetIntrinsic { .. }
                    | VirtualRootUseOp::RecordCreate(..)
                    | VirtualRootUseOp::TypeGuardIncompatibility(..)
                    | VirtualRootUseOp::RenderTypeInstantiation { .. }
                    | VirtualRootUseOp::ComponentRestParamCompatibility(..)
                    | VirtualRootUseOp::PositiveTypeGuardConsistency(..)
                    | VirtualRootUseOp::AssertionFunctionConsistency(..)
                    | VirtualRootUseOp::UnknownUse => false,
                },
                VirtualFrameUseOp::UnifyFlip if !should_replace => match root_of_op2 {
                    VirtualRootUseOp::TypeApplication { .. }
                    | VirtualRootUseOp::MakeReadOnly { .. } => true,
                    _ => should_replace,
                },
                _ => should_replace,
            }
        },
    );
    if should_replace {
        op1.dupe()
    } else {
        op2.dupe()
    }
}

pub(super) fn flow_use_op<CX: ContextFamily>(env: &FlowJsEnv, op1: UseOp, u: UseT<CX>) -> UseT<CX> {
    use flow_typing_type::type_::VirtualRootUseOp;
    use flow_typing_type::type_::root_of_use_op;

    match root_of_use_op(&op1) {
        VirtualRootUseOp::UnknownUse => return u,
        VirtualRootUseOp::Speculation { .. } if !env.speculating() => return u,
        _ => {}
    }
    type_util::mod_use_op_of_use_t(|op2| pick_use_op(env, &op1, op2), &u)
}

pub(super) fn apply_method_action<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    l: &Type,
    use_op: UseOp,
    reason_call: Reason,
    this_arg: Type,
    action: &MethodAction<Cx>,
) -> Result<(), FlowJsException> {
    match action {
        MethodAction::CallM(box CallMData {
            methodcalltype: app,
            return_hint,
            specialized_callee,
        }) => {
            let u = UseT::new(UseTInner::CallT(Box::new(CallTData {
                use_op,
                reason: reason_call,
                call_action: Box::new(CallAction::Funcalltype(Box::new(call_of_method_app(
                    this_arg,
                    specialized_callee.clone(),
                    app.clone(),
                )))),
                return_hint: return_hint.clone(),
            })));
            rec_flow(cx, env, trace, (l, &u))
        }
        MethodAction::ChainM(box ChainMData {
            exp_reason,
            lhs_reason,
            lhs_expression,
            methodcalltype: app,
            voided_out_collector,
            return_hint,
            specialized_callee,
        }) => crate::optional_chain_kit::run_with_env(
            cx,
            env,
            trace,
            l,
            exp_reason,
            lhs_reason,
            lhs_expression,
            &UseT::new(UseTInner::CallT(Box::new(CallTData {
                use_op,
                reason: reason_call,
                call_action: Box::new(CallAction::Funcalltype(Box::new(call_of_method_app(
                    this_arg,
                    specialized_callee.clone(),
                    app.clone(),
                )))),
                return_hint: return_hint.clone(),
            }))),
            voided_out_collector,
        ),
        MethodAction::NoMethodAction(prop_t) => {
            prop_t.add(l.dupe());
            Ok(())
        }
    }
}

pub(super) fn perform_elem_action<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    restrict_deletes: bool,
    reason_op: &Reason,
    l: &Type,
    value: &Type,
    action: &ElemAction<Cx>,
) -> Result<(), FlowJsException> {
    match (action, restrict_deletes) {
        (ElemAction::ReadElem(box ReadElemData { tout, .. }), _) => {
            let loc = reason_op.loc().dupe();
            let value = reposition(cx, env, Some(trace), loc, None, None, value.dupe())?;
            rec_flow_t(
                cx,
                env,
                trace,
                unknown_use(),
                (&value, &Type::new(TypeInner::OpenT(tout.dupe()))),
            )?;
        }
        (
            ElemAction::WriteElem(box WriteElemData {
                tin,
                tout,
                mode: SetMode::Assign,
            }),
            _,
        )
        | (
            ElemAction::WriteElem(box WriteElemData {
                tin,
                tout,
                mode: SetMode::Delete,
            }),
            true,
        ) => {
            rec_flow(
                cx,
                env,
                trace,
                (
                    tin,
                    &UseT::new(UseTInner::UseT(use_op.dupe(), value.dupe())),
                ),
            )?;
            if let Some(t) = tout {
                rec_flow_t(cx, env, trace, unknown_use(), (l, t))?;
            }
        }
        (
            ElemAction::WriteElem(box WriteElemData {
                tin,
                tout,
                mode: SetMode::Delete,
            }),
            false,
        ) => {
            // Ok to delete arbitrary elements on arrays, not OK for tuples
            rec_flow(
                cx,
                env,
                trace,
                (
                    tin,
                    &UseT::new(UseTInner::UseT(
                        use_op.dupe(),
                        void::make(reason_of_t(value).dupe()),
                    )),
                ),
            )?;
            if let Some(t) = tout {
                rec_flow_t(cx, env, trace, unknown_use(), (l, t))?;
            }
        }
        (ElemAction::CallElem(reason_call, action), _) => {
            apply_method_action(
                cx,
                env,
                trace,
                value,
                use_op,
                reason_call.dupe(),
                l.dupe(),
                action,
            )?;
        }
    }
    Ok(())
}

// builtins, contd.

pub(super) fn get_builtin_typeapp<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    use_desc: Option<bool>,
    x: &str,
    targs: Vec<Type>,
) -> Type {
    let t = flow_js_utils::lookup_builtin_type_with_env(cx, env, x, reason.dupe());
    let use_desc = use_desc.unwrap_or(false);
    type_util::typeapp(false, use_desc, reason.dupe(), t, targs)
}

pub(super) fn get_builtin_react_typeapp<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    use_desc: Option<bool>,
    purpose: ExpectedModulePurpose,
    targs: Vec<Type>,
) -> Result<Type, FlowJsException> {
    let t = flow_js_utils::import_export_utils::get_implicitly_imported_react_type_with_env(
        cx,
        env,
        reason.loc().dupe(),
        &|cx, reason, t| {
            singleton_concrete_type(
                ConcretizationKind::ConcretizeForImportsExports,
                cx,
                env,
                &reason,
                &t,
            )
        },
        purpose,
    )?;
    let use_desc = use_desc.unwrap_or(false);
    Ok(type_util::typeapp(false, use_desc, reason.dupe(), t, targs))
}

/// Specialize a polymorphic class, make an instance of the specialized class.
pub(super) fn mk_typeapp_instance_annot<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    use_op: UseOp,
    reason_op: &Reason,
    reason_tapp: &Reason,
    from_value: bool,
    use_desc: Option<bool>,
    c: &Type,
    ts: Rc<[Type]>,
) -> Result<Type, FlowJsException> {
    let use_desc = use_desc.unwrap_or(false);
    let collector = type_collector::TypeCollector::create();
    flow_opt(
        cx,
        env,
        trace.clone(),
        (
            c,
            &UseT::new(UseTInner::SpecializeT(Box::new(SpecializeTData {
                use_op,
                reason: reason_op.dupe(),
                reason2: reason_tapp.dupe(),
                targs: Some(ts),
                upper: Some(Box::new(Cont::Collect(collector.dupe()))),
            }))),
        ),
    )?;
    let t = collector
        .union_opt(reason_tapp.dupe())
        .unwrap_or_else(|| tvar_resolver::default_no_lowers(reason_tapp));
    if from_value {
        Ok(t)
    } else {
        let reason_type = type_util::reason_of_t(c);
        mk_instance_raw(cx, env, None, trace, reason_tapp, use_desc, reason_type, &t)
    }
}

pub(super) fn mk_typeapp_instance<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    use_op: UseOp,
    reason_op: &Reason,
    reason_tapp: &Reason,
    from_value: bool,
    c: &Type,
    ts: Rc<[Type]>,
) -> Result<Type, FlowJsException> {
    let collector = type_collector::TypeCollector::create();
    flow_opt(
        cx,
        env,
        trace.clone(),
        (
            c,
            &UseT::new(UseTInner::SpecializeT(Box::new(SpecializeTData {
                use_op,
                reason: reason_op.dupe(),
                reason2: reason_tapp.dupe(),
                targs: Some(ts),
                upper: Some(Box::new(Cont::Collect(collector.dupe()))),
            }))),
        ),
    )?;
    let t = collector
        .union_opt(reason_tapp.dupe())
        .unwrap_or_else(|| tvar_resolver::default_no_lowers(reason_tapp));
    if from_value {
        Ok(t)
    } else {
        let reason_type = type_util::reason_of_t(c);
        mk_instance_source(
            cx,
            env,
            TypeTKind::InstanceKind,
            trace,
            reason_tapp,
            reason_type,
            &t,
        )
    }
}

pub(super) fn mk_typeapp_instance_of_poly<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    reason_op: &Reason,
    reason_tapp: &Reason,
    from_value: bool,
    id: poly::Id,
    tparams_loc: ALoc,
    xs: Vec1<TypeParam>,
    t: &Type,
    ts: Rc<[Type]>,
) -> Result<Type, FlowJsException> {
    let t = FlowJs::mk_typeapp_of_poly_with_env(
        cx,
        env,
        trace,
        use_op,
        reason_op,
        reason_tapp,
        id,
        tparams_loc,
        xs,
        t,
        ts,
    )?;
    if from_value {
        Ok(t)
    } else {
        mk_instance(cx, env, None, Some(trace), reason_tapp, false, &t)
    }
}

pub(super) fn mk_instance<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    type_t_kind: Option<TypeTKind>,
    trace: Option<DepthTrace>,
    instance_reason: &Reason,
    use_desc: bool,
    c: &Type,
) -> Result<Type, FlowJsException> {
    mk_instance_raw(
        cx,
        env,
        type_t_kind,
        trace,
        instance_reason,
        use_desc,
        instance_reason,
        c,
    )
}

pub(super) fn mk_instance_source<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    type_t_kind: TypeTKind,
    trace: Option<DepthTrace>,
    instance_reason: &Reason,
    reason_type: &Reason,
    c: &Type,
) -> Result<Type, FlowJsException> {
    let tvar_id = flow_typing_tvar::mk_no_wrap(cx, instance_reason);
    // this part is similar to making a runtime value
    let tvar = Tvar::new(instance_reason.dupe(), tvar_id as u32);
    flow_opt(
        cx,
        env,
        trace,
        (
            c,
            &UseT::new(UseTInner::ValueToTypeReferenceT(Box::new(
                ValueToTypeReferenceTData {
                    use_op: unknown_use(),
                    reason: reason_type.dupe(),
                    kind: type_t_kind,
                    tout: Box::new(tvar.dupe()),
                },
            ))),
        ),
    )?;
    Ok(Type::new(TypeInner::OpenT(tvar)))
}

pub(super) fn mk_instance_raw<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    type_t_kind: Option<TypeTKind>,
    trace: Option<DepthTrace>,
    instance_reason: &Reason,
    use_desc: bool,
    reason_type: &Reason,
    c: &Type,
) -> Result<Type, FlowJsException> {
    // Make an annotation
    let type_t_kind = type_t_kind.unwrap_or(TypeTKind::InstanceKind);
    let source = mk_instance_source(cx, env, type_t_kind, trace, instance_reason, reason_type, c)?;
    Ok(Type::new(TypeInner::AnnotT(
        instance_reason.dupe(),
        source,
        use_desc,
    )))
}

pub(super) fn instance_lookup_kind<'cx>(
    cx: &mut Context<'cx>,
    reason_op: &Reason,
    method_accessible: bool,
    instance_t: &Type,
    propref: &PropRef,
    lookup_action: LookupAction,
) -> LookupKind {
    match propref {
        PropRef::Named {
            name,
            from_indexed_access,
            ..
        } if !from_indexed_access || flow_js_utils::is_munged_prop_name(cx, name) => {
            LookupKind::Strict(instance_t.dupe())
        }
        _ => {
            let propref = propref.clone();
            let reason_op = reason_op.dupe();
            let lookup_default_second = LookupDefaultResult::Lookup(Box::new(LookupTData {
                reason: reason_op.dupe(),
                lookup_kind: Box::new(LookupKind::Strict(instance_t.dupe())),
                indexer_fallback: None,
                try_ts_on_failure: Rc::from([]),
                propref: Box::new(propref.clone()),
                lookup_action: Box::new(lookup_action),
                ids: None,
                method_accessible,
                ignore_dicts: false,
            }));
            let lookup_default = (instance_t.dupe(), lookup_default_second);
            LookupKind::NonstrictReturning(Box::new(NonstrictReturningData(
                Some(lookup_default),
                None,
            )))
        }
    }
}

pub(super) fn reposition_reason<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    reason: &Reason,
    use_desc: bool,
    t: &Type,
) -> Result<Type, FlowJsException> {
    let loc = reason.loc().dupe();
    let desc = if use_desc {
        Some(reason.desc(false))
    } else {
        None
    };
    let annot_loc = reason.annot_loc().map(|l| l.dupe());
    reposition(cx, env, trace, loc, desc, annot_loc, t.dupe())
}

// set the position of the given def type from a reason
pub(super) fn reposition<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    loc: ALoc,
    desc: Option<Option<&ReasonDesc>>,
    annot_loc: Option<ALoc>,
    t: Type,
) -> Result<Type, FlowJsException> {
    use std::ops::Deref;
    use std::rc::Rc;

    use flow_data_structure_wrapper::int_map::IntHashMap;
    use flow_typing_flow_common::flow_cache;
    use flow_typing_flow_common::flow_js_utils;
    use flow_typing_type::type_::CanonicalRendersForm;
    use flow_typing_type::type_::DefT as DefTType;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::NominalType;
    use flow_typing_type::type_::NominalTypeInner;
    use flow_typing_type::type_::constraint::Constraints;
    use flow_typing_type::type_::empty_t;
    use flow_typing_type::type_::nominal;
    use flow_typing_type::type_::open_tvar;
    use flow_typing_type::type_util::mod_reason_of_defer_use_t;
    use flow_typing_type::type_util::mod_reason_of_t;
    use flow_typing_type::type_util::reason_of_defer_use_t;
    use flow_typing_type::type_util::reason_of_t;

    let mod_reason = |reason: Reason| -> Reason {
        match desc {
            Some(d) => Reason::new(d.cloned(), loc.dupe()),
            None => reason.reposition_and_opt_annotate(loc.dupe(), annot_loc.dupe()),
        }
    };

    fn recurse<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: Option<DepthTrace>,
        desc: Option<Option<&ReasonDesc>>,
        mod_reason: &dyn Fn(Reason) -> Reason,
        seen: &mut IntHashMap<i32, Type>,
        t: &Type,
    ) -> Result<Type, FlowJsException> {
        match t.deref() {
            _ if let Some(node) = super::constraint_helpers::active_constraint_node(cx, env, t) => {
                let r = reason_of_t(t);
                let id = node.id();
                let t_node = t;
                let reason = mod_reason(r.dupe());
                let use_desc = desc.is_some();
                let (_, constraints) = constraint_node_constraints(cx, node);
                match constraints {
                    Constraints::Resolved(resolved_t) => match seen.get(&id) {
                        Some(t) => Ok(t.dupe()),
                        None => flow_typing_tvar::mk_where(cx, reason.dupe(), |cx, tvar| {
                            seen.insert(id, tvar.dupe());
                            let t_prime =
                                recurse(cx, env, trace, desc, mod_reason, seen, &resolved_t);
                            seen.remove(&id);
                            let t_prime = t_prime?;
                            let use_op = unknown_use();
                            let trace = match trace {
                                None => DepthTrace::unit_trace(),
                                Some(trace) => DepthTrace::rec_trace(trace),
                            };
                            let tvar_inner = open_tvar(tvar);
                            let tvar_id = tvar_inner.id() as i32;
                            resolve_id(cx, env, trace, use_op, tvar_id, &t_prime)?;
                            Ok(())
                        }),
                    },
                    Constraints::FullyResolved(s) => match seen.get(&id) {
                        Some(t) => Ok(t.dupe()),
                        None => {
                            let forced_t = cx.force_fully_resolved_tvar(&s);
                            let t = {
                                let lazy_thunk_cell: Rc<std::cell::OnceCell<Type>> =
                                    Rc::new(std::cell::OnceCell::new());
                                let lazy_t_val = flow_typing_tvar::mk_fully_resolved_lazy(
                                    cx,
                                    reason.dupe(),
                                    false,
                                    {
                                        let cell = lazy_thunk_cell.dupe();
                                        move |_cx: &mut Context<'_>| {
                                            Ok(cell
                                                .get()
                                                .expect(
                                                    "lazy_thunk must be initialized before access",
                                                )
                                                .dupe())
                                        }
                                    },
                                );
                                seen.insert(id, lazy_t_val.dupe());
                                let env = env.signature_tvar_env();
                                let thunk_result = cx.run_in_signature_tvar_env(|cx| {
                                    recurse(cx, &env, trace, desc, mod_reason, seen, &forced_t)
                                });
                                seen.remove(&id);
                                let thunk_result = thunk_result?;
                                lazy_thunk_cell
                                    .set(thunk_result)
                                    .expect("lazy_thunk_cell already set");
                                // Queued for post-component forcing only now that it can be
                                // forced: a solve inside the recursion above forces every
                                // queued state.
                                let lazy_id = open_tvar(&lazy_t_val).id() as i32;
                                if let (_, Constraints::FullyResolved(state)) =
                                    cx.find_constraints(lazy_id)
                                {
                                    cx.add_post_component_tvar_forcing_state(lazy_id, state);
                                }
                                lazy_t_val
                            };
                            match t.deref() {
                                TypeInner::OpenT(repositioned_tvar) if node.is_open_t() => {
                                    cx.report_array_or_object_literal_declaration_reposition(
                                        repositioned_tvar.id() as i32,
                                        id,
                                    );
                                }
                                _ => {}
                            };
                            Ok(t)
                        }
                    },
                    Constraints::Unresolved(_) => match t_node.deref() {
                        TypeInner::ImplicitInstantiationTvar(_)
                            if env.in_implicit_instantiation() =>
                        {
                            Ok(t_node.dupe())
                        }
                        _ => {
                            let reason_for_repos = reason.dupe();
                            flow_typing_tvar::mk_where(cx, reason, |cx, tvar| {
                                flow_opt(
                                    cx,
                                    env,
                                    trace,
                                    (
                                        t_node,
                                        &UseT::new(UseTInner::ReposLowerT {
                                            reason: reason_for_repos,
                                            use_desc,
                                            use_t: Box::new(UseT::new(UseTInner::UseT(
                                                unknown_use(),
                                                tvar.dupe(),
                                            ))),
                                        }),
                                    ),
                                )
                            })
                        }
                    },
                }
            }
            TypeInner::EvalT {
                type_: root,
                defer_use_t,
                id: eval_id,
            } => {
                // Modifying the reason of `EvalT`, as we do for other types, is not
                // enough, since it will only affect the reason of the resulting tvar.
                // Instead, repositioning a `EvalT` should simulate repositioning the
                // resulting tvar, i.e., flowing repositioned *lower bounds* to the
                // resulting tvar. (Another way of thinking about this is that a `EvalT`
                // is just as transparent as its resulting tvar.)
                let d = &defer_use_t.2;
                let new_defer_use_t = mod_reason_of_defer_use_t(mod_reason, defer_use_t);
                let reason = reason_of_defer_use_t(&new_defer_use_t).dupe();
                let use_desc = desc.is_some();
                let no_unresolved = !flow_js_utils::tvar_visitors::has_unresolved_tvars(cx, root)
                    && !flow_js_utils::tvar_visitors::has_unresolved_tvars_in_destructors(cx, d);
                // During implicit instantiation, an `EvalT` over unresolved tvars is left as is,
                // like the solve's own tvars. A tvar fed with its repositioned result would take
                // what flows into it as more lower bounds, without checking it against the result.
                if !no_unresolved && env.in_implicit_instantiation() {
                    return Ok(t.dupe());
                }
                match flow_cache::eval::find_repos(cx, root, &new_defer_use_t, eval_id) {
                    Some(cached_tvar) => match &*cached_tvar {
                        TypeInner::OpenT(cached_open_tvar)
                            if no_unresolved
                                && flow_js_utils::possible_types(
                                    cx,
                                    cached_open_tvar.id() as i32,
                                )
                                .is_empty() =>
                        {
                            Ok(empty_t::why(reason_of_t(&cached_tvar).dupe()))
                        }
                        _ => Ok(cached_tvar),
                    },
                    None => flow_typing_tvar::mk_where(cx, reason.dupe(), |cx, tvar| {
                        flow_cache::eval::add_repos(
                            cx,
                            root.dupe(),
                            new_defer_use_t.dupe(),
                            eval_id.dupe(),
                            tvar.dupe(),
                        );
                        flow_opt(
                            cx,
                            env,
                            trace,
                            (
                                t,
                                &UseT::new(UseTInner::ReposLowerT {
                                    reason: reason.dupe(),
                                    use_desc,
                                    use_t: Box::new(UseT::new(UseTInner::UseT(
                                        unknown_use(),
                                        tvar.dupe(),
                                    ))),
                                }),
                            ),
                        )?;
                        if no_unresolved {
                            tvar_resolver::resolve(cx, tvar_resolver::default_no_lowers, true, t);
                            tvar_resolver::resolve(
                                cx,
                                tvar_resolver::default_no_lowers,
                                true,
                                tvar,
                            );
                        }
                        Ok(())
                    }),
                }
            }
            TypeInner::MaybeT(r, inner_t) => {
                // repositions both the MaybeT and the nested type. MaybeT represets `?T`.
                // elsewhere, when we decompose into T | NullT | VoidT, we use the reason
                // of the MaybeT for NullT and VoidT but don't reposition `t`, so that any
                // errors on the NullT or VoidT point at ?T, but errors on the T point at T.
                let r = mod_reason(r.dupe());
                Ok(Type::new(TypeInner::MaybeT(
                    r,
                    recurse(cx, env, trace, desc, mod_reason, seen, inner_t)?,
                )))
            }
            TypeInner::OptionalT {
                reason,
                type_: inner_t,
                use_desc,
            } => {
                let reason = mod_reason(reason.dupe());
                Ok(Type::new(TypeInner::OptionalT {
                    reason,
                    type_: recurse(cx, env, trace, desc, mod_reason, seen, inner_t)?,
                    use_desc: *use_desc,
                }))
            }
            TypeInner::UnionT(r, rep) => {
                let r = mod_reason(r.dupe());
                let rep = rep.try_ident_map(true, |inner_t| {
                    recurse(cx, env, trace, desc, mod_reason, seen, inner_t)
                })?;
                Ok(Type::new(TypeInner::UnionT(r, rep)))
            }
            TypeInner::AnnotT(r, inner_t, use_desc) => Ok(Type::new(TypeInner::AnnotT(
                mod_reason(r.dupe()),
                inner_t.dupe(),
                matches!(desc, Some(Some(ReasonDesc::RReactElement { .. }))) || *use_desc,
            ))),
            TypeInner::TypeAppT(box TypeAppTData {
                reason,
                use_op,
                type_,
                targs,
                from_value,
                use_desc,
            }) => Ok(Type::new(TypeInner::TypeAppT(Box::new(TypeAppTData {
                reason: mod_reason(reason.dupe()),
                use_op: use_op.dupe(),
                type_: type_.dupe(),
                targs: targs.dupe(),
                from_value: *from_value,
                use_desc: matches!(desc, Some(Some(ReasonDesc::RReactElement { .. }))) || *use_desc,
            })))),
            TypeInner::NominalT {
                reason: r,
                nominal_type,
            } => {
                let r = mod_reason(r.dupe());
                let underlying_t = match &nominal_type.underlying_t {
                    nominal::UnderlyingT::FullyOpaque => nominal_type.underlying_t.clone(),
                    nominal::UnderlyingT::OpaqueWithLocal { t: inner_t } => {
                        let t_prime = recurse(cx, env, trace, desc, mod_reason, seen, inner_t)?;
                        if Type::ptr_eq(inner_t, &t_prime) {
                            nominal_type.underlying_t.clone()
                        } else {
                            nominal::UnderlyingT::OpaqueWithLocal { t: t_prime }
                        }
                    }
                    nominal::UnderlyingT::CustomError(box nominal::CustomErrorData {
                        t: inner_t,
                        custom_error_loc,
                    }) => {
                        let t_prime = recurse(cx, env, trace, desc, mod_reason, seen, inner_t)?;
                        if Type::ptr_eq(inner_t, &t_prime) {
                            nominal_type.underlying_t.clone()
                        } else {
                            nominal::UnderlyingT::CustomError(Box::new(nominal::CustomErrorData {
                                t: t_prime,
                                custom_error_loc: custom_error_loc.dupe(),
                            }))
                        }
                    }
                };
                let lower_t = option_utils::try_ident_map(
                    |lt| recurse(cx, env, trace, desc, mod_reason, seen, lt),
                    Type::ptr_eq,
                    nominal_type.lower_t.dupe(),
                )?;
                let upper_t = option_utils::try_ident_map(
                    |ut| recurse(cx, env, trace, desc, mod_reason, seen, ut),
                    Type::ptr_eq,
                    nominal_type.upper_t.dupe(),
                )?;
                Ok(Type::new(TypeInner::NominalT {
                    reason: r,
                    nominal_type: Rc::new(NominalType::new(NominalTypeInner {
                        nominal_id: nominal_type.nominal_id.clone(),
                        underlying_t,
                        lower_t,
                        upper_t,
                        nominal_type_args: nominal_type.nominal_type_args.dupe(),
                    })),
                }))
            }
            TypeInner::DefT(r, def_t)
                if let DefTInner::RendersT(renders_form) = def_t.deref()
                    && let CanonicalRendersForm::StructuralRenders {
                        renders_variant,
                        renders_structural_type: inner_t,
                    } = renders_form.deref() =>
            {
                let r = mod_reason(r.dupe());
                Ok(Type::new(TypeInner::DefT(
                    r,
                    DefTType::new(DefTInner::RendersT(Rc::new(
                        CanonicalRendersForm::StructuralRenders {
                            renders_variant: renders_variant.clone(),
                            renders_structural_type: recurse(
                                cx, env, trace, desc, mod_reason, seen, inner_t,
                            )?,
                        },
                    ))),
                )))
            }
            _ => Ok(mod_reason_of_t(mod_reason, t)),
        }
    }

    recurse(
        cx,
        env,
        trace,
        desc,
        &mod_reason,
        &mut IntHashMap::default(),
        &t,
    )
}

pub(super) fn get_builtin_type<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    reason: &Reason,
    use_desc: Option<bool>,
    x: &str,
) -> Result<Type, FlowJsException> {
    let t = flow_js_utils::lookup_builtin_type_with_env(cx, env, x, reason.dupe());
    let use_desc = use_desc.unwrap_or(false);
    mk_instance(cx, env, None, trace, reason, use_desc, &t)
}

pub(super) fn get_builtin_react_type<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    reason: &Reason,
    use_desc: Option<bool>,
    purpose: ExpectedModulePurpose,
) -> Result<Type, FlowJsException> {
    let t = flow_js_utils::import_export_utils::get_implicitly_imported_react_type_with_env(
        cx,
        env,
        reason.loc().dupe(),
        &|cx, reason, t| {
            singleton_concrete_type(
                ConcretizationKind::ConcretizeForImportsExports,
                cx,
                env,
                &reason,
                &t,
            )
        },
        purpose,
    )?;
    let use_desc = use_desc.unwrap_or(false);
    mk_instance(cx, env, None, trace, reason, use_desc, &t)
}

pub(super) fn flow_all_in_union<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    rep: &union_rep::UnionRep,
    u: &UseT<Cx>,
) -> Result<(), FlowJsException> {
    flow_js_utils::iter_union(
        |cx, trace, (t, u)| rec_flow(cx, env, *trace, (t, u)),
        (),
        |_, _| (),
        cx,
        &trace,
        rep,
        u,
    )
}

pub(super) fn call_args_iter<E, F: FnMut(&Type) -> Result<(), E>>(
    mut f: F,
    args: &[CallArg],
) -> Result<(), E> {
    for arg in args {
        match arg.deref() {
            CallArgInner::Arg(t) | CallArgInner::SpreadArg(t) => f(t)?,
        }
    }
    Ok(())
}

// Wrapper functions around __flow that manage traces. Use these functions for
// all recursive calls in the implementation of __flow.

// Call __flow while concatenating traces. Typically this is used in code that
// propagates bounds across type variables, where nothing interesting is going
// on other than concatenating subtraces to make longer traces to describe
// transitive data flows
pub(super) fn join_flow<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    ts: &[DepthTrace],
    (t1, t2): (&Type, &UseT<Cx>),
) -> Result<(), FlowJsException> {
    __flow(cx, env, (t1, t2), DepthTrace::concat_trace(ts))
}

// Call __flow while embedding traces. Typically this is used in code that
// simplifies a constraint to generate subconstraints: the current trace is
// "pushed" when recursing into the subconstraints, so that when we finally hit
// an error and walk back, we can know why the particular constraints that
// caused the immediate error were generated.
pub(super) fn rec_flow<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    (t1, t2): (&Type, &UseT<Cx>),
) -> Result<(), FlowJsException> {
    __flow(cx, env, (t1, t2), DepthTrace::rec_trace(trace))
}

pub(super) fn rec_flow_t<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    (t1, t2): (&Type, &Type),
) -> Result<(), FlowJsException> {
    rec_flow(
        cx,
        env,
        trace,
        (t1, &UseT::new(UseTInner::UseT(use_op, t2.dupe()))),
    )
}

// Ideally this function would not be required: either we call `flow` from
// outside without a trace (see below), or we call one of the functions above
// with a trace. However, there are some functions that need to call __flow,
// which are themselves called both from outside and inside (with or without
// traces), so they call this function instead.
pub(super) fn flow_opt<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    (t1, t2): (&Type, &UseT<Cx>),
) -> Result<(), FlowJsException> {
    let trace = match trace {
        None => DepthTrace::unit_trace(),
        Some(trace) => DepthTrace::rec_trace(trace),
    };
    __flow(cx, env, (t1, t2), trace)
}

pub(super) fn flow_opt_t<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: UseOp,
    trace: Option<DepthTrace>,
    (t1, t2): (&Type, &Type),
) -> Result<(), FlowJsException> {
    flow_opt(
        cx,
        env,
        trace,
        (t1, &UseT::new(UseTInner::UseT(use_op, t2.dupe()))),
    )
}

/// Externally visible function for subtyping.
/// Calls internal entry point and traps runaway recursion.
pub(super) fn flow<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    (lower, upper): (&Type, &UseT<Cx>),
) -> Result<(), FlowJsException> {
    match flow_opt(cx, env, None, (lower, upper)) {
        Ok(()) => Ok(()),
        Err(FlowJsException::LimitExceeded) => {
            let rl = reason_of_t(lower).dupe();
            let ru = match type_util::use_op_of_use_t(upper)
                .and_then(|use_op| flow_js_utils::primary_reason_of_use_op(&use_op))
            {
                Some(reason) => reason,
                None => match upper.deref() {
                    UseTInner::UseT(_, t) => reason_of_t(t).dupe(),
                    UseTInner::ResolveUnionT(data) => data.reason.dupe(),
                    _ => rl.dupe(),
                },
            };
            let reasons = match upper.deref() {
                UseTInner::UseT(_, _) => (ru, rl),
                _ => flow_js_utils::ordered_reasons(cx, (rl, ru)),
            };
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::ERecursionLimit(reasons.0.loc().dupe()),
            )
        }
        Err(e) => Err(e),
    }
}

pub(super) fn flow_t<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    (t1, t2): (&Type, &Type),
) -> Result<(), FlowJsException> {
    flow(
        cx,
        env,
        (t1, &UseT::new(UseTInner::UseT(unknown_use(), t2.dupe()))),
    )
}

pub(super) fn flow_p<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: UseOp,
    lower: PropPolarityMismatchObject<&Type, ALoc>,
    upper: PropPolarityMismatchObject<&Type, ALoc>,
    propref: &PropRef,
    (prop1, prop2): (&PropertyType, &PropertyType),
) -> Result<(), FlowJsException> {
    subtyping_kit::rec_flow_p(
        cx, env, None, use_op, true, lower, upper, propref, prop1, prop2,
    )
}

// Wrapper functions around __unify that manage traces. Use these functions for
// all recursive calls in the implementation of __unify.
pub(super) fn rec_unify<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    unify_cause: UnifyCause,
    unify_any: Option<bool>,
    t1: &Type,
    t2: &Type,
) -> Result<(), FlowJsException> {
    let unify_any = unify_any.unwrap_or(false);
    __unify(
        cx,
        env,
        use_op,
        unify_cause,
        unify_any,
        t1,
        t2,
        DepthTrace::rec_trace(trace),
    )
}

pub(super) fn unify_opt<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    use_op: UseOp,
    unify_cause: UnifyCause,
    unify_any: Option<bool>,
    t1: &Type,
    t2: &Type,
) -> Result<(), FlowJsException> {
    let trace = match trace {
        None => DepthTrace::unit_trace(),
        Some(trace) => DepthTrace::rec_trace(trace),
    };
    let unify_any = unify_any.unwrap_or(false);
    __unify(cx, env, use_op, unify_cause, unify_any, t1, t2, trace)
}

// Externally visible function for unification.
// Calls internal entry point and traps runaway recursion.
pub(super) fn unify<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: Option<UseOp>,
    unify_cause: UnifyCause,
    t1: &Type,
    t2: &Type,
) -> Result<(), FlowJsException> {
    let use_op = use_op.unwrap_or_else(unknown_use);
    match unify_opt(cx, env, None, use_op, unify_cause, Some(true), t1, t2) {
        Ok(()) => Ok(()),
        Err(FlowJsException::LimitExceeded) => {
            let reasons = flow_js_utils::ordered_reasons(
                cx,
                (reason_of_t(t1).dupe(), reason_of_t(t2).dupe()),
            );
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::ERecursionLimit(reasons.0.loc().dupe()),
            )
        }
        Err(e) => Err(e),
    }
}

pub(super) fn continue_<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    t: &Type,
    cont: &Cont<Cx>,
) -> Result<(), FlowJsException> {
    match cont {
        Cont::Lower(use_op, l) => rec_flow(
            cx,
            env,
            trace,
            (l, &UseT::new(UseTInner::UseT(use_op.dupe(), t.dupe()))),
        ),
        Cont::Upper(u) => rec_flow(cx, env, trace, (t, u)),
        Cont::Collect(collector) => {
            collector.add(t.dupe());
            Ok(())
        }
    }
}

pub(super) fn continue_repos<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    reason: &Reason,
    use_desc: bool,
    t: &Type,
    cont: &Cont<Cx>,
) -> Result<(), FlowJsException> {
    match cont {
        Cont::Lower(use_op, l) => {
            let repos_t = reposition_reason(cx, env, Some(trace), reason, use_desc, t)?;
            rec_flow_t(cx, env, trace, use_op.dupe(), (l, &repos_t))
        }
        Cont::Upper(u) => {
            let repos_t = reposition_reason(cx, env, Some(trace), reason, use_desc, t)?;
            rec_flow(cx, env, trace, (&repos_t, u))
        }
        Cont::Collect(collector) => {
            collector.add(reposition_reason(
                cx,
                env,
                Some(trace),
                reason,
                use_desc,
                t,
            )?);
            Ok(())
        }
    }
}

pub(super) fn type_app_variance_check<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    reason_op: &Reason,
    reason_tapp: &Reason,
    targs: Vec<(Type, Type)>,
    tparams_loc: ALoc,
    tparams: &Vec1<TypeParam>,
    strictness_kind: flow_typing_type::type_::TypeStrictnessKind,
) -> Result<(), FlowJsException> {
    let minimum_arity = flow_js_utils::poly_minimum_arity(tparams);
    let maximum_arity = tparams.len();
    let arity_loc = tparams_loc;
    if targs.len() > maximum_arity {
        flow_js_utils::add_output_with_env(
            cx,
            env,
            ErrorMessage::ETooManyTypeArgs(Box::new(ETooManyTypeArgsData {
                reason_tapp: reason_tapp.to_error_reference(),
                arity_loc,
                maximum_arity: maximum_arity as i32,
            })),
        )?;
    } else {
        let mut targs = VecDeque::from(targs);
        let mut map1 = FlowOrdMap::<SubstName, Type>::new();
        let mut map2 = FlowOrdMap::<SubstName, Type>::new();
        for tparam in tparams.iter() {
            let name = tparam.name.dupe();
            let default = &tparam.default;
            let polarity = tparam.polarity;
            let reason = &tparam.reason;
            let flow_targs =
                |cx: &mut Context<'cx>, t1: &Type, t2: &Type| -> Result<(), FlowJsException> {
                    let use_op = UseOp::Frame(
                        Arc::new(VirtualFrameUseOp::TypeArgCompatibility(Box::new(
                            TypeArgCompatibilityData {
                                name: name.dupe(),
                                targ: reason.dupe(),
                                lower: reason_op.dupe(),
                                upper: reason_tapp.dupe(),
                                polarity: polarity.unwrap_or(Polarity::Neutral),
                            },
                        ))),
                        Arc::new(use_op.dupe()),
                    );
                    match polarity {
                        Some(Polarity::Positive) => rec_flow(
                            cx,
                            env,
                            trace,
                            (t1, &UseT::new(UseTInner::UseT(use_op, t2.dupe()))),
                        ),
                        Some(Polarity::Negative) => rec_flow(
                            cx,
                            env,
                            trace,
                            (t2, &UseT::new(UseTInner::UseT(use_op, t1.dupe()))),
                        ),
                        None if strictness_kind.is_typescript_loose() => {
                            // TS treats unannotated tparam slots as covariant in instantiation,
                            // not invariant. Match that by doing a covariant subtype check in .ts
                            // instead of unification. An explicit `in out` stays invariant.
                            rec_flow(
                                cx,
                                env,
                                trace,
                                (t1, &UseT::new(UseTInner::UseT(use_op, t2.dupe()))),
                            )
                        }
                        None | Some(Polarity::Neutral) => rec_unify(
                            cx,
                            env,
                            trace,
                            use_op,
                            UnifyCause::Uncategorized,
                            None,
                            t1,
                            t2,
                        ),
                    }
                };
            match (default, targs.pop_front()) {
                (None, None) => {
                    // fewer arguments than params but no default
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::ETooFewTypeArgs(Box::new(ETooFewTypeArgsData {
                            reason_tapp: reason_tapp.to_error_reference(),
                            arity_loc: arity_loc.dupe(),
                            minimum_arity: minimum_arity as i32,
                        })),
                    )?;
                }
                (Some(default), None) => {
                    let t1 = subst(cx, Some(use_op.dupe()), None, None, &map1, default.dupe());
                    let t2 = subst(cx, Some(use_op.dupe()), None, None, &map2, default.dupe());
                    flow_targs(cx, &t1, &t2)?;
                    map1.insert(name.dupe(), t1);
                    map2.insert(name.dupe(), t2);
                }
                (_, Some((t1, t2))) => {
                    flow_targs(cx, &t1, &t2)?;
                    map1.insert(name.dupe(), t1);
                    map2.insert(name.dupe(), t2);
                }
            }
        }
        assert!(targs.is_empty());
    }
    Ok(())
}

pub(super) fn possible_concrete_types<'cx>(
    kind: ConcretizationKind,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
) -> Result<Vec<Type>, FlowJsException> {
    let collector = type_collector::TypeCollector::create();
    flow(
        cx,
        env,
        (
            t,
            &UseT::new(UseTInner::ConcretizeT(Box::new(ConcretizeTData {
                reason: reason.dupe(),
                kind,
                seen: concretize_seen::ConcretizeSeen::new(),
                collector: collector.dupe(),
            }))),
        ),
    )?;
    Ok(collector.collect_to_vec())
}

pub(super) fn singleton_concrete_type<'cx>(
    kind: ConcretizationKind,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
) -> Result<Type, FlowJsException> {
    let types = possible_concrete_types(kind, cx, env, reason, t)?;
    match types.len() {
        0 => Ok(empty_t::make(reason.dupe())),
        1 => Ok(types.into_iter().next().unwrap()),
        _ => {
            let mut iter = types.into_iter();
            let t1 = iter.next().unwrap();
            let t2 = iter.next().unwrap();
            let ts: Rc<[Type]> = iter.collect();
            Ok(Type::new(TypeInner::UnionT(
                reason.dupe(),
                union_rep::make(None, union_rep::UnionKind::UnknownKind, t1, t2, ts),
            )))
        }
    }
}

fn find_resolved_opt<'cx, T, F>(cx: &mut Context<'cx>, default: T, f: F, id: i32) -> T
where
    F: FnOnce(&mut Context<'cx>, &Type) -> T,
{
    use flow_typing_type::type_::constraint::Constraints;
    match cx.find_constraints(id).1 {
        Constraints::Resolved(t) => f(cx, &t),
        Constraints::FullyResolved(s) => {
            let t = cx.force_fully_resolved_tvar(&s);
            f(cx, &t)
        }
        Constraints::Unresolved(_) => default,
    }
}

// Unwrap resolved tvars (and resolved [GenericT] bounds) to their solution
// without otherwise concretizing. Used as a local optimization by
// [evaluate_type_destructor_] so a resolved tvar is evaluated once to an
// annotation rather than getting a bound on both sides.
pub(super) fn drop_resolved<'cx>(cx: &mut Context<'cx>, env: &FlowJsEnv, t: &Type) -> Type {
    if super::constraint_helpers::frozen_implicit_instantiation_tvar(cx, env, t).is_some() {
        return t.dupe();
    }
    match t.deref() {
        _ if let Some(node) = type_util::constraint_node_id(t) => find_resolved_opt(
            cx,
            t.dupe(),
            |cx, resolved_t| drop_resolved(cx, env, resolved_t),
            node.id(),
        ),
        TypeInner::GenericT(box GenericTData {
            reason,
            name,
            id: g_id,
            bound,
            no_infer,
        }) if let Some(node) = constraint_node_id(bound) => find_resolved_opt(
            cx,
            t.dupe(),
            |cx, resolved_t| {
                Type::new(TypeInner::GenericT(Box::new(GenericTData {
                    reason: reason.dupe(),
                    name: name.dupe(),
                    id: g_id.clone(),
                    bound: drop_resolved(cx, env, resolved_t),
                    no_infer: *no_infer,
                })))
            },
            node.id(),
        ),
        _ => t.dupe(),
    }
}

pub(super) fn possible_concrete_types_for_optional_chain<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
) -> Result<Vec<Type>, FlowJsException> {
    possible_concrete_types(
        ConcretizationKind::ConcretizeForOptionalChain,
        cx,
        env,
        reason,
        t,
    )
}

pub(super) fn possible_concrete_types_for_inspection<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
) -> Result<Vec<Type>, FlowJsException> {
    possible_concrete_types(
        ConcretizationKind::ConcretizeForInspection,
        cx,
        env,
        reason,
        t,
    )
}

pub(super) fn possible_concrete_types_for_walk<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
    markers: Arc<BTreeSet<SubstName>>,
) -> Result<Vec<Type>, FlowJsException> {
    possible_concrete_types(
        ConcretizationKind::ConcretizeForWalk(markers),
        cx,
        env,
        reason,
        t,
    )
}

pub(super) fn possible_concrete_types_for_class_extends<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
    this_t: &Type,
) -> Result<Vec<Type>, FlowJsException> {
    possible_concrete_types(
        ConcretizationKind::ConcretizeForClassExtends(this_t.dupe()),
        cx,
        env,
        reason,
        t,
    )
}

pub(super) fn possible_concrete_types_for_enum_exhaustive_check<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
) -> Result<Vec<Type>, FlowJsException> {
    possible_concrete_types(
        ConcretizationKind::ConcretizeForEnumExhaustiveCheck,
        cx,
        env,
        reason,
        t,
    )
}

pub(super) fn singleton_concrete_type_for_cjs_extract_named_exports_and_type_exports<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
) -> Result<Type, FlowJsException> {
    singleton_concrete_type(
        ConcretizationKind::ConcretizeForCJSExtractNamedExportsAndTypeExports,
        cx,
        env,
        reason,
        t,
    )
}

pub(super) fn singleton_concretize_type_for_imports_exports<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
) -> Result<Type, FlowJsException> {
    singleton_concrete_type(
        ConcretizationKind::ConcretizeForImportsExports,
        cx,
        env,
        reason,
        t,
    )
}

pub(super) fn possible_concrete_types_for_imports_exports<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
) -> Result<Vec<Type>, FlowJsException> {
    possible_concrete_types(
        ConcretizationKind::ConcretizeForImportsExports,
        cx,
        env,
        reason,
        t,
    )
}

pub(super) fn singleton_concrete_type_for_inspection<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
) -> Result<Type, FlowJsException> {
    singleton_concrete_type(
        ConcretizationKind::ConcretizeForInspection,
        cx,
        env,
        reason,
        t,
    )
}

pub(super) fn singleton_concrete_type_for_type_cast<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    _reason: &Reason,
    t: &Type,
) -> Result<Type, FlowJsException> {
    use flow_typing_type::type_::constraint::Constraints;
    fn resolve<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        t: &Type,
    ) -> Result<Type, FlowJsException> {
        match t.deref() {
            TypeInner::AnnotT(r, inner, use_desc) => {
                let repositioned = reposition_reason(cx, env, None, r, *use_desc, inner)?;
                resolve(cx, env, &repositioned)
            }
            _ if let Some(node) = type_util::constraint_node_id(t) => {
                let (_root_id, constraints) = cx.find_constraints(node.id());
                match constraints {
                    Constraints::Resolved(t1) => resolve(cx, env, &t1),
                    Constraints::FullyResolved(s1) => {
                        let t1 = cx.force_fully_resolved_tvar(&s1);
                        resolve(cx, env, &t1)
                    }
                    Constraints::Unresolved(_) => Ok(t.dupe()),
                }
            }
            _ => Ok(t.dupe()),
        }
    }
    resolve(cx, env, t)
}

pub(super) fn add_specialized_callee_method_action(
    env: &FlowJsEnv,
    l: &Type,
    action: &MethodAction<Cx>,
) -> Result<(), FlowJsException> {
    match action {
        MethodAction::CallM(box CallMData {
            specialized_callee, ..
        })
        | MethodAction::ChainM(box ChainMData {
            specialized_callee, ..
        }) => {
            callee_recorder::add_callee(
                env,
                callee_recorder::Kind::All,
                l.dupe(),
                specialized_callee.as_ref(),
            );
            Ok(())
        }
        MethodAction::NoMethodAction(prop_t) => {
            prop_t.add(l.dupe());
            Ok(())
        }
    }
}

pub(super) fn possible_concrete_types_for_predicate<'cx>(
    predicate_concretizer_variant: PredicateConcretetizerVariant,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
) -> Result<Vec<Type>, FlowJsException> {
    possible_concrete_types(
        ConcretizationKind::ConcretizeForPredicate(predicate_concretizer_variant),
        cx,
        env,
        reason,
        t,
    )
}

pub(super) fn possible_concrete_types_for_sentinel_prop_test<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
) -> Result<Vec<Type>, FlowJsException> {
    possible_concrete_types(
        ConcretizationKind::ConcretizeForSentinelPropTest,
        cx,
        env,
        reason,
        t,
    )
}

pub(super) fn all_possible_concrete_types<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
) -> Result<Vec<Type>, FlowJsException> {
    possible_concrete_types(ConcretizationKind::ConcretizeAll, cx, env, reason, t)
}

pub(super) fn possible_concrete_types_for_constructor_return<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
) -> Result<Vec<Type>, FlowJsException> {
    possible_concrete_types(
        ConcretizationKind::ConcretizeForConstructorReturn,
        cx,
        env,
        reason,
        t,
    )
}

pub(super) fn possible_concrete_types_for_unused_promise_check<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
) -> Result<Vec<Type>, FlowJsException> {
    possible_concrete_types(
        ConcretizationKind::ConcretizeForUnusedPromiseCheck,
        cx,
        env,
        reason,
        t,
    )
}

pub(super) fn possible_concrete_types_for_operators_checking<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
) -> Result<Vec<Type>, FlowJsException> {
    possible_concrete_types(
        ConcretizationKind::ConcretizeForOperatorsChecking,
        cx,
        env,
        reason,
        t,
    )
}

pub(super) fn possible_concrete_types_for_object_assign<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
) -> Result<Vec<Type>, FlowJsException> {
    possible_concrete_types(
        ConcretizationKind::ConcretizeForObjectAssign,
        cx,
        env,
        reason,
        t,
    )
}

pub(super) fn possible_concrete_types_for_destructuring<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
) -> Result<Vec<Type>, FlowJsException> {
    possible_concrete_types(
        ConcretizationKind::ConcretizeForDestructuring,
        cx,
        env,
        reason,
        t,
    )
}

pub(super) fn possible_concrete_types_for_computed_object_keys<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
) -> Result<Vec<Type>, FlowJsException> {
    possible_concrete_types(
        ConcretizationKind::ConcretizeForComputedObjectKeys,
        cx,
        env,
        reason,
        t,
    )
}

pub(super) fn singleton_concrete_type_for_match_arg<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    keep_unions: bool,
    reason: &Reason,
    t: &Type,
) -> Result<Type, FlowJsException> {
    singleton_concrete_type(
        ConcretizationKind::ConcretizeForMatchArg { keep_unions },
        cx,
        env,
        reason,
        t,
    )
}

pub(super) fn possible_concrete_types_for_match_arg<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    keep_unions: bool,
    reason: &Reason,
    t: &Type,
) -> Result<Vec<Type>, FlowJsException> {
    possible_concrete_types(
        ConcretizationKind::ConcretizeForMatchArg { keep_unions },
        cx,
        env,
        reason,
        t,
    )
}
