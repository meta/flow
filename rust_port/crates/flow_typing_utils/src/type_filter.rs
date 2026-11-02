/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// Port of type_filter.rs

use std::collections::BTreeSet;
use std::ops::Deref;
use std::rc::Rc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_aloc::ALoc;
use flow_common::polarity::Polarity;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::reason::mk_reason;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_typing_context::Context;
use flow_typing_flow_common::obj_type;
use flow_typing_flow_js::template_literal_type;
use flow_typing_type::type_::ArrLiteral;
use flow_typing_type::type_::ArrType;
use flow_typing_type::type_::ArrayATData;
use flow_typing_type::type_::ArrayLengthOp;
use flow_typing_type::type_::BigIntLiteral;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::DictType;
use flow_typing_type::type_::InstType;
use flow_typing_type::type_::InstanceKind;
use flow_typing_type::type_::Literal;
use flow_typing_type::type_::MixedFlavor;
use flow_typing_type::type_::NominalType;
use flow_typing_type::type_::NominalTypeInner;
use flow_typing_type::type_::NumberLiteral;
use flow_typing_type::type_::ObjKind;
use flow_typing_type::type_::ObjLiteral;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::ThisInstanceTData;
use flow_typing_type::type_::TupleATData;
use flow_typing_type::type_::TupleView;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::UnionEnum;
use flow_typing_type::type_::UnionEnumStar;
use flow_typing_type::type_::bigint_module_t;
use flow_typing_type::type_::bool_module_t;
use flow_typing_type::type_::constraint::Constraints;
use flow_typing_type::type_::empty_t;
use flow_typing_type::type_::inter_rep;
use flow_typing_type::type_::nominal;
use flow_typing_type::type_::null;
use flow_typing_type::type_::num_module_t;
use flow_typing_type::type_::str_module_t;
use flow_typing_type::type_::symbol_t;
use flow_typing_type::type_::union_rep;
use flow_typing_type::type_::union_rep::UnionKind;
use flow_typing_type::type_::void;
use flow_typing_type::type_util::is_falsy;
use flow_typing_type::type_util::reason_of_t;

pub struct FilterResult {
    pub type_: Type,
    pub changed: bool,
}

fn changed_result(type_: Type) -> FilterResult {
    FilterResult {
        type_,
        changed: true,
    }
}

pub fn unchanged_result(type_: Type) -> FilterResult {
    FilterResult {
        type_,
        changed: false,
    }
}

fn recurse_into_union<'cx>(
    cx: &mut Context<'cx>,
    filter_fn: &dyn Fn(&mut Context<'cx>, Type) -> FilterResult,
    r: Reason,
    ts: Vec<Type>,
) -> FilterResult {
    let (mut new_ts, changed) =
        ts.into_iter()
            .fold((Vec::new(), false), |(mut new_ts, changed_acc), t| {
                let t = match t.deref() {
                    TypeInner::OpenT(tvar) => {
                        let (_, constraints) = cx.find_constraints(tvar.id() as i32);
                        match constraints {
                            Constraints::FullyResolved(s) => cx.force_fully_resolved_tvar(&s),
                            Constraints::Resolved(t) => t,
                            _ => t,
                        }
                    }
                    _ => t,
                };
                let FilterResult { type_, changed } = filter_fn(cx, t);
                match type_.deref() {
                    TypeInner::DefT(_, d) if matches!(&**d, DefTInner::EmptyT) => {
                        (new_ts, changed_acc || changed)
                    }
                    _ => {
                        new_ts.push(type_);
                        (new_ts, changed_acc || changed)
                    }
                }
            });
    match new_ts.len() {
        0 => FilterResult {
            type_: Type::new(TypeInner::DefT(r, DefT::new(DefTInner::EmptyT))),
            changed,
        },
        1 => FilterResult {
            type_: new_ts.pop().unwrap(),
            changed,
        },
        _ => {
            let mut new_ts = new_ts.into_iter();
            let t0 = new_ts.next().unwrap();
            let t1 = new_ts.next().unwrap();
            FilterResult {
                type_: Type::new(TypeInner::UnionT(
                    r,
                    union_rep::make(None, UnionKind::UnknownKind, t0, t1, new_ts.collect()),
                )),
                changed,
            }
        }
    }
}

fn recurse_into_intersection<'cx>(
    cx: &mut Context<'cx>,
    filter_fn: &dyn Fn(&mut Context<'cx>, Type) -> FilterResult,
    r: Reason,
    ts: Vec<Type>,
) -> FilterResult {
    fn helper<'cx>(
        cx: &mut Context<'cx>,
        filter_fn: &dyn Fn(&mut Context<'cx>, Type) -> FilterResult,
        _r: &Reason,
        mut t_acc: Vec<Type>,
        mut changed_acc: bool,
        remaining: &[Type],
    ) -> (Vec<Type>, bool) {
        if remaining.is_empty() {
            return (t_acc, changed_acc);
        }
        let t = remaining[0].dupe();
        let rest = &remaining[1..];
        let t = match t.deref() {
            TypeInner::OpenT(tvar) => {
                let (_, constraints) = cx.find_constraints(tvar.id() as i32);
                match constraints {
                    Constraints::FullyResolved(s) => cx.force_fully_resolved_tvar(&s),
                    Constraints::Resolved(t) => t,
                    _ => t,
                }
            }
            _ => t,
        };
        let FilterResult { type_, changed } = filter_fn(cx, t);
        match type_.deref() {
            TypeInner::DefT(_, d) if matches!(&**d, DefTInner::EmptyT) => {
                (Vec::new(), changed_acc || changed)
            }
            _ => {
                changed_acc = changed_acc || changed;
                t_acc.push(type_);
                helper(cx, filter_fn, _r, t_acc, changed_acc, rest)
            }
        }
    }

    let ts_vec: Vec<Type> = ts;
    let (mut result, changed) = helper(cx, filter_fn, &r, Vec::new(), false, &ts_vec);
    match result.len() {
        0 => FilterResult {
            type_: Type::new(TypeInner::DefT(r, DefT::new(DefTInner::EmptyT))),
            changed,
        },
        1 => FilterResult {
            type_: result.pop().unwrap(),
            changed,
        },
        _ => {
            let mut result = result.into_iter();
            let t0 = result.next().unwrap();
            let t1 = result.next().unwrap();
            FilterResult {
                type_: Type::new(TypeInner::IntersectionT(
                    r,
                    inter_rep::make(t0, t1, result.collect()),
                )),
                changed,
            }
        }
    }
}

fn filter_opaque(
    filter_fn: &mut dyn FnMut(Type) -> FilterResult,
    reason: Reason,
    nominal_type: &NominalType,
) -> FilterResult {
    match &nominal_type.underlying_t {
        nominal::UnderlyingT::OpaqueWithLocal { t }
            if reason.loc().source() == reason.def_loc().source() =>
        {
            let FilterResult { type_, changed } = filter_fn(t.dupe());
            match type_.deref() {
                TypeInner::DefT(_, d) if matches!(&**d, DefTInner::EmptyT) => changed_result(
                    Type::new(TypeInner::DefT(reason, DefT::new(DefTInner::EmptyT))),
                ),
                _ => FilterResult {
                    type_: Type::new(TypeInner::NominalT {
                        reason,
                        nominal_type: Rc::new(NominalType::new(NominalTypeInner {
                            underlying_t: nominal::UnderlyingT::OpaqueWithLocal { t: type_ },
                            ..NominalTypeInner::clone(&**nominal_type)
                        })),
                    }),
                    changed,
                },
            }
        }
        nominal::UnderlyingT::CustomError(box nominal::CustomErrorData {
            custom_error_loc,
            t,
        }) => {
            let FilterResult { type_, changed } = filter_fn(t.dupe());
            match type_.deref() {
                TypeInner::DefT(_, d) if matches!(&**d, DefTInner::EmptyT) => changed_result(
                    Type::new(TypeInner::DefT(reason, DefT::new(DefTInner::EmptyT))),
                ),
                _ => FilterResult {
                    type_: Type::new(TypeInner::NominalT {
                        reason,
                        nominal_type: Rc::new(NominalType::new(NominalTypeInner {
                            underlying_t: nominal::UnderlyingT::CustomError(Box::new(
                                nominal::CustomErrorData {
                                    custom_error_loc: custom_error_loc.dupe(),
                                    t: type_,
                                },
                            )),
                            ..NominalTypeInner::clone(&**nominal_type)
                        })),
                    }),
                    changed,
                },
            }
        }
        _ => {
            let upper_t = nominal_type.upper_t.clone().unwrap_or_else(|| {
                Type::new(TypeInner::DefT(
                    reason.dupe(),
                    DefT::new(DefTInner::MixedT(MixedFlavor::MixedEverything)),
                ))
            });
            let FilterResult { type_, changed } = filter_fn(upper_t);
            match type_.deref() {
                TypeInner::DefT(_, d) if matches!(&**d, DefTInner::EmptyT) => changed_result(
                    Type::new(TypeInner::DefT(reason, DefT::new(DefTInner::EmptyT))),
                ),
                _ => FilterResult {
                    type_: Type::new(TypeInner::NominalT {
                        reason,
                        nominal_type: Rc::new(NominalType::new(NominalTypeInner {
                            upper_t: Some(type_),
                            ..NominalTypeInner::clone(&**nominal_type)
                        })),
                    }),
                    changed,
                },
            }
        }
    }
}

