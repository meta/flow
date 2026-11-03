/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::ops::Deref;

use dupe::Dupe;
use flow_common::polarity::Polarity;
use flow_common::reason::Name;
use flow_common::subst_name::SubstName;
use flow_typing_context::Context;
use flow_typing_errors::error_message::EPolarityMismatchData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::IncompatibleUpperData;
use flow_typing_errors::error_message::UpperKind;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_type::type_;
use flow_typing_type::type_::ArrType;
use flow_typing_type::type_::ArrayATData;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::DepthTrace;
use flow_typing_type::type_::Destructor;
use flow_typing_type::type_::DestructorSpreadTupleTypeData;
use flow_typing_type::type_::DictType;
use flow_typing_type::type_::FunType;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::ImplicitInstantiationTvarData;
use flow_typing_type::type_::ImplicitInstantiationTvarKind;
use flow_typing_type::type_::InstTypeInner;
use flow_typing_type::type_::NamespaceType;
use flow_typing_type::type_::ObjKind;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::Property;
use flow_typing_type::type_::PropertyInner;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::ReactAbstractComponentTData;
use flow_typing_type::type_::ResolvedArgData;
use flow_typing_type::type_::ResolvedParam;
use flow_typing_type::type_::ThisTypeAppTData;
use flow_typing_type::type_::TupleATData;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::TypeParam;
use flow_typing_type::type_::TypeParamInner;
use flow_typing_type::type_::TypeStrictnessKind;
use flow_typing_type::type_::UnresolvedArgData;
use flow_typing_type::type_::UnresolvedParam;
use flow_typing_type::type_::eval;
use flow_typing_type::type_util::reason_of_t;

use crate::type_operation_utils;

pub fn check_polarity<'cx>(
    cx: &mut Context<'cx>,
    tparams: &BTreeMap<SubstName, TypeParam>,
    polarity: Polarity,
    t: &Type,
) -> Result<(), FlowJsException> {
    check_polarity_with_env(cx, &FlowJsEnv::entry(), tparams, polarity, t)
}

fn check_polarity_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    tparams: &BTreeMap<SubstName, TypeParam>,
    polarity: Polarity,
    t: &Type,
) -> Result<(), FlowJsException> {
    check_polarity_impl(
        cx,
        env,
        Some(DepthTrace::unit_trace()),
        &mut HashSet::new(),
        tparams,
        polarity,
        t,
    )
}

