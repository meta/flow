/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// Port of flow/src/typing/members.ml

use std::collections::BTreeMap;
use std::ops::Deref;
use std::rc::Rc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::polarity::Polarity;
use flow_common::reason::Name;
use flow_common::reason::VirtualReasonDesc;
use flow_common::reason::locationless_reason;
use flow_common::subst_name::SubstName;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_typing_context::Context;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::obj_type;
use flow_typing_flow_js::flow_js;
use flow_typing_type::type_::ArrLiteral;
use flow_typing_type::type_::ArrType;
use flow_typing_type::type_::ArrayATData;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::DictType;
use flow_typing_type::type_::EnumInfoInner;
use flow_typing_type::type_::Flags;
use flow_typing_type::type_::FunParam;
use flow_typing_type::type_::FunRestParam;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::ImplicitInstantiationTvarData;
use flow_typing_type::type_::ImplicitInstantiationTvarKind;
use flow_typing_type::type_::ObjKind;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::Property;
use flow_typing_type::type_::PropertyInner;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::ThisInstanceTData;
use flow_typing_type::type_::ThisTypeAppTData;
use flow_typing_type::type_::TupleATData;
use flow_typing_type::type_::TupleElement;
use flow_typing_type::type_::TupleView;
use flow_typing_type::type_::TupleViewKind;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::any_t;
use flow_typing_type::type_::inter_rep;
use flow_typing_type::type_::mk_functiontype;
use flow_typing_type::type_::mk_object_def_type;
use flow_typing_type::type_::nominal;
use flow_typing_type::type_::properties;
use flow_typing_type::type_::string_of_ctor;
use flow_typing_type::type_::union_rep;
use flow_typing_type::type_util::desc_of_t;
use flow_typing_type::type_util::reason_of_t;
use vec1::Vec1;

pub enum GenericT<Success, SuccessNamespace> {
    Success(Success),
    SuccessNamespace(SuccessNamespace),
    FailureNullishType,
    FailureAnyType,
    FailureUnhandledType(Type),
    FailureUnhandledMembers(Type),
}

pub type Members = GenericT<
    BTreeMap<FlowSmolStr, (Option<Vec1<ALoc>>, Type)>,
    BTreeMap<FlowSmolStr, (Option<Vec1<ALoc>>, Type)>,
>;

