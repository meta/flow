/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Checks that two concrete types share no value, used by type guard
//! refinement in `predicate_kit` and by the normalization of scalar
//! intersections.

use std::collections::BTreeSet;
use std::ops::Deref;

use flow_typing_context::Context;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::InstType;
use flow_typing_type::type_::InstanceKind;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::ThisInstanceTData;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::nominal;

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

fn tags_overlap(t1s: &TypeTagSet, t2s: &TypeTagSet) -> bool {
    t1s.intersection(t2s).next().is_some()
}

pub fn tags_differ<'cx>(cx: &mut Context<'cx>, t1: &Type, t2: &Type) -> bool {
    match (tag_of_t(cx, t1), tag_of_t(cx, t2)) {
        (Some(ref t1_tags), Some(ref t2_tags)) => !tags_overlap(t1_tags, t2_tags),
        _ => false,
    }
}

// Singleton primitive types are not captured in the tag overlap checks. We are
// handling a few cases here explicitly.
pub fn ground_types_differ(t1: &Type, t2: &Type) -> bool {
    match (t1.deref(), t2.deref()) {
        (TypeInner::DefT(_, d1), TypeInner::DefT(_, d2)) => match (d1.deref(), d2.deref()) {
            (
                DefTInner::SingletonStrT { value: v1, .. },
                DefTInner::SingletonStrT { value: v2, .. },
            ) => v1 != v2,
            (
                DefTInner::SingletonNumT { value: v1, .. },
                DefTInner::SingletonNumT { value: v2, .. },
            ) => v1 != v2,
            (
                DefTInner::SingletonBigIntT { value: v1, .. },
                DefTInner::SingletonBigIntT { value: v2, .. },
            ) => v1.0 != v2.0,
            (
                DefTInner::SingletonBoolT { value: v1, .. },
                DefTInner::SingletonBoolT { value: v2, .. },
            ) => v1 != v2,
            _ => false,
        },
        _ => false,
    }
}
