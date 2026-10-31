/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::ops::Deref;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_aloc::ALoc;
use flow_aloc::ALocId;
use flow_common::polarity::Polarity;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::reason::mk_reason;
use flow_common::subst_name::OpKind;
use flow_typing_context::Context;
use flow_typing_errors::error_message::EInvalidExactObjectIntersectionData;
use flow_typing_flow_common::concrete_type_eq;
use flow_typing_flow_common::obj_type;
use flow_typing_generics::spread_empty;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::InstType;
use flow_typing_type::type_::InstanceKind;
use flow_typing_type::type_::Literal;
use flow_typing_type::type_::ObjKind;
use flow_typing_type::type_::PropertyInner;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::object;
use flow_typing_type::type_::properties;
use flow_typing_type::type_::property;
use flow_typing_type::type_::type_or_type_desc::TypeOrTypeDescT;
use flow_typing_type::type_util::reason_of_t;
use flow_typing_type::type_util::ref_loc_of_t;

use super::Normalization;
use super::Normalizer;
use super::make_recursive_eval;
use crate::slice_utils;
use crate::slice_utils::SliceIntersection;

/// `Some(is_exact)` for a member that can be merged. Callable and constructible
/// members are not: an intersection of signatures is an overload, which a
/// spread-style merge would drop. Instantiated generic interfaces are left
/// alone, so they keep their type arguments.
fn supported_object(t: &Type) -> Option<bool> {
    match t.deref() {
        TypeInner::DefT(_, def_t) => match def_t.deref() {
            DefTInner::ObjT(obj) if obj.call_t.is_none() => Some(
                matches!(obj.flags.obj_kind, ObjKind::Exact)
                    && !obj.strictness_kind.is_typescript_loose(),
            ),
            DefTInner::InstanceT(instance)
                if matches!(instance.inst.inst_kind, InstanceKind::InterfaceKind { .. })
                    && instance.inst.type_args.is_empty()
                    && instance.inst.inst_call_t.is_none()
                    && instance.inst.inst_construct_t.is_none() =>
            {
                Some(false)
            }
            _ => None,
        },
        _ => None,
    }
}

/// Own props shadow proto props, as in property lookup on an instance.
fn interface_props<'cx>(cx: &mut Context<'cx>, instance: &InstType) -> properties::Id {
    let mut props = cx.find_props(instance.own_props.dupe());
    for (name, prop) in cx.find_props(instance.proto_props.dupe()).iter() {
        if !props.contains_key(name) {
            props.insert(name.dupe(), prop.dupe());
        }
    }
    cx.generate_property_map(props)
}

/// The merge combines the polarities of a shared prop with `Polarity::mult`,
/// which only agrees with intersection for read-only and neutral props: two
/// write-only props would multiply into a read-only one. It also keeps one type
/// per prop, so a getter/setter pair would lose its setter type.
fn supported_props(props: &properties::PropertiesMap) -> bool {
    props.iter().all(|(_, prop)| {
        property::polarity(prop) != Polarity::Negative
            && !matches!(prop.deref(), PropertyInner::GetSet(_))
    })
}

/// Turns members into the object slices that get merged. An interface
/// contributes a slice for itself and for each of its supers, so inherited
/// props take part in the merge; `seen_interfaces` visits each interface once,
/// which stops cyclic `extends` and skips repeated diamond ancestors.
struct Collector<F> {
    concretize: F,
    seen_interfaces: BTreeSet<(ALocId, properties::Id)>,
}

