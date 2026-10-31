/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_aloc::ALoc;
use flow_common::name_utils;
use flow_common::polarity::Polarity;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_typing_context::Context;
use flow_typing_errors::error_message::ECannotSpreadIndexerOnRightData;
use flow_typing_errors::error_message::ECannotSpreadInterfaceData;
use flow_typing_errors::error_message::EDuplicateComponentPropData;
use flow_typing_errors::error_message::EExponentialSpreadData;
use flow_typing_errors::error_message::EInexactMayOverwriteIndexerData;
use flow_typing_errors::error_message::EInvalidObjectKitData;
use flow_typing_errors::error_message::EPropNotFoundInSubtypingData;
use flow_typing_errors::error_message::EUnableToSpreadData;
use flow_typing_errors::error_message::EUnsupportedExactData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::RecordErrorKind;
use flow_typing_errors::intermediate_error_types;
use flow_typing_errors::intermediate_error_types::ExponentialSpreadOperandGroup;
use flow_typing_errors::intermediate_error_types::LowerRequirement;
use flow_typing_errors::intermediate_error_types::RecordBannedTypeUtilOp;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_common::flow_js_utils::SpeculativeError;
use flow_typing_flow_common::obj_type;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_spread_cache::LowerBound;
use flow_typing_spread_cache::LowerBoundGroup;
use flow_typing_type::type_::AnySource;
use flow_typing_type::type_::ArrType;
use flow_typing_type::type_::ArrayATData;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::Destructor;
use flow_typing_type::type_::DictType;
use flow_typing_type::type_::FieldData;
use flow_typing_type::type_::Flags;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::InstType;
use flow_typing_type::type_::InstTypeInner;
use flow_typing_type::type_::InstanceKind;
use flow_typing_type::type_::InstanceT;
use flow_typing_type::type_::InstanceTInner;
use flow_typing_type::type_::MappedTypeFlags;
use flow_typing_type::type_::MappedTypeOptionality;
use flow_typing_type::type_::MappedTypeVariance;
use flow_typing_type::type_::MixedFlavor;
use flow_typing_type::type_::NominalType;
use flow_typing_type::type_::NominalTypeInner;
use flow_typing_type::type_::ObjKind;
use flow_typing_type::type_::Property;
use flow_typing_type::type_::PropertyInner;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::ReactDro;
use flow_typing_type::type_::ThisInstanceTData;
use flow_typing_type::type_::TupleATData;
use flow_typing_type::type_::TupleElement;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeDestructorT;
use flow_typing_type::type_::TypeDestructorTInner;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::TypeStrictnessKind;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::VirtualFrameUseOp;
use flow_typing_type::type_::any_t;
use flow_typing_type::type_::empty_t;
use flow_typing_type::type_::eval;
use flow_typing_type::type_::inter_rep;
use flow_typing_type::type_::mk_objecttype_with_strictness;
use flow_typing_type::type_::nominal;
use flow_typing_type::type_::object;
use flow_typing_type::type_::object::ObjectToolObjectMapData;
use flow_typing_type::type_::object::ObjectToolReactConfigData;
use flow_typing_type::type_::properties;
use flow_typing_type::type_::property;
use flow_typing_type::type_::str_module_t;
use flow_typing_type::type_::type_or_type_desc::TypeOrTypeDescT;
use flow_typing_type::type_::union_rep;
use flow_typing_type::type_::unknown_use;
use flow_typing_type::type_::void;
use flow_typing_type::type_util;
use vec1::Vec1;

pub fn mk_object_type<'cx>(
    cx: &mut Context<'cx>,
    reason: &Reason,
    wrap_on_exact_obj: bool,
    invalidate_aliases: bool,
    interface: Option<(Type, InstType)>,
    reachable_targs: &[(Type, Polarity)],
    kind: flow_common::subst_name::OpKind,
    flags: &Flags,
    call: Option<i32>,
    id: properties::Id,
    proto: Type,
    generics: object::GenericSpreadId,
    strictness_kind: TypeStrictnessKind,
    literal: bool,
) -> Type {
    let reason = if invalidate_aliases {
        reason
            .dupe()
            .update_desc(|d| d.map(VirtualReasonDesc::invalidate_rtype_alias))
    } else {
        reason.dupe()
    };
    let t = match interface {
        Some((static_, inst)) => {
            let inst_dict = match &flags.obj_kind {
                ObjKind::Indexed(dict) => Some(dict.clone()),
                _ => None,
            };
            // The slice of an interface already includes its proto props.
            let proto_props = match inst.inst_kind {
                InstanceKind::InterfaceKind { .. } => {
                    cx.generate_property_map(properties::PropertiesMap::new())
                }
                _ => inst.proto_props.dupe(),
            };
            let inst = InstType::new(InstTypeInner {
                own_props: id,
                proto_props,
                inst_dict,
                class_id: flow_aloc::ALocId::none(),
                inst_kind: InstanceKind::InterfaceKind {
                    inline: true,
                    reject_primitives: false,
                },
                ..(*inst).clone()
            });
            let reason = reason.dupe().replace_desc(VirtualReasonDesc::RObjectType);
            // Implemented/super interfaces are folded into the property map computed by the slice,
            // so we effectively flatten the hierarchy in the output
            Type::new(TypeInner::DefT(
                reason.dupe(),
                DefT::new(DefTInner::InstanceT(Rc::new(InstanceT::new(
                    InstanceTInner {
                        inst,
                        static_,
                        super_: Type::new(TypeInner::ObjProtoT(reason)),
                        implements: Rc::from([]),
                    },
                )))),
            ))
        }
        None => {
            let t = Type::new(TypeInner::DefT(
                reason.dupe(),
                DefT::new(DefTInner::ObjT(Rc::new(mk_objecttype_with_strictness(
                    Some(flags.clone()),
                    Some(Rc::from(reachable_targs)),
                    call,
                    id,
                    proto,
                    strictness_kind,
                    literal,
                )))),
            ));
            if wrap_on_exact_obj && obj_type::is_exact(&flags.obj_kind) {
                if flow_js_utils::tvar_visitors::has_unresolved_tvars_or_placeholders(cx, &t) {
                    t
                } else {
                    let r = type_util::reason_of_t(&t);
                    flow_typing_tvar::mk_fully_resolved(cx, r.dupe(), t)
                }
            } else {
                t
            }
        }
    };
    wrap_generics(reason, kind, generics, t)
}

/// Wraps `t`, the result of `kind`, to remember the generics spread into it.
fn wrap_generics(
    reason: Reason,
    kind: flow_common::subst_name::OpKind,
    generics: object::GenericSpreadId,
    t: Type,
) -> Type {
    let Some(id) = flow_typing_generics::GenericId::make_op_id(kind, generics) else {
        return t;
    };
    match kind {
        flow_common::subst_name::OpKind::CreateElement
        | flow_common::subst_name::OpKind::CheckConfig
        | flow_common::subst_name::OpKind::MakeExact
        | flow_common::subst_name::OpKind::ReadOnly
        | flow_common::subst_name::OpKind::ReactConfig
        | flow_common::subst_name::OpKind::Spread => {
            Type::new(TypeInner::GenericT(Box::new(GenericTData {
                bound: t,
                reason,
                id: id.clone(),
                name: id.subst_name(),
                no_infer: false,
            })))
        }
        flow_common::subst_name::OpKind::MappedObject
        | flow_common::subst_name::OpKind::MappedArray
        | flow_common::subst_name::OpKind::Partial
        | flow_common::subst_name::OpKind::Required => {
            let name = id.subst_name();
            Type::new(TypeInner::NominalT {
                reason: reason.dupe(),
                nominal_type: Rc::new(NominalType::new(NominalTypeInner {
                    nominal_id: nominal::Id::StuckEval(
                        nominal::StuckEvalKind::StuckEvalForGenericallyMappedObject(name.dupe()),
                    ),
                    underlying_t: nominal::UnderlyingT::FullyOpaque,
                    lower_t: None,
                    upper_t: Some(t.dupe()),
                    nominal_type_args: vec![(name, reason, t, Polarity::Neutral)].into(),
                })),
            })
        }
    }
}

/// The object type a resolved slice stands for, so errors can print it.
fn object_type_of_slice<'cx>(cx: &mut Context<'cx>, slice: &object::Slice) -> Type {
    let object::Slice {
        reason,
        strictness_kind,
        props,
        flags,
        frozen: _,
        generics,
        interface,
        reachable_targs,
        literal,
        exact_object_type: _,
    } = slice;
    let id = cx.generate_property_map(props.clone());
    mk_object_type(
        cx,
        reason,
        false,
        false,
        interface.clone(),
        reachable_targs,
        flow_common::subst_name::OpKind::Spread,
        flags,
        None,
        id,
        Type::new(TypeInner::ObjProtoT(reason.dupe())),
        generics.clone(),
        *strictness_kind,
        *literal,
    )
}

pub fn type_optionality(prop: &Property) -> (Type, bool) {
    let prop_t = property::type_(prop);
    match prop_t.deref() {
        TypeInner::OptionalT { type_: t, .. } => (t.dupe(), true),
        _ => (prop_t.dupe(), false),
    }
}

pub fn merge_dro(a: Option<ReactDro>, b: Option<ReactDro>) -> Option<ReactDro> {
    match (a, b) {
        (Some(x), Some(_)) => Some(x),
        _ => None,
    }
}

// Shared Object Kit Utilities

pub fn mk_slice_prop(
    key_loc: Option<ALoc>,
    type_: Type,
    polarity: Polarity,
    is_method: bool,
) -> Property {
    if is_method {
        Property::new(PropertyInner::Method { key_loc, type_ })
    } else {
        Property::new(PropertyInner::Field(Box::new(FieldData {
            preferred_def_locs: None,
            key_loc,
            type_,
            polarity,
        })))
    }
}

pub fn read_prop(r: &Reason, _flags: &Flags, _x: &Name, p: &Property) -> Property {
    // Extract the read-side type (Mixed if absent — write-only Field, Set-only GetSet)
    // and project into a slice-shape Field/Method.
    match p.deref() {
        PropertyInner::Field(fd) => {
            let is_readable_field = fd.preferred_def_locs.is_none()
                && Polarity::compat(fd.polarity, Polarity::Positive);
            if is_readable_field {
                return p.dupe();
            }
        }
        PropertyInner::Method { .. } => return p.dupe(),
        _ => {}
    }
    let t = property::read_t(p).unwrap_or_else(|| {
        let reason = r.dupe().replace_desc(None);
        Type::new(TypeInner::DefT(
            reason,
            DefT::new(DefTInner::MixedT(MixedFlavor::MixedEverything)),
        ))
    });
    mk_slice_prop(
        property::first_loc(p),
        t,
        property::polarity(p),
        property::is_method(p),
    )
}

pub fn read_dict(r: &Reason, dict: &DictType) -> Type {
    if Polarity::compat(dict.dict_polarity, Polarity::Positive) {
        dict.value.dupe()
    } else {
        let reason = r.dupe().replace_desc(None);
        Type::new(TypeInner::DefT(
            reason,
            DefT::new(DefTInner::MixedT(MixedFlavor::MixedEverything)),
        ))
    }
}

fn object_tool_dict_polarity_override(tool: &object::Tool) -> Option<Polarity> {
    match tool {
        object::Tool::MakeExact
        | object::Tool::ReadOnly
        | object::Tool::Partial
        | object::Tool::Required
        | object::Tool::Rest(box (object::rest::MergeMode::Omit, _))
        | object::Tool::ObjectRep
        | object::Tool::ObjectMap(_) => None,
        object::Tool::Spread(_)
        | object::Tool::Rest(box (
            object::rest::MergeMode::SpreadReversal | object::rest::MergeMode::ReactConfigMerge(_),
            _,
        ))
        | object::Tool::ReactConfig(_)
        | object::Tool::ReactCheckComponentConfig { .. } => Some(Polarity::Neutral),
    }
}

/// Spreading a record is allowed; every other object type operation on a record is banned.
fn record_banned_type_util_op(tool: &object::Tool) -> Option<RecordBannedTypeUtilOp> {
    match tool {
        object::Tool::Spread(_) => None,
        object::Tool::MakeExact => Some(RecordBannedTypeUtilOp::Exact),
        object::Tool::ReadOnly => Some(RecordBannedTypeUtilOp::ReadOnly),
        object::Tool::Partial => Some(RecordBannedTypeUtilOp::Partial),
        object::Tool::Required => Some(RecordBannedTypeUtilOp::Required),
        object::Tool::Rest(box (object::rest::MergeMode::Omit, _)) => {
            Some(RecordBannedTypeUtilOp::Omit)
        }
        object::Tool::Rest(box (object::rest::MergeMode::SpreadReversal, _)) => {
            Some(RecordBannedTypeUtilOp::Rest)
        }
        object::Tool::Rest(box (object::rest::MergeMode::ReactConfigMerge(_), _))
        | object::Tool::ReactConfig(_)
        | object::Tool::ReactCheckComponentConfig { .. } => {
            Some(RecordBannedTypeUtilOp::ReactConfig)
        }
        object::Tool::ObjectRep => Some(RecordBannedTypeUtilOp::Object),
        object::Tool::ObjectMap(_) => Some(RecordBannedTypeUtilOp::MappedType),
    }
}

pub fn object_slice<'cx>(
    cx: &mut Context<'cx>,
    interface: Option<(Type, InstType)>,
    r: &Reason,
    id: properties::Id,
    flags: &Flags,
    dict_polarity_override: Option<Polarity>,
    frozen: bool,
    reachable_targs: Rc<[(Type, Polarity)]>,
    generics: object::GenericSpreadId,
    strictness_kind: TypeStrictnessKind,
    literal: bool,
) -> object::Slice {
    let props_map = cx.find_props(id);
    let props: properties::PropertiesMap = props_map
        .iter()
        .map(|(name, prop)| (name.dupe(), read_prop(r, flags, name, prop)))
        .collect();
    let obj_kind = obj_type::map_dict(
        |dict| DictType {
            dict_name: None,
            key: dict.key.dupe(),
            value: read_dict(r, &dict),
            dict_polarity: dict_polarity_override.unwrap_or(dict.dict_polarity),
        },
        flags.obj_kind.clone(),
    );
    let flags = Flags {
        obj_kind,
        ..flags.clone()
    };
    object::Slice {
        reason: r.dupe(),
        strictness_kind,
        props,
        flags,
        frozen,
        generics,
        interface,
        reachable_targs,
        literal,
        exact_object_type: false,
    }
}

// Treat dictionaries as optional, own properties. Dictionary reads should
// be exact. TODO: Forbid writes to indexers through the photo chain.
// Property accesses which read from dictionaries normally result in a
// non-optional result, but that leads to confusing spread results. For
// example, `p` in `{...{|p:T|},...{[]:U}` should `T|U`, not `U`.
pub fn get_prop<'a>(
    r: &Reason,
    p: Option<&'a Property>,
    dict: Option<&DictType>,
) -> Option<Cow<'a, Property>> {
    match (p, dict) {
        (Some(p), _) => Some(Cow::Borrowed(p)),
        (None, Some(d)) => Some(Cow::Owned(mk_slice_prop(
            Some(type_util::loc_of_t(&d.key).dupe()),
            type_util::optional(read_dict(r, d), None, false),
            Polarity::Neutral,
            false,
        ))),
        (None, None) => None,
    }
}