// TODO: flesh this out
// [seen] records evaluated types at each polarity, including across type arguments.
fn check_polarity_impl<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    seen: &mut HashSet<(eval::Id, Polarity)>,
    tparams: &BTreeMap<SubstName, TypeParam>,
    polarity: Polarity,
    t: &Type,
) -> Result<(), FlowJsException> {
    match t.deref() {
        // base case
        TypeInner::GenericT(box GenericTData {
            reason,
            name,
            bound,
            ..
        }) => match tparams.get(name) {
            None => check_polarity_impl(cx, env, trace, seen, tparams, polarity, bound)?,
            Some(tp) => {
                let tp_polarity = tp.polarity.unwrap_or(Polarity::Neutral);
                if !Polarity::compat(tp_polarity, polarity)
                    && !flow_common::files::has_ts_ext(cx.file())
                {
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EPolarityMismatch(Box::new(EPolarityMismatchData {
                            loc: reason.loc().dupe(),
                            type_param_loc: reason.def_loc().dupe(),
                            name: name.string_of_subst_name().dupe(),
                            expected_polarity: tp_polarity,
                            actual_polarity: polarity,
                        })),
                    )?;
                }
            }
        },
        // No need to walk into tvars
        TypeInner::OpenT(..) | TypeInner::ImplicitInstantiationTvar(..) => {}
        // The annot will resolve to some type, but it doesn't matter because that
        // type will certainly not contain a GenericT.
        TypeInner::AnnotT(..) => {}
        TypeInner::AnyT(..) => {}
        TypeInner::FunProtoT(_) | TypeInner::NullProtoT(_) | TypeInner::ObjProtoT(_) => {}
        TypeInner::OptionalT { type_, .. } | TypeInner::MaybeT(_, type_) => {
            check_polarity_impl(cx, env, trace, seen, tparams, polarity, type_)?;
        }
        TypeInner::TemplateLiteralT { types, .. } => {
            for t in types {
                check_polarity_impl(cx, env, trace, seen, tparams, polarity, t)?;
            }
        }
        TypeInner::StringMappingT { arg, .. } => {
            check_polarity_impl(cx, env, trace, seen, tparams, polarity, arg)?;
        }
        TypeInner::NamespaceT(ns) => {
            let NamespaceType {
                namespace_symbol: _,
                values_type,
                types_tmap,
            } = ns.deref();
            check_polarity_impl(cx, env, trace, seen, tparams, polarity, values_type)?;
            check_polarity_propmap(
                cx,
                env,
                trace,
                false,
                seen,
                tparams,
                polarity,
                types_tmap.dupe(),
            )?;
        }
        TypeInner::UnionT(_, rep) => {
            for member in rep.members_iter() {
                check_polarity_impl(cx, env, trace, seen, tparams, polarity, member)?;
            }
        }
        TypeInner::IntersectionT(_, rep) => {
            for member in rep.members_iter() {
                check_polarity_impl(cx, env, trace, seen, tparams, polarity, member)?;
            }
        }
        TypeInner::ThisTypeAppT(box ThisTypeAppTData { targs: None, .. }) => {
            // Perhaps surprisingly, there is nothing to do here. This type is used
            // specifically for the extends clause of a class declaration. The root
            // type of the extended class is looked up from the environment, and will
            // not contain any type parameters in scope -- only concrete types.
        }
        TypeInner::ThisTypeAppT(box ThisTypeAppTData {
            type_,
            targs: Some(targs),
            ..
        })
        | TypeInner::TypeAppT(box TypeAppTData { type_, targs, .. }) => {
            // Type arguments in a typeapp might contain a GenericT, but the root type
            // which defines the type parameters is not necessarily resolved at this
            // point. We need to know the polarity of the type parameters in order to
            // know the position of any found GenericTs. This call will continue
            // checking the type args once the root type is resolved.
            type_operation_utils::distribute_union_intersection::distribute(
                cx,
                env,
                None,
                &|cx, reason, t| {
                    flow_typing_flow_js::flow_js::FlowJs::possible_concrete_types_for_inspection_with_env(
                        cx, env, reason, t,
                    )
                },
                &|_, reason| reason.loc().dupe(),
                &|cx, env, l| {
                    match l.deref() {
                        TypeInner::AnyT(..)
                        | TypeInner::ImplicitInstantiationTvar(
                            box ImplicitInstantiationTvarData {
                                kind: ImplicitInstantiationTvarKind::Placeholder,
                                ..
                            },
                        ) => return Ok(()),
                        TypeInner::DefT(_, d) => {
                            if let DefTInner::PolyT(box PolyTData {
                                tparams: tps,
                                strictness_kind,
                                ..
                            }) = d.deref()
                            {
                                return variance_check(
                                    cx,
                                    env,
                                    trace,
                                    seen,
                                    tparams,
                                    polarity,
                                    tps,
                                    targs,
                                    *strictness_kind,
                                );
                            }
                            if matches!(d.deref(), DefTInner::InstanceT(_)) {
                                let concretize = |cx: &mut Context<'_>, t: &Type| {
                                    flow_typing_flow_js::flow_js::FlowJs::possible_concrete_types_for_inspection_with_env(
                                        cx,
                                        env,
                                        reason_of_t(t),
                                        t,
                                    )
                                };
                                let construct_ts =
                                    flow_js_utils::collect_construct_ts(&concretize, cx, l)?;
                                if let Some(construct_t) = flow_js_utils::construct_ts_for_targs(
                                    &construct_ts,
                                    targs.len(),
                                )
                                .into_iter()
                                .next()
                                .or_else(|| flow_js_utils::combine_construct_ts(construct_ts))
                                    && let TypeInner::DefT(_, construct_def) = construct_t.deref()
                                    && let DefTInner::PolyT(box PolyTData {
                                        tparams: tps,
                                        strictness_kind,
                                        ..
                                    }) = construct_def.deref()
                                {
                                    return variance_check(
                                        cx,
                                        env,
                                        trace,
                                        seen,
                                        tparams,
                                        polarity,
                                        tps,
                                        targs,
                                        *strictness_kind,
                                    );
                                }
                            }
                        }
                        // We will encounter this when walking an extends clause which does
                        // not have explicit type arguments. The class has an implicit this type
                        // parameter which needs to be specialized to the inheriting class, but
                        // that is uninteresting for the variance check machinery.
                        TypeInner::ThisInstanceT(..) if targs.is_empty() => return Ok(()),
                        _ => {}
                    }
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        flow_js_utils::incompatible_type_error_with_lower_kind(
                            l,
                            flow_js_utils::error_message_kind_of_lower(t),
                            IncompatibleUpperData {
                                loc: reason_of_t(type_).loc().dupe(),
                                kind: UpperKind::IncompatibleVarianceCheckT,
                            },
                            None,
                        ),
                    )
                },
                type_,
            )?;
        }
        TypeInner::KeysT(_, inner_t) => {
            check_polarity_impl(cx, env, trace, seen, tparams, Polarity::Positive, inner_t)?;
        }
        TypeInner::EvalT {
            type_: inner_t,
            defer_use_t,
            id,
        } if matches!(
            defer_use_t.deref(),
            type_::TypeDestructorTInner(_, _, d) if matches!(d.deref(), Destructor::ReadOnlyType)
        ) =>
        {
            if seen.insert((id.dupe(), polarity)) {
                // `$ReadOnly` makes the elements of a tuple type spread read-only.
                if let TypeInner::EvalT {
                    type_: first_spread,
                    defer_use_t: inner_defer_use_t,
                    ..
                } = inner_t.deref()
                    && let type_::TypeDestructorTInner(_, _, inner_d) = inner_defer_use_t.deref()
                    && let Destructor::SpreadTupleType(spread) = inner_d.deref()
                {
                    check_tuple_spread_polarity(
                        cx,
                        env,
                        trace,
                        seen,
                        tparams,
                        polarity,
                        Polarity::Positive,
                        first_spread,
                        spread,
                    )?;
                } else {
                    let type_::TypeDestructorTInner(_, reason, _) = defer_use_t.deref();
                    let concrete_types =
                        flow_typing_flow_js::flow_js::FlowJs::possible_concrete_types_for_inspection_with_env(
                            cx, env, reason, t,
                        )?;
                    for ct in &concrete_types {
                        check_polarity_impl(cx, env, trace, seen, tparams, Polarity::Positive, ct)?;
                    }
                }
            }
        }
        // A tuple type spread copies elements into a tuple whose elements are writable.
        TypeInner::EvalT {
            type_: first_spread,
            defer_use_t,
            id,
        } if let type_::TypeDestructorTInner(_, _, d) = defer_use_t.deref()
            && let Destructor::SpreadTupleType(spread) = d.deref() =>
        {
            if seen.insert((id.dupe(), polarity)) {
                check_tuple_spread_polarity(
                    cx,
                    env,
                    trace,
                    seen,
                    tparams,
                    polarity,
                    Polarity::Neutral,
                    first_spread,
                    spread,
                )?;
            }
        }
        TypeInner::NominalT { nominal_type, .. } => {
            let mut tps_list = Vec::new();
            let mut targs_list = Vec::new();
            for (subst_name, reason, arg_t, pol) in nominal_type.nominal_type_args.iter() {
                tps_list.push(TypeParam::new(TypeParamInner {
                    reason: reason.dupe(),
                    name: subst_name.dupe(),
                    bound: type_::mixed_t::why(reason.dupe()),
                    polarity: *pol,
                    default: None,
                    is_this: false,
                    is_const: false,
                }));
                targs_list.push(arg_t.dupe());
            }
            variance_check(
                cx,
                env,
                trace,
                seen,
                tparams,
                polarity,
                &tps_list,
                &targs_list,
                TypeStrictnessKind::Flow,
            )?;
        }
        // TODO
        TypeInner::EvalT { .. } => {}
        TypeInner::DefT(_, d) => match d.deref() {
            DefTInner::BoolGeneralT
            | DefTInner::BigIntGeneralT(_)
            | DefTInner::EmptyT
            | DefTInner::EnumObjectT { .. }
            | DefTInner::EnumValueT(_)
            | DefTInner::MixedT(_)
            | DefTInner::NullT
            | DefTInner::NumGeneralT(_)
            | DefTInner::NumericStrKeyT(_)
            | DefTInner::SingletonBoolT { .. }
            | DefTInner::SingletonNumT { .. }
            | DefTInner::SingletonStrT { .. }
            | DefTInner::SingletonBigIntT { .. }
            | DefTInner::StrGeneralT(_)
            | DefTInner::VoidT
            | DefTInner::SymbolT
            | DefTInner::UniqueSymbolT(_) => {}
            DefTInner::ClassT(inner_t) => {
                check_polarity_impl(cx, env, trace, seen, tparams, polarity, inner_t)?;
            }
            DefTInner::InstanceT(instance_t) => {
                let InstTypeInner {
                    inst_react_dro: _,
                    class_id: _,
                    class_name: _,
                    type_args: _,
                    own_props,
                    proto_props,
                    inst_call_t: call_t,
                    inst_construct_t: construct_t,
                    initialized_fields: _,
                    initialized_static_fields: _,
                    inst_kind: _,
                    inst_dict,
                    class_private_fields,
                    class_private_static_fields: _,
                    class_private_methods,
                    class_private_static_methods: _,
                    inst_abstract: _,
                    inst_abstract_props: _,
                    strictness_kind: _,
                } = instance_t.inst.deref();
                check_polarity_impl(cx, env, trace, seen, tparams, polarity, &instance_t.static_)?;
                check_polarity_impl(cx, env, trace, seen, tparams, polarity, &instance_t.super_)?;
                for impl_t in instance_t.implements.iter() {
                    check_polarity_impl(cx, env, trace, seen, tparams, polarity, impl_t)?;
                }
                check_polarity_propmap(
                    cx,
                    env,
                    trace,
                    false,
                    seen,
                    tparams,
                    polarity,
                    own_props.dupe(),
                )?;
                check_polarity_propmap(
                    cx,
                    env,
                    trace,
                    true,
                    seen,
                    tparams,
                    polarity,
                    proto_props.dupe(),
                )?;
                check_polarity_propmap(
                    cx,
                    env,
                    trace,
                    false,
                    seen,
                    tparams,
                    polarity,
                    class_private_fields.dupe(),
                )?;
                check_polarity_propmap(
                    cx,
                    env,
                    trace,
                    false,
                    seen,
                    tparams,
                    polarity,
                    class_private_methods.dupe(),
                )?;
                if let Some(call_t) = call_t {
                    check_polarity_call(cx, env, trace, seen, tparams, polarity, *call_t)?;
                }
                if let Some(construct_t) = construct_t {
                    check_polarity_call(cx, env, trace, seen, tparams, polarity, *construct_t)?;
                }
                if let Some(inst_dict) = inst_dict {
                    check_polarity_dict(cx, env, trace, seen, tparams, polarity, inst_dict)?;
                }
            }
            // We can ignore the statics of function annotations, since
            // they will always be "uninteresting," never containing a GenericT.
            DefTInner::FunT(_static, f) => {
                let FunType {
                    // Similarly, we can ignore this types, which can not be explicitly
                    // provided, and thus will not contain a GenericT.
                    this_t: _,
                    params,
                    rest_param,
                    return_t,
                    type_guard,
                    def_reason: _,
                    function_reference_kind: _,
                    effect_: _,
                    strictness_kind: _,
                    literal: _,
                } = f.deref();
                let inv_polarity = Polarity::inv(polarity);
                for param in params.iter() {
                    check_polarity_impl(cx, env, trace, seen, tparams, inv_polarity, &param.1)?;
                }
                if let Some(rest_param) = rest_param {
                    check_polarity_impl(
                        cx,
                        env,
                        trace,
                        seen,
                        tparams,
                        inv_polarity,
                        &rest_param.2,
                    )?;
                }
                check_polarity_impl(cx, env, trace, seen, tparams, polarity, return_t)?;
                if let Some(tg) = type_guard {
                    if let Some(type_guard) = &tg.type_guard {
                        check_polarity_impl(cx, env, trace, seen, tparams, polarity, type_guard)?;
                    }
                }
            }
            DefTInner::ArrT(arr) => {
                use flow_typing_type::type_::ArrType;
                use flow_typing_type::type_::ArrayATData;
                use flow_typing_type::type_::TupleATData;
                match arr.deref() {
                    // This representation signifies a literal, which is not a type.
                    ArrType::ArrayAT(box ArrayATData {
                        tuple_view: Some(_),
                        ..
                    }) => {
                        panic!("UnexpectedType: ArrayAT with tuple_view in check_polarity");
                    }
                    ArrType::ArrayAT(box ArrayATData {
                        elem_t,
                        tuple_view: None,
                        ..
                    }) => {
                        check_polarity_impl(
                            cx,
                            env,
                            trace,
                            seen,
                            tparams,
                            Polarity::Neutral,
                            elem_t,
                        )?;
                    }
                    ArrType::TupleAT(box TupleATData { elements, rest, .. }) => {
                        for elem in elements.iter().chain(rest) {
                            check_polarity_impl(
                                cx,
                                env,
                                trace,
                                seen,
                                tparams,
                                Polarity::mult(polarity, elem.polarity),
                                &elem.t,
                            )?;
                        }
                    }
                    ArrType::ROArrayAT(box ROArrayATData {
                        elem_t: inner_t, ..
                    }) => {
                        check_polarity_impl(cx, env, trace, seen, tparams, polarity, inner_t)?;
                    }
                }
            }
            DefTInner::ObjT(obj) => {
                let type_::ObjType {
                    flags,
                    props_tmap,
                    proto_t,
                    call_t,
                    reachable_targs: _,
                    strictness_kind: _,
                    literal: _,
                } = obj.deref();
                check_polarity_propmap(
                    cx,
                    env,
                    trace,
                    false,
                    seen,
                    tparams,
                    polarity,
                    props_tmap.dupe(),
                )?;
                if let ObjKind::Indexed(ref dict) = flags.obj_kind {
                    check_polarity_dict(cx, env, trace, seen, tparams, polarity, dict)?;
                }
                check_polarity_impl(cx, env, trace, seen, tparams, polarity, proto_t)?;
                if let Some(call_t) = call_t {
                    check_polarity_call(cx, env, trace, seen, tparams, polarity, *call_t)?;
                }
            }
            DefTInner::PolyT(box PolyTData {
                tparams: tps,
                t_out,
                ..
            }) => {
                // We might encounter a polymorphic function type or method inside of an
                // annotation. A newly introduced type parameter's bound or default might
                // refer to one of the tparams we're looking for.
                let mut tparams_acc = tparams.clone();
                for tp in tps.iter() {
                    check_polarity_typeparam(cx, env, trace, seen, &tparams_acc, polarity, tp)?;
                    tparams_acc.insert(tp.name.dupe(), tp.clone());
                }
                check_polarity_impl(cx, env, trace, seen, &tparams_acc, polarity, t_out)?;
            }
            DefTInner::ReactAbstractComponentT(box ReactAbstractComponentTData {
                config,
                renders,
                ..
            }) => {
                check_polarity_impl(
                    cx,
                    env,
                    trace,
                    seen,
                    tparams,
                    Polarity::inv(polarity),
                    config,
                )?;
                check_polarity_impl(cx, env, trace, seen, tparams, polarity, renders)?;
            }
            DefTInner::RendersT(renders) => {
                use flow_typing_type::type_::CanonicalRendersForm;
                match renders.deref() {
                    CanonicalRendersForm::NominalRenders { renders_super, .. } => {
                        check_polarity_impl(
                            cx,
                            env,
                            trace,
                            seen,
                            tparams,
                            polarity,
                            renders_super,
                        )?;
                    }
                    CanonicalRendersForm::StructuralRenders {
                        renders_structural_type,
                        ..
                    } => {
                        check_polarity_impl(
                            cx,
                            env,
                            trace,
                            seen,
                            tparams,
                            polarity,
                            renders_structural_type,
                        )?;
                    }
                    CanonicalRendersForm::IntrinsicRenders(_)
                    | CanonicalRendersForm::DefaultRenders => {}
                }
            }
            // We only expect types which can appear in annotations.
            DefTInner::TypeT(..) => {
                panic!("UnexpectedType: TypeT in check_polarity");
            }
        },
        // We only expect types which can appear in annotations.
        TypeInner::ThisInstanceT(..) => {
            panic!("UnexpectedType: ThisInstanceT in check_polarity");
        }
    }
    Ok(())
}