fn map_poly(f: &mut dyn FnMut(Type) -> FilterResult, t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::PolyT(box PolyTData {
                kind: poly_kind,
                tparams_loc,
                tparams,
                t_out,
                id,
                strictness_kind,
            }) => {
                let FilterResult { type_, changed } = f(t_out.dupe());
                match type_.deref() {
                    TypeInner::DefT(_, d2) if matches!(&**d2, DefTInner::EmptyT) => {
                        changed_result(type_)
                    }
                    _ => FilterResult {
                        type_: Type::new(TypeInner::DefT(
                            r.dupe(),
                            DefT::new(DefTInner::PolyT(Box::new(PolyTData {
                                kind: *poly_kind,
                                tparams_loc: tparams_loc.dupe(),
                                tparams: tparams.dupe(),
                                t_out: type_,
                                id: id.dupe(),
                                strictness_kind: *strictness_kind,
                            }))),
                        )),
                        changed,
                    },
                }
            }
            _ => unchanged_result(t),
        },
        _ => unchanged_result(t),
    }
}

pub fn empty(t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::DefT(_, d) if matches!(&**d, DefTInner::EmptyT) => unchanged_result(t),
        _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
    }
}

pub fn truthy<'cx>(cx: &mut Context<'cx>, t: Type) -> FilterResult {
    if is_falsy(&t) {
        return changed_result(empty_t::why(reason_of_t(&t).dupe()));
    }
    match t.deref() {
        // unknown things become truthy
        TypeInner::NominalT {
            reason,
            nominal_type,
        } => filter_opaque(&mut |t| truthy(cx, t), reason.dupe(), nominal_type),
        TypeInner::UnionT(r, rep) => {
            recurse_into_union(cx, &truthy, r.dupe(), rep.members_iter().duped().collect())
        }
        TypeInner::MaybeT(_, inner) => changed_result(inner.dupe()),
        TypeInner::OptionalT { type_, .. } => truthy(cx, type_.dupe()),
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::BoolGeneralT => changed_result(Type::new(TypeInner::DefT(
                r.dupe(),
                DefT::new(DefTInner::SingletonBoolT {
                    from_annot: false,
                    value: true,
                }),
            ))),
            DefTInner::StrGeneralT(Literal::AnyLiteral) => changed_result(Type::new(
                TypeInner::DefT(r.dupe(), DefT::new(DefTInner::StrGeneralT(Literal::Truthy))),
            )),
            DefTInner::NumGeneralT(Literal::AnyLiteral) => changed_result(Type::new(
                TypeInner::DefT(r.dupe(), DefT::new(DefTInner::NumGeneralT(Literal::Truthy))),
            )),
            DefTInner::MixedT(_) => changed_result(Type::new(TypeInner::DefT(
                r.dupe(),
                DefT::new(DefTInner::MixedT(MixedFlavor::MixedTruthy)),
            ))),
            _ => unchanged_result(t),
        },
        // an intersection passes through iff all of its members pass through
        TypeInner::IntersectionT(r, rep) => {
            recurse_into_intersection(cx, &truthy, r.dupe(), rep.members_iter().duped().collect())
        }
        _ => unchanged_result(t),
    }
}

pub fn not_truthy<'cx>(cx: &mut Context<'cx>, t: Type) -> FilterResult {
    if is_falsy(&t) {
        // falsy things pass through
        return unchanged_result(t);
    }
    match t.deref() {
        TypeInner::DefT(_, d) if matches!(&**d, DefTInner::PolyT(_)) => {
            map_poly(&mut |t| not_truthy(cx, t), t)
        }
        TypeInner::NominalT {
            reason,
            nominal_type,
        } => filter_opaque(&mut |t| not_truthy(cx, t), reason.dupe(), nominal_type),
        TypeInner::AnyT(r, _) => changed_result(empty_t::why(r.dupe())),
        TypeInner::UnionT(r, rep) => recurse_into_union(
            cx,
            &not_truthy,
            r.dupe(),
            rep.members_iter().duped().collect(),
        ),
        // truthy things get removed
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::SingletonBoolT { .. }
            | DefTInner::SingletonStrT { .. }
            | DefTInner::NumericStrKeyT(_)
            | DefTInner::StrGeneralT(Literal::Truthy)
            | DefTInner::ArrT(_)
            | DefTInner::ObjT(_)
            | DefTInner::InstanceT(_)
            | DefTInner::EnumObjectT { .. }
            | DefTInner::FunT(_, _)
            | DefTInner::ReactAbstractComponentT(_)
            | DefTInner::SingletonNumT { .. }
            | DefTInner::NumGeneralT(Literal::Truthy)
            | DefTInner::MixedT(MixedFlavor::MixedTruthy) => changed_result(empty_t::why(r.dupe())),
            DefTInner::EnumValueT(ev) => {
                let is_truthy_enum = match &***ev {
                    flow_typing_type::type_::EnumInfoInner::ConcreteEnum(info) => {
                        match info.representation_t.deref() {
                            TypeInner::DefT(_, rd) => matches!(
                                rd.deref(),
                                DefTInner::SingletonBoolT { .. }
                                    | DefTInner::StrGeneralT(Literal::Truthy)
                                    | DefTInner::NumGeneralT(Literal::Truthy)
                                    | DefTInner::BigIntGeneralT(Literal::Truthy)
                            ),
                            _ => false,
                        }
                    }
                    flow_typing_type::type_::EnumInfoInner::AbstractEnum { representation_t } => {
                        match representation_t.deref() {
                            TypeInner::DefT(_, rd) => matches!(
                                rd.deref(),
                                DefTInner::SingletonBoolT { .. }
                                    | DefTInner::StrGeneralT(Literal::Truthy)
                                    | DefTInner::NumGeneralT(Literal::Truthy)
                                    | DefTInner::BigIntGeneralT(Literal::Truthy)
                            ),
                            _ => false,
                        }
                    }
                };
                if is_truthy_enum {
                    changed_result(empty_t::why(r.dupe()))
                } else {
                    unchanged_result(t)
                }
            }
            DefTInner::ClassT(_) => changed_result(empty_t::why(r.dupe())),
            // unknown boolies become falsy
            DefTInner::BoolGeneralT => changed_result(Type::new(TypeInner::DefT(
                r.dupe(),
                DefT::new(DefTInner::SingletonBoolT {
                    value: false,
                    from_annot: true,
                }),
            ))),
            DefTInner::StrGeneralT(Literal::AnyLiteral) => {
                changed_result(Type::new(TypeInner::DefT(
                    r.dupe(),
                    DefT::new(DefTInner::SingletonStrT {
                        from_annot: true,
                        value: "".into(),
                    }),
                )))
            }
            DefTInner::NumGeneralT(Literal::AnyLiteral) => {
                changed_result(Type::new(TypeInner::DefT(
                    r.dupe(),
                    DefT::new(DefTInner::SingletonNumT {
                        from_annot: true,
                        value: NumberLiteral(0.0, "0".into()),
                    }),
                )))
            }
            _ => unchanged_result(t),
        },
        TypeInner::ThisInstanceT(box ThisInstanceTData { reason, .. }) => {
            changed_result(empty_t::why(reason.dupe()))
        }
        // unknown boolies become falsy
        TypeInner::MaybeT(r, inner) => {
            let FilterResult {
                type_: t_inner,
                changed,
            } = not_truthy(cx, inner.dupe());
            FilterResult {
                type_: Type::new(TypeInner::UnionT(
                    r.dupe(),
                    union_rep::make(
                        None,
                        UnionKind::UnknownKind,
                        null::why(r.dupe()),
                        void::why(r.dupe()),
                        vec![t_inner].into(),
                    ),
                )),
                changed,
            }
        }
        // an intersection passes through iff all of its members pass through
        TypeInner::IntersectionT(r, rep) => recurse_into_intersection(
            cx,
            &not_truthy,
            r.dupe(),
            rep.members_iter().duped().collect(),
        ),
        // things that don't track truthiness pass through
        _ => unchanged_result(t),
    }
}

