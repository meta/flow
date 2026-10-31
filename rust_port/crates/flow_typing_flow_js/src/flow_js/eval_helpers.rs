/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::RefCell;
use std::rc::Rc;

use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_type::type_::ArrRestTData;
use flow_typing_type::type_::ConditionalTData;
use flow_typing_type::type_::DestructorConditionalTypeData;
use flow_typing_type::type_::DestructorMappedTypeData;
use flow_typing_type::type_::DestructorSpreadTupleTypeData;
use flow_typing_type::type_::DestructorSpreadTypeData;
use flow_typing_type::type_::EvalTypeDestructorTData;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::GetElemTData;
use flow_typing_type::type_::GetEnumTData;
use flow_typing_type::type_::LookupDefaultResult;
use flow_typing_type::type_::LookupTData;
use flow_typing_type::type_::MapTypeTData;
use flow_typing_type::type_::NonstrictReturningData;
use flow_typing_type::type_::OptionalIndexedAccessTData;
use flow_typing_type::type_::ReactKitTData;
use flow_typing_type::type_::ResolveSpreadTData;
use flow_typing_type::type_::ResolveUnionTData;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::object::ObjectToolObjectMapData;
use flow_typing_type::type_::type_collector::TypeCollector;

// Disambiguate helpers vs mod re-exports
use super::constraint_helpers::frozen_implicit_instantiation_tvar;
use super::helpers::mk_typeapp_instance_annot;
use super::helpers::*;
use super::*;
use crate::intersection_kit::Normalization;
use crate::intersection_kit::Normalizer;

pub(super) fn eval_selector<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    annot: bool,
    reason: &Reason,
    curr_t: &Type,
    selector: &Selector,
    tvar: &Tvar,
    id: i32,
) -> Result<(), FlowJsException> {
    use flow_common::reason::VirtualReasonDesc;
    use flow_data_structure_wrapper::ord_set::FlowOrdSet;
    use flow_typing_flow_common::obj_type;
    use flow_typing_type::type_::DefT;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::LookupAction;
    use flow_typing_type::type_::LookupKind;
    use flow_typing_type::type_::ReadPropData;
    use flow_typing_type::type_::hint_unavailable;
    use flow_typing_type::type_util::mk_named_prop;

    let use_t = match selector {
        Selector::Prop(name_str, has_default) => {
            let name = Name::new(name_str.dupe());
            let lookup_ub = || -> UseT<Cx> {
                let use_op = unknown_use();
                let action = LookupAction::ReadProp(Box::new(ReadPropData {
                    use_op: use_op.dupe(),
                    obj_t: curr_t.dupe(),
                    tout: tvar.dupe(),
                }));
                let default_tout = LookupDefaultResult::Flow {
                    use_op: use_op.dupe(),
                    tout: Type::new(TypeInner::OpenT(tvar.dupe())),
                };
                let void_reason = tvar.reason().dupe().replace_desc(VirtualReasonDesc::RVoid);
                let lookup_kind = LookupKind::NonstrictReturning(Box::new(NonstrictReturningData(
                    Some((
                        Type::new(TypeInner::DefT(void_reason, DefT::new(DefTInner::VoidT))),
                        default_tout,
                    )),
                    None,
                )));
                UseT::new(UseTInner::LookupT(Box::new(LookupTData {
                    reason: reason.dupe(),
                    lookup_kind: Box::new(lookup_kind),
                    indexer_fallback: None,
                    try_ts_on_failure: Rc::from([]),
                    propref: Box::new(mk_named_prop(reason.dupe(), false, name.dupe())),
                    lookup_action: Box::new(action),
                    method_accessible: false,
                    ids: Some(FlowOrdSet::new()),
                    ignore_dicts: false,
                })))
            };
            let getprop_ub = || -> UseT<Cx> {
                UseT::new(UseTInner::GetPropT(Box::new(GetPropTData {
                    use_op: unknown_use(),
                    reason: reason.dupe(),
                    id: Some(id),
                    from_annot: annot,
                    skip_optional: false,
                    propref: Box::new(mk_named_prop(reason.dupe(), false, name.dupe())),
                    tout: Box::new(tvar.dupe()),
                    hint: hint_unavailable(),
                })))
            };
            if *has_default {
                match curr_t.deref() {
                    TypeInner::DefT(_, def_t) if matches!(&**def_t, DefTInner::NullT) => {
                        getprop_ub()
                    }
                    TypeInner::DefT(_, def_t) => {
                        if let DefTInner::ObjT(obj) = &**def_t {
                            if matches!(obj.proto_t.deref(), TypeInner::ObjProtoT(_))
                                && obj_type::is_exact(&obj.flags.obj_kind)
                            {
                                lookup_ub()
                            } else {
                                getprop_ub()
                            }
                        } else {
                            getprop_ub()
                        }
                    }
                    _ => getprop_ub(),
                }
            } else {
                getprop_ub()
            }
        }
        Selector::Elem(key_t) => UseT::new(UseTInner::GetElemT(Box::new(GetElemTData {
            use_op: unknown_use(),
            reason: reason.dupe(),
            id: None,
            from_annot: annot,
            skip_optional: false,
            access_iterables: false,
            key_t: key_t.dupe(),
            tout: Box::new(tvar.dupe()),
        }))),
        Selector::ObjRest(xs) => UseT::new(UseTInner::ObjRestT(
            reason.dupe(),
            xs.iter().map(|x| x.to_string()).collect(),
            Type::new(TypeInner::OpenT(tvar.dupe())),
            id,
            true,
        )),
        Selector::ArrRest(i) => UseT::new(UseTInner::ArrRestT(Box::new(ArrRestTData {
            literal: true,
            use_op: unknown_use(),
            reason: reason.dupe(),
            index: *i,
            tout: Type::new(TypeInner::OpenT(tvar.dupe())),
        }))),
        Selector::Default => UseT::new(UseTInner::FilterOptionalT(
            unknown_use(),
            Type::new(TypeInner::OpenT(tvar.dupe())),
        )),
    };
    flow_opt(cx, env, trace, (curr_t, &use_t))
}

pub(super) fn evaluate_type_destructor<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    reason: &Reason,
    t: &Type,
    d: &Destructor,
    tvar: &Tvar,
) -> Result<(), FlowJsException> {
    match evaluate_type_destructor_(cx, env, trace, use_op.dupe(), reason, t, d, tvar) {
        Ok(()) => Ok(()),
        Err(FlowJsException::LimitExceeded) => {
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::ERecursionLimit(reason.loc().dupe()),
            )?;
            rec_flow_t(
                cx,
                env,
                trace,
                unknown_use(),
                (
                    &any_t::why(AnySource::AnyError(None), reason.dupe()),
                    &Type::new(TypeInner::OpenT(tvar.dupe())),
                ),
            )
        }
        Err(e) => Err(e),
    }
}