fn merge_type<'cx>(cx: &mut Context<'cx>, pair: (Type, Type)) -> Type {
    fn create_union(rep: union_rep::UnionRep) -> Type {
        Type::new(TypeInner::UnionT(locationless_reason(None), rep))
    }

    let (t1, t2) = pair;
    match (t1.deref(), t2.deref()) {
        (TypeInner::DefT(_, d1), TypeInner::DefT(_, d2)) => match (d1.deref(), d2.deref()) {
            (DefTInner::NumGeneralT(_), DefTInner::NumGeneralT(_))
            | (DefTInner::StrGeneralT(_), DefTInner::StrGeneralT(_))
            | (DefTInner::BoolGeneralT, DefTInner::BoolGeneralT)
            | (DefTInner::NullT, DefTInner::NullT)
            | (DefTInner::VoidT, DefTInner::VoidT) => t2,
            (DefTInner::EmptyT, _) => t2,
            (_, DefTInner::EmptyT) => t1,
            (_, DefTInner::MixedT(_)) => t2,
            (DefTInner::MixedT(_), _) => t1,
            (DefTInner::FunT(_, ft1), DefTInner::FunT(_, ft2)) => {
                // Functions with different number of parameters cannot be merged into a
                // single function type. Instead, we should turn them into a union
                let params = if ft1.params.len() != ft2.params.len() {
                    None
                } else {
                    let mut params_names = Vec::with_capacity(ft1.params.len());
                    let mut tins = Vec::with_capacity(ft1.params.len());
                    for (FunParam(name1, p_t1), FunParam(name2, p_t2)) in
                        ft1.params.iter().zip(ft2.params.iter())
                    {
                        // TODO: How to merge param names?
                        params_names.push(match (name1, name2) {
                            (None, None) => None,
                            (Some(name), _) | (_, Some(name)) => Some(Name::new(name.clone())),
                        });
                        tins.push(merge_type(cx, (p_t1.dupe(), p_t2.dupe())));
                    }
                    match (&ft1.rest_param, &ft2.rest_param) {
                        (None, Some(_)) | (Some(_), None) => None,
                        (None, None) => Some((params_names, tins, None)),
                        (Some(r1), Some(r2)) => {
                            Some((params_names, tins, Some((r1.clone(), r2.clone()))))
                        }
                    }
                };
                match params {
                    None => create_union(union_rep::make(
                        None,
                        union_rep::UnionKind::UnknownKind,
                        t1,
                        t2,
                        Rc::from([]),
                    )),
                    Some((params_names, tins, rest_params)) => {
                        let rest_param = match rest_params {
                            None => None,
                            Some((
                                FunRestParam(name1, loc, rest_t1),
                                FunRestParam(name2, _, rest_t2),
                            )) => {
                                // TODO: How to merge rest names and locs?
                                let name = match (&name1, &name2) {
                                    (None, None) => None,
                                    (Some(name), _) | (_, Some(name)) => Some(name.clone()),
                                };
                                Some(FunRestParam(name, loc, merge_type(cx, (rest_t1, rest_t2))))
                            }
                        };
                        let tout = merge_type(cx, (ft1.return_t.dupe(), ft2.return_t.dupe()));
                        let reason = locationless_reason(VirtualReasonDesc::RFunctionType);
                        // TODO merging type guards would require aligning param names as well
                        let type_guard = None;
                        let static_t = flow_typing_type::type_::dummy_static(reason.dupe());
                        let ft = mk_functiontype(
                            reason.dupe(),
                            None,
                            None,
                            tins,
                            rest_param,
                            reason.dupe(),
                            flow_common::error_ref::FunctionReferenceKind::FunctionType,
                            Some(params_names),
                            type_guard,
                            tout,
                        );
                        Type::new(TypeInner::DefT(
                            reason,
                            DefT::new(DefTInner::FunT(static_t, Rc::new(ft))),
                        ))
                    }
                }
            }
            (DefTInner::ObjT(o1), DefTInner::ObjT(o2)) => {
                let map1 = cx.find_props(o1.props_tmap.dupe());
                let map2 = cx.find_props(o2.props_tmap.dupe());
                // Create an intermediate map of booleans indicating whether two objects can
                // be merged, based on the properties in each map.
                let mut merge_map: BTreeMap<Name, bool> = BTreeMap::new();
                let mut all_keys: std::collections::BTreeSet<Name> =
                    std::collections::BTreeSet::new();
                for (k, _) in map1.iter() {
                    all_keys.insert(k.dupe());
                }
                for (k, _) in map2.iter() {
                    all_keys.insert(k.dupe());
                }
                for name in all_keys {
                    let p1_opt = map1.get(&name);
                    let p2_opt = map2.get(&name);
                    let result = match (p1_opt, p2_opt) {
                        (None, None) => None,
                        // In general, even objects with disjoint key sets can not be merged due
                        // to width subtyping. For example, {x:T} and {y:U} is not the same as
                        // {x:T,y:U}, because {x,y} is a valid inhabitant of {x:T} and the type of
                        // y may != U. However, if either object type is exact, disjointness is
                        // sufficient.
                        (Some(_), None) | (None, Some(_)) => Some(
                            obj_type::is_exact(&o1.flags.obj_kind)
                                || obj_type::is_exact(&o2.flags.obj_kind),
                        ),
                        (Some(p1), Some(p2)) => match (p1.deref(), p2.deref()) {
                            // Covariant fields can be merged.
                            (PropertyInner::Field(fd1), PropertyInner::Field(fd2))
                                if fd1.polarity == Polarity::Positive
                                    && fd2.polarity == Polarity::Positive =>
                            {
                                Some(true)
                            }
                            // Getters are covariant and thus can be merged.
                            (PropertyInner::Get { .. }, PropertyInner::Get { .. }) => Some(true),
                            // Anything else is can't be merged.
                            _ => Some(false),
                        },
                    };
                    if let Some(v) = result {
                        merge_map.insert(name, v);
                    }
                }
                let obj_kind = match (&o1.flags.obj_kind, &o2.flags.obj_kind) {
                    (
                        ObjKind::Indexed(DictType {
                            key: k1,
                            value: v1,
                            dict_polarity: Polarity::Positive,
                            ..
                        }),
                        ObjKind::Indexed(DictType {
                            key: k2,
                            value: v2,
                            dict_polarity: Polarity::Positive,
                            ..
                        }),
                    ) => ObjKind::Indexed(DictType {
                        dict_name: None,
                        key: flow_typing_type::type_::create_intersection(inter_rep::make(
                            k1.dupe(),
                            k2.dupe(),
                            Rc::from([]),
                        )),
                        value: merge_type(cx, (v1.dupe(), v2.dupe())),
                        dict_polarity: Polarity::Positive,
                    }),
                    (ObjKind::Indexed(d), _) | (_, ObjKind::Indexed(d)) => {
                        ObjKind::Indexed(d.clone())
                    }
                    (ObjKind::Inexact, _) | (_, ObjKind::Inexact) => ObjKind::Inexact,
                    (ObjKind::Exact, ObjKind::Exact) => ObjKind::Exact,
                };
                let merge_call = match (o1.call_t, o2.call_t) {
                    (None, None) => Some(None),
                    (Some(_), None) => {
                        if obj_type::is_exact(&o2.flags.obj_kind) {
                            Some(o1.call_t)
                        } else {
                            None
                        }
                    }
                    (None, Some(_)) => {
                        if obj_type::is_exact(&o1.flags.obj_kind) {
                            Some(o2.call_t)
                        } else {
                            None
                        }
                    }
                    (Some(id1), Some(id2)) => {
                        let c1 = cx.find_call(id1);
                        let c2 = cx.find_call(id2);
                        let id = cx.make_call_prop(create_union(union_rep::make(
                            None,
                            union_rep::UnionKind::UnknownKind,
                            c1,
                            c2,
                            Rc::from([]),
                        )));
                        Some(Some(id))
                    }
                };
                // Only merge objects if every property can be merged.
                let should_merge = merge_map.values().all(|x| *x) && o1.proto_t == o2.proto_t;
                match (should_merge, &obj_kind, merge_call) {
                    (true, ObjKind::Indexed(_), Some(call)) => {
                        let map: properties::PropertiesMap = map1
                            .iter()
                            .filter(|(name, _)| map2.get(name).is_none())
                            .chain(map2.iter().filter(|(name, _)| map1.get(name).is_none()))
                            .map(|(name, p)| (name.dupe(), p.dupe()))
                            .collect();
                        let id = cx.generate_property_map(map);
                        let flags = Flags {
                            obj_kind,
                            react_dro: if o1.flags.react_dro.is_some()
                                && o1.flags.react_dro.is_some()
                            {
                                o1.flags.react_dro.clone()
                            } else {
                                None
                            },
                        };
                        let reason = locationless_reason(VirtualReasonDesc::RObjectType);
                        Type::new(mk_object_def_type(
                            reason,
                            Some(flags),
                            call,
                            id,
                            o1.proto_t.dupe(),
                        ))
                    }
                    _ => create_union(union_rep::make(
                        None,
                        union_rep::UnionKind::UnknownKind,
                        t1,
                        t2,
                        Rc::from([]),
                    )),
                }
            }
            (DefTInner::ArrT(arr1), DefTInner::ArrT(arr2)) => {
                match (arr1.deref(), arr2.deref()) {
                    (
                        ArrType::ArrayAT(box ArrayATData {
                            elem_t: et1,
                            tuple_view:
                                Some(TupleView {
                                    kind: kind1,
                                    elements: elements1,
                                    arity: arity1,
                                    inexact: inexact1,
                                    rest: None,
                                }),
                            react_dro: dro1,
                            strictness_kind: strictness_kind1,
                            ..
                        }),
                        ArrType::ArrayAT(box ArrayATData {
                            elem_t: et2,
                            tuple_view:
                                Some(TupleView {
                                    kind: kind2,
                                    elements: elements2,
                                    arity: arity2,
                                    inexact: inexact2,
                                    rest: None,
                                }),
                            react_dro: dro2,
                            strictness_kind: strictness_kind2,
                            ..
                        }),
                    ) if arity1 == arity2
                        && inexact1 == inexact2
                        && elements1.len() == elements2.len()
                        && elements1.iter().zip(elements2.iter()).all(|(e1, e2)| {
                            e1.polarity == e2.polarity && e1.optional == e2.optional
                        }) =>
                    {
                        let elements: Vec<TupleElement> = elements1
                            .iter()
                            .zip(elements2.iter())
                            .map(|(e1, e2)| {
                                let name = if e1.name == e2.name {
                                    e1.name.clone()
                                } else {
                                    None
                                };
                                let t = merge_type(cx, (e1.t.dupe(), e2.t.dupe()));
                                let reason = locationless_reason(None);
                                TupleElement {
                                    name,
                                    t,
                                    polarity: e1.polarity,
                                    optional: e1.optional,
                                    reason,
                                }
                            })
                            .collect();
                        Type::new(TypeInner::DefT(
                            locationless_reason(None),
                            DefT::new(DefTInner::ArrT(Rc::new(ArrType::ArrayAT(Box::new(
                                ArrayATData {
                                    elem_t: merge_type(cx, (et1.dupe(), et2.dupe())),
                                    tuple_view: Some(TupleView {
                                        kind: if kind1 == kind2 {
                                            *kind1
                                        } else {
                                            TupleViewKind::ArrayLiteral
                                        },
                                        elements: elements.into(),
                                        arity: *arity1,
                                        inexact: *inexact1,
                                        rest: None,
                                    }),
                                    react_dro: if dro1.is_some() && dro2.is_some() {
                                        dro1.clone()
                                    } else {
                                        None
                                    },
                                    strictness_kind: strictness_kind1.join(*strictness_kind2),
                                    literal: ArrLiteral::NonLiteral,
                                    fresh: false,
                                },
                            ))))),
                        ))
                    }
                    (
                        ArrType::ArrayAT(box ArrayATData {
                            elem_t: et1,
                            react_dro: dro1,
                            strictness_kind: strictness_kind1,
                            ..
                        }),
                        ArrType::ArrayAT(box ArrayATData {
                            elem_t: et2,
                            react_dro: dro2,
                            strictness_kind: strictness_kind2,
                            ..
                        }),
                    ) => Type::new(TypeInner::DefT(
                        locationless_reason(None),
                        DefT::new(DefTInner::ArrT(Rc::new(ArrType::ArrayAT(Box::new(
                            ArrayATData {
                                elem_t: merge_type(cx, (et1.dupe(), et2.dupe())),
                                tuple_view: None,
                                react_dro: if dro1.is_some() && dro2.is_some() {
                                    dro1.clone()
                                } else {
                                    None
                                },
                                strictness_kind: strictness_kind1.join(*strictness_kind2),
                                literal: ArrLiteral::NonLiteral,
                                fresh: false,
                            },
                        ))))),
                    )),
                    (
                        ArrType::TupleAT(box TupleATData {
                            elem_t: et1,
                            elements: ts1,
                            arity: arity1,
                            inexact: inexact1,
                            rest: None,
                            react_dro: dro1,
                            strictness_kind: strictness_kind1,
                            ..
                        }),
                        ArrType::TupleAT(box TupleATData {
                            elem_t: et2,
                            elements: ts2,
                            arity: arity2,
                            inexact: inexact2,
                            rest: None,
                            react_dro: dro2,
                            strictness_kind: strictness_kind2,
                            ..
                        }),
                    ) if arity1 == arity2
                        && inexact1 == inexact2
                        && ts1.len() == ts2.len()
                        && ts1.iter().zip(ts2.iter()).all(|(e1, e2)| {
                            e1.polarity == e2.polarity && e1.optional == e2.optional
                        }) =>
                    {
                        Type::new(TypeInner::DefT(
                            locationless_reason(VirtualReasonDesc::RTupleType),
                            DefT::new(DefTInner::ArrT(Rc::new(ArrType::TupleAT(Box::new(
                                TupleATData {
                                    elem_t: merge_type(cx, (et1.dupe(), et2.dupe())),
                                    react_dro: if dro1.is_some() && dro2.is_some() {
                                        dro1.clone()
                                    } else {
                                        None
                                    },
                                    elements: ts1
                                        .iter()
                                        .zip(ts2.iter())
                                        .map(|(e1, e2)| {
                                            let name = if e1.name == e2.name {
                                                e1.name.clone()
                                            } else {
                                                None
                                            };
                                            let t = merge_type(cx, (e1.t.dupe(), e2.t.dupe()));
                                            let reason = locationless_reason(None);
                                            TupleElement {
                                                name,
                                                t,
                                                polarity: e1.polarity,
                                                optional: e1.optional,
                                                reason,
                                            }
                                        })
                                        .collect(),
                                    arity: *arity1,
                                    inexact: *inexact1,
                                    rest: None,
                                    strictness_kind: strictness_kind1.join(*strictness_kind2),
                                    literal: ArrLiteral::NonLiteral,
                                    fresh: false,
                                },
                            ))))),
                        ))
                    }
                    (
                        ArrType::ROArrayAT(box ROArrayATData {
                            elem_t: elemt1,
                            react_dro: dro1,
                            ..
                        }),
                        ArrType::ROArrayAT(box ROArrayATData {
                            elem_t: elemt2,
                            react_dro: dro2,
                            ..
                        }),
                    ) => Type::new(TypeInner::DefT(
                        locationless_reason(VirtualReasonDesc::RArrayType),
                        DefT::new(DefTInner::ArrT(Rc::new(ArrType::ROArrayAT(Box::new(
                            ROArrayATData {
                                elem_t: merge_type(cx, (elemt1.dupe(), elemt2.dupe())),
                                react_dro: if dro1.is_some() && dro2.is_some() {
                                    dro1.clone()
                                } else {
                                    None
                                },
                                literal: ArrLiteral::NonLiteral,
                            },
                        ))))),
                    )),
                    // Non-matching ArrT pairs
                    _ => create_union(union_rep::make(
                        None,
                        union_rep::UnionKind::UnknownKind,
                        t1,
                        t2,
                        Rc::from([]),
                    )),
                }
            }
            // Non-matching DefT pairs
            // TODO: do we need to do anything special for merging Null with Void,
            // Optional with other types, etc.?
            _ => create_union(union_rep::make(
                None,
                union_rep::UnionKind::UnknownKind,
                t1,
                t2,
                Rc::from([]),
            )),
        },
        (TypeInner::ObjProtoT(_), TypeInner::ObjProtoT(_)) => t2,
        (
            TypeInner::AnyT(_, _)
            | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
                kind: ImplicitInstantiationTvarKind::Placeholder,
                ..
            }),
            _,
        ) => t2,
        (
            _,
            TypeInner::AnyT(_, _)
            | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
                kind: ImplicitInstantiationTvarKind::Placeholder,
                ..
            }),
        ) => t1,
        (TypeInner::DefT(_, d), _) if matches!(d.deref(), DefTInner::EmptyT) => t2,
        (_, TypeInner::DefT(_, d)) if matches!(d.deref(), DefTInner::EmptyT) => t1,
        (_, TypeInner::DefT(_, d)) if matches!(d.deref(), DefTInner::MixedT(_)) => t2,
        (TypeInner::DefT(_, d), _) if matches!(d.deref(), DefTInner::MixedT(_)) => t1,
        (TypeInner::DefT(_, d), TypeInner::MaybeT(_, _))
            if matches!(d.deref(), DefTInner::NullT | DefTInner::VoidT) =>
        {
            t2
        }
        (TypeInner::MaybeT(_, _), TypeInner::DefT(_, d))
            if matches!(d.deref(), DefTInner::NullT | DefTInner::VoidT) =>
        {
            t1
        }
        (TypeInner::MaybeT(_, inner1), TypeInner::MaybeT(_, inner2)) => {
            let merged = merge_type(cx, (inner1.dupe(), inner2.dupe()));
            let reason = locationless_reason(desc_of_t(&merged).cloned());
            Type::new(TypeInner::MaybeT(reason, merged))
        }
        (TypeInner::MaybeT(_, inner1), _) => {
            let merged = merge_type(cx, (inner1.dupe(), t2));
            let reason = locationless_reason(desc_of_t(&merged).cloned());
            Type::new(TypeInner::MaybeT(reason, merged))
        }
        (_, TypeInner::MaybeT(_, inner2)) => {
            let merged = merge_type(cx, (t1, inner2.dupe()));
            let reason = locationless_reason(desc_of_t(&merged).cloned());
            Type::new(TypeInner::MaybeT(reason, merged))
        }
        (TypeInner::UnionT(_, rep1), TypeInner::UnionT(_, rep2)) => {
            let mut members: Vec<_> = rep1.members_iter().map(|t| t.dupe()).collect();
            members.reverse();
            members.extend(rep2.members_iter().map(|t| t.dupe()));
            let mut members = members.into_iter();
            let t0 = members.next().expect("UnionRep has at least two members");
            let t1 = members.next().expect("UnionRep has at least two members");
            create_union(union_rep::make(
                None,
                union_rep::UnionKind::UnknownKind,
                t0,
                t1,
                members.collect(),
            ))
        }
        (TypeInner::UnionT(_, rep), _) => create_union(union_rep::cons(t2, rep)),
        (_, TypeInner::UnionT(_, rep)) => create_union(union_rep::cons(t1, rep)),
        // TODO: do we need to do anything special for merging Null with Void,
        // Optional with other types, etc.?
        _ => create_union(union_rep::make(
            None,
            union_rep::UnionKind::UnknownKind,
            t1,
            t2,
            Rc::from([]),
        )),
    }
}