pub fn maybe<'cx>(cx: &mut Context<'cx>, t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::NominalT {
            reason,
            nominal_type,
        } => filter_opaque(&mut |t| maybe(cx, t), reason.dupe(), nominal_type),
        TypeInner::UnionT(r, rep) => {
            recurse_into_union(cx, &maybe, r.dupe(), rep.members_iter().duped().collect())
        }
        TypeInner::MaybeT(r, _) => changed_result(Type::new(TypeInner::UnionT(
            r.dupe(),
            union_rep::make(
                None,
                UnionKind::UnknownKind,
                null::why(r.dupe()),
                void::why(r.dupe()),
                Rc::from([]),
            ),
        ))),
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::MixedT(MixedFlavor::MixedEverything) => {
                changed_result(Type::new(TypeInner::UnionT(
                    r.dupe(),
                    union_rep::make(
                        None,
                        UnionKind::UnknownKind,
                        null::why(r.dupe()),
                        void::why(r.dupe()),
                        Rc::from([]),
                    ),
                )))
            }
            DefTInner::MixedT(MixedFlavor::MixedTruthy) => changed_result(empty_t::why(r.dupe())),
            DefTInner::MixedT(MixedFlavor::MixedNonMaybe) => changed_result(empty_t::why(r.dupe())),
            DefTInner::MixedT(MixedFlavor::MixedNonVoid) => changed_result(null::why(r.dupe())),
            DefTInner::MixedT(MixedFlavor::MixedNonNull) => changed_result(void::why(r.dupe())),
            DefTInner::NullT => unchanged_result(t),
            DefTInner::VoidT => unchanged_result(t),
            _ => changed_result(empty_t::why(r.dupe())),
        },
        TypeInner::OptionalT {
            reason, use_desc, ..
        } => changed_result(void::why_with_use_desc(*use_desc, reason.dupe())),
        TypeInner::AnyT(_, _) => unchanged_result(t),
        _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
    }
}

pub fn not_maybe<'cx>(cx: &mut Context<'cx>, t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::NominalT {
            reason,
            nominal_type,
        } => filter_opaque(&mut |t| not_maybe(cx, t), reason.dupe(), nominal_type),
        TypeInner::UnionT(r, rep) => recurse_into_union(
            cx,
            &not_maybe,
            r.dupe(),
            rep.members_iter().duped().collect(),
        ),
        TypeInner::MaybeT(_, inner) => changed_result(inner.dupe()),
        TypeInner::OptionalT { type_, .. } => not_maybe(cx, type_.dupe()),
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::NullT | DefTInner::VoidT => changed_result(empty_t::why(r.dupe())),
            DefTInner::MixedT(MixedFlavor::MixedTruthy) => unchanged_result(t),
            DefTInner::MixedT(MixedFlavor::MixedNonMaybe) => unchanged_result(t),
            DefTInner::MixedT(MixedFlavor::MixedEverything)
            | DefTInner::MixedT(MixedFlavor::MixedNonVoid)
            | DefTInner::MixedT(MixedFlavor::MixedNonNull) => {
                changed_result(Type::new(TypeInner::DefT(
                    r.dupe(),
                    DefT::new(DefTInner::MixedT(MixedFlavor::MixedNonMaybe)),
                )))
            }
            _ => unchanged_result(t),
        },
        _ => unchanged_result(t),
    }
}

pub fn null_filter(t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::NominalT {
            reason,
            nominal_type,
        } => filter_opaque(&mut null_filter, reason.dupe(), nominal_type),
        TypeInner::OptionalT { type_, .. } => match type_.deref() {
            TypeInner::MaybeT(r, _) => changed_result(null::why(r.dupe())),
            _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
        },
        TypeInner::MaybeT(r, _) => changed_result(null::why(r.dupe())),
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::NullT => unchanged_result(t),
            DefTInner::MixedT(MixedFlavor::MixedEverything)
            | DefTInner::MixedT(MixedFlavor::MixedNonVoid) => changed_result(null::why(r.dupe())),
            _ => changed_result(empty_t::why(r.dupe())),
        },
        TypeInner::AnyT(_, _) => changed_result(t),
        _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
    }
}

pub fn not_null<'cx>(cx: &mut Context<'cx>, t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::NominalT {
            reason,
            nominal_type,
        } => filter_opaque(&mut |t| not_null(cx, t), reason.dupe(), nominal_type),
        TypeInner::MaybeT(r, inner) => changed_result(Type::new(TypeInner::UnionT(
            r.dupe(),
            union_rep::make(
                None,
                UnionKind::UnknownKind,
                void::why(r.dupe()),
                inner.dupe(),
                Rc::from([]),
            ),
        ))),
        TypeInner::OptionalT {
            reason,
            type_,
            use_desc,
        } => {
            let FilterResult {
                type_: inner,
                changed,
            } = not_null(cx, type_.dupe());
            FilterResult {
                type_: Type::new(TypeInner::OptionalT {
                    reason: reason.dupe(),
                    type_: inner,
                    use_desc: *use_desc,
                }),
                changed,
            }
        }
        TypeInner::UnionT(r, rep) => recurse_into_union(
            cx,
            &not_null,
            r.dupe(),
            rep.members_iter().duped().collect(),
        ),
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::NullT => changed_result(empty_t::why(r.dupe())),
            DefTInner::MixedT(MixedFlavor::MixedEverything) => {
                changed_result(Type::new(TypeInner::DefT(
                    r.dupe(),
                    DefT::new(DefTInner::MixedT(MixedFlavor::MixedNonNull)),
                )))
            }
            DefTInner::MixedT(MixedFlavor::MixedNonVoid) => {
                changed_result(Type::new(TypeInner::DefT(
                    r.dupe(),
                    DefT::new(DefTInner::MixedT(MixedFlavor::MixedNonMaybe)),
                )))
            }
            _ => unchanged_result(t),
        },
        _ => unchanged_result(t),
    }
}

pub fn undefined(t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::NominalT {
            reason,
            nominal_type,
        } => filter_opaque(&mut undefined, reason.dupe(), nominal_type),
        TypeInner::MaybeT(r, _) => changed_result(void::why(r.dupe())),
        TypeInner::DefT(_, d) if matches!(&**d, DefTInner::VoidT) => unchanged_result(t),
        TypeInner::OptionalT {
            reason, use_desc, ..
        } => changed_result(void::why_with_use_desc(*use_desc, reason.dupe())),
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::MixedT(MixedFlavor::MixedEverything)
            | DefTInner::MixedT(MixedFlavor::MixedNonNull) => changed_result(void::why(r.dupe())),
            _ => changed_result(empty_t::why(r.dupe())),
        },
        TypeInner::AnyT(_, _) => unchanged_result(t),
        _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
    }
}

pub fn not_undefined<'cx>(cx: &mut Context<'cx>, t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::NominalT {
            reason,
            nominal_type,
        } => filter_opaque(&mut |t| not_undefined(cx, t), reason.dupe(), nominal_type),
        TypeInner::MaybeT(r, inner) => changed_result(Type::new(TypeInner::UnionT(
            r.dupe(),
            union_rep::make(
                None,
                UnionKind::UnknownKind,
                null::why(r.dupe()),
                inner.dupe(),
                Rc::from([]),
            ),
        ))),
        TypeInner::OptionalT { type_, .. } => not_undefined(cx, type_.dupe()),
        TypeInner::UnionT(r, rep) => recurse_into_union(
            cx,
            &not_undefined,
            r.dupe(),
            rep.members_iter().duped().collect(),
        ),
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::VoidT => changed_result(empty_t::why(r.dupe())),
            DefTInner::MixedT(MixedFlavor::MixedEverything) => {
                changed_result(Type::new(TypeInner::DefT(
                    r.dupe(),
                    DefT::new(DefTInner::MixedT(MixedFlavor::MixedNonVoid)),
                )))
            }
            DefTInner::MixedT(MixedFlavor::MixedNonNull) => {
                changed_result(Type::new(TypeInner::DefT(
                    r.dupe(),
                    DefT::new(DefTInner::MixedT(MixedFlavor::MixedNonMaybe)),
                )))
            }
            _ => unchanged_result(t),
        },
        _ => unchanged_result(t),
    }
}