fn evaluate_type_destructor_<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    reason: &Reason,
    t: &Type,
    d: &Destructor,
    tvar: &Tvar,
) -> Result<(), FlowJsException> {
    if matches!(d, Destructor::ConditionalType(_)) {
        let nested_env = env.solving_implicit_instantiation();
        if frozen_implicit_instantiation_tvar(cx, &nested_env, t).is_some() {
            return eval_destructor(cx, &nested_env, trace, use_op, reason, t, d, tvar);
        }
    }

    // As an optimization, unwrap resolved tvars so that they are only evaluated
    // once to an annotation instead of a tvar that gets a bound on both sides.
    let t = super::helpers::drop_resolved(cx, env, t);
    match t.deref() {
        _ if type_util::constraint_node_id(&t).is_some() => {
            let x = UseT::new(UseTInner::EvalTypeDestructorT(Box::new(
                EvalTypeDestructorTData {
                    destructor_use_op: use_op,
                    reason: reason.dupe(),
                    repos: None,
                    destructor: Box::new(d.clone()),
                    tout: Box::new(tvar.dupe()),
                },
            )));
            rec_flow(cx, env, trace, (&t, &x))
        }
        TypeInner::GenericT(box GenericTData {
            bound,
            reason: g_reason,
            name,
            id,
            no_infer,
        }) if type_util::constraint_node_id(bound).is_some() => {
            let x = UseT::new(UseTInner::EvalTypeDestructorT(Box::new(
                EvalTypeDestructorTData {
                    destructor_use_op: use_op,
                    reason: reason.dupe(),
                    repos: None,
                    destructor: Box::new(d.clone()),
                    tout: Box::new(tvar.dupe()),
                },
            )));
            rec_flow(cx, env, trace, (&t, &x))
        }
        TypeInner::GenericT(box GenericTData {
            bound,
            reason: g_reason,
            name,
            id,
            no_infer,
        }) => {
            if let TypeInner::AnnotT(r, inner_t, use_desc) = bound.deref() {
                let x = UseT::new(UseTInner::EvalTypeDestructorT(Box::new(
                    EvalTypeDestructorTData {
                        destructor_use_op: use_op,
                        reason: g_reason.dupe(),
                        repos: Some((r.dupe(), *use_desc)),
                        destructor: Box::new(d.clone()),
                        tout: Box::new(tvar.dupe()),
                    },
                )));
                let generic_t = Type::new(TypeInner::GenericT(Box::new(GenericTData {
                    reason: g_reason.dupe(),
                    name: name.dupe(),
                    id: id.clone(),
                    bound: inner_t.dupe(),
                    no_infer: *no_infer,
                })));
                rec_flow(cx, env, trace, (&generic_t, &x))
            } else {
                eval_destructor(cx, env, trace, use_op, reason, &t, d, tvar)
            }
        }
        TypeInner::EvalT { .. } => {
            let x = UseT::new(UseTInner::EvalTypeDestructorT(Box::new(
                EvalTypeDestructorTData {
                    destructor_use_op: use_op,
                    reason: reason.dupe(),
                    repos: None,
                    destructor: Box::new(d.clone()),
                    tout: Box::new(tvar.dupe()),
                },
            )));
            rec_flow(cx, env, trace, (&t, &x))
        }
        TypeInner::AnnotT(r, inner_t, use_desc) => {
            let x = UseT::new(UseTInner::EvalTypeDestructorT(Box::new(
                EvalTypeDestructorTData {
                    destructor_use_op: use_op,
                    reason: reason.dupe(),
                    repos: Some((r.dupe(), *use_desc)),
                    destructor: Box::new(d.clone()),
                    tout: Box::new(tvar.dupe()),
                },
            )));
            rec_flow(cx, env, trace, (inner_t, &x))
        }
        _ => eval_destructor(cx, env, trace, use_op, reason, &t, d, tvar),
    }
}

pub(super) fn mk_type_destructor<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    reason: &Reason,
    t: &Type,
    d: &Destructor,
    id: eval::Id,
) -> Result<Type, FlowJsException> {
    let evaluated = cx.evaluated();
    let conditional_has_implicit_instantiation_tvar = matches!(d, Destructor::ConditionalType(_))
        && frozen_implicit_instantiation_tvar(cx, &env.solving_implicit_instantiation(), t)
            .is_some();
    if !conditional_has_implicit_instantiation_tvar && let Some(cached_t) = evaluated.get(&id) {
        return Ok(cached_t.dupe());
    }
    if id.from_type_sig()
        && !conditional_has_implicit_instantiation_tvar
        && type_subst::free_var_finder(
            cx,
            None,
            &Type::new(TypeInner::EvalT {
                type_: t.dupe(),
                defer_use_t: TypeDestructorT::new(TypeDestructorTInner(
                    use_op.dupe(),
                    reason.dupe(),
                    Rc::new(d.clone()),
                )),
                id: id.dupe(),
            }),
        )
        .is_empty()
    {
        let use_op_clone = use_op.dupe();
        let reason_clone = reason.dupe();
        let d_clone = d.clone();
        let trace = DepthTrace::dummy_trace();
        let evaluation_error = Rc::new(RefCell::new(None));
        let evaluation_error_for_map = Rc::clone(&evaluation_error);
        let eval_t_fallback = Type::new(TypeInner::EvalT {
            type_: t.dupe(),
            defer_use_t: TypeDestructorT::new(TypeDestructorTInner(
                use_op.dupe(),
                reason.dupe(),
                Rc::new(d.clone()),
            )),
            id: id.dupe(),
        });
        let result = flow_js_utils::map_on_resolved_type(
            cx,
            reason.dupe(),
            t.dupe(),
            move |cx, t| -> Result<Type, flow_utils_concurrency::job_error::JobError> {
                let env = &FlowJsEnv::entry();
                match crate::tvar_resolver::mk_tvar_and_fully_resolve_no_wrap_where(
                    cx,
                    reason_clone.dupe(),
                    |cx, tvar_reason, tvar_id| {
                        let tvar = Tvar::new(tvar_reason.dupe(), tvar_id as u32);
                        let errors = cx.errors();
                        let cache_snapshot = cx.take_cache_snapshot();
                        let result = evaluate_type_destructor(
                            cx,
                            env,
                            trace,
                            use_op_clone.dupe(),
                            &reason_clone,
                            &t,
                            &d_clone,
                            &tvar,
                        );
                        cx.restore_cache_snapshot(cache_snapshot);
                        cx.reset_errors(errors);
                        result
                    },
                ) {
                    Ok(result) => Ok(result),
                    Err(err) => {
                        *evaluation_error_for_map.borrow_mut() = Some(err);
                        Ok(eval_t_fallback.dupe())
                    }
                }
            },
        );
        if let Some(err) = evaluation_error.borrow_mut().take() {
            return Err(err);
        }
        let mut evaluated = cx.evaluated();
        evaluated.insert(id, result.dupe());
        cx.set_evaluated(evaluated);
        Ok(result)
    } else {
        let use_op_clone = use_op.dupe();
        let reason_clone = reason.dupe();
        let t_clone = t.dupe();
        let d_clone = d.clone();
        let id_clone = id.dupe();
        let result = flow_typing_tvar::mk_no_wrap_where(
            cx,
            reason.dupe(),
            move |cx, tvar_reason, tvar_id| {
                let tvar = Tvar::new(tvar_reason.dupe(), tvar_id as u32);
                let mut evaluated = cx.evaluated();
                evaluated.insert(id_clone, Type::new(TypeInner::OpenT(tvar.dupe())));
                cx.set_evaluated(evaluated);
                evaluate_type_destructor(
                    cx,
                    env,
                    trace,
                    use_op_clone,
                    &reason_clone,
                    &t_clone,
                    &d_clone,
                    &tvar,
                )
            },
        )?;
        if !flow_js_utils::tvar_visitors::has_unresolved_tvars(cx, t)
            && !flow_js_utils::tvar_visitors::has_unresolved_tvars_in_destructors(cx, d)
        {
            crate::tvar_resolver::resolve(
                cx,
                crate::tvar_resolver::default_no_lowers,
                true,
                &result,
            );
        }
        Ok(result)
    }
}