fn instantiate_poly_t<'cx>(cx: &mut Context<'cx>, t: Type, args: Option<&[Type]>) -> Type {
    match t.deref() {
        TypeInner::DefT(_, def_t) => match def_t.deref() {
            DefTInner::PolyT(box PolyTData {
                tparams: type_params,
                t_out,
                ..
            }) => {
                let args = args.map(|s| s.to_vec()).unwrap_or_default();
                let maximum_arity = type_params.len();
                if args.len() > maximum_arity {
                    flow_hh_logger::error!("Instantiating poly type failed");
                    t
                } else {
                    let mut map: FlowOrdMap<SubstName, Type> = FlowOrdMap::new();
                    let mut ts = args.into_iter();
                    let mut too_few_args = false;
                    for typeparam in type_params.iter() {
                        match ts.next() {
                            None => match &typeparam.default {
                                Some(default) => {
                                    let substed =
                                        flow_js::subst(cx, None, None, None, &map, default.dupe());
                                    map.insert(typeparam.name.dupe(), substed);
                                }
                                None => {
                                    map.insert(
                                        typeparam.name.dupe(),
                                        any_t::error(reason_of_t(&t).dupe()),
                                    );
                                    too_few_args = true;
                                }
                            },
                            Some(arg) => {
                                map.insert(typeparam.name.dupe(), arg);
                            }
                        }
                    }
                    if too_few_args {
                        flow_hh_logger::error!("Instantiating poly type failed");
                        t
                    } else {
                        flow_js::subst(cx, None, None, None, &map, t_out.dupe())
                    }
                }
            }
            DefTInner::EmptyT | DefTInner::MixedT(_) => t,
            DefTInner::TypeT(_, inner_t)
                if matches!(
                    inner_t.deref(),
                    TypeInner::AnyT(_, _)
                        | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
                            kind: ImplicitInstantiationTvarKind::Placeholder,
                            ..
                        })
                ) =>
            {
                t
            }
            _ => match args {
                None => t,
                Some(_) => panic!(
                    "unexpected args passed to instantiate_poly_t: {}",
                    string_of_ctor(&t)
                ),
            },
        },
        TypeInner::AnyT(_, _)
        | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
            kind: ImplicitInstantiationTvarKind::Placeholder,
            ..
        }) => t,
        _ => match args {
            None => t,
            Some(_) => panic!(
                "unexpected args passed to instantiate_poly_t: {}",
                string_of_ctor(&t)
            ),
        },
    }
}