// Lift a pairwise function to a function over a resolved list
pub fn merge<F>(
    mut f: F,
    x0: Vec1<object::Slice>,
    rest: (Vec1<object::Slice>, Vec<Vec1<object::Slice>>),
) -> Vec1<object::Slice>
where
    F: FnMut(&object::Slice, &object::Slice) -> object::Slice,
{
    let mut f_prime = |x0: Vec1<object::Slice>, x1: &Vec1<object::Slice>| -> Vec1<object::Slice> {
        let mut result = Vec::new();
        for slice1 in x1.iter() {
            for slice0 in x0.iter() {
                result.push(f(slice1, slice0));
            }
        }
        Vec1::try_from_vec(result).unwrap()
    };
    let (x1, xs) = rest;
    let mut acc = f_prime(x0, &x1);
    for xi in &xs {
        acc = f_prime(acc, xi);
    }
    acc
}

// Lift a pairwise function that may return an error to a function over a resolved list
// that may return an error, like spread2
pub fn merge_result<A: Clone, B: Clone, F, C>(
    mut f: F,
    conv: C,
    x0: B,
    rest: (B, flow_data_structure_wrapper::list::FlowOcamlList<B>),
) -> Result<Vec1<A>, FlowJsException>
where
    F: FnMut(&A, &A) -> Result<A, FlowJsException>,
    C: Fn(B) -> Vec1<A>,
{
    let mut f_prime = |x0: Vec1<A>, x1: B| -> Result<Vec1<A>, FlowJsException> {
        let x1 = conv(x1);
        let mut all: Vec<A> = Vec::new();
        for slice1 in x1.iter() {
            for x in x0.iter() {
                let slice = f(x, slice1)?;
                all.push(slice);
            }
        }
        // Safe: all inputs non-empty so result is non-empty
        Ok(Vec1::try_from_vec(all).unwrap())
    };
    let (x1, xs) = rest;
    let mut acc = f_prime(conv(x0), x1)?;
    for xi in xs.iter() {
        acc = f_prime(acc, xi.clone())?;
    }
    Ok(acc)
}

fn spread2<'cx>(
    dict_check: &dyn Fn(
        &mut Context<'cx>,
        UseOp,
        &DictType,
        &DictType,
    ) -> Result<(), FlowJsException>,
    cx: &mut Context<'cx>,
    use_op: &UseOp,
    reason: &Reason,
    allow_inexact: bool,
    literal: bool,
    (
        _inline1,
        inexact_slice1,
        slice1 @ object::Slice {
            reason: r1,
            strictness_kind: strictness_kind1,
            props: props1,
            flags: flags1,
            frozen: frozen1,
            generics: generics1,
            interface: _,
            reachable_targs: targs1,
            literal: _,
            exact_object_type: exact_object_type1,
        },
    ): &(bool, Option<object::Slice>, object::Slice),
    (
        inline2,
        _inexact_slice2,
        slice2 @ object::Slice {
            reason: r2,
            strictness_kind: strictness_kind2,
            props: props2,
            flags: flags2,
            frozen: frozen2,
            generics: generics2,
            interface: _,
            reachable_targs: targs2,
            literal: _,
            exact_object_type: exact_object_type2,
        },
    ): &(bool, Option<object::Slice>, object::Slice),
) -> Result<(bool, Option<object::Slice>, object::Slice), FlowJsException> {
    let exact1 = obj_type::is_exact(&flags1.obj_kind);
    let exact2 = obj_type::is_exact(&flags2.obj_kind);
    let allow_inexact1 = allow_inexact || strictness_kind1.is_typescript_loose();
    let allow_inexact2 = allow_inexact || strictness_kind2.is_typescript_loose();
    let dict1 = obj_type::get_dict_opt(&flags1.obj_kind);
    let dict2 = obj_type::get_dict_opt(&flags2.obj_kind);
    let dict = match (dict1, dict2) {
        (None, Some(d2)) if *inline2 => Ok(Some(d2.clone())),
        (None, Some(d2)) if props1.is_empty() && exact1 => Ok(Some(d2.clone())),
        (Some(d1), Some(d2)) if props1.is_empty() => {
            dict_check(cx, use_op.dupe(), d1, d2)?;
            // We take dict1 because we want to use the key from d1
            Ok(Some(d1.clone()))
        }
        (_, Some(d2)) => Err(Box::new(ErrorMessage::ECannotSpreadIndexerOnRight(
            Box::new(ECannotSpreadIndexerOnRightData {
                spread_loc: reason.loc().dupe(),
                object: flow_js_utils::type_reference_with_reason_for_error(
                    &object_type_of_slice(cx, slice2),
                    r2.dupe(),
                ),
                key_loc: type_util::ref_loc_of_t(&d2.key).dupe(),
                key_desc: flow_js_utils::type_or_type_desc_for_error(&d2.key),
                use_op: use_op.dupe(),
            }),
        ))),
        (Some(d1), _) if !(exact2 || *inline2 || allow_inexact) => Err(Box::new(
            ErrorMessage::EInexactMayOverwriteIndexer(Box::new(EInexactMayOverwriteIndexerData {
                spread_loc: reason.loc().dupe(),
                key_loc: type_util::ref_loc_of_t(&d1.key).dupe(),
                key_desc: flow_js_utils::type_or_type_desc_for_error(&d1.key),
                value_loc: type_util::ref_loc_of_t(&d1.value).dupe(),
                value_desc: flow_js_utils::type_or_type_desc_for_error(&d1.value),
                object2: flow_js_utils::type_reference_with_reason_for_error(
                    &object_type_of_slice(cx, slice2),
                    r2.dupe(),
                ),
                use_op: use_op.dupe(),
            })),
        )),
        (d1, _) => Ok(d1.cloned()),
    };
    let dict = dict.map_err(|e| FlowJsException::Speculative(SpeculativeError(e)))?;
    let union = |t1: Type, t2: Type| -> Type {
        Type::new(TypeInner::UnionT(
            reason.dupe(),
            union_rep::make(
                None,
                union_rep::UnionKind::UnknownKind,
                t1,
                t2,
                Rc::from([]),
            ),
        ))
    };
    let merge_props = |t1: &Property, t2: &Property| -> Property {
        let method1 = property::is_method(t1);
        let method2 = property::is_method(t2);
        let kl2 = property::first_loc(t2);
        let (t1_inner, opt1) = type_optionality(t1);
        let (t2_inner, opt2) = type_optionality(t2);
        if !opt2 {
            mk_slice_prop(kl2, t2_inner, Polarity::Neutral, method2)
        } else if opt1 && opt2 {
            let prop_t = type_util::optional(union(t1_inner, t2_inner), None, false);
            // Since we cannot be sure which is spread, if either
            // are methods we must treat the result as a method
            mk_slice_prop(None, prop_t, Polarity::Neutral, method2 || method1)
        } else {
            // In this case, we know opt2 is true and opt1 is false
            mk_slice_prop(
                None,
                union(t1_inner, t2_inner),
                Polarity::Neutral,
                method1 || method2,
            )
        }
    };
    let props: Result<properties::PropertiesMap, Box<ErrorMessage<ALoc>>> = {
        let mut result: BTreeMap<Name, Property> = BTreeMap::new();
        for (x, p1, p2) in props1.iter_union_rev(props2) {
            let merged = match (p1, p2) {
                (None, None) => None,
                (_, Some(p2)) if *inline2 => Some(p2.dupe()),
                (Some(p1), Some(p2)) => Some(merge_props(p1, p2)),
                (Some(p1), None) => {
                    if exact2 || *inline2 || allow_inexact2 {
                        Some(p1.dupe())
                    } else {
                        return Err(FlowJsException::Speculative(SpeculativeError(Box::new(
                            ErrorMessage::EUnableToSpread(Box::new(EUnableToSpreadData {
                                spread_loc: reason.loc().dupe(),
                                object1: flow_js_utils::type_reference_with_reason_for_error(
                                    &object_type_of_slice(cx, slice1),
                                    r1.dupe(),
                                ),
                                object2: flow_js_utils::type_reference_with_reason_for_error(
                                    &object_type_of_slice(cx, slice2),
                                    r2.dupe(),
                                ),
                                propname: x.dupe(),
                                error_kind:
                                    intermediate_error_types::ExactnessErrorKind::UnexpectedInexact,
                                use_op: use_op.dupe(),
                            })),
                        ))));
                    }
                }
                // For value spreads, an inexact left-hand slice may contain p2 even though
                // it is not explicitly specified. Type spreads use only the declared shape.
                //
                // If p2 is not optional, then we don't have to worry because it definitely
                // overwrites a property with a matching key on the left.
                //
                // For value spreads where p2 is optional, there are three cases:
                //   1. o1 is Flow-inexact: error, we cannot infer a precise type since o1 might contain p2
                //   2. o1 has an indexer: error, we would have to infer a union with the indexer type.
                //      This would be sound, but it's not likely that anyone would intend it. If that
                //      assumption turns out to be false, we can easily add support for it later.
                //   3. o1 is exact: no problem, we don't need to worry about o1 having the
                //      same property.
                (None, Some(p2)) => {
                    let (_, opt2) = type_optionality(p2);
                    match (&flags1.obj_kind, opt2) {
                        (kind @ (ObjKind::Indexed(_) | ObjKind::Inexact), true)
                            if !allow_inexact1 || matches!(kind, ObjKind::Indexed(_)) =>
                        {
                            let error_kind = if obj_type::get_dict_opt(&flags1.obj_kind).is_some() {
                                intermediate_error_types::ExactnessErrorKind::UnexpectedIndexer
                            } else {
                                intermediate_error_types::ExactnessErrorKind::UnexpectedInexact
                            };
                            let inexact_slice = inexact_slice1.as_ref().unwrap_or(slice1);
                            return Err(FlowJsException::Speculative(SpeculativeError(Box::new(
                                ErrorMessage::EUnableToSpread(Box::new(EUnableToSpreadData {
                                    spread_loc: reason.loc().dupe(),
                                    object1: flow_js_utils::type_reference_with_reason_for_error(
                                        &object_type_of_slice(cx, slice2),
                                        r2.dupe(),
                                    ),
                                    object2: flow_js_utils::type_reference_with_reason_for_error(
                                        &object_type_of_slice(cx, inexact_slice),
                                        inexact_slice.reason.dupe(),
                                    ),
                                    propname: x.dupe(),
                                    error_kind,
                                    use_op: use_op.dupe(),
                                })),
                            ))));
                        }
                        _ => Some(p2.dupe()),
                    }
                }
            };
            if let Some(prop) = merged {
                result.insert(x.dupe(), prop);
            }
        }
        Ok(result.into())
    };
    let obj_kind = match &dict {
        Some(d) => ObjKind::Indexed(d.clone()),
        None => {
            if obj_type::is_exact(&flags1.obj_kind) && obj_type::is_exact(&flags2.obj_kind) {
                ObjKind::Exact
            } else {
                ObjKind::Inexact
            }
        }
    };
    let exact_object_type = matches!(obj_kind, ObjKind::Inexact)
        && (exact1 || *exact_object_type1)
        && (exact2 || *exact_object_type2);
    let flags = Flags {
        obj_kind,
        react_dro: merge_dro(flags1.react_dro.clone(), flags2.react_dro.clone()),
    };
    let frozen = *frozen1 && *frozen2;
    let generics = flow_typing_generics::spread_append(generics1, generics2);
    let mut reachable_targs = targs1.to_vec();
    reachable_targs.extend_from_slice(targs2);
    let inexact_slice = match (exact1, exact2) {
        // If the inexact slice is None, that means we still haven't hit an inexact object yet, so we can
        // take that slice to propagate as the source of the accumulator's inexactness.
        //
        // If it's already `Some`, then the reason the object on the left is inexact
        // is because of an earlier inexact object. We would have already encountered that inexact
        // object on the right in a previous iteration of spread2, so the next case in the
        // match would have already updated the inexact slice to the most recent inexact object.
        // The only exception to this rule is if the first object is inexact, in which case
        // inexact_slice1 is already None anyway.
        (false, true) if inexact_slice1.is_none() => Some(slice1.clone()),
        (_, false) => Some(slice2.clone()),
        _ => inexact_slice1.clone(),
    };
    match props {
        Ok(props) => Ok((
            false,
            inexact_slice,
            object::Slice {
                reason: reason.dupe(),
                strictness_kind: strictness_kind1.join(*strictness_kind2),
                props,
                flags,
                frozen,
                generics,
                interface: None,
                reachable_targs: reachable_targs.into(),
                literal,
                exact_object_type,
            },
        )),
        Err(e) => Err(FlowJsException::Speculative(SpeculativeError(e))),
    }
}

pub fn spread<'cx>(
    dict_check: &dyn Fn(
        &mut Context<'cx>,
        UseOp,
        &DictType,
        &DictType,
    ) -> Result<(), FlowJsException>,
    cx: &mut Context<'cx>,
    use_op: &UseOp,
    reason: &Reason,
    allow_inexact: bool,
    strictness_kind: TypeStrictnessKind,
    literal: bool,
    nel: (
        object::spread::AccElement,
        flow_data_structure_wrapper::list::FlowOcamlList<object::spread::AccElement>,
    ),
) -> Result<Vec1<(bool, Option<object::Slice>, object::Slice)>, FlowJsException> {
    let resolved_of_acc_element =
        |elem: object::spread::AccElement| -> Vec1<(bool, Option<object::Slice>, object::Slice)> {
            match elem {
                object::spread::AccElement::ResolvedSlice(resolved) => {
                    let items: Vec<_> = resolved.0.into_iter().map(|x| (false, None, x)).collect();
                    Vec1::try_from_vec(items).unwrap()
                }
                object::spread::AccElement::InlineSlice(inline) => {
                    let obj_kind = match &inline.dict {
                        Some(d) => ObjKind::Indexed(d.clone()),
                        None => ObjKind::Exact,
                    };
                    let flags = Flags {
                        obj_kind,
                        react_dro: None,
                    };
                    let props: properties::PropertiesMap = inline
                        .prop_map
                        .iter()
                        .map(|(name, prop)| {
                            (name.dupe(), read_prop(&inline.reason, &flags, name, prop))
                        })
                        .collect();
                    Vec1::new((
                        true,
                        None,
                        object::Slice {
                            reason: inline.reason.dupe(),
                            strictness_kind,
                            props,
                            flags,
                            frozen: false,
                            generics: inline.generics.clone(),
                            interface: None,
                            reachable_targs: inline.reachable_targs.dupe(),
                            literal,
                            exact_object_type: false,
                        },
                    ))
                }
            }
        };
    let (x, xs) = nel;
    if xs.is_empty() {
        Ok(resolved_of_acc_element(x))
    } else {
        let x1 = xs.first().expect("non-empty").clone();
        let mut rest = xs.dupe();
        rest.drop_first();
        merge_result(
            |a, b| spread2(dict_check, cx, use_op, reason, allow_inexact, literal, a, b),
            resolved_of_acc_element,
            x,
            (x1, rest),
        )
    }
}