impl<F> Collector<F> {
    /// `allow_proto` accepts the `ObjProtoT`/`NullProtoT` that ends a chain of
    /// interface supers; it adds no props.
    fn collect<'cx, E>(
        &mut self,
        cx: &mut Context<'cx>,
        t: &Type,
        allow_proto: bool,
    ) -> Result<Option<Vec<object::Slice>>, E>
    where
        F: FnMut(&mut Context<'cx>, &Reason, &Type) -> Result<Vec<Type>, E>,
    {
        match t.deref() {
            TypeInner::DefT(r, def_t) => match def_t.deref() {
                DefTInner::ObjT(obj) if obj.call_t.is_none() => {
                    if !supported_props(&cx.find_props(obj.props_tmap.dupe())) {
                        return Ok(None);
                    }
                    Ok(Some(vec![slice_utils::object_slice(
                        cx,
                        None,
                        r,
                        obj.props_tmap.dupe(),
                        &obj.flags,
                        None,
                        false,
                        obj.reachable_targs.dupe(),
                        spread_empty(),
                        obj.strictness_kind,
                        false,
                    )]))
                }
                DefTInner::InstanceT(instance) if supported_object(t).is_some() => {
                    if !self.seen_interfaces.insert((
                        instance.inst.class_id.dupe(),
                        instance.inst.own_props.dupe(),
                    )) {
                        return Ok(Some(Vec::new()));
                    }
                    let props = interface_props(cx, &instance.inst);
                    if !supported_props(&cx.find_props(props.dupe())) {
                        return Ok(None);
                    }
                    let mut slices = vec![slice_utils::interface_slice(
                        cx,
                        r,
                        &instance.static_,
                        instance.inst.clone(),
                        props,
                        false,
                        None,
                        spread_empty(),
                    )];
                    let concrete_supers =
                        (self.concretize)(cx, reason_of_t(&instance.super_), &instance.super_)?;
                    if concrete_supers.is_empty() {
                        return Ok(None);
                    }
                    for super_t in flatten(concrete_supers) {
                        let Some(mut super_slices) = self.collect(cx, &super_t, true)? else {
                            return Ok(None);
                        };
                        slices.append(&mut super_slices);
                    }
                    Ok(Some(slices))
                }
                _ => Ok(None),
            },
            TypeInner::IntersectionT(_, rep) => {
                let mut slices = Vec::new();
                for member in rep.members_iter() {
                    let concrete = (self.concretize)(cx, reason_of_t(member), member)?;
                    let [concrete] = concrete.as_slice() else {
                        return Ok(None);
                    };
                    let Some(mut member_slices) = self.collect(cx, concrete, allow_proto)? else {
                        return Ok(None);
                    };
                    slices.append(&mut member_slices);
                }
                Ok(Some(slices))
            }
            TypeInner::ObjProtoT(_) | TypeInner::NullProtoT(_) if allow_proto => {
                Ok(Some(Vec::new()))
            }
            _ => Ok(None),
        }
    }
}

fn flatten(members: Vec<Type>) -> Vec<Type> {
    let mut todo: VecDeque<Type> = members.into();
    let mut result = Vec::new();
    while let Some(t) = todo.pop_front() {
        match t.deref() {
            TypeInner::IntersectionT(_, rep) => {
                let nested: Vec<Type> = rep.members_iter().duped().collect();
                for member in nested.into_iter().rev() {
                    todo.push_front(member);
                }
            }
            _ => result.push(t),
        }
    }
    result
}

/// The merge reads a member's indexer for every named prop the other members
/// have, whatever its key, so a `number`-keyed indexer would wrongly intersect
/// a named prop with its value type; only a `string` key covers every name. One
/// merged indexer cannot describe two key types, so all keys must agree.
/// Write-only indexers are rejected for the same reason as write-only props.
fn supported_dicts<'cx>(
    cx: &mut Context<'cx>,
    normalizer: &Normalizer<'_>,
    slices: &[object::Slice],
) -> bool {
    let mut dicts = slices
        .iter()
        .filter_map(|slice| obj_type::get_dict_opt(&slice.flags.obj_kind));
    let has_named_props = slices.iter().any(|slice| !slice.props.is_empty());
    if dicts.clone().any(|dict| {
        dict.dict_polarity == Polarity::Negative
            || (has_named_props
                && !matches!(
                    dict.key.deref(),
                    TypeInner::DefT(_, def_t)
                        if matches!(def_t.deref(), DefTInner::StrGeneralT(Literal::AnyLiteral))
                ))
    }) {
        return false;
    }
    let Some(first) = dicts.next() else {
        return true;
    };
    dicts.all(|dict| concrete_type_eq::eq_with_env(cx, normalizer.env, &first.key, &dict.key))
}

/// Unlike object kit, whose result is a fresh writable copy, normalization
/// keeps indexer polarity and normalizes the intersection of shared types.
impl SliceIntersection for Normalizer<'_> {
    fn intersect(&self, cx: &mut Context<'_>, left: Type, right: Type) -> Type {
        if concrete_type_eq::eq_with_env(cx, self.env, &left, &right) {
            left
        } else {
            make_recursive_eval(cx, self.reason, left, right)
        }
    }

    fn dict_polarity(&self, polarity: Polarity) -> Polarity {
        polarity
    }
}