pub fn string_literal(expected_loc: ALoc, expected: FlowSmolStr, t: Type) -> FilterResult {
    let expected_desc = VirtualReasonDesc::RStringLit(expected.dupe());
    match t.deref() {
        TypeInner::DefT(_, d) => match d.deref() {
            DefTInner::SingletonStrT { from_annot, value } => {
                if *value == expected {
                    unchanged_result(t)
                } else {
                    changed_result(Type::new(TypeInner::DefT(
                        mk_reason(expected_desc, expected_loc),
                        DefT::new(DefTInner::SingletonStrT {
                            from_annot: *from_annot,
                            value: expected,
                        }),
                    )))
                }
            }
            DefTInner::StrGeneralT(Literal::Truthy) if expected.as_str() != "" => {
                changed_result(Type::new(TypeInner::DefT(
                    reason_of_t(&t).dupe().replace_desc_new(expected_desc),
                    DefT::new(DefTInner::SingletonStrT {
                        from_annot: false,
                        value: expected,
                    }),
                )))
            }
            DefTInner::StrGeneralT(Literal::AnyLiteral) | DefTInner::MixedT(_) => {
                changed_result(Type::new(TypeInner::DefT(
                    reason_of_t(&t).dupe().replace_desc_new(expected_desc),
                    DefT::new(DefTInner::SingletonStrT {
                        from_annot: false,
                        value: expected,
                    }),
                )))
            }
            _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
        },
        TypeInner::AnyT(_, _) => unchanged_result(t),
        TypeInner::TemplateLiteralT { quasis, types, .. } => {
            // `x === 'lit'` keeps the literal when it matches the template
            // shape, and empties only on certain mismatch. Placeholders that
            // cannot be decided match conservatively.
            let (quasis, types) = template_literal_type::fold_concrete_placeholders(quasis, types);
            match template_literal_type::match_string_against_template(
                expected.as_str(),
                &quasis,
                &types,
            ) {
                template_literal_type::MatchResult::Match(_) => {
                    changed_result(Type::new(TypeInner::DefT(
                        reason_of_t(&t).dupe().replace_desc_new(expected_desc),
                        DefT::new(DefTInner::SingletonStrT {
                            from_annot: false,
                            value: expected,
                        }),
                    )))
                }
                template_literal_type::MatchResult::Mismatch => {
                    changed_result(empty_t::why(reason_of_t(&t).dupe()))
                }
                // Malformed templates cannot occur; leave them alone rather
                // than inventing an empty.
                template_literal_type::MatchResult::InvariantViolation => unchanged_result(t),
            }
        }
        _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
    }
}

pub fn not_string_literal(expected: FlowSmolStr, t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::SingletonStrT { value, .. } if *value == expected => {
                changed_result(empty_t::why(r.dupe()))
            }
            _ => unchanged_result(t),
        },
        _ => unchanged_result(t),
    }
}

pub fn number_literal(expected_loc: ALoc, expected: NumberLiteral, t: Type) -> FilterResult {
    let expected_raw = &expected.1;
    let expected_desc = VirtualReasonDesc::RNumberLit(expected_raw.clone());
    match t.deref() {
        TypeInner::DefT(_, d) => match d.deref() {
            DefTInner::SingletonNumT { from_annot, value } => {
                if value.1 == expected.1 {
                    unchanged_result(t)
                } else {
                    changed_result(Type::new(TypeInner::DefT(
                        mk_reason(expected_desc, expected_loc),
                        DefT::new(DefTInner::SingletonNumT {
                            from_annot: *from_annot,
                            value: expected,
                        }),
                    )))
                }
            }
            DefTInner::NumGeneralT(Literal::Truthy) if expected.1.as_str() != "0" => {
                changed_result(Type::new(TypeInner::DefT(
                    reason_of_t(&t).dupe().replace_desc_new(expected_desc),
                    DefT::new(DefTInner::SingletonNumT {
                        from_annot: false,
                        value: expected,
                    }),
                )))
            }
            DefTInner::NumGeneralT(Literal::AnyLiteral) | DefTInner::MixedT(_) => {
                changed_result(Type::new(TypeInner::DefT(
                    reason_of_t(&t).dupe().replace_desc_new(expected_desc),
                    DefT::new(DefTInner::SingletonNumT {
                        from_annot: false,
                        value: expected,
                    }),
                )))
            }
            _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
        },
        TypeInner::AnyT(_, _) => unchanged_result(t),
        _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
    }
}

pub fn not_number_literal(expected: NumberLiteral, t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::SingletonNumT { value, .. } if value.1 == expected.1 => {
                changed_result(empty_t::why(r.dupe()))
            }
            _ => unchanged_result(t),
        },
        _ => unchanged_result(t),
    }
}

pub fn bigint_literal(expected_loc: ALoc, expected: BigIntLiteral, t: Type) -> FilterResult {
    let expected_raw = &expected.1;
    let expected_desc = VirtualReasonDesc::RBigIntLit(expected_raw.clone());
    match t.deref() {
        TypeInner::DefT(_, d) => match d.deref() {
            DefTInner::SingletonBigIntT { from_annot, value } => {
                if value.1 == expected.1 {
                    unchanged_result(t)
                } else {
                    changed_result(Type::new(TypeInner::DefT(
                        mk_reason(expected_desc, expected_loc),
                        DefT::new(DefTInner::SingletonBigIntT {
                            from_annot: *from_annot,
                            value: expected,
                        }),
                    )))
                }
            }
            DefTInner::BigIntGeneralT(Literal::Truthy) if expected_raw.as_str() != "0n" => {
                changed_result(Type::new(TypeInner::DefT(
                    reason_of_t(&t).dupe().replace_desc_new(expected_desc),
                    DefT::new(DefTInner::SingletonBigIntT {
                        from_annot: false,
                        value: expected,
                    }),
                )))
            }
            DefTInner::BigIntGeneralT(Literal::AnyLiteral) | DefTInner::MixedT(_) => {
                changed_result(Type::new(TypeInner::DefT(
                    reason_of_t(&t).dupe().replace_desc_new(expected_desc),
                    DefT::new(DefTInner::SingletonBigIntT {
                        from_annot: false,
                        value: expected,
                    }),
                )))
            }
            _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
        },
        TypeInner::AnyT(_, _) => unchanged_result(t),
        _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
    }
}

pub fn not_bigint_literal(expected: BigIntLiteral, t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::SingletonBigIntT { value, .. } if value.1 == expected.1 => {
                changed_result(empty_t::why(r.dupe()))
            }
            _ => unchanged_result(t),
        },
        _ => unchanged_result(t),
    }
}

pub fn true_(t: Type) -> FilterResult {
    let lit_reason_desc = VirtualReasonDesc::RBooleanLit(true);
    match t.deref() {
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::SingletonBoolT {
                from_annot,
                value: true,
            } => unchanged_result(Type::new(TypeInner::DefT(
                r.dupe().replace_desc_new(lit_reason_desc.clone()),
                DefT::new(DefTInner::SingletonBoolT {
                    from_annot: *from_annot,
                    value: true,
                }),
            ))),
            DefTInner::BoolGeneralT | DefTInner::MixedT(_) => {
                changed_result(Type::new(TypeInner::DefT(
                    r.dupe().replace_desc_new(lit_reason_desc),
                    DefT::new(DefTInner::SingletonBoolT {
                        from_annot: false,
                        value: true,
                    }),
                )))
            }
            _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
        },
        TypeInner::AnyT(_, _) => unchanged_result(t),
        _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
    }
}

pub fn not_true(t: Type) -> FilterResult {
    let lit_reason_desc = VirtualReasonDesc::RBooleanLit(false);
    match t.deref() {
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::SingletonBoolT { value: true, .. } => changed_result(empty_t::why(r.dupe())),
            DefTInner::BoolGeneralT => changed_result(Type::new(TypeInner::DefT(
                r.dupe().replace_desc_new(lit_reason_desc),
                DefT::new(DefTInner::SingletonBoolT {
                    from_annot: false,
                    value: false,
                }),
            ))),
            _ => unchanged_result(t),
        },
        _ => unchanged_result(t),
    }
}

pub fn false_(t: Type) -> FilterResult {
    let lit_reason_desc = VirtualReasonDesc::RBooleanLit(false);
    match t.deref() {
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::SingletonBoolT {
                from_annot,
                value: false,
            } => unchanged_result(Type::new(TypeInner::DefT(
                r.dupe().replace_desc_new(lit_reason_desc.clone()),
                DefT::new(DefTInner::SingletonBoolT {
                    from_annot: *from_annot,
                    value: false,
                }),
            ))),
            DefTInner::BoolGeneralT | DefTInner::MixedT(_) => {
                changed_result(Type::new(TypeInner::DefT(
                    r.dupe().replace_desc_new(lit_reason_desc),
                    DefT::new(DefTInner::SingletonBoolT {
                        from_annot: false,
                        value: false,
                    }),
                )))
            }
            _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
        },
        TypeInner::AnyT(_, _) => unchanged_result(t),
        _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
    }
}

pub fn not_false(t: Type) -> FilterResult {
    let lit_reason_desc = VirtualReasonDesc::RBooleanLit(true);
    match t.deref() {
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::SingletonBoolT { value: false, .. } => {
                changed_result(empty_t::why(r.dupe()))
            }
            DefTInner::BoolGeneralT => changed_result(Type::new(TypeInner::DefT(
                r.dupe().replace_desc_new(lit_reason_desc),
                DefT::new(DefTInner::SingletonBoolT {
                    from_annot: false,
                    value: true,
                }),
            ))),
            _ => unchanged_result(t),
        },
        _ => unchanged_result(t),
    }
}