fn check_polarity_propmap<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    skip_ctor: bool,
    seen: &mut HashSet<(eval::Id, Polarity)>,
    tparams: &BTreeMap<SubstName, TypeParam>,
    polarity: Polarity,
    id: type_::properties::Id,
) -> Result<(), FlowJsException> {
    let pmap = cx.find_props(id);
    for (x, p) in pmap.iter() {
        if skip_ctor && *x == Name::new("constructor") {
        } else {
            check_polarity_prop(cx, env, trace, seen, tparams, polarity, p)?;
        }
    }
    Ok(())
}

fn check_polarity_prop<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    seen: &mut HashSet<(eval::Id, Polarity)>,
    tparams: &BTreeMap<SubstName, TypeParam>,
    polarity: Polarity,
    prop: &Property,
) -> Result<(), FlowJsException> {
    match prop.deref() {
        PropertyInner::Field(fd) => check_polarity_impl(
            cx,
            env,
            trace,
            seen,
            tparams,
            Polarity::mult(polarity, fd.polarity),
            &fd.type_,
        ),
        PropertyInner::Get { type_, .. } => {
            check_polarity_impl(cx, env, trace, seen, tparams, polarity, type_)
        }
        PropertyInner::Set { type_, .. } => check_polarity_impl(
            cx,
            env,
            trace,
            seen,
            tparams,
            Polarity::inv(polarity),
            type_,
        ),
        PropertyInner::GetSet(gs) => {
            check_polarity_impl(cx, env, trace, seen, tparams, polarity, &gs.get_type)?;
            check_polarity_impl(
                cx,
                env,
                trace,
                seen,
                tparams,
                Polarity::inv(polarity),
                &gs.set_type,
            )
        }
        PropertyInner::Method { type_, .. } => {
            check_polarity_impl(cx, env, trace, seen, tparams, polarity, type_)
        }
    }
}