pub fn spread_mk_object<'cx>(
    cx: &mut Context<'cx>,
    reason: &Reason,
    target: &object::spread::Target,
    slice: object::Slice,
) -> Type {
    let object::Slice {
        reason: _,
        strictness_kind,
        props,
        flags,
        frozen: _,
        generics,
        interface: _,
        reachable_targs,
        literal: _,
        exact_object_type,
    } = slice;
    let mk_dro = |t: Type| -> Type {
        match &flags.react_dro {
            Some(react_dro) => Type::new(TypeInner::EvalT {
                type_: t,
                defer_use_t: TypeDestructorT::new(TypeDestructorTInner(
                    unknown_use(),
                    reason.dupe(),
                    Rc::new(Destructor::ReactDRO(Box::new(react_dro.clone()))),
                )),
                id: eval::Id::generate_id(),
            }),
            None => t,
        }
    };
    let (exact, exact_object_type, sealed) = match target {
        object::spread::Target::Annot { make_exact, .. } => {
            // Type spread result is exact if annotated to be exact
            (*make_exact, false, object::spread::SealType::Sealed)
        }
        object::spread::Target::Value { make_seal, .. } => {
            // Value spread result is exact if all inputs are exact, and an `ExactObjectType` if
            // the inputs that are not exact are `ExactObjectType`s
            (
                obj_type::is_exact(&flags.obj_kind),
                exact_object_type,
                *make_seal,
            )
        }
    };
    let as_const = sealed == object::spread::SealType::AsConst;
    let dict = obj_type::get_dict_opt(&flags.obj_kind).map(|d| DictType {
        dict_polarity: Polarity::apply_const(as_const, d.dict_polarity),
        ..d.clone()
    });
    let obj_kind = match (exact, &dict) {
        (_, Some(d)) => ObjKind::Indexed(d.clone()),
        (true, _) => ObjKind::Exact,
        _ => ObjKind::Inexact,
    };
    let frozen_seal = sealed == object::spread::SealType::Frozen;
    let flags = Flags {
        obj_kind,
        react_dro: None,
    };
    let positive_polarity = as_const || frozen_seal;
    let props_map: properties::PropertiesMap = props
        .iter()
        .map(|(k, p)| {
            let new_p = property::map_t_with_polarity(
                p,
                Polarity::object_literal_polarity(positive_polarity),
                |t| mk_dro(t.dupe()),
            );
            (k.dupe(), new_p)
        })
        .collect();
    let id = cx.generate_property_map(props_map);
    let proto = Type::new(TypeInner::ObjProtoT(reason.dupe()));
    let call = None;
    // The generics spread into an `ExactObjectType` wrap it, not its upper bound
    let obj_generics = if exact_object_type {
        flow_typing_generics::spread_empty()
    } else {
        generics.clone()
    };
    let t = mk_object_type(
        cx,
        reason,
        true,
        true,
        None,
        &reachable_targs,
        flow_common::subst_name::OpKind::Spread,
        &flags,
        call,
        id,
        proto,
        obj_generics,
        target.strictness_kind().join(strictness_kind),
        target.literal(),
    );
    if !exact_object_type {
        return t;
    }
    let t = obj_type::mk_exact_object_type_with_upper(reason.dupe(), t);
    wrap_generics(
        reason.dupe(),
        flow_common::subst_name::OpKind::Spread,
        generics,
        t,
    )
}

pub fn object_spread<'cx, A>(
    dict_check: &dyn Fn(
        &mut Context<'cx>,
        UseOp,
        &DictType,
        &DictType,
    ) -> Result<(), FlowJsException>,
    add_output: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        ErrorMessage<ALoc>,
    ) -> Result<(), FlowJsException>,
    return_: &dyn Fn(&mut Context<'cx>, UseOp, Type) -> Result<A, FlowJsException>,
    recurse: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        UseOp,
        &Reason,
        object::ResolveTool,
        object::Tool,
        Type,
    ) -> Result<A, FlowJsException>,
    options: &object::spread::Target,
    state: object::spread::State,
    cx: &mut Context<'cx>,
    use_op: UseOp,
    reason: &Reason,
    x: Vec1<object::Slice>,
) -> Result<A, FlowJsException> {
    object_spread_with_env::<A>(
        dict_check,
        add_output,
        return_,
        recurse,
        options,
        state,
        cx,
        &FlowJsEnv::entry(),
        use_op,
        reason,
        x,
    )
}

pub fn object_spread_with_env<'cx, A>(
    dict_check: &dyn Fn(
        &mut Context<'cx>,
        UseOp,
        &DictType,
        &DictType,
    ) -> Result<(), FlowJsException>,
    add_output: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        ErrorMessage<ALoc>,
    ) -> Result<(), FlowJsException>,
    return_: &dyn Fn(&mut Context<'cx>, UseOp, Type) -> Result<A, FlowJsException>,
    recurse: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        UseOp,
        &Reason,
        object::ResolveTool,
        object::Tool,
        Type,
    ) -> Result<A, FlowJsException>,
    options: &object::spread::Target,
    state: object::spread::State,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: UseOp,
    reason: &Reason,
    x: Vec1<object::Slice>,
) -> Result<A, FlowJsException> {
    let reason = reason
        .dupe()
        .update_desc(|d| d.map(VirtualReasonDesc::invalidate_rtype_alias));
    let todo_rev = state.todo_rev;
    let acc = state.acc;
    let spread_id = state.spread_id;
    let union_t = state.union_t;
    let curr_resolve_idx = state.curr_resolve_idx;
    let resolved = object::spread::AccElement::ResolvedSlice(object::Resolved(x.clone()));

    let mut acc: flow_data_structure_wrapper::list::FlowOcamlList<object::spread::AccElement> = acc;
    let mut resolved = resolved;
    let mut curr_resolve_idx = curr_resolve_idx;
    let mut todo_rev: flow_data_structure_wrapper::list::FlowOcamlList<object::spread::Operand> =
        todo_rev;
    // Before proceeding to the next spread step, we need to ensure that we aren't going to hit
    // exponential blowup due to multiple spread operands having multiple lower bounds. To do
    // that, we increment the amount of lower bounds found at this resolution index by
    // the amount of lower bounds found at this index. If multiple indices have multiple lower
    // bounds, Spread_cache.can_spread will return false and we can error instead of proceeding.
    //
    // Any other latent constraints involving this spread_id will also stop when they hit this
    // logic.
    let prev_can_spread = flow_typing_spread_cache::can_spread(cx.spread_cache(), spread_id);
    if !prev_can_spread {
        return return_(cx, use_op, any_t::error(reason.dupe()));
    }
    let cache = cx.spread_cache_mut();
    match (union_t, &x) {
        (None, x) => {
            for slice in x.iter() {
                flow_typing_spread_cache::add_lower_bound(
                    cache,
                    spread_id,
                    curr_resolve_idx,
                    LowerBound::Slice(slice.clone()),
                    &Vec1::new(slice.clone()),
                );
            }
        }
        (Some(union_t), _) => {
            flow_typing_spread_cache::add_lower_bound(
                cache,
                spread_id,
                curr_resolve_idx,
                LowerBound::Union(union_t),
                &x,
            );
        }
    }
    let can_spread = flow_typing_spread_cache::can_spread(cx.spread_cache(), spread_id);
    if prev_can_spread && !can_spread {
        let (group1, group2) =
            flow_typing_spread_cache::get_error_groups(cx.spread_cache(), spread_id);
        let operand_ref = |cx: &mut Context<'cx>, bound: LowerBound| match bound {
            LowerBound::Union(t) => flow_js_utils::type_reference_with_reason_for_error(
                &t,
                type_util::reason_of_t(&t).dupe(),
            ),
            LowerBound::Slice(slice) => flow_js_utils::type_reference_with_reason_for_error(
                &object_type_of_slice(cx, &slice),
                slice.reason.dupe(),
            ),
        };
        let operand_group =
            |cx: &mut Context<'cx>, group: LowerBoundGroup| ExponentialSpreadOperandGroup {
                first: operand_ref(cx, group.first),
                second: group.second.map(|bound| operand_ref(cx, bound)),
            };
        let operand1 = operand_group(cx, group1);
        let operand2 = operand_group(cx, group2);
        add_output(
            cx,
            env,
            ErrorMessage::EExponentialSpread(Box::new(EExponentialSpreadData {
                spread_loc: reason.loc().dupe(),
                operand1,
                operand2,
            })),
        )?;
        return return_(cx, use_op, any_t::error(reason.dupe()));
    }
    curr_resolve_idx += 1;
    loop {
        let operand = match todo_rev.first() {
            None => break,
            Some(o) => o.clone(),
        };
        todo_rev.drop_first();
        match operand {
            object::spread::Operand::Type(t) => {
                let resolve_tool = object::ResolveTool::Resolve(object::Resolve::Next);
                let mut new_acc = acc;
                new_acc.push_front(resolved);
                let state = object::spread::State {
                    todo_rev,
                    acc: new_acc,
                    spread_id,
                    union_t: None,
                    curr_resolve_idx,
                };
                let tool = object::Tool::Spread(Box::new((options.clone(), state)));
                return recurse(cx, env, use_op.dupe(), &reason, resolve_tool, tool, t);
            }
            object::spread::Operand::Slice(operand_slice) => {
                acc.push_front(resolved);
                resolved = object::spread::AccElement::InlineSlice(operand_slice);
                curr_resolve_idx += 1;
            }
        }
    }

    let t = match spread(
        dict_check,
        cx,
        &use_op,
        &reason,
        matches!(options, object::spread::Target::Annot { .. }),
        options.strictness_kind(),
        options.literal(),
        (resolved, acc),
    ) {
        Ok(result) => {
            let (first, rest) = result.split_off_first();
            match rest.len() {
                0 => spread_mk_object(cx, &reason, options, first.2),
                _ => {
                    let mut rest_iter = rest.into_iter();
                    let second = rest_iter.next().unwrap();
                    let t0 = spread_mk_object(cx, &reason, options, first.2);
                    let t1 = spread_mk_object(cx, &reason, options, second.2);
                    let ts: Rc<[Type]> = rest_iter
                        .map(|(_, _, s)| spread_mk_object(cx, &reason, options, s))
                        .collect();
                    Type::new(TypeInner::UnionT(
                        reason.dupe(),
                        union_rep::make(None, union_rep::UnionKind::UnknownKind, t0, t1, ts),
                    ))
                }
            }
        }
        Err(FlowJsException::Speculative(e)) => {
            add_output(cx, env, *e.0)?;
            any_t::error(reason.dupe())
        }
        Err(other) => return Err(other),
    };
    return_(cx, use_op, t)
}

fn check_config2<'cx>(
    cx: &mut Context<'cx>,
    pmap: &properties::PropertiesMap,
    slice: &object::Slice,
) -> (Type, Vec<(ALoc, Name, ALoc)>) {
    let reason = &slice.reason;
    let props = &slice.props;
    let flags = &slice.flags;
    let generics = slice.generics.clone();
    let reachable_targs = &slice.reachable_targs;
    let dict = obj_type::get_dict_opt(&flags.obj_kind);
    let mut duplicate_props_in_spread: Vec<(ALoc, Name, ALoc)> = vec![];
    // Project any Property into a slice-shape Property (flattening Get/Set/GetSet via
    // read_prop) and then force Positive polarity for Field.
    let property_to_config_property = |x: &Name, p: &Property| -> Property {
        property::with_polarity(&read_prop(reason, flags, x, p), Polarity::Positive)
    };
    let mut merged_props: properties::PropertiesMap = pmap
        .iter()
        .map(|(x, p1)| (x.dupe(), property_to_config_property(x, p1)))
        .collect();
    for (x, p2) in props.iter() {
        match pmap.get(x) {
            Some(p1) => {
                let first = property::first_loc(p1).unwrap_or_else(|| reason.loc().dupe());
                let second = property::first_loc(p2)
                    .unwrap_or_else(|| type_util::reason_of_t(property::type_(p2)).loc().dupe());
                duplicate_props_in_spread.push((first, x.dupe(), second));
            }
            None => {
                merged_props.insert(x.dupe(), property_to_config_property(x, p2));
            }
        }
    }
    let obj_kind = match dict {
        Some(d) => ObjKind::Indexed(d.clone()),
        None => {
            if obj_type::is_exact(&flags.obj_kind) {
                ObjKind::Exact
            } else {
                ObjKind::Inexact
            }
        }
    };
    let flags = Flags {
        obj_kind,
        react_dro: flags.react_dro.clone(),
    };
    let id = cx.generate_property_map(merged_props);
    let proto = Type::new(TypeInner::ObjProtoT(reason.dupe()));
    let call = None;
    let t = mk_object_type(
        cx,
        reason,
        false,
        true,
        None,
        reachable_targs,
        flow_common::subst_name::OpKind::CheckConfig,
        &flags,
        call,
        id,
        proto,
        generics,
        slice.strictness_kind,
        slice.literal,
    );
    (t, duplicate_props_in_spread)
}

pub fn check_component_config<'cx, A>(
    add_output: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        ErrorMessage<ALoc>,
    ) -> Result<(), FlowJsException>,
    return_: &dyn Fn(&mut Context<'cx>, UseOp, Type) -> Result<A, FlowJsException>,
    pmap: &properties::PropertiesMap,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: UseOp,
    reason: &Reason,
    x: Vec1<object::Slice>,
) -> Result<A, FlowJsException> {
    let xs: Vec<Type> = x
        .iter()
        .map(|xelt| {
            let (o, duplicate_props_in_spread) = check_config2(cx, pmap, xelt);
            if !duplicate_props_in_spread.is_empty() {
                let duplicates = Vec1::try_from_vec(duplicate_props_in_spread).unwrap();
                add_output(
                    cx,
                    env,
                    ErrorMessage::EDuplicateComponentProp(Box::new(EDuplicateComponentPropData {
                        spread: xelt.reason.loc().dupe(),
                        duplicates,
                    })),
                )?;
            }
            Ok(o)
        })
        .collect::<Result<_, FlowJsException>>()?;
    let t = match xs.as_slice() {
        [x] => x.dupe(),
        [x0, x1, rest @ ..] => Type::new(TypeInner::UnionT(
            reason.dupe(),
            union_rep::make(
                None,
                union_rep::UnionKind::UnknownKind,
                x0.dupe(),
                x1.dupe(),
                rest.iter().duped().collect::<Rc<[_]>>(),
            ),
        )),
        [] => unreachable!(),
    };
    return_(cx, use_op, t)
}

// Object Rest

pub fn object_rest<'cx, A>(
    add_output: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        ErrorMessage<ALoc>,
    ) -> Result<(), FlowJsException>,
    return_: &dyn Fn(
        &mut Context<'cx>,
        Box<dyn Fn(Polarity) -> UseOp>,
        object::rest::MergeMode,
        Type,
    ) -> Result<A, FlowJsException>,
    recurse: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        UseOp,
        &Reason,
        object::ResolveTool,
        object::Tool,
        Type,
    ) -> Result<A, FlowJsException>,
    subt_check: &dyn Fn(UseOp, &mut Context<'cx>, (&Type, &Type)) -> Result<(), FlowJsException>,
    options: &object::rest::MergeMode,
    state: &object::rest::State,
    cx: &mut Context<'cx>,
    use_op: UseOp,
    reason: &Reason,
    x: Vec1<object::Slice>,
) -> Result<A, FlowJsException> {
    object_rest_with_env::<A>(
        add_output,
        return_,
        recurse,
        subt_check,
        options,
        state,
        cx,
        &FlowJsEnv::entry(),
        use_op,
        reason,
        x,
    )
}