pub fn boolean(loc: ALoc, t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::MixedT(MixedFlavor::MixedTruthy) => {
                changed_result(Type::new(TypeInner::DefT(
                    r.dupe().replace_desc_new(bool_module_t::desc()),
                    DefT::new(DefTInner::SingletonBoolT {
                        from_annot: false,
                        value: true,
                    }),
                )))
            }
            DefTInner::MixedT(_) => changed_result(Type::new(TypeInner::DefT(
                mk_reason(VirtualReasonDesc::RBoolean, loc),
                DefT::new(DefTInner::BoolGeneralT),
            ))),
            DefTInner::BoolGeneralT | DefTInner::SingletonBoolT { .. } => unchanged_result(t),
            DefTInner::EnumValueT(ev) => {
                let is_bool_enum = match &***ev {
                    flow_typing_type::type_::EnumInfoInner::ConcreteEnum(info) => {
                        match info.representation_t.deref() {
                            TypeInner::DefT(_, rd) => matches!(
                                rd.deref(),
                                DefTInner::BoolGeneralT | DefTInner::SingletonBoolT { .. }
                            ),
                            _ => false,
                        }
                    }
                    flow_typing_type::type_::EnumInfoInner::AbstractEnum { representation_t } => {
                        match representation_t.deref() {
                            TypeInner::DefT(_, rd) => matches!(
                                rd.deref(),
                                DefTInner::BoolGeneralT | DefTInner::SingletonBoolT { .. }
                            ),
                            _ => false,
                        }
                    }
                };
                if is_bool_enum {
                    unchanged_result(t)
                } else {
                    changed_result(empty_t::why(reason_of_t(&t).dupe()))
                }
            }
            _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
        },
        TypeInner::AnyT(_, _) => changed_result(Type::new(TypeInner::DefT(
            mk_reason(VirtualReasonDesc::RBoolean, loc),
            DefT::new(DefTInner::BoolGeneralT),
        ))),
        _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
    }
}

pub fn not_boolean(t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::DefT(_, d) => match d.deref() {
            DefTInner::BoolGeneralT | DefTInner::SingletonBoolT { .. } => {
                changed_result(empty_t::why(reason_of_t(&t).dupe()))
            }
            DefTInner::EnumValueT(ev) => {
                let is_bool_enum = match &***ev {
                    flow_typing_type::type_::EnumInfoInner::ConcreteEnum(info) => {
                        match info.representation_t.deref() {
                            TypeInner::DefT(_, rd) => matches!(
                                rd.deref(),
                                DefTInner::BoolGeneralT | DefTInner::SingletonBoolT { .. }
                            ),
                            _ => false,
                        }
                    }
                    flow_typing_type::type_::EnumInfoInner::AbstractEnum { representation_t } => {
                        match representation_t.deref() {
                            TypeInner::DefT(_, rd) => matches!(
                                rd.deref(),
                                DefTInner::BoolGeneralT | DefTInner::SingletonBoolT { .. }
                            ),
                            _ => false,
                        }
                    }
                };
                if is_bool_enum {
                    changed_result(empty_t::why(reason_of_t(&t).dupe()))
                } else {
                    unchanged_result(t)
                }
            }
            _ => unchanged_result(t),
        },
        _ => unchanged_result(t),
    }
}

pub fn string(loc: ALoc, t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::MixedT(MixedFlavor::MixedTruthy) => {
                changed_result(Type::new(TypeInner::DefT(
                    r.dupe().replace_desc_new(str_module_t::desc()),
                    DefT::new(DefTInner::StrGeneralT(Literal::Truthy)),
                )))
            }
            DefTInner::MixedT(_) => changed_result(Type::new(TypeInner::DefT(
                mk_reason(VirtualReasonDesc::RString, loc),
                DefT::new(DefTInner::StrGeneralT(Literal::AnyLiteral)),
            ))),
            DefTInner::StrGeneralT(_) | DefTInner::SingletonStrT { .. } => unchanged_result(t),
            DefTInner::EnumValueT(ev) => {
                let is_str_enum = match &***ev {
                    flow_typing_type::type_::EnumInfoInner::ConcreteEnum(info) => {
                        match info.representation_t.deref() {
                            TypeInner::DefT(_, rd) => matches!(
                                rd.deref(),
                                DefTInner::StrGeneralT(_) | DefTInner::SingletonStrT { .. }
                            ),
                            _ => false,
                        }
                    }
                    flow_typing_type::type_::EnumInfoInner::AbstractEnum { representation_t } => {
                        match representation_t.deref() {
                            TypeInner::DefT(_, rd) => matches!(
                                rd.deref(),
                                DefTInner::StrGeneralT(_) | DefTInner::SingletonStrT { .. }
                            ),
                            _ => false,
                        }
                    }
                };
                if is_str_enum {
                    unchanged_result(t)
                } else {
                    changed_result(empty_t::why(reason_of_t(&t).dupe()))
                }
            }
            _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
        },
        TypeInner::TemplateLiteralT { .. } | TypeInner::StringMappingT { .. } => {
            unchanged_result(t)
        }
        TypeInner::AnyT(_, _) => changed_result(Type::new(TypeInner::DefT(
            mk_reason(VirtualReasonDesc::RString, loc),
            DefT::new(DefTInner::StrGeneralT(Literal::AnyLiteral)),
        ))),
        _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
    }
}

pub fn not_string(t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::TemplateLiteralT { .. } | TypeInner::StringMappingT { .. } => {
            changed_result(empty_t::why(reason_of_t(&t).dupe()))
        }
        TypeInner::DefT(_, d) => match d.deref() {
            DefTInner::StrGeneralT(_) | DefTInner::SingletonStrT { .. } => {
                changed_result(empty_t::why(reason_of_t(&t).dupe()))
            }
            DefTInner::EnumValueT(ev) => {
                let is_str_enum = match &***ev {
                    flow_typing_type::type_::EnumInfoInner::ConcreteEnum(info) => {
                        match info.representation_t.deref() {
                            TypeInner::DefT(_, rd) => matches!(
                                rd.deref(),
                                DefTInner::StrGeneralT(_) | DefTInner::SingletonStrT { .. }
                            ),
                            _ => false,
                        }
                    }
                    flow_typing_type::type_::EnumInfoInner::AbstractEnum { representation_t } => {
                        match representation_t.deref() {
                            TypeInner::DefT(_, rd) => matches!(
                                rd.deref(),
                                DefTInner::StrGeneralT(_) | DefTInner::SingletonStrT { .. }
                            ),
                            _ => false,
                        }
                    }
                };
                if is_str_enum {
                    changed_result(empty_t::why(reason_of_t(&t).dupe()))
                } else {
                    unchanged_result(t)
                }
            }
            _ => unchanged_result(t),
        },
        _ => unchanged_result(t),
    }
}

pub fn symbol(loc: ALoc, t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::DefT(_, d)
            if matches!(&**d, DefTInner::SymbolT | DefTInner::UniqueSymbolT(_)) =>
        {
            unchanged_result(t)
        }
        TypeInner::DefT(_, d) if matches!(&**d, DefTInner::MixedT(_)) => {
            changed_result(symbol_t::why(mk_reason(VirtualReasonDesc::RSymbol, loc)))
        }
        TypeInner::AnyT(_, _) => {
            changed_result(symbol_t::why(mk_reason(VirtualReasonDesc::RSymbol, loc)))
        }
        _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
    }
}

pub fn not_symbol(t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::DefT(_, d)
            if matches!(&**d, DefTInner::SymbolT | DefTInner::UniqueSymbolT(_)) =>
        {
            changed_result(empty_t::why(reason_of_t(&t).dupe()))
        }
        _ => unchanged_result(t),
    }
}

pub fn number(loc: ALoc, t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::MixedT(MixedFlavor::MixedTruthy) => {
                changed_result(Type::new(TypeInner::DefT(
                    r.dupe().replace_desc_new(num_module_t::desc()),
                    DefT::new(DefTInner::NumGeneralT(Literal::Truthy)),
                )))
            }
            DefTInner::MixedT(_) => changed_result(Type::new(TypeInner::DefT(
                mk_reason(VirtualReasonDesc::RNumber, loc),
                DefT::new(DefTInner::NumGeneralT(Literal::AnyLiteral)),
            ))),
            DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. } => unchanged_result(t),
            DefTInner::EnumValueT(ev) => {
                let is_num_enum = match &***ev {
                    flow_typing_type::type_::EnumInfoInner::ConcreteEnum(info) => {
                        match info.representation_t.deref() {
                            TypeInner::DefT(_, rd) => matches!(
                                rd.deref(),
                                DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. }
                            ),
                            _ => false,
                        }
                    }
                    flow_typing_type::type_::EnumInfoInner::AbstractEnum { representation_t } => {
                        match representation_t.deref() {
                            TypeInner::DefT(_, rd) => matches!(
                                rd.deref(),
                                DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. }
                            ),
                            _ => false,
                        }
                    }
                };
                if is_num_enum {
                    unchanged_result(t)
                } else {
                    changed_result(empty_t::why(reason_of_t(&t).dupe()))
                }
            }
            _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
        },
        TypeInner::AnyT(_, _) => changed_result(Type::new(TypeInner::DefT(
            mk_reason(VirtualReasonDesc::RNumber, loc),
            DefT::new(DefTInner::NumGeneralT(Literal::AnyLiteral)),
        ))),
        _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
    }
}