fn check_polarity_dict<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    seen: &mut HashSet<(eval::Id, Polarity)>,
    tparams: &BTreeMap<SubstName, TypeParam>,
    polarity: Polarity,
    d: &DictType,
) -> Result<(), FlowJsException> {
    let DictType {
        dict_name: _,
        key,
        value,
        dict_polarity,
    } = d;
    check_polarity_impl(cx, env, trace, seen, tparams, Polarity::Neutral, key)?;
    check_polarity_impl(
        cx,
        env,
        trace,
        seen,
        tparams,
        Polarity::mult(polarity, *dict_polarity),
        value,
    )?;
    Ok(())
}

fn check_polarity_call<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    seen: &mut HashSet<(eval::Id, Polarity)>,
    tparams: &BTreeMap<SubstName, TypeParam>,
    polarity: Polarity,
    id: i32,
) -> Result<(), FlowJsException> {
    let t = cx.find_call(id);
    check_polarity_impl(cx, env, trace, seen, tparams, polarity, &t)
}

fn check_polarity_typeparam<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    seen: &mut HashSet<(eval::Id, Polarity)>,
    tparams: &BTreeMap<SubstName, TypeParam>,
    polarity: Polarity,
    tp: &TypeParam,
) -> Result<(), FlowJsException> {
    let TypeParamInner {
        reason: _,
        name: _,
        bound,
        polarity: tp_polarity,
        default,
        is_this: _,
        is_const: _,
    } = tp.deref();
    let mult_polarity = Polarity::mult(polarity, tp_polarity.unwrap_or(Polarity::Neutral));
    check_polarity_impl(cx, env, trace, seen, tparams, mult_polarity, bound)?;
    if let Some(default) = default {
        check_polarity_impl(cx, env, trace, seen, tparams, mult_polarity, default)?;
    }
    Ok(())
}