pub fn object_rest_with_env<'cx, A>(
    add_output: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        ErrorMessage<ALoc>,
    ) -> Result<(), FlowJsException>,
    return_: &dyn Fn(
        &mut Context<'cx>,
        Box<dyn Fn(Polarity) -> UseOp>,
        object::rest::MergeMode,
        Type,
    ) -> Result<A, FlowJsException>,
    recurse: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        UseOp,
        &Reason,
        object::ResolveTool,
        object::Tool,
        Type,
    ) -> Result<A, FlowJsException>,
    subt_check: &dyn Fn(UseOp, &mut Context<'cx>, (&Type, &Type)) -> Result<(), FlowJsException>,
    options: &object::rest::MergeMode,
    state: &object::rest::State,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: UseOp,
    reason: &Reason,
    x: Vec1<object::Slice>,
) -> Result<A, FlowJsException> {
    let optional = |t: Type| -> Type {
        match t.deref() {
            TypeInner::OptionalT { .. } => t.dupe(),
            _ => type_util::optional(t, None, false),
        }
    };
    // Subtract the second slice from the first slice and return the difference
    // slice. The runtime implementation of this type operation is:
    //
    // ```js
    //      const result = {};
    //
    //      for (const p in props1) {
    //        if (hasOwnProperty(props1, p)) {
    //          if (!hasOwnProperty(props2, p)) {
    //            result[p] = props1[p];
    //          }
    //        }
    //      }
    // ```
    //
    // The resulting object only has a property if the property is own in props1 and
    // it is not an own property of props2.
    let rest = |cx: &mut Context<'cx>,
                use_op: &UseOp,
                merge_mode: &object::rest::MergeMode,
                slice1 @ object::Slice {
                    reason: r1,
                    strictness_kind,
                    props: props1,
                    flags: flags1,
                    frozen: _,
                    generics: generics1,
                    interface,
                    reachable_targs,
                    literal: literal1,
                    exact_object_type: _,
                }: &object::Slice,
                slice2 @ object::Slice {
                    reason: r2,
                    strictness_kind: _,
                    props: props2,
                    flags: flags2,
                    frozen: _,
                    generics: generics2,
                    interface: _,
                    reachable_targs: _,
                    literal: _,
                    exact_object_type: _,
                }: &object::Slice|
     -> Result<Type, FlowJsException> {
        let dict1 = obj_type::get_dict_opt(&flags1.obj_kind);
        let dict2 = obj_type::get_dict_opt(&flags2.obj_kind);
        let mut props = properties::PropertiesMap::new();
        for (k, p1, p2) in props1.iter_union(props2) {
            let gp1 = get_prop(r1, p1, dict1);
            let gp2 = get_prop(r2, p2, dict2);
            let gp1 = gp1.as_deref();
            let gp2 = gp2.as_deref();
            let exact2 = obj_type::is_exact(&flags2.obj_kind);
            let result: Option<Property> = match (merge_mode, gp1, gp2, exact2) {
                // If neither object has the prop then we don't add a prop to our result here.
                (
                    object::rest::MergeMode::SpreadReversal
                    | object::rest::MergeMode::Omit
                    | object::rest::MergeMode::ReactConfigMerge(_),
                    None,
                    None,
                    _,
                ) => None,
                (
                    object::rest::MergeMode::SpreadReversal
                    | object::rest::MergeMode::ReactConfigMerge(_),
                    Some(p1),
                    None,
                    _,
                ) => Some(property::with_polarity(p1, Polarity::Positive)),
                (object::rest::MergeMode::Omit, Some(p1), None, _) => Some(p1.dupe()),
                (
                    object::rest::MergeMode::SpreadReversal | object::rest::MergeMode::Omit,
                    Some(_),
                    Some(_),
                    _,
                ) => None,
                (object::rest::MergeMode::ReactConfigMerge(_), Some(p1), Some(p2), _) => {
                    let p1_t = property::type_(p1);
                    let p2_t = property::type_(p2);
                    match p2_t.deref() {
                        // React config merging is special. We are trying to solve for C
                        // in the equation (where ... represents spread instead of rest):
                        //
                        //     {...DP, ...C} = P
                        //
                        // Where DP and P are known. Consider this case:
                        //
                        //     {...{p?}, ...C} = {p}
                        //
                        // The solution for C here is {p} instead of {p?} since
                        // {...{p?}, ...{p?}} is {p?} instead of {p}. This is inconsistent with
                        // the behavior of other object rest merge modes implemented in this
                        // pattern match.
                        TypeInner::OptionalT { type_, .. } => {
                            // We only test the subtyping relation of t1 and t2 if both t1 and t2
                            // are optional types. If t1 is required then t2 will always
                            // be overwritten.;
                            if let TypeInner::OptionalT {
                                type_: t1_inner, ..
                            } = p1_t.deref()
                            {
                                subt_check(unknown_use(), cx, (type_, t1_inner))?;
                            }
                            Some(property::with_polarity(p1, Polarity::Neutral))
                        }
                        _ => {
                            // Using our same equation. Consider this case:
                            //
                            //     {...{p}, ...C} = {p}
                            //
                            // The solution for C here is {p?}. An empty object, {}, is not a valid
                            // solution unless that empty object is exact. Even for exact objects,
                            // {|p?|} is the best solution since it accepts more valid
                            // programs then {||}.
                            // The DP type for p must be a subtype of the P type for p.
                            subt_check(unknown_use(), cx, (p2_t, p1_t))?;
                            Some(property::map_t_with_polarity(p1, Polarity::Positive, |t| {
                                optional(t.dupe())
                            }))
                        }
                    }
                }
                (
                    object::rest::MergeMode::SpreadReversal | object::rest::MergeMode::Omit,
                    None,
                    _,
                    _,
                ) => None,
                // Consider this case:
                //
                // `{...{p}, ...C} = {}`
                //
                // For C there will be no prop. However, if the props object is exact
                // then we need to throw an error.
                (object::rest::MergeMode::ReactConfigMerge(_), None, Some(_), _) => {
                    if obj_type::is_exact(&flags1.obj_kind) {
                        let err = ErrorMessage::EPropNotFoundInSubtyping(Box::new(
                            EPropNotFoundInSubtypingData {
                                prop_name: Some(k.dupe()),
                                lower: flow_js_utils::type_reference_with_reason_for_error(
                                    &object_type_of_slice(cx, slice1),
                                    r1.dupe(),
                                ),
                                upper: flow_js_utils::type_reference_with_reason_for_error(
                                    &object_type_of_slice(cx, slice2),
                                    r2.dupe(),
                                ),
                                use_op: unknown_use(),
                                suggestion: None,
                            },
                        ));
                        add_output(cx, env, err)?;
                    }
                    None
                }
            };
            if let Some(prop) = result {
                props.insert(k.dupe(), prop);
            }
        }
        let dict = match (dict1, dict2) {
            (None, None) => None,
            (Some(dict), None) => Some(dict.clone()),
            (None, Some(_)) => None,
            // If our first and second objects have a dictionary then we use our first
            // dictionary, but we make the value optional since any set of keys may have
            // been removed.
            (Some(dict1_v), Some(dict2_v)) => {
                subt_check(use_op.dupe(), cx, (&dict1_v.value, &dict2_v.value))?;
                Some(DictType {
                    dict_name: None,
                    key: dict1_v.key.dupe(),
                    value: optional(dict1_v.value.dupe()),
                    dict_polarity: Polarity::Neutral,
                })
            }
        };
        let obj_kind = match (&flags1.obj_kind, &dict, merge_mode) {
            (ObjKind::Exact, _, _) => ObjKind::Exact,
            (ObjKind::Indexed(_), Some(d), _) => ObjKind::Indexed(d.clone()),
            (ObjKind::Inexact, _, object::rest::MergeMode::Omit) => ObjKind::Exact,
            _ => ObjKind::Inexact,
        };
        let flags = Flags {
            obj_kind,
            react_dro: flags1.react_dro.clone(),
        };
        let generics = flow_typing_generics::spread_subtract(generics1, generics2);
        let id = cx.generate_property_map(props);
        let proto = Type::new(TypeInner::ObjProtoT(r1.dupe()));
        let call = None;
        let (rest_reason, rest_interface, rest_literal) = match merge_mode {
            object::rest::MergeMode::Omit => (reason.dupe(), interface.clone(), false),
            _ => (r1.dupe(), None, *literal1),
        };
        Ok(mk_object_type(
            cx,
            &rest_reason,
            false,
            true,
            rest_interface.clone(),
            // Keep the reachable targs from o1,
            // because we don't know whether all appearences of them were removed
            reachable_targs,
            flow_common::subst_name::OpKind::Spread,
            &flags,
            call,
            id,
            proto,
            generics.clone(),
            *strictness_kind,
            rest_literal,
        ))
    };
    match state {
        object::rest::State::One(t) => {
            let resolve_tool = object::ResolveTool::Resolve(object::Resolve::Next);
            let state = object::rest::State::Done(object::Resolved(x));
            let tool = object::Tool::Rest(Box::new((*options, state)));
            recurse(cx, env, use_op, reason, resolve_tool, tool, t.dupe())
        }
        object::rest::State::Done(base) => {
            let mut xs: Vec<Type> = Vec::new();
            for slice in base.0.iter() {
                for x_slice in x.iter() {
                    xs.push(rest(cx, &use_op, options, slice, x_slice)?);
                }
            }
            let t = match xs.as_slice() {
                [x] => x.dupe(),
                [x0, x1, rest @ ..] => Type::new(TypeInner::UnionT(
                    reason.dupe(),
                    union_rep::make(
                        None,
                        union_rep::UnionKind::UnknownKind,
                        x0.dupe(),
                        x1.dupe(),
                        rest.iter().duped().collect::<Rc<[_]>>(),
                    ),
                )),
                [] => unreachable!(),
            };
            let use_op_clone = use_op.dupe();
            let use_op_fn = Box::new(move |p: Polarity| -> UseOp {
                UseOp::Frame(
                    Arc::new(VirtualFrameUseOp::ReactGetConfig { polarity: p }),
                    Arc::new(use_op_clone.dupe()),
                )
            });
            return_(cx, use_op_fn, *options, t)
        }
    }
}

// Object Make Exact
pub fn object_make_exact<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    x: Vec1<object::Slice>,
) -> Result<Type, FlowJsException> {
    let mk_exact_object = |object::Slice {
                               reason: r,
                               strictness_kind,
                               props,
                               flags,
                               frozen: _,
                               generics,
                               interface,
                               reachable_targs,
                               literal,
                               exact_object_type: _,
                           }: &object::Slice|
     -> Result<Type, FlowJsException> {
        match interface {
            Some((interface_t, _)) => {
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::EUnsupportedExact(Box::new(EUnsupportedExactData {
                        value_loc: r.loc().dupe(),
                        value_desc: TypeOrTypeDescT::Type(interface_t.dupe()),
                    })),
                )?;
                Ok(any_t::error(reason.dupe()))
            }
            None => {
                let props_map = props.dupe();
                // This case analysis aims at recovering a potential type alias associated
                // with an $Exact<> constructor.
                let reason_obj = match reason.desc(false) {
                    Some(VirtualReasonDesc::RTypeAlias(box (n, loc, _))) => {
                        let n = n.dupe();
                        let loc = loc.dupe();
                        r.dupe().update_desc(|d| match &d {
                            Some(VirtualReasonDesc::RTypeAlias(box (_, _, desc))) => {
                                Some(VirtualReasonDesc::RTypeAlias(Box::new((
                                    n.dupe(),
                                    loc.dupe(),
                                    desc.clone(),
                                ))))
                            }
                            _ => Some(VirtualReasonDesc::RTypeAlias(Box::new((
                                n.dupe(),
                                loc.dupe(),
                                d.map(Arc::new),
                            )))),
                        })
                    }
                    _ => {
                        // If [r] is an RTypeAlias, then this alias is no longer valid.
                        r.dupe()
                            .update_desc(|d| d.map(VirtualReasonDesc::invalidate_rtype_alias))
                    }
                };
                let flags = Flags {
                    obj_kind: match &flags.obj_kind {
                        ObjKind::Inexact => ObjKind::Exact,
                        k => k.clone(),
                    },
                    react_dro: flags.react_dro.clone(),
                };
                let call = None;
                let id = cx.generate_property_map(props_map);
                let proto = Type::new(TypeInner::ObjProtoT(reason.dupe()));
                Ok(mk_object_type(
                    cx,
                    &reason_obj,
                    false,
                    true,
                    None,
                    reachable_targs,
                    flow_common::subst_name::OpKind::MakeExact,
                    &flags,
                    call,
                    id,
                    proto,
                    generics.clone(),
                    *strictness_kind,
                    *literal,
                ))
            }
        }
    };
    let mapped: Vec<Type> = x.iter().map(mk_exact_object).collect::<Result<_, _>>()?;
    Ok(match mapped.as_slice() {
        [t] => t.dupe(),
        [t0, t1, rest @ ..] => Type::new(TypeInner::UnionT(
            reason.dupe(),
            union_rep::make(
                None,
                union_rep::UnionKind::UnknownKind,
                t0.dupe(),
                t1.dupe(),
                rest.iter().duped().collect::<Rc<[_]>>(),
            ),
        )),
        _ => unreachable!("Vec1 guarantees non-empty"),
    })
}