pub fn not_number(t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::DefT(_, d) => match d.deref() {
            DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. } => {
                changed_result(empty_t::why(reason_of_t(&t).dupe()))
            }
            DefTInner::EnumValueT(ev) => {
                let is_num_enum = match &***ev {
                    flow_typing_type::type_::EnumInfoInner::ConcreteEnum(info) => {
                        match info.representation_t.deref() {
                            TypeInner::DefT(_, rd) => matches!(
                                rd.deref(),
                                DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. }
                            ),
                            _ => false,
                        }
                    }
                    flow_typing_type::type_::EnumInfoInner::AbstractEnum { representation_t } => {
                        match representation_t.deref() {
                            TypeInner::DefT(_, rd) => matches!(
                                rd.deref(),
                                DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. }
                            ),
                            _ => false,
                        }
                    }
                };
                if is_num_enum {
                    changed_result(empty_t::why(reason_of_t(&t).dupe()))
                } else {
                    unchanged_result(t)
                }
            }
            _ => unchanged_result(t),
        },
        _ => unchanged_result(t),
    }
}

pub fn bigint(loc: ALoc, t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::MixedT(MixedFlavor::MixedTruthy) => {
                changed_result(Type::new(TypeInner::DefT(
                    r.dupe().replace_desc_new(bigint_module_t::desc()),
                    DefT::new(DefTInner::BigIntGeneralT(Literal::Truthy)),
                )))
            }
            DefTInner::MixedT(_) => changed_result(Type::new(TypeInner::DefT(
                mk_reason(VirtualReasonDesc::RBigInt, loc),
                DefT::new(DefTInner::BigIntGeneralT(Literal::AnyLiteral)),
            ))),
            DefTInner::BigIntGeneralT(_) | DefTInner::SingletonBigIntT { .. } => {
                unchanged_result(t)
            }
            DefTInner::EnumValueT(ev) => {
                let is_bigint_enum = match &***ev {
                    flow_typing_type::type_::EnumInfoInner::ConcreteEnum(info) => {
                        match info.representation_t.deref() {
                            TypeInner::DefT(_, rd) => matches!(
                                rd.deref(),
                                DefTInner::BigIntGeneralT(_) | DefTInner::SingletonBigIntT { .. }
                            ),
                            _ => false,
                        }
                    }
                    flow_typing_type::type_::EnumInfoInner::AbstractEnum { representation_t } => {
                        match representation_t.deref() {
                            TypeInner::DefT(_, rd) => matches!(
                                rd.deref(),
                                DefTInner::BigIntGeneralT(_) | DefTInner::SingletonBigIntT { .. }
                            ),
                            _ => false,
                        }
                    }
                };
                if is_bigint_enum {
                    unchanged_result(t)
                } else {
                    changed_result(empty_t::why(reason_of_t(&t).dupe()))
                }
            }
            _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
        },
        TypeInner::AnyT(_, _) => changed_result(Type::new(TypeInner::DefT(
            mk_reason(VirtualReasonDesc::RBigInt, loc),
            DefT::new(DefTInner::BigIntGeneralT(Literal::AnyLiteral)),
        ))),
        _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
    }
}

pub fn not_bigint(t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::DefT(_, d) => match d.deref() {
            DefTInner::BigIntGeneralT(_) | DefTInner::SingletonBigIntT { .. } => {
                changed_result(empty_t::why(reason_of_t(&t).dupe()))
            }
            DefTInner::EnumValueT(ev) => {
                let is_bigint_enum = match &***ev {
                    flow_typing_type::type_::EnumInfoInner::ConcreteEnum(info) => {
                        match info.representation_t.deref() {
                            TypeInner::DefT(_, rd) => matches!(
                                rd.deref(),
                                DefTInner::BigIntGeneralT(_) | DefTInner::SingletonBigIntT { .. }
                            ),
                            _ => false,
                        }
                    }
                    flow_typing_type::type_::EnumInfoInner::AbstractEnum { representation_t } => {
                        match representation_t.deref() {
                            TypeInner::DefT(_, rd) => matches!(
                                rd.deref(),
                                DefTInner::BigIntGeneralT(_) | DefTInner::SingletonBigIntT { .. }
                            ),
                            _ => false,
                        }
                    }
                };
                if is_bigint_enum {
                    changed_result(empty_t::why(reason_of_t(&t).dupe()))
                } else {
                    unchanged_result(t)
                }
            }
            _ => unchanged_result(t),
        },
        _ => unchanged_result(t),
    }
}

pub fn object_<'cx>(cx: &mut Context<'cx>, t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::DefT(_, d) if matches!(&**d, DefTInner::PolyT(_)) => {
            map_poly(&mut |t| object_(cx, t), t)
        }
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::MixedT(flavor) => {
                let reason = r.dupe().replace_desc_new(None);
                let dict = DictType {
                    key: str_module_t::why(r.dupe()),
                    value: Type::new(TypeInner::DefT(
                        r.dupe()
                            .replace_desc_new(flow_typing_type::type_::mixed_t::desc()),
                        DefT::new(DefTInner::MixedT(MixedFlavor::MixedEverything)),
                    )),
                    dict_name: None,
                    dict_polarity: Polarity::Positive,
                };
                let proto = Type::new(TypeInner::ObjProtoT(reason.dupe()));
                let obj = obj_type::mk_with_proto(
                    cx,
                    reason.dupe(),
                    ObjKind::Indexed(dict),
                    None,
                    None,
                    None,
                    None,
                    proto,
                    ObjLiteral::NonLiteral,
                );
                match flavor {
                    MixedFlavor::MixedTruthy
                    | MixedFlavor::MixedNonMaybe
                    | MixedFlavor::MixedNonNull => changed_result(obj),
                    MixedFlavor::MixedFunction => {
                        changed_result(empty_t::why(reason_of_t(&t).dupe()))
                    }
                    MixedFlavor::MixedEverything | MixedFlavor::MixedNonVoid => {
                        let union_reason = reason_of_t(&t).dupe().replace_desc_new(None);
                        changed_result(Type::new(TypeInner::UnionT(
                            union_reason,
                            union_rep::make(
                                None,
                                UnionKind::UnknownKind,
                                null::why(r.dupe()),
                                obj,
                                Rc::from([]),
                            ),
                        )))
                    }
                }
            }
            DefTInner::ObjT(_)
            | DefTInner::ArrT(_)
            | DefTInner::NullT
            | DefTInner::InstanceT(_)
            | DefTInner::EnumObjectT { .. } => unchanged_result(t),
            _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
        },
        TypeInner::AnyT(_, _) => unchanged_result(t),
        _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
    }
}

pub fn not_object(t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::DefT(_, d) if matches!(&**d, DefTInner::PolyT(_)) => {
            map_poly(&mut not_object, t)
        }
        TypeInner::AnyT(_, _) => changed_result(empty_t::why(reason_of_t(&t).dupe())),
        TypeInner::DefT(_, d) => match d.deref() {
            DefTInner::ObjT(_)
            | DefTInner::ArrT(_)
            | DefTInner::NullT
            | DefTInner::InstanceT(_)
            | DefTInner::EnumObjectT { .. } => changed_result(empty_t::why(reason_of_t(&t).dupe())),
            _ => unchanged_result(t),
        },
        _ => unchanged_result(t),
    }
}

pub fn function_(t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::DefT(_, d) if matches!(&**d, DefTInner::PolyT(_)) => map_poly(&mut function_, t),
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::MixedT(_) => changed_result(Type::new(TypeInner::DefT(
                r.dupe().replace_desc_new(VirtualReasonDesc::RFunction(
                    flow_common::reason::ReasonDescFunction::RUnknown,
                )),
                DefT::new(DefTInner::MixedT(MixedFlavor::MixedFunction)),
            ))),
            DefTInner::FunT(_, _) | DefTInner::ClassT(_) => unchanged_result(t),
            // A value with a call or construct signature (`declare var Map: MapConstructor`)
            // is a function at runtime.
            DefTInner::InstanceT(inst)
                if inst.inst.inst_call_t.is_some() || inst.inst.inst_construct_t.is_some() =>
            {
                unchanged_result(t)
            }
            _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
        },
        TypeInner::AnyT(_, _) => unchanged_result(t),
        _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
    }
}

pub fn not_function(t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::DefT(_, d) if matches!(&**d, DefTInner::PolyT(_)) => {
            map_poly(&mut not_function, t)
        }
        TypeInner::AnyT(_, _) => changed_result(empty_t::why(reason_of_t(&t).dupe())),
        TypeInner::DefT(_, d) => match d.deref() {
            DefTInner::FunT(_, _) | DefTInner::ClassT(_) => {
                changed_result(empty_t::why(reason_of_t(&t).dupe()))
            }
            DefTInner::InstanceT(inst)
                if inst.inst.inst_call_t.is_some() || inst.inst.inst_construct_t.is_some() =>
            {
                changed_result(empty_t::why(reason_of_t(&t).dupe()))
            }
            _ => unchanged_result(t),
        },
        _ => unchanged_result(t),
    }
}