pub fn intersect_members<'cx>(
    cx: &mut Context<'cx>,
    members: Vec<BTreeMap<FlowSmolStr, (Option<Vec1<ALoc>>, Type)>>,
) -> BTreeMap<FlowSmolStr, (Option<Vec1<ALoc>>, Type)> {
    if members.is_empty() {
        return BTreeMap::new();
    }
    let mut members_iter = members.into_iter();
    let mut map: BTreeMap<_, _> = members_iter
        .next()
        .unwrap()
        .into_iter()
        .map(|(k, v)| (k, vec![v]))
        .collect();
    for mut member in members_iter {
        let mut new_map = BTreeMap::new();
        for (key, mut tl) in map {
            if let Some(t) = member.remove(&key) {
                tl.push(t);
                new_map.insert(key, tl);
            }
        }
        map = new_map;
    }
    map.into_iter()
        .map(|(key, items)| {
            let (loc, acc) = items.into_iter().rev().fold(
                (None, flow_typing_type::type_::locationless::empty_t::t()),
                |(_, acc), (loc, t)| {
                    // Arbitrarily use the last location encountered
                    (loc, merge_type(cx, (acc, t)))
                },
            );
            (key, (loc, acc))
        })
        .collect()
}

fn instantiate_type(t: Type) -> Type {
    match t.deref() {
        TypeInner::DefT(_, def_t) => match def_t.deref() {
            DefTInner::ClassT(inner) => match inner.deref() {
                TypeInner::ThisInstanceT(box ThisInstanceTData {
                    reason, instance, ..
                }) => Type::new(TypeInner::DefT(
                    reason.dupe(),
                    DefT::new(DefTInner::InstanceT(Rc::new(instance.dupe()))),
                )),
                _ => inner.dupe(),
            },
            DefTInner::TypeT(_, inner) => inner.dupe(),
            DefTInner::EmptyT => t,
            _ => panic!("cannot instantiate non-class type {}", string_of_ctor(&t)),
        },
        TypeInner::AnyT(_, _)
        | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
            kind: ImplicitInstantiationTvarKind::Placeholder,
            ..
        }) => t,
        _ => panic!("cannot instantiate non-class type {}", string_of_ctor(&t)),
    }
}