// Object Read Only
pub fn object_read_only<'cx>(
    cx: &mut Context<'cx>,
    reason: &Reason,
    x: Vec1<object::Slice>,
) -> Type {
    let polarity = Polarity::Positive;
    let mk_read_only_object = |object::Slice {
                                   reason: r,
                                   strictness_kind,
                                   props,
                                   flags,
                                   frozen: _,
                                   generics,
                                   interface,
                                   reachable_targs,
                                   literal,
                                   exact_object_type: _,
                               }: &object::Slice|
     -> Type {
        let props_map = props.ident_map(|p| property::with_polarity(p, polarity));
        let flags = Flags {
            obj_kind: obj_type::map_dict(
                |mut dict| {
                    dict.dict_polarity = polarity;
                    dict
                },
                flags.obj_kind.clone(),
            ),
            react_dro: flags.react_dro.clone(),
        };
        let call = None;
        let id = cx.generate_property_map(props_map);
        let proto = Type::new(TypeInner::ObjProtoT(r.dupe()));
        mk_object_type(
            cx,
            r,
            false,
            true,
            interface.clone(),
            reachable_targs,
            flow_common::subst_name::OpKind::ReadOnly,
            &flags,
            call,
            id,
            proto,
            generics.clone(),
            *strictness_kind,
            *literal,
        )
    };
    let mapped: Vec<Type> = x.iter().map(mk_read_only_object).collect();
    match mapped.as_slice() {
        [t] => t.dupe(),
        [t0, t1, rest @ ..] => Type::new(TypeInner::UnionT(
            reason.dupe(),
            union_rep::make(
                None,
                union_rep::UnionKind::UnknownKind,
                t0.dupe(),
                t1.dupe(),
                rest.iter().duped().collect::<Rc<[_]>>(),
            ),
        )),
        _ => unreachable!("Vec1 guarantees non-empty"),
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ObjectUpdateOptionalityKind {
    Partial,
    Required,
}

pub fn object_update_optionality<'cx>(
    kind: ObjectUpdateOptionalityKind,
    cx: &mut Context<'cx>,
    reason: &Reason,
    x: Vec1<object::Slice>,
) -> Type {
    let mk_object = |object::Slice {
                         reason: r,
                         strictness_kind,
                         props,
                         flags,
                         frozen,
                         generics,
                         interface,
                         reachable_targs,
                         literal,
                         exact_object_type: _,
                     }: &object::Slice|
     -> Type {
        let polarity_for = |orig: Polarity| {
            if *frozen { Polarity::Positive } else { orig }
        };
        let props_map: properties::PropertiesMap = props
            .iter()
            .map(|(k, p)| {
                // Methods pass through optionality unchanged. Fields get the type wrapped
                // or unwrapped per `kind`, plus the frozen-overridden polarity.
                let new_p = property::map_field_t_with_polarity(
                    p,
                    polarity_for(property::polarity(p)),
                    |t| match (t.deref(), kind) {
                        (TypeInner::OptionalT { .. }, ObjectUpdateOptionalityKind::Partial) => {
                            t.dupe()
                        }
                        (_, ObjectUpdateOptionalityKind::Partial) => {
                            Type::new(TypeInner::OptionalT {
                                reason: type_util::reason_of_t(t).dupe(),
                                type_: t.dupe(),
                                use_desc: false,
                            })
                        }
                        (
                            TypeInner::OptionalT { type_, .. },
                            ObjectUpdateOptionalityKind::Required,
                        ) => type_.dupe(),
                        (_, ObjectUpdateOptionalityKind::Required) => t.dupe(),
                    },
                );
                (k.dupe(), new_p)
            })
            .collect();
        let call = None;
        let id = cx.generate_property_map(props_map);
        let proto = Type::new(TypeInner::ObjProtoT(reason.dupe()));
        let (reason, literal) = match (reason.desc(false), kind) {
            (Some(VirtualReasonDesc::RPartialOf(_)), ObjectUpdateOptionalityKind::Partial) => {
                (r.dupe(), *literal)
            }
            (Some(VirtualReasonDesc::RRequiredOf(_)), ObjectUpdateOptionalityKind::Required) => {
                (r.dupe(), *literal)
            }
            _ => (reason.dupe(), false),
        };
        let op_kind = match kind {
            ObjectUpdateOptionalityKind::Partial => flow_common::subst_name::OpKind::Partial,
            ObjectUpdateOptionalityKind::Required => flow_common::subst_name::OpKind::Required,
        };
        mk_object_type(
            cx,
            &reason,
            false,
            true,
            interface.clone(),
            reachable_targs,
            op_kind,
            flags,
            call,
            id,
            proto,
            generics.clone(),
            *strictness_kind,
            literal,
        )
    };
    let mapped: Vec<Type> = x.iter().map(mk_object).collect();
    match mapped.as_slice() {
        [t] => t.dupe(),
        [t0, t1, rest @ ..] => Type::new(TypeInner::UnionT(
            reason.dupe(),
            union_rep::make(
                None,
                union_rep::UnionKind::UnknownKind,
                t0.dupe(),
                t1.dupe(),
                rest.iter().duped().collect::<Rc<[_]>>(),
            ),
        )),
        _ => unreachable!("Vec1 guarantees non-empty"),
    }
}

/// How [`intersect_slices`] combines what two slices share.
pub(crate) trait SliceIntersection {
    /// The type of a prop or indexer present in both slices.
    fn intersect(&self, cx: &mut Context<'_>, left: Type, right: Type) -> Type;
    /// The polarity of an indexer, merged or read for a named prop.
    fn dict_polarity(&self, polarity: Polarity) -> Polarity;
}

/// Spreads and rests copy props into a new object, so indexers become neutral
/// and shared types stay plain intersections.
struct ObjectKitIntersection<'a> {
    env: &'a FlowJsEnv,
    reason: &'a Reason,
}

impl SliceIntersection for ObjectKitIntersection<'_> {
    fn intersect(&self, cx: &mut Context<'_>, left: Type, right: Type) -> Type {
        if flow_typing_flow_common::concrete_type_eq::eq_with_env(cx, self.env, &left, &right) {
            left
        } else {
            Type::new(TypeInner::IntersectionT(
                self.reason.dupe(),
                inter_rep::make(left, right, Rc::from([])),
            ))
        }
    }

    fn dict_polarity(&self, _: Polarity) -> Polarity {
        Polarity::Neutral
    }
}

// Intersect two object slices into one slice.
//
// In general it is unsound to combine intersection types, but since object
// kit utilities never write to their arguments, it is safe in this specific
// case.
//
// {...{p:T}&{q:U}} = {...{p:T,q:U}}
// {...{p:T}&{p:U}} = {...{p:T&U}}
// {...A&(B|C)} = {...(A&B)|(A&C)}
// {...(A|B)&C} = {...(A&C)|(B&C)}
pub fn intersect2<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    x1: &object::Slice,
    x2: &object::Slice,
) -> (
    properties::PropertiesMap,
    Flags,
    bool,
    object::GenericSpreadId,
    Rc<[(Type, Polarity)]>,
) {
    intersect_slice_contents(cx, &ObjectKitIntersection { env, reason }, x1, x2)
}

fn intersect_slice_contents<'cx>(
    cx: &mut Context<'cx>,
    policy: &impl SliceIntersection,
    object::Slice {
        reason: r1,
        strictness_kind: _,
        props: props1,
        flags: flags1,
        frozen: frozen1,
        generics: generics1,
        interface: _,
        reachable_targs: targs1,
        literal: _,
        exact_object_type: _,
    }: &object::Slice,
    object::Slice {
        reason: r2,
        strictness_kind: _,
        props: props2,
        flags: flags2,
        frozen: frozen2,
        generics: generics2,
        interface: _,
        reachable_targs: targs2,
        literal: _,
        exact_object_type: _,
    }: &object::Slice,
) -> (
    properties::PropertiesMap,
    Flags,
    bool,
    object::GenericSpreadId,
    Rc<[(Type, Polarity)]>,
) {
    let dict1 = obj_type::get_dict_opt(&flags1.obj_kind);
    let dict2 = obj_type::get_dict_opt(&flags2.obj_kind);
    let mut merge_props = |q1: &Property, q2: &Property| -> Property {
        let t1 = property::type_(q1);
        let t2 = property::type_(q2);
        let m1 = property::is_method(q1);
        let m2 = property::is_method(q2);
        let p1 = property::polarity(q1);
        let p2 = property::polarity(q2);
        let key_loc = property::first_loc(q1);
        let (t1_inner, t2_inner, opt) = match (t1.deref(), t2.deref()) {
            (
                TypeInner::OptionalT { type_: t1_opt, .. },
                TypeInner::OptionalT { type_: t2_opt, .. },
            ) => (t1_opt.dupe(), t2_opt.dupe(), true),
            (TypeInner::OptionalT { type_: t1_opt, .. }, _) => (t1_opt.dupe(), t2.dupe(), false),
            (_, TypeInner::OptionalT { type_: t2_opt, .. }) => (t1.dupe(), t2_opt.dupe(), false),
            _ => (t1.dupe(), t2.dupe(), false),
        };
        let t = policy.intersect(cx, t1_inner, t2_inner);
        let t = if opt {
            type_util::optional(t, None, false)
        } else {
            t
        };
        mk_slice_prop(key_loc, t, Polarity::mult(p1, p2), m1 || m2)
    };
    let read_dict_prop = |r: &Reason, d: &DictType| -> Property {
        mk_slice_prop(
            Some(type_util::loc_of_t(&d.key).dupe()),
            type_util::optional(read_dict(r, d), None, false),
            policy.dict_polarity(d.dict_polarity),
            false,
        )
    };
    let mut props = properties::PropertiesMap::new();
    for (k, p1, p2) in props1.iter_union(props2) {
        let result = match (p1, p2) {
            (None, None) => None,
            (Some(p1), Some(p2)) => Some(merge_props(p1, p2)),
            (Some(p1), None) => match dict2 {
                Some(d2) => Some(merge_props(p1, &read_dict_prop(r2, d2))),
                None => Some(p1.clone()),
            },
            (None, Some(p2)) => match dict1 {
                Some(d1) => Some(merge_props(&read_dict_prop(r1, d1), p2)),
                None => Some(p2.clone()),
            },
        };
        if let Some(prop) = result {
            props.insert(k.dupe(), prop);
        }
    }
    let dict = match (dict1, dict2) {
        (Some(d1), Some(d2)) => Some(DictType {
            dict_name: None,
            key: policy.intersect(cx, d1.key.dupe(), d2.key.dupe()),
            value: policy.intersect(cx, read_dict(r1, d1), read_dict(r2, d2)),
            dict_polarity: policy.dict_polarity(Polarity::mult(d1.dict_polarity, d2.dict_polarity)),
        }),
        (Some(d), None) | (None, Some(d)) => Some(d.clone()),
        (None, None) => None,
    };
    let obj_kind = obj_type::obj_kind_from_optional_dict(
        dict,
        // TODO(jmbrown): Audit this condition. Should this be a conjunction?
        if obj_type::is_exact(&flags1.obj_kind) || obj_type::is_exact(&flags2.obj_kind) {
            ObjKind::Exact
        } else {
            ObjKind::Inexact
        },
    );
    let flags = Flags {
        obj_kind,
        react_dro: flags1.react_dro.clone().or(flags2.react_dro.clone()),
    };
    let frozen = *frozen1 || *frozen2;
    let generics = flow_typing_generics::spread_append(generics1, generics2);
    let mut reachable_targs = targs1.to_vec();
    reachable_targs.extend_from_slice(targs2);
    (props, flags, frozen, generics, reachable_targs.into())
}

pub fn intersect2_with_reason<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    intersection_loc: ALoc,
    x1: &object::Slice,
    x2: &object::Slice,
) -> object::Slice {
    let slice_reason =
        flow_common::reason::mk_reason(VirtualReasonDesc::RObjectType, intersection_loc);
    intersect_slices(
        cx,
        &ObjectKitIntersection { env, reason },
        &slice_reason,
        x1,
        x2,
    )
}

pub(crate) fn intersect_slices<'cx>(
    cx: &mut Context<'cx>,
    policy: &impl SliceIntersection,
    reason: &Reason,
    x1: &object::Slice,
    x2: &object::Slice,
) -> object::Slice {
    let (props, flags, frozen, generics, reachable_targs) =
        intersect_slice_contents(cx, policy, x1, x2);
    object::Slice {
        reason: reason.dupe(),
        strictness_kind: x1.strictness_kind.join(x2.strictness_kind),
        props,
        flags,
        frozen,
        generics,
        interface: None,
        reachable_targs,
        literal: false,
        exact_object_type: false,
    }
}

pub fn resolved<'cx, A>(
    next: &dyn Fn(&mut Context<'cx>, UseOp, &object::Tool, &Reason, Vec1<object::Slice>) -> A,
    recurse: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        UseOp,
        &Reason,
        object::ResolveTool,
        &object::Tool,
        Type,
    ) -> A,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: UseOp,
    reason: &Reason,
    resolve_tool: object::Resolve,
    tool: &object::Tool,
    x: Vec1<object::Slice>,
) -> A {
    match resolve_tool {
        object::Resolve::Next => next(cx, use_op, tool, reason, x),
        object::Resolve::List0(types, join) => {
            let (t, todo) = types.split_off_first();
            let resolve_tool = object::ResolveTool::Resolve(object::Resolve::List(
                todo.into(),
                Vec1::new(object::Resolved(x)),
                join,
            ));
            recurse(cx, env, use_op, reason, resolve_tool, tool, t)
        }
        object::Resolve::List(todo, done_rev, join) => {
            match &*todo {
                [] => {
                    let x = match join.1 {
                        object::JoinOp::Or => {
                            let mut all_resolved = vec![x];
                            for resolved in done_rev.iter() {
                                all_resolved.push(resolved.0.clone());
                            }
                            // Concatenate all Vec1<Slice> into one Vec1<Slice>
                            let mut items: Vec<object::Slice> = Vec::new();
                            for r in all_resolved {
                                items.extend(r);
                            }
                            Vec1::try_from_vec(items).unwrap()
                        }
                        object::JoinOp::And => {
                            let loc = join.0.dupe();
                            let (first_done, rest_done) = done_rev.split_off_first();
                            merge(
                                |s1, s2| {
                                    intersect2_with_reason(cx, env, reason, loc.dupe(), s1, s2)
                                },
                                x,
                                (first_done.0, rest_done.into_iter().map(|r| r.0).collect()),
                            )
                        }
                    };
                    next(cx, use_op, tool, reason, x)
                }
                [t, rest @ ..] => {
                    let mut new_done_rev = vec![object::Resolved(x)];
                    new_done_rev.extend(done_rev.iter().cloned());
                    let new_done_rev = Vec1::try_from_vec(new_done_rev).unwrap();
                    let resolve_tool = object::ResolveTool::Resolve(object::Resolve::List(
                        rest.iter().duped().collect::<Rc<[_]>>(),
                        new_done_rev,
                        join.clone(),
                    ));
                    recurse(cx, env, use_op, reason, resolve_tool, tool, t.dupe())
                }
            }
        }
    }
}

pub fn interface_slice<'cx>(
    cx: &mut Context<'cx>,
    r: &Reason,
    static_: &Type,
    inst: InstType,
    id: properties::Id,
    include_proto_props: bool,
    dict_polarity_override: Option<Polarity>,
    generics: object::GenericSpreadId,
) -> object::Slice {
    let strictness_kind = inst.strictness_kind;
    let obj_kind = match (&inst.inst_kind, &inst.inst_dict) {
        (_, Some(dict)) => ObjKind::Indexed(dict.clone()),
        (InstanceKind::RecordKind { .. }, _) => ObjKind::Exact,
        _ => ObjKind::Inexact,
    };
    let flags = Flags {
        obj_kind,
        react_dro: None,
    };
    let id = if include_proto_props {
        let proto_props = cx.find_props(inst.proto_props.dupe());
        let own_props = cx.find_props(id);
        let props = proto_props
            .iter()
            .chain(own_props.iter())
            .map(|(name, prop)| (name.dupe(), prop.dupe()))
            .collect();
        cx.generate_property_map(props)
    } else {
        id
    };
    object_slice(
        cx,
        Some((static_.dupe(), inst)),
        r,
        id,
        &flags,
        dict_polarity_override,
        false,
        Rc::from([]),
        generics,
        strictness_kind,
        false,
    )
}

pub fn resolve<'cx, A>(
    add_output: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        ErrorMessage<ALoc>,
    ) -> Result<(), FlowJsException>,
    return_: &dyn Fn(&mut Context<'cx>, UseOp, Type) -> Result<A, FlowJsException>,
    next: &dyn Fn(
        &mut Context<'cx>,
        UseOp,
        &object::Tool,
        &Reason,
        Vec1<object::Slice>,
    ) -> Result<A, FlowJsException>,
    recurse: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        UseOp,
        &Reason,
        object::ResolveTool,
        &object::Tool,
        Type,
    ) -> Result<A, FlowJsException>,
    statics: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        &Reason,
        &Type,
    ) -> Result<Type, FlowJsException>,
    cx: &mut Context<'cx>,
    use_op: UseOp,
    reason: &Reason,
    resolve_tool: object::Resolve,
    tool: &object::Tool,
    t: &Type,
) -> Result<A, FlowJsException> {
    resolve_with_env::<A>(
        add_output,
        return_,
        next,
        recurse,
        statics,
        cx,
        &FlowJsEnv::entry(),
        use_op,
        reason,
        resolve_tool,
        tool,
        t,
    )
}