/// Exact objects forbid the properties they do not declare, so a required
/// property that an exact member lacks makes the intersection impossible.
/// Optional properties may be absent, so they are allowed.
fn invalid_exact_prop(
    normalizer: &Normalizer<'_>,
    exact_objects: &[(&Type, properties::PropertiesMap)],
    slices: &[object::Slice],
) -> Option<Box<EInvalidExactObjectIntersectionData<ALoc>>> {
    let forbidding = |name: &Name| {
        exact_objects
            .iter()
            .find(|(_, props)| !props.contains_key(name))
            .map(|(exact_object, _)| *exact_object)
    };
    slices
        .iter()
        .flat_map(|slice| slice.props.iter())
        .filter(|(_, prop)| !matches!(property::type_(prop).deref(), TypeInner::OptionalT { .. }))
        .find_map(|(name, _)| {
            let exact_object = forbidding(name)?;
            Some(Box::new(EInvalidExactObjectIntersectionData {
                loc: normalizer.reason.loc().dupe(),
                prop: name.dupe(),
                exact_object_loc: ref_loc_of_t(exact_object).dupe(),
                exact_object_desc: TypeOrTypeDescT::Type(exact_object.dupe()),
            }))
        })
}

/// Members merge like spreads, except that shared properties are intersected.
///
/// As with spreads, the result is exact if any member is, so
/// `{|a: A|} & {|b: B|}` is `{|a: A, b: B|}`. It is an inline interface when
/// every slice comes from an interface, and an object otherwise.
pub(super) fn normalize<'cx, E>(
    cx: &mut Context<'cx>,
    normalizer: &Normalizer<'_>,
    members: &[Type],
    concretize: impl FnMut(&mut Context<'cx>, &Reason, &Type) -> Result<Vec<Type>, E>,
) -> Result<Normalization, E> {
    let Some(exactness) = members
        .iter()
        .map(supported_object)
        .collect::<Option<Vec<_>>>()
    else {
        return Ok(Normalization::Unchanged);
    };
    let mut collector = Collector {
        concretize,
        seen_interfaces: BTreeSet::new(),
    };
    let mut slices = Vec::new();
    let mut exact_objects = Vec::new();
    for (member, is_flow_exact) in members.iter().zip(exactness) {
        let Some(member_slices) = collector.collect(cx, member, false)? else {
            return Ok(Normalization::Unchanged);
        };
        if is_flow_exact {
            exact_objects.extend(
                member_slices
                    .iter()
                    .map(|slice| (member, slice.props.dupe())),
            );
        }
        slices.extend(member_slices);
    }
    if !supported_dicts(cx, normalizer, &slices) {
        return Ok(Normalization::Unchanged);
    }
    // A TypeScript member would make the merged object loose, dropping the
    // exactness checks of the exact members.
    if !exact_objects.is_empty()
        && slices
            .iter()
            .any(|slice| slice.strictness_kind.is_typescript_loose())
    {
        return Ok(Normalization::Unchanged);
    }
    let invalid_exact = invalid_exact_prop(normalizer, &exact_objects, &slices);
    let interface = slices.first().and_then(|first| {
        slices
            .iter()
            .all(|slice| slice.interface.is_some())
            .then(|| first.interface.clone())
            .flatten()
    });
    let mut slices = slices.into_iter();
    let Some(first) = slices.next() else {
        return Ok(Normalization::Unchanged);
    };
    let slice_reason = mk_reason(
        VirtualReasonDesc::RObjectType,
        normalizer.reason.loc().dupe(),
    );
    let mut merged = slices.fold(first, |merged, slice| {
        slice_utils::intersect_slices(cx, normalizer, &slice_reason, &merged, &slice)
    });
    merged.interface = interface;
    let id = cx.generate_property_map(merged.props);
    let proto = Type::new(TypeInner::ObjProtoT(normalizer.reason.dupe()));
    let t = slice_utils::mk_object_type(
        cx,
        &merged.reason,
        false,
        true,
        merged.interface,
        &merged.reachable_targs,
        OpKind::CreateElement,
        &merged.flags,
        None,
        id,
        proto,
        merged.generics,
        merged.strictness_kind,
        false,
    );
    Ok(match invalid_exact {
        Some(invalid) => Normalization::InvalidExact(t, invalid),
        None => Normalization::Normalized(t),
    })
}