/// For debugging purposes
pub fn string_of_extracted_type(t: &GenericT<Type, Type>) -> String {
    match t {
        GenericT::Success(t) => format!("Success ({})", string_of_ctor(t)),
        GenericT::SuccessNamespace(t) => format!("SuccessNamespace ({})", string_of_ctor(t)),
        GenericT::FailureNullishType => "FailureNullishType".to_string(),
        GenericT::FailureAnyType => "FailureAnyType".to_string(),
        GenericT::FailureUnhandledType(t) => {
            format!("FailureUnhandledType ({})", string_of_ctor(t))
        }
        GenericT::FailureUnhandledMembers(t) => {
            format!("FailureUnhandledMembers ({})", string_of_ctor(t))
        }
    }
}

pub fn to_command_result(
    t: Members,
) -> Result<BTreeMap<FlowSmolStr, (Option<Vec1<ALoc>>, Type)>, String> {
    match t {
        GenericT::Success(map) | GenericT::SuccessNamespace(map) => Ok(map),
        GenericT::FailureNullishType => {
            Err("autocomplete on possibly null or undefined value".to_string())
        }
        GenericT::FailureAnyType => Err("not enough type information to autocomplete".to_string()),
        GenericT::FailureUnhandledType(t) => Err(format!(
            "autocomplete on unexpected type of value {} (please file a task!)",
            string_of_ctor(&t)
        )),
        GenericT::FailureUnhandledMembers(t) => Err(format!(
            "autocomplete on unexpected members of value {} (please file a task!)",
            string_of_ctor(&t)
        )),
    }
}

fn find_props<'cx>(cx: &mut Context<'cx>, id: properties::Id) -> BTreeMap<FlowSmolStr, Property> {
    let props = cx.find_props(id);
    // Filter out keys starting with "$" and convert Name -> String. Symbol-typed
    // keys are skipped: they have no identifier spelling to autocomplete/hover on,
    // and collapsing them to a placeholder string would merge distinct symbols.
    let mut result = BTreeMap::new();
    for (name, prop) in props.iter() {
        let Name::Str(key) = name else {
            continue;
        };
        // filter out keys that start with "$"
        if !key.starts_with('$') {
            result.insert(key.dupe(), prop.dupe());
        }
    }
    result
}

fn resolve_tvar<'cx>(cx: &mut Context<'cx>, id: i32) -> Type {
    let ts = flow_js_utils::possible_types(cx, id);
    // The list of types returned by possible_types is often empty, and the
    // most common reason is that we don't have enough type coverage to
    // resolve id. Thus, we take the unit of merging to be `any`. (Something
    // similar happens when summarizing exports in ContextOptimizer.)
    //
    // In the future, we might report errors in some cases where
    // possible_types returns an empty list: e.g., when we detect unreachable
    // code, or even we don't have enough type coverage. Irrespective of these
    // changes, the above decision would continue to make sense: as errors
    // become stricter, type resolution should become even more lenient to
    // improve failure tolerance.
    let init = flow_typing_type::type_::unsoundness::unresolved_any(locationless_reason(None));
    ts.into_iter().fold(init, |u, t| merge_type(cx, (t, u)))
}