fn resolve_with_env<'cx, A>(
    add_output: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        ErrorMessage<ALoc>,
    ) -> Result<(), FlowJsException>,
    return_: &dyn Fn(&mut Context<'cx>, UseOp, Type) -> Result<A, FlowJsException>,
    next: &dyn Fn(
        &mut Context<'cx>,
        UseOp,
        &object::Tool,
        &Reason,
        Vec1<object::Slice>,
    ) -> Result<A, FlowJsException>,
    recurse: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        UseOp,
        &Reason,
        object::ResolveTool,
        &object::Tool,
        Type,
    ) -> Result<A, FlowJsException>,
    statics: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        &Reason,
        &Type,
    ) -> Result<Type, FlowJsException>,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: UseOp,
    reason: &Reason,
    resolve_tool: object::Resolve,
    tool: &object::Tool,
    t: &Type,
) -> Result<A, FlowJsException> {
    let dict_polarity_override = object_tool_dict_polarity_override(tool);
    let literal = match tool {
        object::Tool::Spread(box (target, _)) => target.literal(),
        object::Tool::ReactConfig(box ObjectToolReactConfigData { literal, .. }) => *literal,
        _ => false,
    };
    let (t_generic_id, t) = {
        fn loop_generic<'cx>(
            cx: &mut Context<'cx>,
            t: &Type,
            ls: object::GenericSpreadId,
        ) -> (object::GenericSpreadId, Type) {
            match t.deref() {
                TypeInner::GenericT(box GenericTData {
                    id, bound, reason, ..
                }) => loop_generic(
                    cx,
                    &type_util::mod_reason_of_t(&|_| reason.dupe(), bound),
                    flow_typing_generics::spread_append(&id.make_spread(), &ls),
                ),
                TypeInner::ThisInstanceT(box ThisInstanceTData {
                    reason,
                    instance,
                    is_this,
                    subst_name,
                }) => (
                    ls,
                    flow_js_utils::fix_this_instance(
                        cx,
                        reason.dupe(),
                        reason.dupe(),
                        instance,
                        *is_this,
                        subst_name.dupe(),
                    ),
                ),
                _ => (ls, t.dupe()),
            }
        }
        loop_generic(cx, t, flow_typing_generics::spread_empty())
    };
    match t.deref() {
        // An `ExactObjectType` has the props of its inexact upper bound, but is exact.
        TypeInner::NominalT { nominal_type, .. }
            if nominal_type.nominal_id == nominal::Id::ExactObjectType
                && let Some(upper_t) = &nominal_type.upper_t
                && let TypeInner::DefT(r, def_t) = upper_t.deref()
                && let DefTInner::ObjT(obj) = def_t.deref() =>
        {
            let slice = object_slice(
                cx,
                None,
                r,
                obj.props_tmap.dupe(),
                &obj.flags,
                dict_polarity_override,
                false,
                obj.reachable_targs.dupe(),
                t_generic_id,
                obj.strictness_kind,
                obj.literal,
            );
            let x = Vec1::new(object::Slice {
                exact_object_type: true,
                ..slice
            });
            return resolved(
                next,
                recurse,
                cx,
                env,
                use_op,
                reason,
                resolve_tool,
                tool,
                x,
            );
        }
        TypeInner::DefT(r, def_t) => match def_t.deref() {
            // We extract the props from an ObjT.
            DefTInner::ObjT(obj) => {
                let x = Vec1::new(object_slice(
                    cx,
                    None,
                    r,
                    obj.props_tmap.dupe(),
                    &obj.flags,
                    dict_polarity_override,
                    false,
                    obj.reachable_targs.dupe(),
                    t_generic_id,
                    obj.strictness_kind,
                    obj.literal,
                ));
                return resolved(
                    next,
                    recurse,
                    cx,
                    env,
                    use_op,
                    reason,
                    resolve_tool,
                    tool,
                    x,
                );
            }
            // Interface declarations do not model runtime own-ness, so interface spreads include
            // their prototype properties. We also recurse to collect inherited properties.
            DefTInner::InstanceT(inst_t) => {
                let static_ = &inst_t.static_;
                let super_t = &inst_t.super_;
                let inst = &inst_t.inst;
                let own_props = inst.own_props.dupe();
                let inst_kind = &inst.inst_kind;
                let is_type_spread = matches!(
                    tool,
                    object::Tool::Spread(box (object::spread::Target::Annot { .. }, _))
                );
                let is_typescript_interface_value_spread =
                    matches!(
                        tool,
                        object::Tool::Spread(box (object::spread::Target::Value { .. }, _))
                    ) && inst.strictness_kind.is_typescript_loose();
                let is_type_operation = matches!(
                    tool,
                    object::Tool::MakeExact
                        | object::Tool::ReadOnly
                        | object::Tool::Partial
                        | object::Tool::Required
                        | object::Tool::Rest(box (object::rest::MergeMode::Omit, _))
                        | object::Tool::ObjectRep
                        | object::Tool::ObjectMap(_)
                );
                let include_proto_props = matches!(inst_kind, InstanceKind::InterfaceKind { .. })
                    && (is_type_operation
                        || is_type_spread
                        || is_typescript_interface_value_spread);
                let resolve_tool_inner = object::ResolveTool::Super(
                    interface_slice(
                        cx,
                        r,
                        static_,
                        inst.clone(),
                        own_props,
                        include_proto_props,
                        dict_polarity_override,
                        t_generic_id,
                    ),
                    Vec::new(),
                    Rc::new(resolve_tool.clone()),
                );
                return match (tool, inst_kind) {
                    (object::Tool::Spread(box (_, _)), InstanceKind::InterfaceKind { .. })
                        if !(is_type_spread || is_typescript_interface_value_spread) =>
                    {
                        add_output(
                            cx,
                            env,
                            ErrorMessage::ECannotSpreadInterface(Box::new(
                                ECannotSpreadInterfaceData {
                                    spread_loc: reason.loc().dupe(),
                                    interface: flow_js_utils::type_reference_with_reason_for_error(
                                        &t,
                                        r.dupe(),
                                    ),
                                    use_op: use_op.dupe(),
                                },
                            )),
                        )?;
                        return_(cx, use_op, any_t::error(reason.dupe()))
                    }
                    (object::Tool::Spread(box (_, _)), InstanceKind::RecordKind { .. }) => recurse(
                        cx,
                        env,
                        use_op,
                        reason,
                        resolve_tool_inner,
                        tool,
                        super_t.dupe(),
                    ),
                    (_, InstanceKind::RecordKind { .. })
                        if let Some(op) = record_banned_type_util_op(tool) =>
                    {
                        add_output(
                            cx,
                            env,
                            ErrorMessage::ERecordError(RecordErrorKind::RecordBannedTypeUtil {
                                op_loc: reason
                                    .annot_loc()
                                    .unwrap_or_else(|| reason.def_loc())
                                    .dupe(),
                                op,
                                record: flow_js_utils::type_reference_with_reason_for_error(
                                    &t,
                                    r.dupe(),
                                ),
                            }),
                        )?;
                        return_(cx, use_op, any_t::error(reason.dupe()))
                    }
                    _ => recurse(
                        cx,
                        env,
                        use_op,
                        reason,
                        resolve_tool_inner,
                        tool,
                        super_t.dupe(),
                    ),
                };
            }
            // Statics of a class. TODO: This logic is unfortunately duplicated from the
            // top-level pattern matching against class lower bounds to object-like
            // uses. This duplication should be removed.
            DefTInner::ClassT(i) => {
                let statics_t = statics(cx, env, r, i)?;
                return recurse(
                    cx,
                    env,
                    use_op,
                    reason,
                    object::ResolveTool::Resolve(resolve_tool.clone()),
                    tool,
                    statics_t,
                );
            }
            // `null` and `void` should pass through Partial, Required, $Exact,
            // since we would like e.g. Partial<?Foo> to be equivalent to ?Partial<Foo>
            DefTInner::NullT | DefTInner::VoidT
                if matches!(
                    tool,
                    object::Tool::Partial
                        | object::Tool::Required
                        | object::Tool::MakeExact
                        | object::Tool::ObjectMap(box ObjectToolObjectMapData { .. })
                ) =>
            {
                return return_(cx, use_op, t.dupe());
            }
            // Mirroring Object.assign() and {...null} semantics, treat null/void as
            // empty objects.
            DefTInner::NullT | DefTInner::VoidT => {
                let flags = Flags {
                    obj_kind: ObjKind::Exact,
                    react_dro: None,
                };
                let x = Vec1::new(object::Slice {
                    reason: reason.dupe(),
                    strictness_kind: TypeStrictnessKind::Flow,
                    props: properties::PropertiesMap::new(),
                    flags,
                    frozen: true,
                    generics: t_generic_id,
                    interface: None,
                    reachable_targs: Rc::from([]),
                    literal,
                    exact_object_type: false,
                });
                return resolved(
                    next,
                    recurse,
                    cx,
                    env,
                    use_op,
                    reason,
                    resolve_tool,
                    tool,
                    x,
                );
            }
            // TODO(jmbrown): Investigate if these cases can be used for ReactConfig/ObjecRep/Rest.
            // In principle, we should be able to use it for Rest, but right now
            // `const {x, ...y} = 3;` tries to get `x` from Number.
            // They don't make sense with $ReadOnly's semantics, since $ReadOnly doesn't model
            // copying/spreading an object.
            DefTInner::BoolGeneralT | DefTInner::SingletonBoolT { .. }
                if matches!(tool, object::Tool::Spread(box (_, _))) =>
            {
                let flags = Flags {
                    obj_kind: ObjKind::Exact,
                    react_dro: None,
                };
                let x = Vec1::new(object::Slice {
                    reason: reason.dupe(),
                    strictness_kind: TypeStrictnessKind::Flow,
                    props: properties::PropertiesMap::new(),
                    flags,
                    frozen: true,
                    generics: t_generic_id,
                    interface: None,
                    reachable_targs: Rc::from([]),
                    literal,
                    exact_object_type: false,
                });
                return resolved(
                    next,
                    recurse,
                    cx,
                    env,
                    use_op,
                    reason,
                    resolve_tool,
                    tool,
                    x,
                );
            }
            // mixed is treated as {readonly [string]: unknown} except in type spread and react config checking, where
            // it's treated as {}. Any JavaScript value may be treated as an object so this is safe.
            //
            // We ought to use {} for everything since it is a more sound representation
            // of `mixed` as an object. The fact that we don't today is technical debt that we should
            // clean up.
            DefTInner::MixedT(_) => {
                // TODO(jmbrown): This should be Inexact
                let flags = Flags {
                    obj_kind: ObjKind::Exact,
                    react_dro: None,
                };
                let x = match tool {
                    object::Tool::Spread(box (_, _))
                    | object::Tool::ObjectRep
                    | object::Tool::ReactConfig(box ObjectToolReactConfigData { .. }) => {
                        Vec1::new(object::Slice {
                            reason: reason.dupe(),
                            strictness_kind: TypeStrictnessKind::Flow,
                            props: properties::PropertiesMap::new(),
                            flags,
                            frozen: true,
                            generics: t_generic_id.clone(),
                            interface: None,
                            reachable_targs: Rc::from([]),
                            literal,
                            exact_object_type: false,
                        })
                    }
                    _ => {
                        let flags = Flags {
                            obj_kind: ObjKind::Indexed(DictType {
                                dict_name: None,
                                key: str_module_t::make(r.dupe()),
                                value: t.dupe(),
                                dict_polarity: Polarity::Positive,
                            }),
                            react_dro: None,
                        };
                        Vec1::new(object::Slice {
                            reason: reason.dupe(),
                            strictness_kind: TypeStrictnessKind::Flow,
                            props: properties::PropertiesMap::new(),
                            flags,
                            frozen: true,
                            generics: t_generic_id.clone(),
                            interface: None,
                            reachable_targs: Rc::from([]),
                            literal,
                            exact_object_type: false,
                        })
                    }
                };
                return resolved(
                    next,
                    recurse,
                    cx,
                    env,
                    use_op,
                    reason,
                    resolve_tool,
                    tool,
                    x,
                );
            }
            DefTInner::ArrT(arr) => match arr.deref() {
                ArrType::ArrayAT(box ArrayATData {
                    elem_t, react_dro, ..
                })
                | ArrType::ROArrayAT(box ROArrayATData {
                    elem_t, react_dro, ..
                }) if matches!(tool, object::Tool::ReadOnly) => {
                    return return_(
                        cx,
                        use_op,
                        Type::new(TypeInner::DefT(
                            r.dupe(),
                            DefT::new(DefTInner::ArrT(Rc::new(ArrType::ROArrayAT(Box::new(
                                ROArrayATData {
                                    elem_t: elem_t.dupe(),
                                    react_dro: react_dro.clone(),
                                    literal: arr.literal(),
                                },
                            ))))),
                        )),
                    );
                }
                ArrType::TupleAT(box TupleATData {
                    elem_t,
                    elements,
                    arity,
                    inexact,
                    rest,
                    react_dro,
                    strictness_kind,
                    ..
                }) if matches!(tool, object::Tool::ReadOnly) => {
                    let read_only = |elem: &TupleElement| TupleElement {
                        t: elem.t.dupe(),
                        name: elem.name.clone(),
                        polarity: Polarity::Positive,
                        optional: elem.optional,
                        reason: elem.reason.dupe(),
                    };
                    let elements: Vec<TupleElement> = elements.iter().map(read_only).collect();
                    return return_(
                        cx,
                        use_op,
                        Type::new(TypeInner::DefT(
                            r.dupe(),
                            DefT::new(DefTInner::ArrT(Rc::new(ArrType::TupleAT(Box::new(
                                TupleATData {
                                    elem_t: elem_t.dupe(),
                                    elements: elements.into(),
                                    arity: *arity,
                                    inexact: *inexact,
                                    rest: rest.as_ref().map(read_only),
                                    react_dro: react_dro.clone(),
                                    strictness_kind: *strictness_kind,
                                    literal: arr.literal(),
                                    fresh: arr.fresh(),
                                },
                            ))))),
                        )),
                    );
                }
                ArrType::TupleAT(box TupleATData {
                    elem_t,
                    elements,
                    arity,
                    inexact,
                    rest,
                    react_dro,
                    strictness_kind,
                    ..
                }) if matches!(tool, object::Tool::Partial) => {
                    let elements: Vec<TupleElement> = elements
                        .iter()
                        .map(|elem| {
                            let t = type_util::optional(elem.t.dupe(), None, false);
                            TupleElement {
                                t,
                                name: elem.name.clone(),
                                polarity: elem.polarity,
                                optional: true,
                                reason: elem.reason.dupe(),
                            }
                        })
                        .collect();
                    // Each element of the rest element may be `undefined`.
                    let rest = rest.as_ref().map(|elem| TupleElement {
                        t: type_util::union_of_ts(
                            elem.reason.dupe(),
                            vec![elem.t.dupe(), void::why(elem.reason.dupe())],
                            None,
                        ),
                        ..elem.clone()
                    });
                    let def_reason = match reason.desc(false) {
                        Some(VirtualReasonDesc::RPartialOf(_)) => r.dupe(),
                        _ => reason.dupe(),
                    };
                    let elem_t = type_util::union_of_ts(
                        type_util::reason_of_t(elem_t).dupe(),
                        elements
                            .iter()
                            .chain(&rest)
                            .map(|elem| elem.t.dupe())
                            .collect(),
                        None,
                    );
                    let (_, num_total) = arity;
                    let arity = (0, *num_total);
                    return return_(
                        cx,
                        use_op,
                        Type::new(TypeInner::DefT(
                            def_reason,
                            DefT::new(DefTInner::ArrT(Rc::new(ArrType::TupleAT(Box::new(
                                TupleATData {
                                    elem_t,
                                    elements: elements.into(),
                                    arity,
                                    inexact: *inexact,
                                    rest,
                                    react_dro: react_dro.clone(),
                                    strictness_kind: *strictness_kind,
                                    literal: arr.literal(),
                                    fresh: arr.fresh(),
                                },
                            ))))),
                        )),
                    );
                }
                ArrType::TupleAT(box TupleATData {
                    elem_t,
                    elements,
                    arity,
                    inexact,
                    rest,
                    react_dro,
                    strictness_kind,
                    ..
                }) if matches!(tool, object::Tool::Required) => {
                    let elements: Vec<TupleElement> = elements
                        .iter()
                        .map(|elem| {
                            let t = match elem.t.deref() {
                                TypeInner::OptionalT { type_, .. } => type_.dupe(),
                                _ => elem.t.dupe(),
                            };
                            TupleElement {
                                t,
                                name: elem.name.clone(),
                                polarity: elem.polarity,
                                optional: false,
                                reason: elem.reason.dupe(),
                            }
                        })
                        .collect();
                    let def_reason = match reason.desc(false) {
                        Some(VirtualReasonDesc::RRequiredOf(_)) => r.dupe(),
                        _ => reason.dupe(),
                    };
                    let elem_t = type_util::union_of_ts(
                        type_util::reason_of_t(elem_t).dupe(),
                        elements
                            .iter()
                            .chain(rest)
                            .map(|elem| elem.t.dupe())
                            .collect(),
                        None,
                    );
                    let (_, num_total) = arity;
                    let arity = (*num_total, *num_total);
                    return return_(
                        cx,
                        use_op,
                        Type::new(TypeInner::DefT(
                            def_reason,
                            DefT::new(DefTInner::ArrT(Rc::new(ArrType::TupleAT(Box::new(
                                TupleATData {
                                    elem_t,
                                    elements: elements.into(),
                                    arity,
                                    inexact: *inexact,
                                    rest: rest.dupe(),
                                    react_dro: react_dro.clone(),
                                    strictness_kind: *strictness_kind,
                                    literal: arr.literal(),
                                    fresh: arr.fresh(),
                                },
                            ))))),
                        )),
                    );
                }
                _ => {}
            },
            DefTInner::EmptyT => {
                return return_(cx, use_op, empty_t::make(r.dupe()));
            }
            _ => {}
        },
        // Resolve each member of a union.
        TypeInner::UnionT(union_reason, rep) => {
            let union_loc = union_reason.loc().dupe();
            let members_filtered =
                flow_typing_visitors::type_mapper::union_flatten(cx, rep.members_iter().duped());
            let tool = match tool {
                object::Tool::Spread(box (options, state)) => {
                    let mut state = state.clone();
                    state.union_t = Some(t.dupe());
                    object::Tool::Spread(Box::new((options.clone(), state)))
                }
                _ => tool.clone(),
            };

            return match members_filtered.as_slice() {
                [] => return_(cx, use_op, empty_t::make(union_reason.dupe())),
                [t_only] => recurse(
                    cx,
                    env,
                    use_op,
                    reason,
                    object::ResolveTool::Resolve(object::Resolve::Next),
                    &tool,
                    t_only.dupe(),
                ),
                [t_first, rest @ ..] => {
                    let rest_vec1 = Vec1::try_from_vec(rest.to_vec()).unwrap();
                    let resolve_tool_new = object::ResolveTool::Resolve(object::Resolve::List0(
                        rest_vec1,
                        object::Join(union_loc, object::JoinOp::Or),
                    ));
                    recurse(
                        cx,
                        env,
                        use_op,
                        reason,
                        resolve_tool_new,
                        &tool,
                        t_first.dupe(),
                    )
                }
            };
        }
        // Resolve each member of an intersection.
        TypeInner::IntersectionT(intersection_reason, rep) => {
            let intersection_loc = intersection_reason.loc().dupe();
            let mut members = rep.members_iter().duped();
            let t_first = members.next().unwrap();
            let todo: Vec<Type> = members.collect();
            let resolve_tool_new = object::ResolveTool::Resolve(object::Resolve::List0(
                Vec1::try_from_vec(todo).unwrap(),
                object::Join(intersection_loc, object::JoinOp::And),
            ));

            return recurse(cx, env, use_op, reason, resolve_tool_new, tool, t_first);
        }
        // Propagate any.
        TypeInner::AnyT(_, src) => {
            return return_(cx, use_op, any_t::why(*src, reason.dupe()));
        }
        _ => {}
    }
    // Other types have reasonable object representations that may be added as
    // new uses of the object kit resolution code is found.
    match tool {
        object::Tool::MakeExact => {
            add_output(
                cx,
                env,
                ErrorMessage::EUnsupportedExact(Box::new(EUnsupportedExactData {
                    value_loc: type_util::ref_loc_of_t(&t).dupe(),
                    value_desc: flow_js_utils::type_or_type_desc_for_error(&t),
                })),
            )?;
        }
        _ => {
            add_output(
                cx,
                env,
                ErrorMessage::EInvalidObjectKit(Box::new(EInvalidObjectKitData {
                    loc: type_util::reason_of_t(&t).loc().dupe(),
                    value: flow_js_utils::type_reference_for_error(&t),
                    use_op: use_op.clone(),
                    requirement: if matches!(tool, object::Tool::ReadOnly) {
                        LowerRequirement::ObjectOrArrayOrTuple
                    } else {
                        LowerRequirement::Object
                    },
                })),
            )?;
        }
    }
    return_(cx, use_op, any_t::error(reason.dupe()))
}