// The elements of a tuple type spread end up as elements of the resulting tuple: the elements of
// the spread arguments with `elem_polarity`, and the tuple type's own elements, before and after
// the spreads, with their own variance at the tuple type's `polarity`, unless `elem_polarity`
// makes them read-only. The spread itself is not evaluated, since evaluating it with type
// parameters reports errors at the definition.
fn check_tuple_spread_polarity<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    seen: &mut HashSet<(eval::Id, Polarity)>,
    tparams: &BTreeMap<SubstName, TypeParam>,
    polarity: Polarity,
    elem_polarity: Polarity,
    first_spread: &Type,
    spread: &DestructorSpreadTupleTypeData,
) -> Result<(), FlowJsException> {
    let DestructorSpreadTupleTypeData {
        resolved,
        unresolved,
        ..
    } = spread;
    let own_polarity = |own: Polarity| match elem_polarity {
        Polarity::Positive => Polarity::Positive,
        Polarity::Neutral | Polarity::Negative => Polarity::mult(polarity, own),
    };
    check_spread_argument_polarity(cx, env, trace, seen, tparams, elem_polarity, first_spread)?;
    for param in resolved.iter() {
        if let ResolvedParam::ResolvedArg(box ResolvedArgData(elem, _)) = param {
            let polarity = own_polarity(elem.polarity);
            check_polarity_impl(cx, env, trace, seen, tparams, polarity, &elem.t)?;
        }
    }
    for param in unresolved.iter() {
        match param {
            UnresolvedParam::UnresolvedArg(box UnresolvedArgData(elem, _)) => {
                let polarity = own_polarity(elem.polarity);
                check_polarity_impl(cx, env, trace, seen, tparams, polarity, &elem.t)?;
            }
            UnresolvedParam::UnresolvedSpreadArg(t) => {
                check_spread_argument_polarity(cx, env, trace, seen, tparams, elem_polarity, t)?;
            }
        }
    }
    Ok(())
}