pub(super) fn eval_destructor<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    reason: &Reason,
    t: &Type,
    d: &Destructor,
    tout: &Tvar,
) -> Result<(), FlowJsException> {
    // Normalization concretizes the members of the intersection itself instead of
    // dispatching on the operand like the rest of this function, so we handle it upfront.
    if let Destructor::NormalizeIntersection { check_exact } = d {
        let normalized =
            match Normalizer::new(env, reason).normalize(cx, t, |cx, member_reason, member| {
                possible_concrete_types_for_inspection(cx, env, member_reason, member)
            })? {
                Normalization::Normalized(normalized) => normalized,
                Normalization::InvalidExact(normalized, invalid) => {
                    if *check_exact {
                        flow_js_utils::add_output_with_env(
                            cx,
                            env,
                            ErrorMessage::EInvalidExactObjectIntersection(invalid),
                        )?;
                    }
                    normalized
                }
                Normalization::Unchanged => t.dupe(),
            };
        return rec_flow_t(
            cx,
            env,
            trace,
            unknown_use(),
            (&normalized, &Type::new(TypeInner::OpenT(tout.dupe()))),
        );
    }

    if let Destructor::MappedType(box DestructorMappedTypeData {
        homomorphic: MappedTypeHomomorphicFlag::Unspecialized,
        mapped_type_flags,
        property_type,
        name_type,
        distributive_tparam_name: _,
    }) = d
    {
        // Non-homomorphic mapped types have their own special resolution code, so they do not fit well
        // into the structure of the rest of this function. We handle them upfront instead.
        let mapped_t = crate::object_kit::mapped_type_of_keys(
            cx,
            env,
            trace,
            use_op.dupe(),
            reason,
            t,
            property_type,
            name_type.as_ref(),
            mapped_type_flags,
        );
        let mapped_t = mapped_t?;
        // Intentional unknown_use for the tout Flow
        return rec_flow(
            cx,
            env,
            trace,
            (
                &mapped_t,
                &UseT::new(UseTInner::UseT(
                    unknown_use(),
                    Type::new(TypeInner::OpenT(tout.dupe())),
                )),
            ),
        );
    }

    let destruct_union = |cx: &mut Context<'cx>,
                          f: Option<Box<dyn Fn(Type) -> Type>>,
                          r: Reason,
                          members: Vec<Type>,
                          upper: UseT<Cx>|
     -> Result<(), FlowJsException> {
        let f: Box<dyn Fn(Type) -> Type> = f.unwrap_or_else(|| Box::new(|t| t));
        let destructor = TypeDestructorT::new(TypeDestructorTInner(
            use_op.dupe(),
            reason.dupe(),
            Rc::new(d.clone()),
        ));
        // ResolveUnionT resolves in reverse order, so rev_map here so we
        // resolve in the original order.
        let mut unresolved = members
            .into_iter()
            .rev()
            .map(|t| flow_cache::eval::id(cx, f(t), destructor.dupe()));
        let first = unresolved.next().unwrap();
        let u = UseT::new(UseTInner::ResolveUnionT(Box::new(ResolveUnionTData {
            reason: r,
            unresolved: unresolved.collect(),
            resolved: flow_data_structure_wrapper::list::FlowOcamlList::new(),
            upper: Box::new(upper),
            id: flow_common::reason::mk_id() as i32,
        })));
        rec_flow(cx, env, trace, (&first, &u))
    };

    let destruct_maybe = |cx: &mut Context<'cx>,
                          f: Option<Box<dyn Fn(Type) -> Type>>,
                          r: Reason,
                          inner_t: Type,
                          upper: UseT<Cx>|
     -> Result<(), FlowJsException> {
        let null_or_void_reason = r.dupe().replace_desc_new(VirtualReasonDesc::RNullOrVoid);
        let null = null::make(null_or_void_reason.dupe());
        let void = void::make(null_or_void_reason.dupe());
        destruct_union(cx, f, null_or_void_reason, vec![inner_t, null, void], upper)
    };

    let destruct_optional = |cx: &mut Context<'cx>,
                             f: Option<Box<dyn Fn(Type) -> Type>>,
                             r: Reason,
                             inner_t: Type,
                             upper: UseT<Cx>|
     -> Result<(), FlowJsException> {
        let void_reason = r.dupe().replace_desc_new(VirtualReasonDesc::RVoid);
        let void = void::make(void_reason.dupe());
        destruct_union(cx, f, void_reason, vec![inner_t, void], upper)
    };

    let destruct_and_preserve_nominal_t = |cx: &mut Context<'cx>,
                                           r: &Reason,
                                           nominal_type: &Rc<NominalType>|
     -> Result<(), FlowJsException> {
        let eval_t = |cx: &mut Context<'cx>, inner_t: &Type| -> Result<Type, FlowJsException> {
            let tvar_id = flow_typing_tvar::mk_no_wrap(cx, reason);
            // We have to eagerly evaluate these destructors when possible because
            // various other systems, like type_filter, expect NominalT underlying_t upper_t,
            // and lower_t to be inspectable
            eagerly_eval_destructor_if_resolved(
                cx,
                env,
                trace,
                use_op.dupe(),
                reason,
                inner_t,
                d,
                tvar_id,
            )
        };

        let underlying_t = match &nominal_type.underlying_t {
            nominal::UnderlyingT::FullyOpaque => nominal::UnderlyingT::FullyOpaque,
            nominal::UnderlyingT::CustomError(box nominal::CustomErrorData {
                t: inner_t,
                custom_error_loc,
            }) => nominal::UnderlyingT::CustomError(Box::new(nominal::CustomErrorData {
                t: eval_t(cx, inner_t)?,
                custom_error_loc: custom_error_loc.dupe(),
            })),
            nominal::UnderlyingT::OpaqueWithLocal { t: inner_t } => {
                nominal::UnderlyingT::OpaqueWithLocal {
                    t: eval_t(cx, inner_t)?,
                }
            }
        };
        let lower_t = match nominal_type.lower_t.as_ref() {
            Some(lt) => Some(eval_t(cx, lt)?),
            None => None,
        };
        let upper_t = match nominal_type.upper_t.as_ref() {
            Some(ut) => Some(eval_t(cx, ut)?),
            None => None,
        };
        let nominal_t = Type::new(TypeInner::NominalT {
            reason: r.dupe(),
            nominal_type: Rc::new(NominalType::new(NominalTypeInner {
                nominal_id: nominal_type.nominal_id.clone(),
                underlying_t,
                lower_t,
                upper_t,
                nominal_type_args: nominal_type.nominal_type_args.dupe(),
            })),
        });
        rec_flow_t(
            cx,
            env,
            trace,
            use_op.dupe(),
            (&nominal_t, &Type::new(TypeInner::OpenT(tout.dupe()))),
        )
    };

    let should_destruct_union = |cx: &mut Context<'cx>| -> bool {
        match d {
            Destructor::ConditionalType(box DestructorConditionalTypeData {
                distributive_tparam_name,
                ..
            }) => distributive_tparam_name.is_some(),
            Destructor::ReactDRO(_) if let TypeInner::UnionT(_, rep) = t.deref() => {
                if !rep.is_optimized_finally() {
                    rep.optimize_enum_only(cx, |cx, members| {
                        flow_typing_visitors::type_mapper::union_flatten(cx, members.duped())
                    });
                }
                rep.check_enum().is_none()
            }
            _ => true,
        }
    };

    match (t.deref(), d) {
        (
            TypeInner::GenericT(box GenericTData {
                bound,
                reason: r,
                id: g_id,
                name,
                no_infer,
            }),
            _,
        ) if let TypeInner::NominalT { nominal_type, .. } = bound.deref()
            && let nominal::UnderlyingT::OpaqueWithLocal { t: inner_t } =
                &nominal_type.underlying_t
            && matches!(&nominal_type.nominal_id, nominal::Id::UserDefinedOpaqueTypeId(box nominal::UserDefinedOpaqueTypeIdData(aloc_id, _))
                if aloc_id.0.source() == Some(cx.file())) =>
        {
            let new_generic = Type::new(TypeInner::GenericT(Box::new(GenericTData {
                bound: inner_t.dupe(),
                reason: r.dupe(),
                id: g_id.clone(),
                name: name.dupe(),
                no_infer: *no_infer,
            })));
            eval_destructor(cx, env, trace, use_op, reason, &new_generic, d, tout)
        }
        (
            TypeInner::GenericT(box GenericTData {
                bound,
                reason: r,
                id: g_id,
                name,
                no_infer,
            }),
            _,
        ) if let TypeInner::NominalT { nominal_type, .. } = bound.deref()
            && let nominal::UnderlyingT::CustomError(box nominal::CustomErrorData {
                t: inner_t,
                custom_error_loc: _,
            }) = &nominal_type.underlying_t =>
        {
            let new_generic = Type::new(TypeInner::GenericT(Box::new(GenericTData {
                bound: inner_t.dupe(),
                reason: r.dupe(),
                id: g_id.clone(),
                name: name.dupe(),
                no_infer: *no_infer,
            })));
            eval_destructor(cx, env, trace, use_op, reason, &new_generic, d, tout)
        }
        (
            TypeInner::NominalT {
                reason: r,
                nominal_type,
            },
            Destructor::ReactDRO(_),
        ) => destruct_and_preserve_nominal_t(cx, r, nominal_type),
        (
            TypeInner::NominalT {
                reason: _,
                nominal_type,
            },
            _,
        ) if matches!(&nominal_type.nominal_id, nominal::Id::UserDefinedOpaqueTypeId(box nominal::UserDefinedOpaqueTypeIdData(aloc_id, _))
                    if aloc_id.0.source() == Some(cx.file()))
            && let nominal::UnderlyingT::OpaqueWithLocal { t: inner_t } =
                &nominal_type.underlying_t =>
        {
            eval_destructor(cx, env, trace, use_op, reason, inner_t, d, tout)
        }
        (
            TypeInner::NominalT {
                reason: _,
                nominal_type,
            },
            _,
        ) if matches!(
            &nominal_type.nominal_id,
            nominal::Id::UserDefinedOpaqueTypeId(_)
        ) && let nominal::UnderlyingT::CustomError(box nominal::CustomErrorData {
            t: inner_t,
            custom_error_loc: _,
        }) = &nominal_type.underlying_t =>
        {
            eval_destructor(cx, env, trace, use_op, reason, inner_t, d, tout)
        }
        // Specialize TypeAppTs before evaluating them so that we can handle special
        // cases. Like the union case below. mk_typeapp_instance will return an AnnotT
        // which will be fully resolved using the AnnotT case above.
        (
            TypeInner::GenericT(box GenericTData {
                bound,
                reason: reason_tapp,
                id: g_id,
                name,
                no_infer,
            }),
            _,
        ) if let TypeInner::TypeAppT(box TypeAppTData {
            reason: _,
            use_op: use_op_tapp,
            type_: c,
            targs,
            from_value,
            use_desc: _,
        }) = bound.deref() =>
        {
            let destructor = TypeDestructorT::new(TypeDestructorTInner(
                use_op.dupe(),
                reason.dupe(),
                Rc::new(d.clone()),
            ));
            let specialized_t = mk_typeapp_instance_annot(
                cx,
                env,
                Some(trace),
                use_op_tapp.dupe(),
                reason,
                reason_tapp,
                *from_value,
                None,
                c,
                targs.dupe(),
            )?;
            let generic_t = Type::new(TypeInner::GenericT(Box::new(GenericTData {
                bound: specialized_t,
                name: name.dupe(),
                id: g_id.clone(),
                reason: reason_tapp.dupe(),
                no_infer: *no_infer,
            })));
            let eval_t = flow_cache::eval::id(cx, generic_t, destructor);
            rec_flow(
                cx,
                env,
                trace,
                (
                    &eval_t,
                    &UseT::new(UseTInner::UseT(
                        use_op.dupe(),
                        Type::new(TypeInner::OpenT(tout.dupe())),
                    )),
                ),
            )
        }
        (
            TypeInner::TypeAppT(box TypeAppTData {
                reason: reason_tapp,
                use_op: use_op_tapp,
                type_: c,
                targs,
                from_value,
                use_desc: _,
            }),
            _,
        ) => {
            // A type alias that keeps expanding within its own evaluation, like `FlatArray`
            // whose depth argument stays unresolved in an implicit instantiation, would
            // otherwise be expanded until the recursion limit.
            let Some(env) = instantiation_utils::type_app_expansion::push_unless_loop(
                cx,
                env,
                type_app_expansion::Bound::Destructed,
                c,
                targs,
            ) else {
                return rec_flow_t(
                    cx,
                    env,
                    trace,
                    unknown_use(),
                    (
                        &any_t::error(reason.dupe()),
                        &Type::new(TypeInner::OpenT(tout.dupe())),
                    ),
                );
            };
            let env = &env;
            let destructor = TypeDestructorT::new(TypeDestructorTInner(
                use_op.dupe(),
                reason.dupe(),
                Rc::new(d.clone()),
            ));
            let specialized_t = mk_typeapp_instance_annot(
                cx,
                env,
                Some(trace),
                use_op_tapp.dupe(),
                reason,
                reason_tapp,
                *from_value,
                None,
                c,
                targs.dupe(),
            )?;
            let eval_t = flow_cache::eval::id(cx, specialized_t, destructor);
            rec_flow_t(
                cx,
                env,
                trace,
                unknown_use(),
                (&eval_t, &Type::new(TypeInner::OpenT(tout.dupe()))),
            )
        }

        // If we are destructuring a union, evaluating the destructor on the union
        // itself may have the effect of splitting the union into separate lower
        // bounds, which prevents the speculative match process from working.
        // Instead, we preserve the union by pushing down the destructor onto the
        // branches of the unions.
        (TypeInner::UnionT(_, rep), _) if should_destruct_union(cx) => {
            let members: Vec<Type> = rep.members_iter().duped().collect();
            destruct_union(
                cx,
                None,
                reason.dupe(),
                members,
                UseT::new(UseTInner::UseT(
                    unknown_use(),
                    Type::new(TypeInner::OpenT(tout.dupe())),
                )),
            )
        }
        (
            TypeInner::GenericT(box GenericTData {
                reason: _,
                bound,
                id: g_id,
                name,
                no_infer,
            }),
            _,
        ) if let TypeInner::UnionT(_, rep) = bound.deref()
            && should_destruct_union(cx) =>
        {
            let g_id = g_id.clone();
            let name = name.dupe();
            let no_infer = *no_infer;
            let members: Vec<Type> = rep.members_iter().duped().collect();
            destruct_union(
                cx,
                Some(Box::new(move |bound: Type| {
                    Type::new(TypeInner::GenericT(Box::new(GenericTData {
                        reason: reason_of_t(&bound).dupe(),
                        bound,
                        id: g_id.clone(),
                        name: name.dupe(),
                        no_infer,
                    })))
                })),
                reason.dupe(),
                members,
                UseT::new(UseTInner::UseT(
                    use_op.dupe(),
                    Type::new(TypeInner::OpenT(tout.dupe())),
                )),
            )
        }
        (TypeInner::MaybeT(r, inner_t), _) if should_destruct_union(cx) => destruct_maybe(
            cx,
            None,
            r.dupe(),
            inner_t.dupe(),
            UseT::new(UseTInner::UseT(
                unknown_use(),
                Type::new(TypeInner::OpenT(tout.dupe())),
            )),
        ),
        (
            TypeInner::GenericT(box GenericTData {
                reason: g_reason,
                bound,
                id: g_id,
                name,
                no_infer,
            }),
            _,
        ) if let TypeInner::MaybeT(_, inner_t) = bound.deref()
            && should_destruct_union(cx) =>
        {
            let g_id = g_id.clone();
            let name = name.dupe();
            let no_infer = *no_infer;
            destruct_maybe(
                cx,
                Some(Box::new(move |bound: Type| {
                    Type::new(TypeInner::GenericT(Box::new(GenericTData {
                        reason: reason_of_t(&bound).dupe(),
                        bound,
                        id: g_id.clone(),
                        name: name.dupe(),
                        no_infer,
                    })))
                })),
                g_reason.dupe(),
                inner_t.dupe(),
                UseT::new(UseTInner::UseT(
                    use_op.dupe(),
                    Type::new(TypeInner::OpenT(tout.dupe())),
                )),
            )
        }
        (
            TypeInner::OptionalT {
                reason: r,
                type_: inner_t,
                use_desc: _,
            },
            _,
        ) if should_destruct_union(cx) => destruct_optional(
            cx,
            None,
            r.dupe(),
            inner_t.dupe(),
            UseT::new(UseTInner::UseT(
                unknown_use(),
                Type::new(TypeInner::OpenT(tout.dupe())),
            )),
        ),
        (
            TypeInner::GenericT(box GenericTData {
                reason: g_reason,
                bound,
                id: g_id,
                name,
                no_infer,
            }),
            _,
        ) if let TypeInner::OptionalT {
            reason: _,
            type_: inner_t,
            use_desc: _,
        } = bound.deref()
            && should_destruct_union(cx) =>
        {
            let g_id = g_id.clone();
            let name = name.dupe();
            let no_infer = *no_infer;
            destruct_optional(
                cx,
                Some(Box::new(move |bound: Type| {
                    Type::new(TypeInner::GenericT(Box::new(GenericTData {
                        reason: reason_of_t(&bound).dupe(),
                        bound,
                        id: g_id.clone(),
                        name: name.dupe(),
                        no_infer,
                    })))
                })),
                g_reason.dupe(),
                inner_t.dupe(),
                UseT::new(UseTInner::UseT(
                    use_op.dupe(),
                    Type::new(TypeInner::OpenT(tout.dupe())),
                )),
            )
        }
        (TypeInner::AnnotT(r, inner_t, use_desc), _) => {
            let repos_t = helpers::reposition_reason(cx, env, Some(trace), r, *use_desc, inner_t)?;
            let destructor = TypeDestructorT::new(TypeDestructorTInner(
                use_op.dupe(),
                reason.dupe(),
                Rc::new(d.clone()),
            ));
            let eval_t = flow_cache::eval::id(cx, repos_t, destructor);
            rec_flow_t(
                cx,
                env,
                trace,
                unknown_use(),
                (&eval_t, &Type::new(TypeInner::OpenT(tout.dupe()))),
            )
        }
        (
            TypeInner::GenericT(box GenericTData {
                bound,
                reason: r,
                id: g_id,
                name,
                no_infer,
            }),
            _,
        ) if let TypeInner::AnnotT(_, inner_t, use_desc) = bound.deref() => {
            let repos_t = helpers::reposition_reason(cx, env, Some(trace), r, *use_desc, inner_t)?;
            let destructor = TypeDestructorT::new(TypeDestructorTInner(
                use_op.dupe(),
                reason.dupe(),
                Rc::new(d.clone()),
            ));
            let generic_t = Type::new(TypeInner::GenericT(Box::new(GenericTData {
                reason: r.dupe(),
                id: g_id.clone(),
                name: name.dupe(),
                bound: repos_t,
                no_infer: *no_infer,
            })));
            let eval_t = flow_cache::eval::id(cx, generic_t, destructor);
            rec_flow_t(
                cx,
                env,
                trace,
                use_op.dupe(),
                (&eval_t, &Type::new(TypeInner::OpenT(tout.dupe()))),
            )
        }
        _ => {
            match d {
                // Already handled at the top of this function.
                Destructor::NormalizeIntersection { .. } => rec_flow_t(
                    cx,
                    env,
                    trace,
                    unknown_use(),
                    (t, &Type::new(TypeInner::OpenT(tout.dupe()))),
                ),
                Destructor::NonMaybeType => {
                    // We intentionally use `unknown_use` here! When we flow to a tout we never
                    // want to carry a `use_op`. We want whatever `use_op` the tout is used with
                    // to win.
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (
                            t,
                            &UseT::new(UseTInner::FilterMaybeT(
                                unknown_use(),
                                Type::new(TypeInner::OpenT(tout.dupe())),
                            )),
                        ),
                    )
                }
                Destructor::PropertyType { name } => {
                    let reason_op = reason
                        .dupe()
                        .replace_desc(VirtualReasonDesc::RProperty(Some(name.dupe())));
                    let u = UseT::new(UseTInner::GetPropT(Box::new(GetPropTData {
                        use_op: use_op.dupe(),
                        reason: reason.dupe(),
                        id: None,
                        from_annot: true,
                        skip_optional: false,
                        propref: Box::new(flow_typing_type::type_util::mk_named_prop(
                            reason_op,
                            false,
                            name.dupe(),
                        )),
                        tout: Box::new(tout.dupe()),
                        hint: hint_unavailable(),
                    })));
                    rec_flow(cx, env, trace, (t, &u))
                }
                Destructor::ElementType { index_type } => {
                    let u = UseT::new(UseTInner::GetElemT(Box::new(GetElemTData {
                        use_op: use_op.dupe(),
                        reason: reason.dupe(),
                        id: None,
                        from_annot: true,
                        skip_optional: false,
                        access_iterables: false,
                        key_t: index_type.dupe(),
                        tout: Box::new(tout.dupe()),
                    })));
                    rec_flow(cx, env, trace, (t, &u))
                }
                Destructor::OptionalIndexedAccessNonMaybeType { index } => rec_flow(
                    cx,
                    env,
                    trace,
                    (
                        t,
                        &UseT::new(UseTInner::OptionalIndexedAccessT(Box::new(
                            OptionalIndexedAccessTData {
                                use_op: use_op.dupe(),
                                reason: reason.dupe(),
                                index: index.dupe(),
                                tout_tvar: Box::new(tout.dupe()),
                            },
                        ))),
                    ),
                ),
                Destructor::OptionalIndexedAccessResultType { void_reason } => {
                    let void = void::why(void_reason.dupe());
                    let u = UseT::new(UseTInner::ResolveUnionT(Box::new(ResolveUnionTData {
                        reason: reason.dupe(),
                        resolved: flow_data_structure_wrapper::list::FlowOcamlList::unit(void),
                        unresolved: flow_data_structure_wrapper::list::FlowOcamlList::new(),
                        upper: Box::new(UseT::new(UseTInner::UseT(
                            unknown_use(),
                            Type::new(TypeInner::OpenT(tout.dupe())),
                        ))),
                        id: flow_common::reason::mk_id() as i32,
                    })));
                    rec_flow(cx, env, trace, (t, &u))
                }
                Destructor::SpreadType(box DestructorSpreadTypeData(
                    options,
                    todo_rev,
                    head_slice,
                )) => {
                    use flow_typing_type::type_::object;
                    let tool = object::ResolveTool::Resolve(object::Resolve::Next);
                    let acc: flow_data_structure_wrapper::list::FlowOcamlList<
                        object::spread::AccElement,
                    > = match head_slice {
                        Some(x) => flow_data_structure_wrapper::list::FlowOcamlList::unit(
                            object::spread::AccElement::InlineSlice(x.clone()),
                        ),
                        None => flow_data_structure_wrapper::list::FlowOcamlList::new(),
                    };
                    let state = object::spread::State {
                        todo_rev: todo_rev.dupe(),
                        acc,
                        spread_id: flow_common::reason::mk_id() as i32,
                        union_t: None,
                        curr_resolve_idx: 0,
                    };
                    let u = UseT::new(UseTInner::ObjKitT(
                        use_op.dupe(),
                        reason.dupe(),
                        Box::new(tool),
                        Box::new(object::Tool::Spread(Box::new((options.clone(), state)))),
                        Type::new(TypeInner::OpenT(tout.dupe())),
                    ));
                    rec_flow(cx, env, trace, (t, &u))
                }
                Destructor::SpreadTupleType(box DestructorSpreadTupleTypeData {
                    reason_tuple,
                    reason_spread: _,
                    inexact,
                    resolved: resolved_rev,
                    unresolved,
                }) => {
                    let elem_t = flow_typing_tvar::mk(cx, reason_tuple.dupe());
                    let u = UseT::new(UseTInner::ResolveSpreadT(Box::new(ResolveSpreadTData {
                        use_op: use_op.dupe(),
                        reason: reason_tuple.dupe(),
                        resolve_spread_type: Box::new(ResolveSpreadType {
                            rrt_resolved: resolved_rev.dupe(),
                            rrt_unresolved: unresolved.dupe(),
                            rrt_resolve_to: SpreadResolve::ResolveSpreadsToTupleType {
                                id: flow_common::reason::mk_id() as i32,
                                inexact: *inexact,
                                elem_t,
                                tout: Type::new(TypeInner::OpenT(tout.dupe())),
                            },
                        }),
                    })));
                    rec_flow(cx, env, trace, (t, &u))
                }
                Destructor::ReactCheckComponentConfig { props } => {
                    use flow_typing_type::type_::object;
                    let tool = object::ResolveTool::Resolve(object::Resolve::Next);
                    let u = UseT::new(UseTInner::ObjKitT(
                        use_op.dupe(),
                        reason.dupe(),
                        Box::new(tool),
                        Box::new(object::Tool::ReactCheckComponentConfig {
                            props: props.clone(),
                        }),
                        Type::new(TypeInner::OpenT(tout.dupe())),
                    ));
                    rec_flow(cx, env, trace, (t, &u))
                }
                Destructor::RestType(options, t_prime) => {
                    use flow_typing_type::type_::object;
                    let tool = object::ResolveTool::Resolve(object::Resolve::Next);
                    let state = object::rest::State::One(t_prime.dupe());
                    let u = UseT::new(UseTInner::ObjKitT(
                        use_op.dupe(),
                        reason.dupe(),
                        Box::new(tool),
                        Box::new(object::Tool::Rest(Box::new((options.clone(), state)))),
                        Type::new(TypeInner::OpenT(tout.dupe())),
                    ));
                    rec_flow(cx, env, trace, (t, &u))
                }
                Destructor::ExactType => {
                    use flow_typing_type::type_::object;
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (
                            t,
                            &UseT::new(UseTInner::ObjKitT(
                                use_op.dupe(),
                                reason.dupe(),
                                Box::new(object::ResolveTool::Resolve(object::Resolve::Next)),
                                Box::new(object::Tool::MakeExact),
                                Type::new(TypeInner::OpenT(tout.dupe())),
                            )),
                        ),
                    )
                }
                Destructor::ReadOnlyType => {
                    use flow_typing_type::type_::object;
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (
                            t,
                            &UseT::new(UseTInner::ObjKitT(
                                use_op.dupe(),
                                reason.dupe(),
                                Box::new(object::ResolveTool::Resolve(object::Resolve::Next)),
                                Box::new(object::Tool::ReadOnly),
                                Type::new(TypeInner::OpenT(tout.dupe())),
                            )),
                        ),
                    )
                }
                Destructor::ReactDRO(box react_dro) => rec_flow(
                    cx,
                    env,
                    trace,
                    (
                        t,
                        &UseT::new(UseTInner::DeepReadOnlyT(
                            Box::new(tout.dupe()),
                            react_dro.clone(),
                        )),
                    ),
                ),
                Destructor::PartialType => {
                    use flow_typing_type::type_::object;
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (
                            t,
                            &UseT::new(UseTInner::ObjKitT(
                                use_op.dupe(),
                                reason.dupe(),
                                Box::new(object::ResolveTool::Resolve(object::Resolve::Next)),
                                Box::new(object::Tool::Partial),
                                Type::new(TypeInner::OpenT(tout.dupe())),
                            )),
                        ),
                    )
                }
                Destructor::RequiredType => {
                    use flow_typing_type::type_::object;
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (
                            t,
                            &UseT::new(UseTInner::ObjKitT(
                                use_op.dupe(),
                                reason.dupe(),
                                Box::new(object::ResolveTool::Resolve(object::Resolve::Next)),
                                Box::new(object::Tool::Required),
                                Type::new(TypeInner::OpenT(tout.dupe())),
                            )),
                        ),
                    )
                }
                Destructor::ValuesType => rec_flow(
                    cx,
                    env,
                    trace,
                    (
                        t,
                        &UseT::new(UseTInner::GetValuesT(
                            reason.dupe(),
                            Type::new(TypeInner::OpenT(tout.dupe())),
                        )),
                    ),
                ),
                Destructor::ConditionalType(box DestructorConditionalTypeData {
                    distributive_tparam_name,
                    infer_tparams,
                    extends_t,
                    true_t,
                    false_t,
                }) => {
                    let u = UseT::new(UseTInner::ConditionalT(Box::new(ConditionalTData {
                        use_op: use_op.dupe(),
                        reason: reason.dupe(),
                        distributive_tparam_name: distributive_tparam_name.clone(),
                        infer_tparams: infer_tparams.dupe(),
                        extends_t: extends_t.dupe(),
                        true_t: true_t.dupe(),
                        false_t: false_t.dupe(),
                        tout: Box::new(tout.dupe()),
                    })));
                    rec_flow(cx, env, trace, (t, &u))
                }
                Destructor::TypeMap(tmap) => rec_flow(
                    cx,
                    env,
                    trace,
                    (
                        t,
                        &UseT::new(UseTInner::MapTypeT(Box::new(MapTypeTData {
                            use_op: use_op.dupe(),
                            reason: reason.dupe(),
                            type_map: *tmap,
                            tout: Type::new(TypeInner::OpenT(tout.dupe())),
                        }))),
                    ),
                ),
                Destructor::ReactElementConfigType => rec_flow(
                    cx,
                    env,
                    trace,
                    (
                        t,
                        &UseT::new(UseTInner::ReactKitT(Box::new(ReactKitTData {
                            use_op: use_op.dupe(),
                            reason: reason.dupe(),
                            tool: Box::new(react::Tool::<Cx>::GetConfig {
                                tout: Type::new(TypeInner::OpenT(tout.dupe())),
                            }),
                        }))),
                    ),
                ),
                Destructor::MappedType(box DestructorMappedTypeData {
                    property_type,
                    name_type,
                    mapped_type_flags,
                    homomorphic,
                    distributive_tparam_name,
                }) => {
                    let (property_type, name_type, homomorphic) =
                        flow_js_utils::substitute_mapped_type_distributive_tparams(
                            cx,
                            Some(use_op.dupe()),
                            distributive_tparam_name.clone(),
                            property_type.dupe(),
                            name_type.clone(),
                            homomorphic.dupe(),
                            t.dupe(),
                        );
                    let selected_keys_opt = match &homomorphic {
                        MappedTypeHomomorphicFlag::SemiHomomorphic(t) => Some(t.dupe()),
                        _ => None,
                    };
                    use flow_typing_type::type_::object;
                    let u = UseT::new(UseTInner::ObjKitT(
                        use_op.dupe(),
                        reason.dupe(),
                        Box::new(object::ResolveTool::Resolve(object::Resolve::Next)),
                        Box::new(object::Tool::ObjectMap(Box::new(ObjectToolObjectMapData {
                            prop_type: property_type,
                            name_type,
                            mapped_type_flags: *mapped_type_flags,
                            selected_keys_opt,
                        }))),
                        Type::new(TypeInner::OpenT(tout.dupe())),
                    ));
                    rec_flow(cx, env, trace, (t, &u))
                }
                Destructor::EnumType => {
                    let collector = TypeCollector::create();
                    let u = UseT::new(UseTInner::GetEnumT(Box::new(GetEnumTData {
                        use_op: use_op.dupe(),
                        loc: reason.loc().dupe(),
                        orig_t: Some(t.dupe()),
                        kind: GetEnumKind::GetEnumObject,
                        collector: collector.dupe(),
                    })));
                    rec_flow(cx, env, trace, (t, &u))?;
                    let tout = Type::new(TypeInner::OpenT(tout.dupe()));
                    for t in collector.collect_to_vec() {
                        rec_flow_t(cx, env, trace, use_op.dupe(), (&t, &tout))?;
                    }
                    Ok(())
                }
            }
        }
    }
}