pub fn resolve_type<'cx>(cx: &mut Context<'cx>, t: Type) -> Type {
    match t.deref() {
        TypeInner::OpenT(tvar) => {
            let resolved = resolve_tvar(cx, tvar.id() as i32);
            resolve_type(cx, resolved)
        }
        TypeInner::ImplicitInstantiationTvar(
            data @ box ImplicitInstantiationTvarData {
                kind: ImplicitInstantiationTvarKind::Inferred,
                ..
            },
        ) => {
            let resolved = resolve_tvar(cx, data.id);
            resolve_type(cx, resolved)
        }
        TypeInner::AnnotT(_, inner, _) => resolve_type(cx, inner.dupe()),
        _ => t,
    }
}

pub fn extract_type<'cx>(cx: &mut Context<'cx>, this_t: Type) -> GenericT<Type, Type> {
    match this_t.deref() {
        TypeInner::OpenT(_)
        | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
            kind: ImplicitInstantiationTvarKind::Inferred,
            ..
        })
        | TypeInner::AnnotT(_, _, _) => {
            let resolved = resolve_type(cx, this_t);
            extract_type(cx, resolved)
        }
        TypeInner::OptionalT { type_, .. } => extract_type(cx, type_.dupe()),
        TypeInner::MaybeT(_, ty) => extract_type(cx, ty.dupe()),
        TypeInner::AnyT(_, _)
        | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
            kind: ImplicitInstantiationTvarKind::Placeholder,
            ..
        }) => GenericT::FailureAnyType,
        TypeInner::ThisInstanceT(box ThisInstanceTData {
            reason, instance, ..
        }) => GenericT::Success(Type::new(TypeInner::DefT(
            reason.dupe(),
            DefT::new(DefTInner::InstanceT(Rc::new(instance.dupe()))),
        ))),
        TypeInner::GenericT(box GenericTData { bound, .. }) => extract_type(cx, bound.dupe()),
        TypeInner::NamespaceT(_) => GenericT::SuccessNamespace(this_t),
        TypeInner::ThisTypeAppT(box ThisTypeAppTData {
            type_: c,
            targs: ts_opt,
            ..
        }) => {
            let c = resolve_type(cx, c.dupe());
            let inst_t = instantiate_poly_t(cx, c, ts_opt.as_deref());
            let inst_t = instantiate_type(inst_t);
            extract_type(cx, inst_t)
        }
        TypeInner::TypeAppT(box TypeAppTData {
            type_,
            targs,
            from_value,
            ..
        }) => {
            let c = resolve_type(cx, type_.dupe());
            let inst_t = instantiate_poly_t(cx, c, Some(&**targs));
            let inst_t = if *from_value {
                inst_t
            } else {
                instantiate_type(inst_t)
            };
            extract_type(cx, inst_t)
        }
        TypeInner::IntersectionT(_, _) => GenericT::Success(this_t),
        TypeInner::UnionT(_, _) => GenericT::Success(this_t),
        TypeInner::TemplateLiteralT { reason, .. } | TypeInner::StringMappingT { reason, .. } => {
            let builtin = flow_js::get_builtin_type(cx, reason, None, "String");
            match builtin {
                Ok(t) => extract_type(cx, t),
                Err(_) => GenericT::FailureUnhandledType(this_t),
            }
        }
        TypeInner::NominalT { nominal_type, .. } => match &nominal_type.underlying_t {
            nominal::UnderlyingT::OpaqueWithLocal { t } => extract_type(cx, t.dupe()),
            nominal::UnderlyingT::CustomError(box nominal::CustomErrorData { t, .. }) => {
                extract_type(cx, t.dupe())
            }
            _ => match &nominal_type.upper_t {
                Some(t) => extract_type(cx, t.dupe()),
                None => GenericT::FailureUnhandledType(this_t),
            },
        },
        TypeInner::EvalT {
            type_: eval_t,
            defer_use_t,
            id,
        } => {
            use flow_typing_type::type_::TypeDestructorTInner;
            match defer_use_t.deref() {
                TypeDestructorTInner(use_op, reason, d) => {
                    let result = flow_js::mk_type_destructor(
                        cx,
                        use_op.clone(),
                        reason,
                        eval_t,
                        d,
                        id.dupe(),
                    );
                    match result {
                        Ok(result) => extract_type(cx, result),
                        Err(_) => GenericT::FailureUnhandledType(this_t),
                    }
                }
            }
        }
        TypeInner::FunProtoT(_)
        | TypeInner::KeysT(_, _)
        | TypeInner::NullProtoT(_)
        | TypeInner::ObjProtoT(_) => GenericT::FailureUnhandledType(this_t),
        TypeInner::DefT(reason, def_t) => match def_t.deref() {
            DefTInner::NullT | DefTInner::VoidT => GenericT::FailureNullishType,
            DefTInner::InstanceT(_) => GenericT::Success(this_t),
            DefTInner::ObjT(_) => GenericT::Success(this_t),
            DefTInner::EnumObjectT { .. } => GenericT::Success(this_t),
            // TODO: replace type parameters with stable/proper names?
            DefTInner::PolyT(box PolyTData { t_out, .. }) => extract_type(cx, t_out.dupe()),
            DefTInner::ClassT(inner) => match inner.deref() {
                TypeInner::ThisInstanceT(box ThisInstanceTData { instance, .. }) => {
                    extract_type(cx, instance.static_.dupe())
                }
                TypeInner::DefT(_, inner_def) => match inner_def.deref() {
                    DefTInner::InstanceT(inst) => extract_type(cx, inst.static_.dupe()),
                    _ => GenericT::FailureUnhandledType(this_t),
                },
                _ => GenericT::FailureUnhandledType(this_t),
            },
            DefTInner::FunT(_, _) => GenericT::Success(this_t),
            DefTInner::SingletonStrT { .. }
            | DefTInner::StrGeneralT(_)
            | DefTInner::NumericStrKeyT(_) => {
                let builtin = flow_js::get_builtin_type(cx, reason, None, "String");
                match builtin {
                    Ok(t) => extract_type(cx, t),
                    Err(_) => GenericT::FailureUnhandledType(this_t),
                }
            }
            DefTInner::SingletonNumT { .. } | DefTInner::NumGeneralT(_) => {
                let builtin = flow_js::get_builtin_type(cx, reason, None, "Number");
                match builtin {
                    Ok(t) => extract_type(cx, t),
                    Err(_) => GenericT::FailureUnhandledType(this_t),
                }
            }
            DefTInner::SingletonBoolT { .. } | DefTInner::BoolGeneralT => {
                let builtin = flow_js::get_builtin_type(cx, reason, None, "Boolean");
                match builtin {
                    Ok(t) => extract_type(cx, t),
                    Err(_) => GenericT::FailureUnhandledType(this_t),
                }
            }
            DefTInner::SingletonBigIntT { .. } | DefTInner::BigIntGeneralT(_) => {
                let builtin = flow_js::get_builtin_type(cx, reason, None, "BigInt");
                match builtin {
                    Ok(t) => extract_type(cx, t),
                    Err(_) => GenericT::FailureUnhandledType(this_t),
                }
            }
            DefTInner::SymbolT | DefTInner::UniqueSymbolT(_) => {
                let builtin = flow_js::get_builtin_type(cx, reason, None, "Symbol");
                match builtin {
                    Ok(t) => extract_type(cx, t),
                    Err(_) => GenericT::FailureUnhandledType(this_t),
                }
            }
            DefTInner::ReactAbstractComponentT(_) => GenericT::Success(this_t),
            DefTInner::RendersT(_) => GenericT::Success(this_t),
            DefTInner::ArrT(arrtype) => {
                let (builtin, elem_t) = match arrtype.deref() {
                    ArrType::ArrayAT(box ArrayATData { elem_t, .. }) => (
                        flow_js_utils::lookup_builtin_value(cx, "Array", reason.dupe()),
                        elem_t.dupe(),
                    ),
                    ArrType::TupleAT(box TupleATData { elem_t, .. }) => (
                        flow_js_utils::lookup_builtin_type(cx, "ReadonlyArray", reason.dupe()),
                        elem_t.dupe(),
                    ),
                    ArrType::ROArrayAT(box ROArrayATData { elem_t, .. }) => (
                        flow_js_utils::lookup_builtin_type(cx, "ReadonlyArray", reason.dupe()),
                        elem_t.dupe(),
                    ),
                };
                let array_t = resolve_type(cx, builtin);
                let inst_t = instantiate_poly_t(cx, array_t, Some(&[elem_t]));
                let inst_t = instantiate_type(inst_t);
                extract_type(cx, inst_t)
            }
            DefTInner::EmptyT
            | DefTInner::MixedT(_)
            | DefTInner::TypeT(_, _)
            | DefTInner::EnumValueT(_) => GenericT::FailureUnhandledType(this_t),
        },
    }
}