pub fn super_<'cx, A>(
    return_: &dyn Fn(&mut Context<'cx>, UseOp, Type) -> Result<A, FlowJsException>,
    next: &dyn Fn(
        &mut Context<'cx>,
        UseOp,
        &object::Tool,
        &Reason,
        Vec1<object::Slice>,
    ) -> Result<A, FlowJsException>,
    recurse: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        UseOp,
        &Reason,
        object::ResolveTool,
        &object::Tool,
        Type,
    ) -> Result<A, FlowJsException>,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: UseOp,
    reason: &Reason,
    resolve_tool: object::Resolve,
    tool: &object::Tool,
    acc: object::Slice,
    mut pending: Vec<Type>,
    t: &Type,
) -> Result<A, FlowJsException> {
    let finish = |cx: &mut Context<'cx>, use_op, acc, mut pending: Vec<Type>| match pending.pop() {
        Some(base) => recurse(
            cx,
            env,
            use_op,
            reason,
            object::ResolveTool::Super(acc, pending, Rc::new(resolve_tool.clone())),
            tool,
            base,
        ),
        None => resolved(
            next,
            recurse,
            cx,
            env,
            use_op,
            reason,
            resolve_tool.clone(),
            tool,
            Vec1::new(acc),
        ),
    };
    match t.deref() {
        TypeInner::IntersectionT(_, rep) => {
            let bases: Vec<Type> = rep.members_iter().duped().collect();
            pending.extend(bases.into_iter().rev());
            finish(cx, use_op, acc, pending)
        }
        TypeInner::DefT(r, def_t) => {
            let (slice, super_t) = match def_t.deref() {
                DefTInner::InstanceT(inst_t) => {
                    let inst = &inst_t.inst;
                    let include_proto_props =
                        matches!(
                            tool,
                            object::Tool::Spread(_)
                                | object::Tool::MakeExact
                                | object::Tool::ReadOnly
                                | object::Tool::Partial
                                | object::Tool::Required
                                | object::Tool::Rest(box (object::rest::MergeMode::Omit, _))
                                | object::Tool::ObjectRep
                                | object::Tool::ObjectMap(_)
                        ) && matches!(inst.inst_kind, InstanceKind::InterfaceKind { .. });
                    (
                        interface_slice(
                            cx,
                            r,
                            &inst_t.static_,
                            inst.clone(),
                            inst.own_props.dupe(),
                            include_proto_props,
                            object_tool_dict_polarity_override(tool),
                            flow_typing_generics::spread_empty(),
                        ),
                        Some(inst_t.super_.dupe()),
                    )
                }
                DefTInner::ObjT(obj) => (
                    object_slice(
                        cx,
                        None,
                        r,
                        obj.props_tmap.dupe(),
                        &obj.flags,
                        object_tool_dict_polarity_override(tool),
                        false,
                        obj.reachable_targs.dupe(),
                        flow_typing_generics::spread_empty(),
                        obj.strictness_kind,
                        obj.literal,
                    ),
                    None,
                ),
                _ => return finish(cx, use_op, acc, pending),
            };
            let (props, flags, frozen, generics, reachable_targs) =
                intersect2(cx, env, &acc.reason, &acc, &slice);
            let acc = object::Slice {
                reason: acc.reason.dupe(),
                strictness_kind: acc.strictness_kind.join(slice.strictness_kind),
                props,
                flags,
                frozen,
                generics,
                interface: slice.interface.or(acc.interface),
                reachable_targs,
                literal: acc.literal,
                exact_object_type: false,
            };
            match super_t {
                Some(base) => recurse(
                    cx,
                    env,
                    use_op,
                    reason,
                    object::ResolveTool::Super(acc, pending, Rc::new(resolve_tool)),
                    tool,
                    base,
                ),
                None => finish(cx, use_op, acc, pending),
            }
        }
        TypeInner::AnyT(_, src) => {
            return_(cx, use_op, Type::new(TypeInner::AnyT(reason.dupe(), *src)))
        }
        _ => finish(cx, use_op, acc, pending),
    }
}

pub fn mk_variance(variance: MappedTypeVariance, prop_polarity: Polarity) -> Polarity {
    match variance {
        MappedTypeVariance::KeepVariance => prop_polarity,
        MappedTypeVariance::OverrideVariance(pol) => pol,
        MappedTypeVariance::RemoveVariance(pol) => {
            if prop_polarity == pol {
                Polarity::Neutral
            } else {
                prop_polarity
            }
        }
    }
}

pub fn mk_mapped_prop_type<'cx>(
    cx: &mut Context<'cx>,
    filter_optional: &dyn Fn(&mut Context<'cx>, Type) -> Result<Type, FlowJsException>,
    use_op: &UseOp,
    mapped_type_optionality: &MappedTypeOptionality,
    poly_prop: &Type,
    key_t: Type,
    prop_optional: bool,
) -> Result<Type, FlowJsException> {
    // We persist the original use_op here so that errors involving the typeapp are positioned
    // at the use site and not the typeapp site
    let t = type_util::typeapp_with_use_op(
        true,
        false,
        type_util::reason_of_t(poly_prop).dupe(),
        use_op.dupe(),
        poly_prop.dupe(),
        vec![key_t],
    );
    match mapped_type_optionality {
        MappedTypeOptionality::MakeOptional => Ok(type_util::optional(t, None, false)),
        MappedTypeOptionality::RemoveOptional => {
            if prop_optional {
                filter_optional(cx, t)
            } else {
                Ok(t)
            }
        }
        MappedTypeOptionality::KeepOptionality => {
            if prop_optional {
                Ok(type_util::optional(t, None, false))
            } else {
                Ok(t)
            }
        }
    }
}

pub fn is_prop_optional(t: &Type) -> bool {
    matches!(t.deref(), TypeInner::OptionalT { .. })
}

/* Per-source-key dispatch produced by [object_kit.rs] when a mapped type carries a [name_type]
 * (the TS `as` clause). For each source key, [dest_names] lists the substituted destination
 * names (singleton-string/numeric); [indexer_keys] lists the substituted name types that should
 * contribute to the output indexer (e.g. [string], [number], [symbol]). */
#[derive(Debug, Clone)]
pub struct PerSourceDispatch {
    pub dest_names: Vec<(Name, Reason)>,
    pub indexer_keys: Vec<Type>,
}

/* What to do with the source object's indexer (the `(None, ObjKind::Indexed(dict_t))` arm). */
#[derive(Debug, Clone)]
pub enum SourceIndexerRemap {
    DropSourceIndexer,
    KeepSourceIndexerKey(Type),
}

#[derive(Debug, Clone)]
pub struct NameRemap {
    pub per_source: BTreeMap<Name, PerSourceDispatch>,
    /* Source dict (when `(None, ObjKind::Indexed(_))`) remap: drop the dict, or keep with a new key. */
    pub source_indexer: Option<SourceIndexerRemap>,
    /* Substituted destination keys derived from the "indexer side" of selected_keys (xs in
     * `Some((named, xs))`). These are the non-literal entries of a SemiHomomorphic key tparam. */
    pub selected_xs_keys: Vec<Type>,
}

/* The greatest lower bound on the access lattice: the result must satisfy every contributor's
 * contract. Positive (readonly) and Negative (write-only) are restrictions of Neutral
 * (invariant), so when one contributor is restricted the merged property must inherit that
 * restriction. The Positive/Negative case has no satisfying solution (one demands read-only,
 * the other write-only); we conservatively fall through to Neutral, but in practice mapped
 * types don't produce write-only fields so this case is unreachable. */
pub fn mapped_type_meet_polarity(p1: Polarity, p2: Polarity) -> Polarity {
    match (p1, p2) {
        (Polarity::Positive, Polarity::Positive) => Polarity::Positive,
        (Polarity::Negative, Polarity::Negative) => Polarity::Negative,
        (Polarity::Neutral, Polarity::Neutral) => Polarity::Neutral,
        (Polarity::Positive, Polarity::Neutral) | (Polarity::Neutral, Polarity::Positive) => {
            Polarity::Positive
        }
        (Polarity::Negative, Polarity::Neutral) | (Polarity::Neutral, Polarity::Negative) => {
            Polarity::Negative
        }
        (Polarity::Positive, Polarity::Negative) | (Polarity::Negative, Polarity::Positive) => {
            Polarity::Neutral
        }
    }
}

/* Merge two field props from the same destination key — used when multiple source keys remap to
 * the same destination, or when a source key remaps to multiple names that collide.
 *
 * Optionality must be preserved at the top level: `OptionalT<number> | string` would not be a
 * top-level OptionalT, and Flow's missing-property logic would treat the merged property as
 * required. We unwrap any contributing OptionalT, union the inner types, and re-wrap if any
 * contributor was optional. */
pub fn merge_field(
    reason: &Reason,
    existing: Option<&Property>,
    incoming: &Property,
) -> Option<Property> {
    let unwrap_optional = |t: &Type| -> (Type, bool) {
        match t.deref() {
            TypeInner::OptionalT { type_, .. } => (type_.dupe(), true),
            _ => (t.dupe(), false),
        }
    };
    match existing {
        None => Some(incoming.dupe()),
        Some(existing_prop) => {
            match (existing_prop.deref(), incoming.deref()) {
                (
                    PropertyInner::Field(box FieldData {
                        preferred_def_locs: pl1,
                        key_loc: kl1,
                        type_: t1,
                        polarity: p1,
                    }),
                    PropertyInner::Field(box FieldData {
                        preferred_def_locs: _,
                        key_loc: _,
                        type_: t2,
                        polarity: p2,
                    }),
                ) => {
                    let (inner1, opt1) = unwrap_optional(t1);
                    let (inner2, opt2) = unwrap_optional(t2);
                    let merged_inner =
                        type_util::union_of_ts(reason.dupe(), vec![inner1, inner2], None);
                    let merged_type = if opt1 || opt2 {
                        type_util::optional(merged_inner, None, false)
                    } else {
                        merged_inner
                    };
                    let merged_polarity = mapped_type_meet_polarity(*p1, *p2);
                    Some(Property::new(PropertyInner::Field(Box::new(FieldData {
                        preferred_def_locs: pl1.clone(),
                        key_loc: kl1.clone(),
                        type_: merged_type,
                        polarity: merged_polarity,
                    }))))
                }
                _ => {
                    /* map_object only ever produces Field props (line below). Other prop kinds shouldn't appear. */
                    Some(existing_prop.dupe())
                }
            }
        }
    }
}