pub(super) fn eagerly_eval_destructor_if_resolved<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    reason: &Reason,
    t: &Type,
    d: &Destructor,
    tvar: i32,
) -> Result<Type, FlowJsException> {
    let tvar_obj = Tvar::new(reason.dupe(), tvar as u32);
    eval_destructor(cx, env, trace, use_op.dupe(), reason, t, d, &tvar_obj)?;
    let result = Type::new(TypeInner::OpenT(Tvar::new(reason.dupe(), tvar as u32)));
    // Check has_unresolved_tvars first: these use exception-based early exit
    // and are cheaper than free_var_finder which traverses the entire type.
    // Due to short-circuit evaluation, if unresolved tvars are found, the
    // more expensive free_var_finder calls are skipped entirely.
    if flow_js_utils::tvar_visitors::has_unresolved_tvars(cx, t)
        || flow_js_utils::tvar_visitors::has_unresolved_tvars_in_destructors(cx, d)
        || !type_subst::free_var_finder(cx, None, t).is_empty()
        || !type_subst::free_var_finder_in_destructor(cx, None, d).is_empty()
    {
        Ok(result)
    } else {
        crate::tvar_resolver::resolve(cx, crate::tvar_resolver::default_no_lowers, true, &result);
        let t = singleton_concrete_type_for_inspection(cx, env, reason, &result)?;
        Ok(match t.deref() {
            _ if let Some(node) = type_util::constraint_node_id(&t) => {
                let (_, constraints) = cx.find_constraints(node.id());
                match constraints {
                    constraint::Constraints::FullyResolved(s) => cx.force_fully_resolved_tvar(&s),
                    _ => t,
                }
            }
            _ => t,
        })
    }
}