// A spread argument's elements are read and copied, so their own polarity does not matter.
fn check_spread_argument_polarity<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    seen: &mut HashSet<(eval::Id, Polarity)>,
    tparams: &BTreeMap<SubstName, TypeParam>,
    elem_polarity: Polarity,
    t: &Type,
) -> Result<(), FlowJsException> {
    if let TypeInner::GenericT(_) = t.deref() {
        return check_polarity_impl(cx, env, trace, seen, tparams, elem_polarity, t);
    }
    let concrete_types =
        flow_typing_flow_js::flow_js::FlowJs::possible_concrete_types_for_inspection_with_env(
            cx,
            env,
            reason_of_t(t),
            t,
        )?;
    for ct in &concrete_types {
        match ct.deref() {
            TypeInner::DefT(_, d) if let DefTInner::ArrT(arr) = d.deref() => match arr.deref() {
                ArrType::TupleAT(box TupleATData { elements, .. }) => {
                    for elem in elements.iter() {
                        check_polarity_impl(cx, env, trace, seen, tparams, elem_polarity, &elem.t)?;
                    }
                }
                ArrType::ArrayAT(box ArrayATData { elem_t, .. })
                | ArrType::ROArrayAT(box ROArrayATData { elem_t, .. }) => {
                    check_polarity_impl(cx, env, trace, seen, tparams, elem_polarity, elem_t)?;
                }
            },
            _ => check_polarity_impl(cx, env, trace, seen, tparams, elem_polarity, ct)?,
        }
    }
    Ok(())
}

fn variance_check<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    seen: &HashSet<(eval::Id, Polarity)>,
    tparams: &BTreeMap<SubstName, TypeParam>,
    polarity: Polarity,
    tps: &[TypeParam],
    targs: &[Type],
    strictness_kind: TypeStrictnessKind,
) -> Result<(), FlowJsException> {
    // ignore typeapp arity mismatch, since it's handled elsewhere
    for (tp, targ) in tps.iter().zip(targs.iter()) {
        // As in `type_app_variance_check`, TS treats unannotated tparam slots as covariant.
        let tp_polarity = match tp.polarity {
            None if strictness_kind.is_typescript_loose() => Polarity::Positive,
            tp_polarity => tp_polarity.unwrap_or(Polarity::Neutral),
        };
        check_polarity_impl(
            cx,
            env,
            trace,
            &mut seen.clone(),
            tparams,
            Polarity::mult(polarity, tp_polarity),
            targ,
        )?;
    }
    Ok(())
}