pub fn extract_members<'cx>(
    exclude_proto_members: bool,
    cx: &mut Context<'cx>,
    extracted: GenericT<Type, Type>,
) -> Members {
    match extracted {
        GenericT::FailureNullishType => GenericT::FailureNullishType,
        GenericT::FailureAnyType => GenericT::FailureAnyType,
        GenericT::FailureUnhandledType(t) => GenericT::FailureUnhandledType(t),
        GenericT::FailureUnhandledMembers(t) => GenericT::FailureUnhandledMembers(t),
        GenericT::SuccessNamespace(ref t) => match t.deref() {
            TypeInner::NamespaceT(ns) => {
                let values_type = ns.values_type.dupe();
                let types_tmap = ns.types_tmap.dupe();
                let types_props = cx.find_props(types_tmap);
                let values_members = extract_members_as_map(exclude_proto_members, cx, values_type);
                let mut members = values_members;
                for (name, p) in types_props.iter() {
                    // A symbol-typed key has no identifier spelling to hover on; skip
                    // it rather than collapse distinct symbols onto a placeholder.
                    let Some(key) = name.as_smol_str_opt() else {
                        continue;
                    };
                    if let Some(t) = flow_typing_type::type_::property::read_t(p) {
                        let loc = flow_typing_type::type_::property::def_locs(p);
                        members.insert(key.dupe(), (loc, t.dupe()));
                    }
                }
                GenericT::SuccessNamespace(members)
            }
            _ => GenericT::FailureUnhandledMembers(t.dupe()),
        },
        GenericT::Success(ref t) => {
            let mut handle_instance = |super_t, own_props, proto_props| {
                let own_props_map = find_props(cx, own_props);
                let mut members: BTreeMap<FlowSmolStr, (Option<Vec1<ALoc>>, Type)> =
                    BTreeMap::new();
                for (x, p) in &own_props_map {
                    // TODO: It isn't currently possible to return two types for a given
                    // property in autocomplete, so for now we just return the getter
                    // type.
                    let t = flow_typing_type::type_::property::type_(p).dupe();
                    let locs = flow_typing_type::type_::property::def_locs(p);
                    members.insert(x.clone(), (locs, t));
                }
                if exclude_proto_members {
                    GenericT::Success(members)
                } else {
                    // TODO: own props should take precedence
                    let proto_props_map = find_props(cx, proto_props);
                    for (x, p) in &proto_props_map {
                        if let Some(t) = flow_typing_type::type_::property::read_t(p) {
                            let loc = flow_typing_type::type_::property::def_locs(p);
                            members.insert(x.clone(), (loc, t));
                        }
                    }
                    let super_flds = extract_members_as_map(exclude_proto_members, cx, super_t);
                    let mut result = super_flds;
                    result.extend(members);
                    GenericT::Success(result)
                }
            };
            match t.deref() {
                TypeInner::GenericT(box GenericTData { bound, .. }) => {
                    extract_members(exclude_proto_members, cx, GenericT::Success(bound.dupe()))
                }
                TypeInner::ThisInstanceT(box ThisInstanceTData { instance, .. }) => {
                    handle_instance(
                        instance.super_.dupe(),
                        instance.inst.own_props.dupe(),
                        instance.inst.proto_props.dupe(),
                    )
                }
                TypeInner::DefT(reason, def_t) => match def_t.deref() {
                    DefTInner::InstanceT(inst) => handle_instance(
                        inst.super_.dupe(),
                        inst.inst.own_props.dupe(),
                        inst.inst.proto_props.dupe(),
                    ),
                    DefTInner::ObjT(o) => {
                        let flds = o.props_tmap.dupe();
                        let proto = o.proto_t.dupe();
                        let proto_reason = reason_of_t(&proto).dupe();
                        let builtin_obj =
                            flow_js::get_builtin_type(cx, &proto_reason, None, "Object");
                        let builtin_obj = match builtin_obj {
                            Ok(t) => t,
                            Err(_) => flow_typing_type::type_::locationless::empty_t::t(),
                        };
                        let rep = inter_rep::make(proto, builtin_obj, Rc::from([]));
                        let proto_t = Type::new(TypeInner::IntersectionT(proto_reason, rep));
                        let prot_members = if exclude_proto_members {
                            BTreeMap::new()
                        } else {
                            extract_members_as_map(exclude_proto_members, cx, proto_t)
                        };
                        let flds_map = find_props(cx, flds);
                        let mut members = BTreeMap::new();
                        for (x, p) in &flds_map {
                            if let Some(t) = flow_typing_type::type_::property::read_t(p) {
                                let loc = flow_typing_type::type_::property::def_locs(p);
                                members.insert(x.dupe(), (loc, t));
                            }
                        }
                        let mut result = prot_members;
                        result.extend(members);
                        GenericT::Success(result)
                    }
                    DefTInner::FunT(static_t, _) => GenericT::Success(extract_members_as_map(
                        exclude_proto_members,
                        cx,
                        static_t.dupe(),
                    )),
                    DefTInner::EnumObjectT { enum_info, .. } => {
                        let enum_reason = reason;
                        let enum_info_rc = enum_info;
                        let enum_object_t = t.dupe();
                        match enum_info_rc.deref().deref() {
                            EnumInfoInner::ConcreteEnum(concrete_info) => {
                                let enum_members = &concrete_info.members;
                                let representation_t = &concrete_info.representation_t;
                                let enum_t = flow_typing_type::type_::mk_enum_type(
                                    enum_reason.dupe(),
                                    enum_info_rc.dupe(),
                                );
                                let proto_members = if exclude_proto_members {
                                    BTreeMap::new()
                                } else {
                                    let proto = flow_js_utils::lookup_builtin_typeapp(
                                        cx,
                                        enum_reason.dupe(),
                                        "$EnumProto",
                                        vec![enum_object_t, enum_t.dupe(), representation_t.dupe()],
                                    );
                                    // `$EnumProto` has a null proto, so we set `exclude_proto_members` to true
                                    extract_members_as_map(true, cx, proto)
                                };
                                let result: BTreeMap<_, _> = enum_members
                                    .iter()
                                    .map(|(name, member_loc)| {
                                        (
                                            name.dupe(),
                                            (Some(Vec1::new(member_loc.dupe())), enum_t.dupe()),
                                        )
                                    })
                                    .collect();
                                let mut combined = proto_members;
                                combined.extend(result);
                                GenericT::Success(combined)
                            }
                            _ => GenericT::FailureUnhandledMembers(t.dupe()),
                        }
                    }
                    _ => GenericT::FailureUnhandledMembers(t.dupe()),
                },
                TypeInner::IntersectionT(_, rep) => {
                    // Intersection type should autocomplete for every property of
                    // every type in the intersection
                    let ts: Vec<&Type> = rep.members_iter().collect();
                    let members: Vec<_> = ts
                        .into_iter()
                        .map(|t| extract_members_as_map(exclude_proto_members, cx, t.dupe()))
                        .collect();
                    let result = members.into_iter().fold(BTreeMap::new(), |mut acc, m| {
                        acc.extend(m);
                        acc
                    });
                    GenericT::Success(result)
                }
                TypeInner::UnionT(_, rep) => {
                    // Union type should autocomplete for only the properties that are in
                    // every type in the intersection
                    let members: Vec<_> = rep
                        .members_iter()
                        .filter(|t| match t.deref().deref() {
                            TypeInner::DefT(_, d) => {
                                !matches!(d.deref(), DefTInner::NullT | DefTInner::VoidT)
                            }
                            TypeInner::AnyT(_, _)
                            | TypeInner::ImplicitInstantiationTvar(
                                box ImplicitInstantiationTvarData {
                                    kind: ImplicitInstantiationTvarKind::Placeholder,
                                    ..
                                },
                            ) => false,
                            _ => true,
                        })
                        .map(|t| extract_members_as_map(exclude_proto_members, cx, t.dupe()))
                        .collect();
                    let members = intersect_members(cx, members);
                    GenericT::Success(members)
                }
                _ => GenericT::FailureUnhandledMembers(t.dupe()),
            }
        }
    }
}

pub fn extract<'cx>(exclude_proto_members: bool, cx: &mut Context<'cx>, t: Type) -> Members {
    let extracted = extract_type(cx, t);
    extract_members(exclude_proto_members, cx, extracted)
}

fn extract_members_as_map<'cx>(
    exclude_proto_members: bool,
    cx: &mut Context<'cx>,
    this_t: Type,
) -> BTreeMap<FlowSmolStr, (Option<Vec1<ALoc>>, Type)> {
    to_command_result(extract(exclude_proto_members, cx, this_t)).unwrap_or_default()
}