pub fn array(t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::DefT(r, d) => match d.deref() {
            DefTInner::MixedT(_) => {
                let r = r.dupe().replace_desc_new(None);
                changed_result(Type::new(TypeInner::DefT(
                    r.dupe().replace_desc_new(VirtualReasonDesc::RROArrayType),
                    DefT::new(DefTInner::ArrT(Rc::new(ArrType::ROArrayAT(Box::new(
                        ROArrayATData {
                            elem_t: Type::new(TypeInner::DefT(
                                r,
                                DefT::new(DefTInner::MixedT(MixedFlavor::MixedEverything)),
                            )),
                            react_dro: None,
                            literal: ArrLiteral::NonLiteral,
                        },
                    ))))),
                )))
            }
            DefTInner::ArrT(_) => unchanged_result(t),
            _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
        },
        TypeInner::AnyT(_, _) => unchanged_result(t),
        _ => changed_result(empty_t::why(reason_of_t(&t).dupe())),
    }
}

pub fn not_array(t: Type) -> FilterResult {
    match t.deref() {
        TypeInner::AnyT(_, _) => changed_result(empty_t::why(reason_of_t(&t).dupe())),
        TypeInner::DefT(_, d) if matches!(&**d, DefTInner::ArrT(_)) => {
            changed_result(empty_t::why(reason_of_t(&t).dupe()))
        }
        _ => unchanged_result(t),
    }
}

pub fn array_length(sense: bool, op: &ArrayLengthOp, n: i32, t: Type) -> FilterResult {
    if let TypeInner::DefT(_, d) = t.deref()
        && let DefTInner::ArrT(arr) = d.deref()
    {
        match arr.deref() {
            ArrType::TupleAT(box TupleATData {
                arity: (num_req, num_total),
                inexact,
                ..
            })
            | ArrType::ArrayAT(box ArrayATData {
                tuple_view:
                    Some(TupleView {
                        arity: (num_req, num_total),
                        inexact,
                        ..
                    }),
                ..
            }) => {
                // A tuple with a rest element has no maximum length, like an inexact tuple.
                let inexact = *inexact
                    || matches!(
                        arr.deref(),
                        ArrType::TupleAT(box TupleATData { rest: Some(_), .. })
                            | ArrType::ArrayAT(box ArrayATData {
                                tuple_view: Some(TupleView { rest: Some(_), .. }),
                                ..
                            })
                    );
                let matches = match op {
                    ArrayLengthOp::ArrLenEqual => {
                        if n == *num_req && n == *num_total && !inexact {
                            Some(true)
                        } else if n >= *num_req && (n <= *num_total || inexact) {
                            None
                        } else {
                            Some(false)
                        }
                    }
                    ArrayLengthOp::ArrLenGreaterThanEqual => {
                        if n <= *num_req {
                            Some(true)
                        } else if n <= *num_total || inexact {
                            None
                        } else {
                            Some(false)
                        }
                    }
                };
                match (matches, sense) {
                    (Some(true), true) | (Some(false), false) | (None, _) => unchanged_result(t),
                    (Some(false), true) | (Some(true), false) => {
                        changed_result(empty_t::why(reason_of_t(&t).dupe()))
                    }
                }
            }
            ArrType::ArrayAT(box ArrayATData { .. })
            | ArrType::ROArrayAT(box ROArrayATData { .. }) => {
                // [...]` matches every length, so arrays are matched.
                let matches = n == 0 && *op == ArrayLengthOp::ArrLenGreaterThanEqual;
                if matches == sense {
                    unchanged_result(t)
                } else {
                    changed_result(empty_t::why(reason_of_t(&t).dupe()))
                }
            }
        }
    } else {
        unchanged_result(t)
    }
}

pub fn sentinel_refinement(
    v: &Type,
    reason: Reason,
    l: Type,
    sense: bool,
    enum_star: &UnionEnumStar,
) -> FilterResult {
    fn enum_match(sense: bool, v: &Type, e: &UnionEnum) -> bool {
        match (v.deref(), e) {
            (TypeInner::DefT(_, d), UnionEnum::Str(sentinel)) => match d.deref() {
                DefTInner::SingletonStrT { value, .. } => (*value == *sentinel) != sense,
                _ => false,
            },
            (TypeInner::DefT(_, d), UnionEnum::Num(NumberLiteral(sentinel, _))) => {
                match d.deref() {
                    DefTInner::SingletonNumT {
                        value: NumberLiteral(value, _),
                        ..
                    } => (*value == *sentinel) != sense,
                    _ => false,
                }
            }
            (TypeInner::DefT(_, d), UnionEnum::Bool(sentinel)) => match d.deref() {
                DefTInner::SingletonBoolT { value, .. } => (*value == *sentinel) != sense,
                _ => false,
            },
            (TypeInner::DefT(_, d), UnionEnum::BigInt(BigIntLiteral(sentinel, _))) => {
                match d.deref() {
                    DefTInner::SingletonBigIntT {
                        value: BigIntLiteral(value, _),
                        ..
                    } => (*value == *sentinel) != sense,
                    _ => false,
                }
            }
            (TypeInner::DefT(_, d), UnionEnum::Null) if matches!(&**d, DefTInner::NullT) => true,
            (TypeInner::DefT(_, d), UnionEnum::Void) if matches!(&**d, DefTInner::VoidT) => true,
            _ => false,
        }
    }

    fn is_str_or_num_or_bool_or_bigint_or_null_or_void(v: &Type) -> bool {
        match v.deref() {
            TypeInner::DefT(_, d) => matches!(
                d.deref(),
                DefTInner::StrGeneralT(_)
                    | DefTInner::SingletonStrT { .. }
                    | DefTInner::NumGeneralT(_)
                    | DefTInner::SingletonNumT { .. }
                    | DefTInner::BoolGeneralT
                    | DefTInner::SingletonBoolT { .. }
                    | DefTInner::BigIntGeneralT(_)
                    | DefTInner::SingletonBigIntT { .. }
                    | DefTInner::NullT
                    | DefTInner::VoidT
            ),
            _ => false,
        }
    }

    fn filtered_loop(v: &Type, sense: bool, enum_star: &UnionEnumStar) -> bool {
        match (v.deref(), enum_star) {
            (_, UnionEnumStar::One(e)) if enum_match(sense, v, e) && !sense => true,
            (TypeInner::DefT(_, d), UnionEnumStar::One(UnionEnum::Str(sentinel)))
                if matches!(
                    d.deref(),
                    DefTInner::StrGeneralT(_) | DefTInner::SingletonStrT { .. }
                ) && enum_match(sense, v, &UnionEnum::Str(sentinel.dupe())) =>
            {
                true
            }
            (TypeInner::DefT(_, d), UnionEnumStar::One(UnionEnum::Num(sentinel)))
                if matches!(
                    d.deref(),
                    DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. }
                ) && enum_match(sense, v, &UnionEnum::Num(sentinel.clone())) =>
            {
                true
            }
            (TypeInner::DefT(_, d), UnionEnumStar::One(UnionEnum::Bool(sentinel)))
                if matches!(
                    d.deref(),
                    DefTInner::BoolGeneralT | DefTInner::SingletonBoolT { .. }
                ) && enum_match(sense, v, &UnionEnum::Bool(*sentinel)) =>
            {
                true
            }
            (TypeInner::DefT(_, d), UnionEnumStar::One(UnionEnum::BigInt(sentinel)))
                if matches!(
                    d.deref(),
                    DefTInner::BigIntGeneralT(_) | DefTInner::SingletonBigIntT { .. }
                ) && enum_match(sense, v, &UnionEnum::BigInt(sentinel.clone())) =>
            {
                true
            }
            (_, UnionEnumStar::Many(enums))
                if sense && is_str_or_num_or_bool_or_bigint_or_null_or_void(v) =>
            {
                enums.iter().all(|enum_val| {
                    if !enum_match(sense, v, enum_val) {
                        filtered_loop(v, sense, &UnionEnumStar::One(enum_val.clone()))
                    } else {
                        true
                    }
                })
            }
            (TypeInner::DefT(_, d), UnionEnumStar::One(UnionEnum::Str(_)))
                if matches!(
                    d.deref(),
                    DefTInner::StrGeneralT(_) | DefTInner::SingletonStrT { .. }
                ) =>
            {
                false
            }
            (TypeInner::DefT(_, d), UnionEnumStar::One(UnionEnum::Num(_)))
                if matches!(
                    d.deref(),
                    DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. }
                ) =>
            {
                false
            }
            (TypeInner::DefT(_, d), UnionEnumStar::One(UnionEnum::Bool(_)))
                if matches!(
                    d.deref(),
                    DefTInner::BoolGeneralT | DefTInner::SingletonBoolT { .. }
                ) =>
            {
                false
            }
            (TypeInner::DefT(_, d), UnionEnumStar::One(UnionEnum::BigInt(_)))
                if matches!(
                    d.deref(),
                    DefTInner::BigIntGeneralT(_) | DefTInner::SingletonBigIntT { .. }
                ) =>
            {
                false
            }
            (TypeInner::DefT(_, d), UnionEnumStar::One(UnionEnum::Null))
                if matches!(d.deref(), DefTInner::NullT) =>
            {
                false
            }
            (TypeInner::DefT(_, d), UnionEnumStar::One(UnionEnum::Void))
                if matches!(d.deref(), DefTInner::VoidT) =>
            {
                false
            }
            (_, UnionEnumStar::Many(_)) if is_str_or_num_or_bool_or_bigint_or_null_or_void(v) => {
                // types don't match (would've been matched above)
                false
            }
            // we don't prune other types like objects or instances, even though
            // a test like `if (ObjT === StrT)` seems obviously unreachable, but
            // we have to be wary of toString and valueOf on objects/instances.
            _ if sense && is_str_or_num_or_bool_or_bigint_or_null_or_void(v) => true,
            // property exists, but is not something we can use for refinement
            _ => false,
        }
    }

    if filtered_loop(v, sense, enum_star) {
        changed_result(empty_t::why(reason))
    } else {
        unchanged_result(l)
    }
}