pub(super) fn mk_possibly_evaluated_destructor_for_annotations<'cx>(
    cx: &mut Context<'cx>,
    use_op: UseOp,
    reason: &Reason,
    t: &Type,
    d: &Destructor,
    id: eval::Id,
) -> Result<Type, FlowJsException> {
    let eval_t = Type::new(TypeInner::EvalT {
        type_: t.dupe(),
        defer_use_t: TypeDestructorT::new(TypeDestructorTInner(
            use_op.dupe(),
            reason.dupe(),
            Rc::new(d.clone()),
        )),
        id: id.dupe(),
    });
    let evaluated = cx.evaluated();
    if evaluated.get(&id).is_none() {
        if flow_js_utils::tvar_visitors::has_unresolved_tvars(cx, &eval_t) {
            panic!(
                "There are unresolved tvars in the evaluated type from annotations: {}",
                flow_typing_debug::dump_t(Some(3), cx, &eval_t)
            );
        } else if flow_js_utils::tvar_visitors::has_placeholders(cx, &eval_t) {
            panic!(
                "There are placeholders in the evaluated type from annotations: {}",
                flow_typing_debug::dump_t(Some(3), cx, &eval_t)
            );
        }

        if type_subst::free_var_finder(cx, None, &eval_t).is_empty() {
            let trace = DepthTrace::dummy_trace();
            let use_op_clone = use_op.dupe();
            let reason_clone = reason.dupe();
            let d_clone = d.clone();
            let evaluation_error = Rc::new(RefCell::new(None));
            let evaluation_error_for_map = Rc::clone(&evaluation_error);
            let eval_t_fallback = eval_t.dupe();
            let result = flow_js_utils::map_on_resolved_type(
                cx,
                reason.dupe(),
                t.dupe(),
                move |cx, t| -> Result<Type, flow_utils_concurrency::job_error::JobError> {
                    let env = &FlowJsEnv::entry();
                    match crate::tvar_resolver::mk_tvar_and_fully_resolve_no_wrap_where(
                        cx,
                        reason_clone.dupe(),
                        |cx, tvar_reason, tvar_id| {
                            let tvar = Tvar::new(tvar_reason.dupe(), tvar_id as u32);
                            evaluate_type_destructor(
                                cx,
                                env,
                                trace,
                                use_op_clone.dupe(),
                                &reason_clone,
                                &t,
                                &d_clone,
                                &tvar,
                            )
                        },
                    ) {
                        Ok(result) => Ok(result),
                        Err(err) => {
                            *evaluation_error_for_map.borrow_mut() = Some(err);
                            Ok(eval_t_fallback.dupe())
                        }
                    }
                },
            );
            if evaluation_error.borrow().is_none() {
                let mut evaluated = cx.evaluated();
                evaluated.insert(id, result);
                cx.set_evaluated(evaluated);
            }
            if matches!(
                evaluation_error.borrow().as_ref(),
                Some(FlowJsException::WorkerCanceled(_))
                    | Some(FlowJsException::TimedOut(_))
                    | Some(FlowJsException::DebugThrow { .. })
            ) {
                if let Some(err) = evaluation_error.borrow_mut().take() {
                    return Err(err);
                }
            }
        } else {
            // Hoisted out of try_evaluate so the post-loop check below can read it.
            let evaluation_error: Rc<RefCell<Option<FlowJsException>>> =
                Rc::new(RefCell::new(None));
            let mut try_evaluate = |stuck_eval_kind: nominal::StuckEvalKind,
                                    stuck_eval_targs: Vec<Type>,
                                    upper_t: Option<Type>| {
                let trace = DepthTrace::dummy_trace();
                let use_op_for_map = use_op.dupe();
                let reason_for_map = reason.dupe();
                let d_for_map = d.clone();
                let stuck_eval_kind_for_map = stuck_eval_kind.clone();
                let stuck_eval_targs_for_map = stuck_eval_targs.clone();
                let upper_t_for_map = upper_t.clone();
                let evaluation_error_for_map = Rc::clone(&evaluation_error);
                let eval_t_fallback = eval_t.dupe();
                let result = flow_js_utils::map_on_resolved_type(
                    cx,
                    reason.dupe(),
                    t.dupe(),
                    move |cx, t_resolved| -> Result<
                        Type,
                        flow_utils_concurrency::job_error::JobError,
                    > {
                        let env = &FlowJsEnv::entry();
                        let use_op = use_op_for_map.dupe();
                        let reason = reason_for_map.dupe();
                        let d = d_for_map.clone();
                        let stuck_eval_kind = stuck_eval_kind_for_map.clone();
                        let stuck_eval_targs = stuck_eval_targs_for_map.clone();
                        let upper_t = upper_t_for_map.clone();
                        match crate::tvar_resolver::mk_tvar_and_fully_resolve_no_wrap_where(
                            cx,
                            reason.dupe(),
                            |cx, tvar_reason, tvar_id| {
                                let tvar = Tvar::new(tvar_reason.dupe(), tvar_id as u32);
                                let use_op_spec = use_op.dupe();
                                let reason_spec = reason.dupe();
                                let t_spec = t_resolved.dupe();
                                let d_spec = d.clone();
                                let tvar_spec = tvar.dupe();
                                let result =
                                    crate::speculation_kit::try_singleton_custom_throw_on_failure(
                                        cx, env,
                                        Box::new(move |cx, env: &FlowJsEnv| {
                                            evaluate_type_destructor(
                                                cx, env,
                                                trace,
                                                use_op_spec,
                                                &reason_spec,
                                                &t_spec,
                                                &d_spec,
                                                &tvar_spec,
                                            )?;
                                            Ok(())
                                        }),
                                    );
                                match result {
                                    Ok(()) => Ok(()),
                                    // WorkerCanceled, TimedOut, and DebugThrow
                                    // must escape — re-raise so the outer fn
                                    // surfaces them instead of building a
                                    // stuck nominal fallback. See plan.md
                                    // §"JobError".
                                    Err(FlowJsException::WorkerCanceled(c)) => {
                                        Err(FlowJsException::WorkerCanceled(c))
                                    }
                                    Err(FlowJsException::TimedOut(t)) => {
                                        Err(FlowJsException::TimedOut(t))
                                    }
                                    Err(FlowJsException::DebugThrow { loc }) => {
                                        Err(FlowJsException::DebugThrow { loc })
                                    }
                                    Err(FlowJsException::Speculative(_))
                                    | Err(FlowJsException::SpeculationSingletonError)
                                    | Err(FlowJsException::LimitExceeded) => {
                                        let nominal_type_args: Rc<
                                            [(SubstName, Reason, Type, Polarity)],
                                        > = stuck_eval_targs
                                            .iter()
                                            .enumerate()
                                            .map(|(i, t_arg)| {
                                                (
                                                    SubstName::synthetic(
                                                        i.to_string().into(),
                                                        None,
                                                        vec![],
                                                    ),
                                                    reason_of_t(t_arg).dupe(),
                                                    t_arg.dupe(),
                                                    Polarity::Neutral,
                                                )
                                            })
                                            .collect();
                                        let stuck = Type::new(TypeInner::NominalT {
                                            reason: reason.dupe(),
                                            nominal_type: Rc::new(NominalType::new(
                                                NominalTypeInner {
                                                    nominal_id: nominal::Id::StuckEval(
                                                        stuck_eval_kind.clone(),
                                                    ),
                                                    underlying_t: nominal::UnderlyingT::FullyOpaque,
                                                    lower_t: None,
                                                    upper_t: upper_t.dupe(),
                                                    nominal_type_args,
                                                },
                                            )),
                                        });
                                        rec_flow_t(
                                            cx, env,
                                            trace,
                                            unknown_use(),
                                            (&stuck, &Type::new(TypeInner::OpenT(tvar.dupe()))),
                                        )
                                        .map(|_| ())
                                    }
                                }
                            },
                        ) {
                            Ok(result) => Ok(result),
                            Err(err) => {
                                *evaluation_error_for_map.borrow_mut() = Some(err);
                                Ok(eval_t_fallback.dupe())
                            }
                        }
                    },
                );
                if evaluation_error.borrow().is_none() {
                    let mut evaluated = cx.evaluated();
                    evaluated.insert(id.dupe(), result);
                    cx.set_evaluated(evaluated);
                }
            };

            // After try_evaluate runs, surface any latched WorkerCanceled,
            // TimedOut, or DebugThrow. Other FlowJsException variants stay in
            // evaluation_error and the existing fallback behavior is preserved.
            let check_canceled_evaluation_error = || -> Result<(), FlowJsException> {
                if matches!(
                    evaluation_error.borrow().as_ref(),
                    Some(FlowJsException::WorkerCanceled(_))
                        | Some(FlowJsException::TimedOut(_))
                        | Some(FlowJsException::DebugThrow { .. })
                ) {
                    if let Some(err) = evaluation_error.borrow_mut().take() {
                        return Err(err);
                    }
                }
                Ok(())
            };

            match d {
                // For the following generic EvalTs, if they can be successfully
                // evaluated, then we will take that as the eval result. While it's certainly unsafe
                // (See https://fburl.com/flow-generic-indexed-access-unsafe), TS has the same issue, and
                // too much code already depends on the broken behavior.
                //
                //  However, if they cannot be successfully evaluated, we should turn them into the stuck form
                // (e.g. `T['foo']`) where `T` has no upper bound)
                Destructor::NonMaybeType => {
                    try_evaluate(
                        nominal::StuckEvalKind::StuckEvalForNonMaybeType,
                        vec![t.dupe()],
                        Some(t.dupe()),
                    );
                }
                Destructor::PropertyType { name } => {
                    try_evaluate(
                        nominal::StuckEvalKind::StuckEvalForPropertyType {
                            name: Box::new(name.dupe()),
                        },
                        vec![t.dupe()],
                        None,
                    );
                }
                Destructor::ElementType { index_type } => {
                    try_evaluate(
                        nominal::StuckEvalKind::StuckEvalForElementType,
                        vec![t.dupe(), index_type.dupe()],
                        None,
                    );
                }
                Destructor::OptionalIndexedAccessNonMaybeType {
                    index: OptionalIndexedAccessIndex::OptionalIndexedAccessStrLitIndex(name),
                } => {
                    try_evaluate(
                        nominal::StuckEvalKind::StuckEvalForOptionalIndexedAccessWithStrLitIndexNonMaybeType {
                            name: Box::new(name.dupe()),
                        },
                        vec![t.dupe()],
                        None,
                    );
                }
                Destructor::OptionalIndexedAccessNonMaybeType {
                    index: OptionalIndexedAccessIndex::OptionalIndexedAccessTypeIndex(index),
                } => {
                    try_evaluate(
                        nominal::StuckEvalKind::StuckEvalForOptionalIndexedAccessWithTypeIndexNonMaybeType,
                        vec![t.dupe(), index.dupe()],
                        None,
                    );
                }
                Destructor::OptionalIndexedAccessResultType { void_reason: _ } => {
                    try_evaluate(
                        nominal::StuckEvalKind::StuckEvalForOptionalIndexedAccessResultType,
                        vec![t.dupe()],
                        None,
                    );
                }
                Destructor::ExactType => {
                    try_evaluate(
                        nominal::StuckEvalKind::StuckEvalForExactType,
                        vec![t.dupe()],
                        None,
                    );
                }
                Destructor::ReadOnlyType => {
                    try_evaluate(
                        nominal::StuckEvalKind::StuckEvalForReadOnlyType,
                        vec![t.dupe()],
                        Some(t.dupe()),
                    );
                }
                Destructor::PartialType => {
                    try_evaluate(
                        nominal::StuckEvalKind::StuckEvalForPartialType,
                        vec![t.dupe()],
                        None,
                    );
                }
                Destructor::RequiredType => {
                    try_evaluate(
                        nominal::StuckEvalKind::StuckEvalForRequiredType,
                        vec![t.dupe()],
                        None,
                    );
                }
                Destructor::ValuesType => {
                    try_evaluate(
                        nominal::StuckEvalKind::StuckEvalForValuesType,
                        vec![t.dupe()],
                        None,
                    );
                }
                Destructor::TypeMap(TypeMap::ObjectKeyMirror) => {
                    try_evaluate(
                        nominal::StuckEvalKind::StuckEvalForKeyMirrorType,
                        vec![t.dupe()],
                        None,
                    );
                }
                Destructor::ReactDRO(box ReactDro(_loc, dro_type)) => {
                    try_evaluate(
                        nominal::StuckEvalKind::StuckEvalForReactDRO(dro_type.clone()),
                        vec![t.dupe()],
                        Some(t.dupe()),
                    );
                }
                Destructor::EnumType => {
                    try_evaluate(
                        nominal::StuckEvalKind::StuckEvalForEnumType,
                        vec![t.dupe()],
                        None,
                    );
                }
                _ => {}
            }
            check_canceled_evaluation_error()?;
        }
    }
    Ok(eval_t)
}