pub fn map_object<'cx>(
    filter_optional: &dyn Fn(&mut Context<'cx>, Type) -> Result<Type, FlowJsException>,
    name_remap: Option<&NameRemap>,
    poly_prop: &Type,
    mapped_type_flags: &MappedTypeFlags,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    use_op: &UseOp,
    selected_keys: Option<&(Vec<(Name, Reason)>, Vec<Type>)>,
    slice: &object::Slice,
) -> Result<Type, FlowJsException> {
    let variance = mapped_type_flags.variance;
    let mapped_type_optionality = &mapped_type_flags.optional;
    let object::Slice {
        reason: _,
        strictness_kind,
        props,
        flags,
        frozen,
        generics,
        interface,
        reachable_targs,
        literal: _,
        exact_object_type: _,
    } = slice;
    let mk_prop_type = |cx: &mut Context<'cx>,
                        key_t: Type,
                        prop_optional: bool|
     -> Result<Type, FlowJsException> {
        mk_mapped_prop_type(
            cx,
            filter_optional,
            use_op,
            mapped_type_optionality,
            poly_prop,
            key_t,
            prop_optional,
        )
    };
    /* Build the field for a given source key and its prop in the source object. */
    let build_source_field = |cx: &mut Context<'cx>,
                              key: &Name,
                              prop: &Property|
     -> Result<(ALoc, Property), FlowJsException> {
        let prop_t = property::type_(prop);
        let prop_polarity = property::polarity(prop);
        let key_loc = property::first_loc(prop)
            .unwrap_or_else(|| type_util::reason_of_t(prop_t).loc().dupe());
        // A symbol-typed key contributes its `unique symbol` type, not a string literal.
        let key_t = match key {
            Name::Symbol(_) => {
                let r = flow_common::reason::mk_reason(
                    VirtualReasonDesc::RUniqueSymbol,
                    key_loc.dupe(),
                );
                flow_js_utils::type_of_key_name_with_env(env, key.dupe(), &r)
            }
            Name::Str(key_str) => {
                let key_str = key_str.dupe();
                Type::new(TypeInner::DefT(
                    flow_common::reason::mk_reason(
                        VirtualReasonDesc::RStringLit(key_str.dupe()),
                        key_loc.dupe(),
                    ),
                    DefT::new(DefTInner::SingletonStrT {
                        from_annot: true,
                        value: key_str,
                    }),
                ))
            }
        };
        let prop_optional = is_prop_optional(prop_t);
        let polarity = if *frozen {
            Polarity::Positive
        } else {
            mk_variance(variance, prop_polarity)
        };
        let field = Property::new(PropertyInner::Field(Box::new(FieldData {
            preferred_def_locs: None,
            key_loc: Some(key_loc.dupe()),
            type_: mk_prop_type(cx, key_t, prop_optional)?,
            polarity,
        })));
        Ok((key_loc, field))
    };
    let any_field = || -> Property {
        Property::new(PropertyInner::Field(Box::new(FieldData {
            preferred_def_locs: None,
            key_loc: None,
            type_: any_t::why(AnySource::AnyError(None), reason.dupe()),
            polarity: if *frozen {
                Polarity::Positive
            } else {
                Polarity::Neutral
            },
        })))
    };
    let meet_polarity = mapped_type_meet_polarity;
    let (out_props, extra_indexer_keys, extra_indexer_value_ts, extra_indexer_polarity_opt): (
        BTreeMap<Name, Property>,
        Vec<Type>,
        Vec<Type>,
        Option<Polarity>,
    ) = match name_remap {
        None => {
            let keys: Vec<Name> = match selected_keys {
                Some((keys_with_reason, _)) => {
                    keys_with_reason.iter().map(|(k, _)| k.clone()).collect()
                }
                None => props.keys().duped().collect(),
            };
            let mut out: BTreeMap<Name, Property> = BTreeMap::new();
            for key in &keys {
                match props.get(key) {
                    None => {
                        out.insert(key.dupe(), any_field());
                    }
                    Some(prop) => {
                        let (_, field) = build_source_field(cx, key, prop)?;
                        out.insert(key.dupe(), field);
                    }
                }
            }
            (out, vec![], vec![], None)
        }
        Some(NameRemap {
            per_source,
            source_indexer: _,
            selected_xs_keys: _,
        }) => {
            let source_keys: Vec<Name> = match selected_keys {
                Some((keys_with_reason, _)) => {
                    keys_with_reason.iter().map(|(k, _)| k.clone()).collect()
                }
                None => props.keys().duped().collect(),
            };
            let mut out_map: BTreeMap<Name, Property> = BTreeMap::new();
            let mut idx_keys: Vec<Type> = Vec::new();
            let mut idx_values: Vec<Type> = Vec::new();
            let mut idx_polarity: Option<Polarity> = None;
            for src_key in &source_keys {
                match per_source.get(src_key) {
                    None => {
                        /* Unreachable: [compute_name_remap] populates [per_source] for every source key
                         * iterated here. Drop silently if it ever happens. */
                    }
                    Some(PerSourceDispatch {
                        dest_names,
                        indexer_keys,
                    }) => match props.get(src_key) {
                        None => {
                            let f = any_field();
                            for (n, _r) in dest_names {
                                name_utils::map::update(
                                    n.dupe(),
                                    |e| merge_field(reason, e, &f),
                                    &mut out_map,
                                );
                            }
                        }
                        Some(prop) => {
                            let (_, field) = build_source_field(cx, src_key, prop)?;
                            for (n, _r) in dest_names {
                                name_utils::map::update(
                                    n.dupe(),
                                    |e| merge_field(reason, e, &field),
                                    &mut out_map,
                                );
                            }
                            let (value_ts_for_indexer, field_polarity) = match field.deref() {
                                PropertyInner::Field(box FieldData {
                                    type_, polarity, ..
                                }) => (vec![type_.dupe()], Some(*polarity)),
                                _ => (vec![], None),
                            };
                            if !indexer_keys.is_empty() {
                                let mut new_idx_keys = indexer_keys.clone();
                                new_idx_keys.append(&mut idx_keys);
                                idx_keys = new_idx_keys;
                                let mut new_idx_values = value_ts_for_indexer;
                                new_idx_values.append(&mut idx_values);
                                idx_values = new_idx_values;
                                idx_polarity = match (idx_polarity, field_polarity) {
                                    (None, p) => p,
                                    (p, None) => p,
                                    (Some(a), Some(b)) => Some(meet_polarity(a, b)),
                                };
                            }
                        }
                    },
                }
            }
            (out_map, idx_keys, idx_values, idx_polarity)
        }
    };
    let props_map = out_props;
    let obj_kind = match name_remap {
        None => match (selected_keys, &flags.obj_kind) {
            (Some((_, indexers)), _) if indexers.is_empty() => ObjKind::Exact,
            (Some((_, xs)), ObjKind::Indexed(dict_t)) => {
                let dict_optional = is_prop_optional(&dict_t.value);
                let dict_key = type_util::union_of_ts(reason.dupe(), xs.clone(), None);
                let dict_t_prime = DictType {
                    key: dict_key,
                    value: mk_prop_type(cx, dict_t.key.dupe(), dict_optional)?,
                    dict_polarity: mk_variance(variance, dict_t.dict_polarity),
                    ..dict_t.clone()
                };
                ObjKind::Indexed(dict_t_prime)
            }
            (Some((_, xs)), _) => {
                let key = type_util::union_of_ts(reason.dupe(), xs.clone(), None);
                let dict = DictType {
                    dict_name: None,
                    key,
                    value: any_t::why(AnySource::AnyError(None), reason.dupe()),
                    dict_polarity: Polarity::Neutral,
                };
                ObjKind::Indexed(dict)
            }
            (None, ObjKind::Indexed(dict_t)) => {
                let dict_optional = is_prop_optional(&dict_t.value);
                let dict_t_prime = DictType {
                    value: mk_prop_type(cx, dict_t.key.dupe(), dict_optional)?,
                    dict_polarity: mk_variance(variance, dict_t.dict_polarity),
                    ..dict_t.clone()
                };
                ObjKind::Indexed(dict_t_prime)
            }
            (None, _) => flags.obj_kind.clone(),
        },
        Some(NameRemap {
            per_source,
            source_indexer,
            selected_xs_keys,
        }) => {
            let source_indexer_dropped =
                matches!(source_indexer, Some(SourceIndexerRemap::DropSourceIndexer));
            let is_identity_remap = per_source.iter().all(|(src_key, dispatch)| {
                /* True when every source key remaps to exactly itself with no indexer keys.
                 * Non-identity remaps cannot inherit the source's open shape: the result's
                 * only known properties are the explicitly-named destinations. */
                let PerSourceDispatch {
                    dest_names,
                    indexer_keys,
                } = dispatch;
                match (dest_names.as_slice(), indexer_keys.as_slice()) {
                    ([(dst_name, _)], []) => src_key == dst_name,
                    _ => false,
                }
            });
            /* Compute output indexer from three sources of dest keys:
             *   1. per_source: a source named key's substituted name was non-singleton
             *   2. source_indexer: source object's own dict, remapped via name_type
             *   3. selected_xs_keys: substituted xs entries from selected_keys's indexer side
             * Cases 2 and 3 share the same source dict (when present) for the value/polarity. */
            let (dict_idx_keys, dict_value_opt, dict_polarity_opt): (
                Vec<Type>,
                Option<Type>,
                Option<Polarity>,
            ) = match (source_indexer, selected_xs_keys, &flags.obj_kind) {
                (Some(SourceIndexerRemap::DropSourceIndexer), _, _) => (vec![], None, None),
                (
                    Some(SourceIndexerRemap::KeepSourceIndexerKey(new_key)),
                    xs,
                    ObjKind::Indexed(dict_t),
                ) => {
                    let dict_optional = is_prop_optional(&dict_t.value);
                    let value = mk_prop_type(cx, dict_t.key.dupe(), dict_optional)?;
                    let pol = mk_variance(variance, dict_t.dict_polarity);
                    let mut keys = vec![new_key.dupe()];
                    keys.extend(xs.iter().duped());
                    (keys, Some(value), Some(pol))
                }
                (None, xs, ObjKind::Indexed(dict_t)) if !xs.is_empty() => {
                    /* Selected_keys.xs entries with source dict — value comes from source dict_t.key,
                     * matching the legacy `(Some((_, xs)), ObjKind::Indexed(dict_t))` arm. */
                    let dict_optional = is_prop_optional(&dict_t.value);
                    let value = mk_prop_type(cx, dict_t.key.dupe(), dict_optional)?;
                    let pol = mk_variance(variance, dict_t.dict_polarity);
                    (xs.iter().duped().collect(), Some(value), Some(pol))
                }
                (None, xs, _) if !xs.is_empty() => {
                    /* Selected_keys.xs entries but no source dict — matches the legacy `(Some((_, xs)), _)`
                     * arm: any value, neutral polarity. */
                    (
                        xs.iter().duped().collect(),
                        Some(any_t::why(AnySource::AnyError(None), reason.dupe())),
                        Some(Polarity::Neutral),
                    )
                }
                _ => (vec![], None, None),
            };
            let mut all_idx_keys = extra_indexer_keys.clone();
            all_idx_keys.extend(dict_idx_keys.iter().duped());
            let combined_polarity = match (extra_indexer_polarity_opt, dict_polarity_opt) {
                (None, None) => Polarity::Neutral,
                (Some(p), None) | (None, Some(p)) => p,
                (Some(a), Some(b)) => meet_polarity(a, b),
            };
            let combined_value = || -> Type {
                match (&extra_indexer_value_ts, &dict_value_opt) {
                    (vs, Some(v)) if vs.is_empty() => v.dupe(),
                    (vs, None) => type_util::union_of_ts(reason.dupe(), vs.clone(), None),
                    (vs, Some(v)) => {
                        let mut all = vec![v.dupe()];
                        all.extend(vs.iter().duped());
                        type_util::union_of_ts(reason.dupe(), all, None)
                    }
                }
            };
            match all_idx_keys.as_slice() {
                [] => {
                    /* No destination indexer contributions. We default to Exact when selected_keys is Some
                     * (matching the legacy arm for `Some` with no indexers) or when name_type explicitly dropped the
                     * source dict (`as empty` on the dict key). For homomorphic remaps without a destination
                     * indexer, preserve flags.obj_kind only when the remap is identity (every source key maps
                     * to itself); a non-identity remap produces a closed shape because the result's only
                     * known properties are the explicitly-named destinations. */
                    match selected_keys {
                        Some(_) => ObjKind::Exact,
                        None if source_indexer_dropped => ObjKind::Exact,
                        None if is_identity_remap => flags.obj_kind.clone(),
                        None => ObjKind::Exact,
                    }
                }
                [k] => ObjKind::Indexed(DictType {
                    dict_name: None,
                    key: k.dupe(),
                    value: combined_value(),
                    dict_polarity: combined_polarity,
                }),
                _ => {
                    let key = type_util::union_of_ts(reason.dupe(), all_idx_keys.clone(), None);
                    ObjKind::Indexed(DictType {
                        dict_name: None,
                        key,
                        value: combined_value(),
                        dict_polarity: combined_polarity,
                    })
                }
            }
        }
    };
    let call = None;
    let id = cx.generate_property_map(props_map.into());
    let proto = Type::new(TypeInner::ObjProtoT(reason.dupe()));
    let flags = Flags {
        obj_kind,
        ..flags.clone()
    };
    Ok(mk_object_type(
        cx,
        reason,
        false,
        true,
        interface.clone(),
        reachable_targs,
        flow_common::subst_name::OpKind::MappedObject,
        &flags,
        call,
        id,
        proto,
        generics.clone(),
        *strictness_kind,
        false,
    ))
}

pub fn run<'cx, A>(
    add_output: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        ErrorMessage<ALoc>,
    ) -> Result<(), FlowJsException>,
    return_: &dyn Fn(&mut Context<'cx>, UseOp, Type) -> Result<A, FlowJsException>,
    next: &dyn Fn(
        &mut Context<'cx>,
        UseOp,
        &object::Tool,
        &Reason,
        Vec1<object::Slice>,
    ) -> Result<A, FlowJsException>,
    recurse: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        UseOp,
        &Reason,
        object::ResolveTool,
        &object::Tool,
        Type,
    ) -> Result<A, FlowJsException>,
    statics: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        &Reason,
        &Type,
    ) -> Result<Type, FlowJsException>,
    cx: &mut Context<'cx>,
    use_op: UseOp,
    reason: &Reason,
    resolve_tool: object::ResolveTool,
    tool: &object::Tool,
    l: &Type,
) -> Result<A, FlowJsException> {
    run_with_env::<A>(
        add_output,
        return_,
        next,
        recurse,
        statics,
        cx,
        &FlowJsEnv::entry(),
        use_op,
        reason,
        resolve_tool,
        tool,
        l,
    )
}

pub fn run_with_env<'cx, A>(
    add_output: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        ErrorMessage<ALoc>,
    ) -> Result<(), FlowJsException>,
    return_: &dyn Fn(&mut Context<'cx>, UseOp, Type) -> Result<A, FlowJsException>,
    next: &dyn Fn(
        &mut Context<'cx>,
        UseOp,
        &object::Tool,
        &Reason,
        Vec1<object::Slice>,
    ) -> Result<A, FlowJsException>,
    recurse: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        UseOp,
        &Reason,
        object::ResolveTool,
        &object::Tool,
        Type,
    ) -> Result<A, FlowJsException>,
    statics: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        &Reason,
        &Type,
    ) -> Result<Type, FlowJsException>,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: UseOp,
    reason: &Reason,
    resolve_tool: object::ResolveTool,
    tool: &object::Tool,
    l: &Type,
) -> Result<A, FlowJsException> {
    match resolve_tool {
        object::ResolveTool::Resolve(rt) => resolve_with_env(
            add_output, return_, next, recurse, statics, cx, env, use_op, reason, rt, tool, l,
        ),
        object::ResolveTool::Super(acc, pending, rt) => super_(
            return_,
            next,
            recurse,
            cx,
            env,
            use_op,
            reason,
            (*rt).clone(),
            tool,
            acc,
            pending,
            l,
        ),
    }
}