// Type guard filtering

// These tags represent sets of values that have no overlap.
// - ObjTag represents objects that are not callable. Callable objects are associated
//   with both the ObjTag and the FunTag.
// - Interfaces can be objects, class instances, arrays, functions and symbols
pub mod type_tag {
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    pub(super) enum TypeTagInner {
        BoolTag,
        StringTag,
        NumberTag,
        NullTag,
        VoidTag,
        SymbolTag,
        ObjTag,
        FunTag,
        BigIntTag,
        ClassInstanceTag,
        ArrTag,
        EnumTag,
        RendersTag,
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    pub struct TypeTag(pub(super) TypeTagInner);
}

pub type TypeTagSet = BTreeSet<type_tag::TypeTag>;

fn tag_of_def_t<'cx>(cx: &mut Context<'cx>, d: &DefTInner) -> Option<TypeTagSet> {
    use type_tag::TypeTag;
    use type_tag::TypeTagInner;
    match d {
        DefTInner::NullT => Some(BTreeSet::from([TypeTag(TypeTagInner::NullTag)])),
        DefTInner::VoidT => Some(BTreeSet::from([TypeTag(TypeTagInner::VoidTag)])),
        DefTInner::SymbolT | DefTInner::UniqueSymbolT(_) => {
            Some(BTreeSet::from([TypeTag(TypeTagInner::SymbolTag)]))
        }
        DefTInner::FunT(_, _) => Some(BTreeSet::from([TypeTag(TypeTagInner::FunTag)])),
        DefTInner::SingletonBoolT { .. } | DefTInner::BoolGeneralT => {
            Some(BTreeSet::from([TypeTag(TypeTagInner::BoolTag)]))
        }
        DefTInner::SingletonStrT { .. }
        | DefTInner::NumericStrKeyT(_)
        | DefTInner::StrGeneralT(_) => Some(BTreeSet::from([TypeTag(TypeTagInner::StringTag)])),
        DefTInner::SingletonNumT { .. } | DefTInner::NumGeneralT(_) => {
            Some(BTreeSet::from([TypeTag(TypeTagInner::NumberTag)]))
        }
        DefTInner::BigIntGeneralT(_) | DefTInner::SingletonBigIntT { .. } => {
            Some(BTreeSet::from([TypeTag(TypeTagInner::BigIntTag)]))
        }
        DefTInner::ObjT(obj) => {
            if obj.call_t.is_some() {
                Some(BTreeSet::from([
                    TypeTag(TypeTagInner::ObjTag),
                    TypeTag(TypeTagInner::FunTag),
                ]))
            } else {
                Some(BTreeSet::from([TypeTag(TypeTagInner::ObjTag)]))
            }
        }
        DefTInner::InstanceT(instance_t) => tag_of_inst(&instance_t.inst),
        DefTInner::ArrT(_) => Some(BTreeSet::from([TypeTag(TypeTagInner::ArrTag)])),
        DefTInner::PolyT(box PolyTData { t_out, .. }) => tag_of_t(cx, t_out),
        DefTInner::EnumValueT(_) => Some(BTreeSet::from([TypeTag(TypeTagInner::EnumTag)])),
        DefTInner::EmptyT => Some(BTreeSet::new()),
        DefTInner::RendersT(_) => Some(BTreeSet::from([TypeTag(TypeTagInner::RendersTag)])),
        DefTInner::MixedT(_)
        | DefTInner::ClassT(_)
        | DefTInner::TypeT { .. }
        | DefTInner::ReactAbstractComponentT(_)
        | DefTInner::EnumObjectT { .. } => None,
    }
}

fn tag_of_inst(inst: &InstType) -> Option<TypeTagSet> {
    use type_tag::TypeTag;
    use type_tag::TypeTagInner;
    // Only call signatures justify [FunTag] for tag-based narrowing. A
    // construct signature alone makes [typeof x === 'function'] true at
    // runtime, but Flow's [InstanceT <: FunT] cast and [CallT]
    // rules only honor [inst_call_t] — adding [FunTag] for construct-only
    // interfaces would let [typeof] narrow into a branch where the value
    // can't actually be used as a function, surfacing as confusing "missing
    // method bind" errors.
    let _ = inst.inst_construct_t;
    let mut tags = if inst.inst_call_t.is_some() {
        BTreeSet::from([TypeTag(TypeTagInner::FunTag)])
    } else {
        BTreeSet::new()
    };
    match &inst.inst_kind {
        InstanceKind::ClassKind | InstanceKind::RecordKind { .. } => {
            tags.insert(TypeTag(TypeTagInner::ClassInstanceTag));
        }
        InstanceKind::InterfaceKind { .. } => {
            tags.insert(TypeTag(TypeTagInner::ClassInstanceTag));
            tags.insert(TypeTag(TypeTagInner::ObjTag));
            tags.insert(TypeTag(TypeTagInner::ArrTag));
            tags.insert(TypeTag(TypeTagInner::SymbolTag));
            tags.insert(TypeTag(TypeTagInner::FunTag));
        }
    }
    Some(tags)
}

pub fn tag_of_t<'cx>(cx: &mut Context<'cx>, t: &Type) -> Option<TypeTagSet> {
    match t.deref() {
        TypeInner::DefT(_, d) => tag_of_def_t(cx, d.deref()),
        TypeInner::ThisInstanceT(box ThisInstanceTData { instance, .. }) => {
            tag_of_inst(&instance.inst)
        }
        TypeInner::OpenT(_)
        | TypeInner::ImplicitInstantiationTvar(_)
        | TypeInner::AnnotT(_, _, _) => cx
            .find_resolved(t)
            .and_then(|resolved| tag_of_t(cx, &resolved)),
        TypeInner::NominalT {
            reason,
            nominal_type,
        } => match &nominal_type.underlying_t {
            nominal::UnderlyingT::OpaqueWithLocal { t: inner }
                if reason.loc().source() == reason.def_loc().source() =>
            {
                tag_of_t(cx, inner)
            }
            nominal::UnderlyingT::CustomError(box nominal::CustomErrorData {
                t: inner, ..
            }) => tag_of_t(cx, inner),
            _ => match &nominal_type.upper_t {
                Some(upper) => tag_of_t(cx, upper),
                None => None,
            },
        },
        // Most of the types below should have boiled away thanks to concretization.
        TypeInner::NamespaceT(ns) => tag_of_t(cx, &ns.values_type),
        TypeInner::TemplateLiteralT { .. } | TypeInner::StringMappingT { .. } => Some(
            BTreeSet::from([type_tag::TypeTag(type_tag::TypeTagInner::StringTag)]),
        ),
        TypeInner::EvalT { .. }
        | TypeInner::GenericT(..)
        | TypeInner::ThisTypeAppT(..)
        | TypeInner::TypeAppT(..)
        | TypeInner::FunProtoT(_)
        | TypeInner::ObjProtoT(_)
        | TypeInner::NullProtoT(_)
        | TypeInner::IntersectionT(_, _)
        | TypeInner::UnionT(_, _)
        | TypeInner::MaybeT(_, _)
        | TypeInner::OptionalT { .. }
        | TypeInner::KeysT(_, _)
        | TypeInner::AnyT(_, _) => None,
    }
}

pub fn tags_overlap(t1s: &TypeTagSet, t2s: &TypeTagSet) -> bool {
    t1s.intersection(t2s).next().is_some()
}
