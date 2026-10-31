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
use dupe::IterDupedExt;
use dupe::OptionDupedExt;
use flow_aloc::ALoc;
use flow_common::polarity::Polarity;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_typing_context::Context;
use flow_typing_context::Cx;
use flow_typing_errors::error_message::EClassToObjectData;
use flow_typing_errors::error_message::EConstructSignatureMissingInSubtypingData;
use flow_typing_errors::error_message::EExpectedBigIntLitData;
use flow_typing_errors::error_message::EExpectedBooleanLitData;
use flow_typing_errors::error_message::EExpectedNumberLitData;
use flow_typing_errors::error_message::EExpectedStringLitData;
use flow_typing_errors::error_message::EHookIncompatibleData;
use flow_typing_errors::error_message::EHookUniqueIncompatibleData;
use flow_typing_errors::error_message::EIndexerCheckFailedData;
use flow_typing_errors::error_message::EInvariantSubtypingWithUseOpData;
use flow_typing_errors::error_message::EPrimitiveAsInterfaceData;
use flow_typing_errors::error_message::EPropNotFoundInSubtypingData;
use flow_typing_errors::error_message::EPropPolarityMismatchData;
use flow_typing_errors::error_message::EPropsExtraAgainstExactObjectData;
use flow_typing_errors::error_message::EPropsNotFoundInInvariantSubtypingData;
use flow_typing_errors::error_message::EPropsNotFoundInSubtypingData;
use flow_typing_errors::error_message::ETupleArityMismatchData;
use flow_typing_errors::error_message::ETupleElementPolarityMismatchData;
use flow_typing_errors::error_message::ETypeParamConstIncompatibilityData;
use flow_typing_errors::error_message::EUnionOptimizationOnNonUnionData;
use flow_typing_errors::error_message::EnumErrorKind;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::IncompatibleUpperData;
use flow_typing_errors::error_message::InternalError;
use flow_typing_errors::error_message::TypeGuardFuncReference;
use flow_typing_errors::error_message::TypeGuardParameterData;
use flow_typing_errors::intermediate_error_types;
use flow_typing_errors::intermediate_error_types::NamedReferenceData;
use flow_typing_errors::intermediate_error_types::PropPolarityMismatchObject;
use flow_typing_errors::intermediate_error_types::TypeGuardReferenceData;
use flow_typing_errors::intermediate_error_types::TypeGuardReferenceKind;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_common::instantiation_utils;
use flow_typing_flow_common::obj_type;
use flow_typing_flow_common::string_case_transform;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_flow_js_env::type_app_expansion;
use flow_typing_implicit_instantiation_check::ImplicitInstantiationCheck;
use flow_typing_implicit_instantiation_check::Operation;
use flow_typing_type::type_::AnyErrorKind;
use flow_typing_type::type_::AnySource;
use flow_typing_type::type_::ArrType::*;
use flow_typing_type::type_::ArrayATData;
use flow_typing_type::type_::ArrayElementCompatibilityData;
use flow_typing_type::type_::CallArg;
use flow_typing_type::type_::CanonicalRendersForm;
use flow_typing_type::type_::ComponentKind;
use flow_typing_type::type_::Cont;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner::*;
use flow_typing_type::type_::DepthTrace;
use flow_typing_type::type_::DictType;
use flow_typing_type::type_::EnumInfoInner;
use flow_typing_type::type_::FieldData;
use flow_typing_type::type_::FunParamData;
use flow_typing_type::type_::FunRestParam;
use flow_typing_type::type_::FunReturnData;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::HasOwnPropTData;
use flow_typing_type::type_::IndexerKeyCompatibilityData;
use flow_typing_type::type_::InstType;
use flow_typing_type::type_::InstTypeInner;
use flow_typing_type::type_::InstanceKind;
use flow_typing_type::type_::InstanceT;
use flow_typing_type::type_::InstanceTInner;
use flow_typing_type::type_::Literal;
use flow_typing_type::type_::LookupAction;
use flow_typing_type::type_::LookupKind;
use flow_typing_type::type_::LookupPropsForSubtypingData;
use flow_typing_type::type_::LookupTData;
use flow_typing_type::type_::MixedFlavor;
use flow_typing_type::type_::NominalType;
use flow_typing_type::type_::NominalTypeInner;
use flow_typing_type::type_::NonstrictReturningData;
use flow_typing_type::type_::ObjKind;
use flow_typing_type::type_::ObjType;
use flow_typing_type::type_::OpaqueTypeCustomErrorCompatibilityData;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::PropRef;
use flow_typing_type::type_::Property;
use flow_typing_type::type_::PropertyCompatibilityData;
use flow_typing_type::type_::PropertyInner;
use flow_typing_type::type_::PropertyType;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::ReactAbstractComponentTData;
use flow_typing_type::type_::ReactEffectType;
use flow_typing_type::type_::ReactKitTData;
use flow_typing_type::type_::ReposUseTData;
use flow_typing_type::type_::ThisInstanceTData;
use flow_typing_type::type_::ThisTypeAppTData;
use flow_typing_type::type_::TupleATData;
use flow_typing_type::type_::TupleElement;
use flow_typing_type::type_::TupleElementCompatibilityData;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeGuard;
use flow_typing_type::type_::TypeInner::*;
use flow_typing_type::type_::TypeStrictnessKind;
use flow_typing_type::type_::UnifyCause;
use flow_typing_type::type_::UnionEnum;
use flow_typing_type::type_::UnionEnumSet;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::UseT;
use flow_typing_type::type_::UseTInner::*;
use flow_typing_type::type_::VirtualFrameUseOp;
use flow_typing_type::type_::VirtualUseOp;
use flow_typing_type::type_::any_t;
use flow_typing_type::type_::constraint;
use flow_typing_type::type_::dummy_prototype;
use flow_typing_type::type_::empty_t;
use flow_typing_type::type_::hint_unavailable;
use flow_typing_type::type_::mixed_t;
use flow_typing_type::type_::mk_functiontype;
use flow_typing_type::type_::name_of_propref;
use flow_typing_type::type_::nominal;
use flow_typing_type::type_::properties;
use flow_typing_type::type_::property;
use flow_typing_type::type_::react;
use flow_typing_type::type_::type_collector::TypeCollector;
use flow_typing_type::type_::type_or_type_desc;
use flow_typing_type::type_::type_or_type_desc::ErrorTypeDesc;
use flow_typing_type::type_::union_rep;
use flow_typing_type::type_::void;
use flow_typing_type::type_util;
use flow_typing_visitors::type_mapper;
use vec1::Vec1;

use crate::flow_js::FlowJs;
use crate::flow_js::prop_typo_suggestion_for_name;
use crate::implicit_instantiation;
use crate::renders_kit;
use crate::speculation_kit;
use crate::template_literal_type;
use crate::type_inference_hooks_js;

fn has_syntactic_nullish_member(t: &Type) -> bool {
    match t.deref() {
        MaybeT(_, _) | OptionalT { .. } => true,
        UnionT(_, rep) => rep.members_iter().any(has_syntactic_nullish_member),
        DefT(_, deref!(NullT | VoidT)) => true,
        _ => false,
    }
}

struct OpaqueUpperBoundSpeculation<'a> {
    trace: DepthTrace,
    use_op: UseOp,
    opaque_t_reason: &'a Reason,
    upper_t: &'a Type,
    upper_u: &'a Type,
    fallback_l: &'a Type,
    fallback_t: Type,
}

impl OpaqueUpperBoundSpeculation<'_> {
    fn flow<'cx>(self, cx: &mut Context<'cx>, env: &FlowJsEnv) -> Result<(), FlowJsException> {
        let Self {
            trace,
            use_op,
            opaque_t_reason,
            upper_t,
            upper_u,
            fallback_l,
            fallback_t,
        } = self;
        let upper_t = upper_t.dupe();
        let upper_u = upper_u.dupe();
        let fallback_l = fallback_l.dupe();
        let opaque_t_reason_c = opaque_t_reason.dupe();
        let upper_use_op = use_op.dupe();
        let fallback_use_op = use_op.dupe();
        speculation_kit::try_custom(
            cx,
            env,
            Some(use_op),
            None,
            None,
            opaque_t_reason.loc().clone(),
            vec![
                Box::new(move |cx: &mut Context, env: &FlowJsEnv| {
                    let use_op = VirtualUseOp::Frame(
                        Arc::new(VirtualFrameUseOp::OpaqueTypeUpperBound {
                            opaque_t_reason: opaque_t_reason_c,
                        }),
                        Arc::new(upper_use_op),
                    );
                    FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &upper_t, &upper_u)
                }),
                Box::new(move |cx: &mut Context, env: &FlowJsEnv| {
                    FlowJs::rec_flow_t_with_env(
                        cx,
                        env,
                        trace,
                        fallback_use_op,
                        &fallback_l,
                        &fallback_t,
                    )
                }),
            ],
        )
    }
}

fn flow_all_in_union<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: &DepthTrace,
    rep: &union_rep::UnionRep,
    u: &UseT<Cx>,
) -> Result<(), FlowJsException> {
    flow_js_utils::iter_union(
        |cx, trace, (t, u)| FlowJs::rec_flow_with_env(cx, env, *trace, t, u),
        (),
        |_, _| (),
        cx,
        trace,
        rep,
        u,
    )
}

fn add_output_prop_polarity_mismatch<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: UseOp,
    lower: PropPolarityMismatchObject<&Type, ALoc>,
    upper: PropPolarityMismatchObject<&Type, ALoc>,
    props: Vec<(Option<Name>, (Polarity, Polarity))>,
    strictness_kind: flow_typing_type::type_::TypeStrictnessKind,
) -> Result<(), FlowJsException> {
    if strictness_kind.is_typescript_loose() {
        Ok(())
    } else if let Ok(props) = Vec1::try_from_vec(props) {
        let error_object = |object| match object {
            PropPolarityMismatchObject::Type(t) => PropPolarityMismatchObject::Type(
                flow_js_utils::type_reference_with_reason_for_error(
                    t,
                    type_util::reason_of_t(t).dupe(),
                ),
            ),
            PropPolarityMismatchObject::Class(class) => PropPolarityMismatchObject::Class(class),
        };
        flow_js_utils::add_output_with_env(
            cx,
            env,
            ErrorMessage::EPropPolarityMismatch(Box::new(EPropPolarityMismatchData {
                lower: error_object(lower),
                upper: error_object(upper),
                props,
                use_op,
            })),
        )?;
        Ok(())
    } else {
        Ok(())
    }
}

pub(crate) fn property_is_missing_from_lookup_terminal(terminal: &Type, name: &Name) -> bool {
    match terminal.deref() {
        ObjProtoT(_) => !flow_js_utils::is_object_prototype_method(name),
        FunProtoT(_) => !flow_js_utils::is_function_prototype(name),
        DefT(_, deref!(NullT)) => true,
        _ => false,
    }
}

fn polarity_error_content(
    propref: &Option<Name>,
    lp: &PropertyType,
    up: &PropertyType,
) -> (Option<Name>, (Polarity, Polarity)) {
    let lpol = property::polarity_of_property_type(lp);
    let upol = property::polarity_of_property_type(up);
    (propref.dupe(), (lpol, upol))
}

fn property_type_for_subtyping(
    prop: &Property,
    strictness_kind: TypeStrictnessKind,
) -> PropertyType {
    match prop.deref() {
        PropertyInner::Method { type_, .. } if strictness_kind.is_typescript_loose() => {
            PropertyType::SyntheticField {
                get_type: Some(properties::method_to_function_for_subtyping(type_)),
                set_type: None,
            }
        }
        _ => property::property_type(prop),
    }
}

fn try_property_method_bivariant<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    lower_property: &Property,
    upper_property: &Property,
) -> Result<bool, FlowJsException> {
    fn is_method(t: &Type) -> bool {
        match t.deref() {
            DefT(_, deref!(FunT(_, ft))) => ft.this_t.1.is_method(),
            DefT(_, deref!(PolyT(poly))) => is_method(&poly.t_out),
            IntersectionT(_, rep) => rep.members_iter().all(is_method),
            _ => false,
        }
    }
    let ut = match upper_property.deref() {
        PropertyInner::Method { type_, .. } => type_,
        PropertyInner::Field(field) => &field.type_,
        _ => return Ok(false),
    };
    let ut = cx.find_resolved(ut).unwrap_or_else(|| ut.dupe());
    let (ut, upper_optional) = match ut.deref() {
        OptionalT { type_, .. } => (
            cx.find_resolved(type_).unwrap_or_else(|| type_.dupe()),
            true,
        ),
        _ => (ut, false),
    };
    if !matches!(upper_property.deref(), PropertyInner::Method { .. }) && !is_method(&ut) {
        return Ok(false);
    }
    let lt = match lower_property.deref() {
        PropertyInner::Field(field) => &field.type_,
        PropertyInner::Method { type_, .. } => type_,
        _ => return Ok(false),
    };
    let lt = cx.find_resolved(lt).unwrap_or_else(|| lt.dupe());
    let lt = match lt.deref() {
        OptionalT { type_, .. } if upper_optional => {
            cx.find_resolved(type_).unwrap_or_else(|| type_.dupe())
        }
        _ => lt,
    };
    try_method_bivariant(cx, env, trace, use_op, &lt, &ut)
}

fn rec_flow_p_inner<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    use_op: UseOp,
    lower_upper_property: Option<(&Property, &Property)>,
    lower_upper_subtyping_obj_ts: Option<(&Type, &Type)>,
    strictness_kind: TypeStrictnessKind,
    report_polarity: bool,
    propref: &PropRef,
    lp: &PropertyType,
    up: &PropertyType,
) -> Result<Vec<(Option<Name>, (Polarity, Polarity))>, FlowJsException> {
    if let Some((lower_property, upper_property)) = lower_upper_property
        && strictness_kind.is_typescript_loose()
        && try_property_method_bivariant(
            cx,
            env,
            trace.unwrap_or_else(DepthTrace::dummy_trace),
            use_op.dupe(),
            lower_property,
            upper_property,
        )?
    {
        return Ok(vec![]);
    }
    let normalized_lp = if strictness_kind.is_typescript_loose() {
        lower_upper_property
            .map(|(lower_property, _)| property_type_for_subtyping(lower_property, strictness_kind))
    } else {
        None
    };
    let lp = normalized_lp.as_ref().unwrap_or(lp);
    match (lp, up) {
        // unification cases
        (
            PropertyType::OrdinaryField {
                type_: lt,
                polarity: Polarity::Neutral,
            },
            PropertyType::OrdinaryField {
                type_: ut,
                polarity: Polarity::Neutral,
            },
        ) if strictness_kind.is_typescript_loose() => {
            // TS treats read-write-on-both-sides property type mismatch in
            // extends/implements/instantiation contexts as covariant, not invariant.
            // Match that by skipping unification and doing a covariant subtype check.
            FlowJs::flow_opt_with_env(cx, env, trace, lt, &UseT::new(UseT(use_op, ut.dupe())))?;
            Ok(vec![])
        }
        (
            PropertyType::OrdinaryField {
                type_: lt,
                polarity: Polarity::Neutral,
            },
            PropertyType::OrdinaryField {
                type_: ut,
                polarity: Polarity::Neutral,
            },
        ) => {
            let unify_cause = match lower_upper_subtyping_obj_ts {
                None => UnifyCause::Uncategorized,
                Some((lower_obj_t, upper_obj_t)) => {
                    let property_name = match propref {
                        PropRef::Named { name, .. } => Some(name.display_smol_str()),
                        PropRef::Computed(_) => None,
                    };
                    UnifyCause::MutableProperty {
                        lower_obj_t: lower_obj_t.dupe(),
                        upper_obj_t: upper_obj_t.dupe(),
                        property_name,
                    }
                }
            };
            FlowJs::unify_opt_with_env(cx, env, trace, use_op, unify_cause, None, lt, ut)?;
            Ok(vec![])
        }
        // directional cases
        (lp, up) => {
            let propref_error = name_of_propref(propref);
            let errs1 = match (
                property::read_t_of_property_type(lp),
                property::read_t_of_property_type(up),
            ) {
                (Some(lt), Some(ut)) => {
                    FlowJs::flow_opt_with_env(
                        cx,
                        env,
                        trace,
                        &lt,
                        &UseT::new(UseT(use_op.dupe(), ut)),
                    )?;
                    vec![]
                }
                (None, Some(_)) if report_polarity => {
                    vec![polarity_error_content(&propref_error, lp, up)]
                }
                _ => vec![],
            };
            let errs2 = match (
                property::write_t_of_property_type(lp, None),
                property::write_t_of_property_type(up, None),
            ) {
                (Some(lt), Some(ut)) => {
                    FlowJs::flow_opt_with_env(
                        cx,
                        env,
                        trace,
                        &ut,
                        &UseT::new(UseT(use_op.dupe(), lt)),
                    )?;
                    vec![]
                }
                (None, Some(_)) if report_polarity => {
                    vec![polarity_error_content(&propref_error, lp, up)]
                }
                _ => vec![],
            };
            let mut result = errs1;
            result.extend(errs2);
            Ok(result)
        }
    }
}

fn index_of_param(params: &[(Option<Name>, Type)], x: &Name) -> Option<usize> {
    params.iter().enumerate().find_map(|(i, p)| match &p.0 {
        Some(x_prime) if x == x_prime => Some(i),
        _ => None,
    })
}

fn func_type_guard_compat<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    grd1: (
        &TypeGuard,
        &[(Option<Name>, Type)],
        bool,
        (&ALoc, &Name),
        Option<&Type>,
    ),
    grd2: (
        &TypeGuard,
        &[(Option<Name>, Type)],
        bool,
        (&ALoc, &Name),
        Option<&Type>,
    ),
) -> Result<(), FlowJsException> {
    let (tg1, params1, impl1, (loc1, x1), t1) = grd1;
    let (tg2, params2, impl2, (loc2, x2), t2) = grd2;
    if impl1 && !impl2 {
        flow_js_utils::add_output_with_env(
            cx,
            env,
            ErrorMessage::ETypeGuardImpliesMismatch {
                use_op: use_op.dupe(),
                lower: flow_js_utils::type_guard_reference_for_error(tg1),
                upper: flow_js_utils::type_guard_reference_for_error(tg2),
            },
        )?;
    }
    let idx1 = index_of_param(params1, x1);
    let idx2 = index_of_param(params2, x2);
    let use_op = VirtualUseOp::Frame(
        Arc::new(VirtualFrameUseOp::TypeGuardCompatibility),
        Arc::new(use_op),
    );
    if idx1 != idx2 {
        flow_js_utils::add_output_with_env(
            cx,
            env,
            ErrorMessage::ETypeGuardIndexMismatch {
                use_op: use_op.dupe(),
                parameters: (
                    TypeGuardParameterData {
                        loc: loc1.dupe(),
                        name: x1.display_smol_str(),
                    },
                    TypeGuardParameterData {
                        loc: loc2.dupe(),
                        name: x2.display_smol_str(),
                    },
                ),
            },
        )?;
    }
    match (t1, t2) {
        (Some(t1), Some(t2)) if impl2 => {
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, t1, t2)
        }
        (Some(t1), Some(t2)) => FlowJs::rec_unify_with_env(
            cx,
            env,
            trace,
            use_op,
            UnifyCause::Uncategorized,
            None,
            t1,
            t2,
        ),
        (None, None) => Ok(()),
        _ => {
            let reference = |t: Option<&Type>, reason: &Reason| match t {
                Some(t) => TypeGuardFuncReference::Type(
                    flow_js_utils::type_reference_with_reason_for_error(t, reason.dupe()),
                ),
                None => TypeGuardFuncReference::TypeGuard(TypeGuardReferenceData {
                    loc: reason.loc().dupe(),
                    kind: TypeGuardReferenceKind::TypeGuard,
                }),
            };
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::ETypeGuardFuncIncompatibility {
                    use_op,
                    lower: reference(t1, &tg1.reason),
                    upper: reference(t2, &tg2.reason),
                },
            )?;
            Ok(())
        }
    }
}

fn funt_to_funt_check_this_contravariant<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    lower: &Type,
    upper: &Type,
    lreason: &Reason,
    ft1: &flow_typing_type::type_::FunType,
    ureason: &Reason,
    ft2: &flow_typing_type::type_::FunType,
    use_op: &UseOp,
) -> Result<(), FlowJsException> {
    let use_op = VirtualUseOp::Frame(
        Arc::new(VirtualFrameUseOp::FunParam(Box::new(FunParamData {
            n: 0,
            name: Some(FlowSmolStr::new_inline("this")),
            lower: lreason.dupe(),
            upper: ureason.dupe(),
            lower_t: flow_js_utils::type_or_type_desc_for_error(lower),
            upper_t: flow_js_utils::type_or_type_desc_for_error(upper),
        }))),
        Arc::new(use_op.dupe()),
    );
    let (this_param1, this_status_1) = &ft1.this_t;
    let (this_param2, this_status_2) = &ft2.this_t;
    match (this_status_1, this_status_2) {
        // Method receivers are deliberately not compared: `class C implements I`
        // needs `C.m`'s `this: C` to be compatible with `I.m`'s `this: I`, which
        // a contravariant check would reject.
        // It would not compromise safety since after unbinding method,
        // ThisMethod is turned into ThisFunction.
        (lower, upper) if lower.is_method() && upper.is_method() => {}
        _ => {
            FlowJs::rec_flow_with_env(
                cx,
                env,
                trace,
                this_param2,
                &UseT::new(UseT(use_op, this_param1.dupe())),
            )?;
        }
    }
    Ok(())
}

fn funt_to_funt_check_this_ignore(_use_op: &UseOp) -> Result<(), FlowJsException> {
    Ok(())
}

// Shared FunT ~> FunT subtyping driver. The effect, return, and type-guard
// checks are identical across callers; the differences are how `this` and
// regular params are flowed. `check_params` receives the FunCompatibility-
// rewritten use_op and is responsible for the param-flow step (e.g.
// `multiflow_subtype` for the standard path, a bivariant pairwise walk for
// the TS-mode path).
fn funt_to_funt_check<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    lower: &Type,
    upper: &Type,
    lreason: &Reason,
    ft1: &flow_typing_type::type_::FunType,
    ureason: &Reason,
    ft2: &flow_typing_type::type_::FunType,
    check_this: &dyn Fn(&mut Context<'cx>, &UseOp) -> Result<(), FlowJsException>,
    check_params: &dyn Fn(&mut Context<'cx>, &UseOp) -> Result<(), FlowJsException>,
) -> Result<(), FlowJsException> {
    let inner_use_op = if let VirtualUseOp::Frame(
        deref!(
            VirtualFrameUseOp::PropertyCompatibility(box PropertyCompatibilityData {
                prop: Some(name),
                ..
            })
        ),
        inner,
    ) = &use_op
        && name.matches_str("$call")
    {
        // The $call PropertyCompatibility is redundant when we have a
        // FunCompatibility use_op.
        inner.deref().dupe()
    } else {
        use_op
    };
    let use_op = VirtualUseOp::Frame(
        Arc::new(VirtualFrameUseOp::FunCompatibility {
            lower: lreason.dupe(),
            upper: ureason.dupe(),
        }),
        Arc::new(inner_use_op),
    );

    check_this(cx, &use_op)?;
    check_params(cx, &use_op)?;
    match (&ft1.effect_, &ft2.effect_) {
        (ReactEffectType::AnyEffect, _)
        | (_, ReactEffectType::AnyEffect)
        | (ReactEffectType::ArbitraryEffect, ReactEffectType::ArbitraryEffect)
        | (ReactEffectType::HookDecl(_) | ReactEffectType::HookAnnot, ReactEffectType::HookAnnot) =>
            {}
        (ReactEffectType::HookDecl(a), ReactEffectType::HookDecl(b)) if a == b => {}
        (
            ReactEffectType::HookDecl(_) | ReactEffectType::HookAnnot,
            ReactEffectType::ArbitraryEffect,
        ) => {
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::EHookIncompatible(Box::new(EHookIncompatibleData {
                    use_op: use_op.dupe(),
                    lower: flow_js_utils::type_reference_with_reason_for_error(
                        lower,
                        lreason.dupe(),
                    ),
                    upper: flow_js_utils::type_reference_with_reason_for_error(
                        upper,
                        ureason.dupe(),
                    ),
                    lower_is_hook: true,
                    hook_is_annot: ft1.effect_ == ReactEffectType::HookAnnot,
                })),
            )?;
        }
        (
            ReactEffectType::ArbitraryEffect,
            ReactEffectType::HookDecl(_) | ReactEffectType::HookAnnot,
        ) => {
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::EHookIncompatible(Box::new(EHookIncompatibleData {
                    use_op: use_op.dupe(),
                    lower: flow_js_utils::type_reference_with_reason_for_error(
                        lower,
                        lreason.dupe(),
                    ),
                    upper: flow_js_utils::type_reference_with_reason_for_error(
                        upper,
                        ureason.dupe(),
                    ),
                    lower_is_hook: false,
                    hook_is_annot: ft2.effect_ == ReactEffectType::HookAnnot,
                })),
            )?;
        }
        (
            ReactEffectType::HookDecl(_) | ReactEffectType::HookAnnot,
            ReactEffectType::HookDecl(_),
        ) => {
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::EHookUniqueIncompatible(Box::new(EHookUniqueIncompatibleData {
                    use_op: use_op.dupe(),
                    lower: flow_js_utils::type_reference_with_reason_for_error(
                        lower,
                        lreason.dupe(),
                    ),
                    upper: flow_js_utils::type_reference_with_reason_for_error(
                        upper,
                        ureason.dupe(),
                    ),
                })),
            )?;
        }
    }
    let is_asserts = |guard: &Option<flow_typing_type::type_::TypeGuard>| {
        guard.as_deref().is_some_and(|guard| guard.is_asserts())
    };
    if ft1
        .strictness_kind
        .join(ft2.strictness_kind)
        .is_typescript_loose()
        && is_asserts(&ft2.type_guard)
    {
        return Ok(());
    }
    let assertion_kind_mismatch =
        is_asserts(&ft1.type_guard) != is_asserts(&ft2.type_guard) && ft2.type_guard.is_some();
    if !assertion_kind_mismatch {
        let ret_use_op = VirtualUseOp::Frame(
            Arc::new(VirtualFrameUseOp::FunReturn(Box::new(FunReturnData {
                lower: type_util::reason_of_t(&ft1.return_t).clone(),
                upper: type_util::reason_of_t(&ft2.return_t).clone(),
                lower_t: flow_js_utils::type_or_type_desc_for_error(&ft1.return_t),
                upper_t: flow_js_utils::type_or_type_desc_for_error(&ft2.return_t),
            }))),
            Arc::new(use_op.dupe()),
        );
        FlowJs::rec_flow_with_env(
            cx,
            env,
            trace,
            &ft1.return_t,
            &UseT::new(UseT(ret_use_op, ft2.return_t.dupe())),
        )?;
    }
    match (&ft1.type_guard, &ft2.type_guard) {
        (None, Some(_)) => {
            // Non-predicate functions are incompatible with predicate ones
            // TODO: somehow the original flow needs to be propagated as well
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::ETypeGuardFuncIncompatibility {
                    use_op: use_op.dupe(),
                    lower: TypeGuardFuncReference::Type(
                        flow_js_utils::type_reference_with_reason_for_error(lower, lreason.dupe()),
                    ),
                    upper: TypeGuardFuncReference::Type(
                        flow_js_utils::type_reference_with_reason_for_error(upper, ureason.dupe()),
                    ),
                },
            )?;
        }
        (Some(tg1), Some(tg2)) => {
            if tg1.is_asserts() != tg2.is_asserts() {
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::ETypeGuardFuncIncompatibility {
                        use_op: use_op.dupe(),
                        lower: TypeGuardFuncReference::Type(
                            flow_js_utils::type_reference_with_reason_for_error(
                                lower,
                                lreason.dupe(),
                            ),
                        ),
                        upper: TypeGuardFuncReference::Type(
                            flow_js_utils::type_reference_with_reason_for_error(
                                upper,
                                ureason.dupe(),
                            ),
                        ),
                    },
                )?;
                return Ok(());
            }
            let params1: Vec<(Option<Name>, Type)> = ft1
                .params
                .iter()
                .map(|p| (p.0.as_ref().map(|s| Name::new(s.dupe())), p.1.dupe()))
                .collect();
            let params2: Vec<(Option<Name>, Type)> = ft2
                .params
                .iter()
                .map(|p| (p.0.as_ref().map(|s| Name::new(s.dupe())), p.1.dupe()))
                .collect();
            let name1 = Name::new(tg1.param_name.1.dupe());
            let name2 = Name::new(tg2.param_name.1.dupe());
            func_type_guard_compat(
                cx,
                env,
                trace,
                use_op.dupe(),
                (
                    tg1,
                    &params1,
                    tg1.one_sided(),
                    (&tg1.param_name.0, &name1),
                    tg1.type_guard.as_ref(),
                ),
                (
                    tg2,
                    &params2,
                    tg2.one_sided(),
                    (&tg2.param_name.0, &name2),
                    tg2.type_guard.as_ref(),
                ),
            )?;
        }
        (Some(_), None) | (None, None) => {}
    }
    Ok(())
}

// TS-mode bivariant param flow for method-syntax properties: try the
// contravariant direction (target -> source, the standard subtyping
// direction) first via speculation; on failure, fall back to the covariant
// direction (source -> target). Mirrors TS's bivariance for methods, which
// accepts assignability whenever either direction holds.
//
// If the method parameter is itself a callback function with an explicit
// `this` parameter, do not recursively reject on that callback `this`.
// TypeScript's DOM libraries rely on this when a subinterface narrows
// listener context from `GlobalEventHandlers` to `Document`, `HTMLElement`,
// and similar subtypes. The regular callback argument and return checks still
// run in the chosen direction.
fn bivariant_param_flow<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    source: &Type,
    target: &Type,
) -> Result<(), FlowJsException> {
    fn flow_param<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: &UseOp,
        lower: &Type,
        upper: &Type,
    ) -> Result<(), FlowJsException> {
        match (cx.find_resolved(lower), cx.find_resolved(upper)) {
            (Some(lower_resolved), Some(upper_resolved)) => {
                match (lower_resolved.deref(), upper_resolved.deref()) {
                    (DefT(lreason, deref!(FunT(_, ft1))), DefT(ureason, deref!(FunT(_, ft2)))) => {
                        funt_to_funt_check(
                            cx,
                            env,
                            trace,
                            use_op.dupe(),
                            &lower_resolved,
                            &upper_resolved,
                            lreason,
                            ft1,
                            ureason,
                            ft2,
                            &|_, use_op| funt_to_funt_check_this_ignore(use_op),
                            &|cx, use_op| {
                                funt_to_funt_check_params_contravariant(
                                    cx, env, trace, ureason, ft1, ft2, use_op,
                                )
                            },
                        )
                    }
                    _ => FlowJs::rec_flow_with_env(
                        cx,
                        env,
                        trace,
                        lower,
                        &UseT::new(UseT(use_op.dupe(), upper.dupe())),
                    ),
                }
            }
            _ => FlowJs::rec_flow_with_env(
                cx,
                env,
                trace,
                lower,
                &UseT::new(UseT(use_op.dupe(), upper.dupe())),
            ),
        }
    }

    let source_spec = source.dupe();
    let target_spec = target.dupe();
    let use_op_spec = use_op.dupe();
    match speculation_kit::try_singleton_custom_throw_on_failure(
        cx,
        env,
        Box::new(move |cx, env: &FlowJsEnv| {
            flow_param(cx, env, trace, &use_op_spec, &target_spec, &source_spec)
        }),
    ) {
        Ok(()) => Ok(()),
        Err(FlowJsException::SpeculationSingletonError) => {
            flow_param(cx, env, trace, &use_op, source, target)
        }
        Err(other) => Err(other),
    }
}

// The standard (contravariant) param-flow check: builds an arglist from
// ft2's params + rest, then routes it through `multiflow_subtype` so it
// flows into ft1's params.
fn funt_to_funt_check_params_contravariant<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    ureason: &Reason,
    ft1: &flow_typing_type::type_::FunType,
    ft2: &flow_typing_type::type_::FunType,
    use_op: &UseOp,
) -> Result<(), FlowJsException> {
    let mut args: Vec<CallArg> = ft2
        .params
        .iter()
        .map(|p| CallArg::arg(p.1.dupe()))
        .collect();
    if let Some(FunRestParam(_, _, rest)) = &ft2.rest_param {
        args.push(CallArg::spread_arg(rest.dupe()));
    }
    FlowJs::multiflow_subtype_with_env(cx, env, trace, use_op.dupe(), ureason, &args, ft1)
}

// TS-mode bivariant param-flow check used for method-syntax properties: a
// pairwise walk over ft1.params/ft2.params using `bivariant_param_flow`,
// plus arity enforcement. Trailing ft1 params not provided by ft2 are
// filled with VoidT so optional params (whose type unions in void) accept
// and required ones produce a `FunMissingArg` error -- the same mechanism
// `multiflow_full` uses. When ft2 has more fixed params than ft1, the
// extras flow into `ft1.rest_param` (if present) so callers of the upper
// signature cannot smuggle the wrong type into the lower's rest -- this
// mirrors what `multiflow_subtype` does in the standard path.
//
// Caveat: `rest_param`'s type is the whole array (e.g. `Array<string>`),
// not the element type, so flowing a single param's type bivariantly
// against it is a single-vs-array mismatch and will always fail. That
// means the rest-vs-extra-fixed cases (both the symmetric one above and
// the pre-existing one below) are correctly *sound* (incompatible types
// produce an error) but may *over-reject* compatible cases like
// `(x, ...ys: number[]) <: (x, y: number)`. multiflow_subtype handles
// these by building a tuple literal and flowing it; replicating that
// here was deferred since method-syntax with rest is uncommon.
fn funt_to_funt_check_params_bivariant<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    lower: &Type,
    upper: &Type,
    lreason: &Reason,
    ureason: &Reason,
    ft1: &flow_typing_type::type_::FunType,
    ft2: &flow_typing_type::type_::FunType,
    use_op: &UseOp,
) -> Result<(), FlowJsException> {
    let lower_t = flow_js_utils::type_or_type_desc_for_error(lower);
    let upper_t = flow_js_utils::type_or_type_desc_for_error(upper);
    let mut n: i32 = 1;
    let mut params1: &[flow_typing_type::type_::FunParam] = &ft1.params;
    let mut params2: &[flow_typing_type::type_::FunParam] = &ft2.params;
    loop {
        match (params1.split_first(), params2.split_first()) {
            (None, None) => break,
            (None, Some((p2, rest2))) => {
                let t2 = &p2.1;
                match &ft1.rest_param {
                    Some(FunRestParam(_, _, t1_rest)) => {
                        let inner_use_op = VirtualUseOp::Frame(
                            Arc::new(VirtualFrameUseOp::FunRestParam {
                                lower: lreason.dupe(),
                                upper: ureason.dupe(),
                            }),
                            Arc::new(use_op.dupe()),
                        );
                        bivariant_param_flow(cx, env, trace, inner_use_op, t1_rest, t2)?;
                        n += 1;
                        params2 = rest2;
                    }
                    None => {
                        // ft1 has no slot for the extra; JS drops the arg at runtime, sound.
                        break;
                    }
                }
            }
            (Some((p1, rest1)), None) => {
                let (name1, t1) = (&p1.0, &p1.1);
                match &ft2.rest_param {
                    Some(FunRestParam(_, _, rest_t)) => {
                        let inner_use_op = VirtualUseOp::Frame(
                            Arc::new(VirtualFrameUseOp::FunParam(Box::new(FunParamData {
                                n,
                                name: name1.dupe(),
                                lower: lreason.dupe(),
                                upper: ureason.dupe(),
                                lower_t: lower_t.clone(),
                                upper_t: upper_t.clone(),
                            }))),
                            Arc::new(use_op.dupe()),
                        );
                        bivariant_param_flow(cx, env, trace, inner_use_op, t1, rest_t)?;
                        n += 1;
                        params1 = rest1;
                    }
                    None => {
                        let inner_use_op = VirtualUseOp::Frame(
                            Arc::new(VirtualFrameUseOp::FunMissingArg(Box::new(
                                flow_typing_type::type_::FunMissingArgData {
                                    n,
                                    op: ureason.dupe(),
                                    def: lreason.dupe(),
                                    def_t: lower_t.clone(),
                                },
                            ))),
                            Arc::new(use_op.dupe()),
                        );
                        let void = Type::new(DefT(ureason.dupe(), DefT::new(VoidT)));
                        FlowJs::rec_flow_with_env(
                            cx,
                            env,
                            trace,
                            &void,
                            &UseT::new(UseT(inner_use_op, t1.dupe())),
                        )?;
                        n += 1;
                        params1 = rest1;
                    }
                }
            }
            (Some((p1, rest1)), Some((p2, rest2))) => {
                let t1 = &p1.1;
                let (name2, t2) = (&p2.0, &p2.1);
                let inner_use_op = VirtualUseOp::Frame(
                    Arc::new(VirtualFrameUseOp::FunParam(Box::new(FunParamData {
                        n,
                        name: name2.dupe(),
                        lower: lreason.dupe(),
                        upper: ureason.dupe(),
                        lower_t: lower_t.clone(),
                        upper_t: upper_t.clone(),
                    }))),
                    Arc::new(use_op.dupe()),
                );
                bivariant_param_flow(cx, env, trace, inner_use_op, t1, t2)?;
                n += 1;
                params1 = rest1;
                params2 = rest2;
            }
        }
    }
    match (&ft1.rest_param, &ft2.rest_param) {
        (Some(FunRestParam(_, _, t1_rest)), Some(FunRestParam(_, _, t2_rest))) => {
            let use_op = VirtualUseOp::Frame(
                Arc::new(VirtualFrameUseOp::FunRestParam {
                    lower: lreason.dupe(),
                    upper: ureason.dupe(),
                }),
                Arc::new(use_op.dupe()),
            );
            bivariant_param_flow(cx, env, trace, use_op, t1_rest, t2_rest)
        }
        _ => Ok(()),
    }
}

fn funt_to_funt_method_bivariant<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    lower: &Type,
    upper: &Type,
    lreason: &Reason,
    ft1: &flow_typing_type::type_::FunType,
    ureason: &Reason,
    ft2: &flow_typing_type::type_::FunType,
) -> Result<(), FlowJsException> {
    funt_to_funt_check(
        cx,
        env,
        trace,
        use_op,
        lower,
        upper,
        lreason,
        ft1,
        ureason,
        ft2,
        &|cx, use_op| {
            funt_to_funt_check_this_contravariant(
                cx, env, trace, lower, upper, lreason, ft1, ureason, ft2, use_op,
            )
        },
        &|cx, use_op| {
            funt_to_funt_check_params_bivariant(
                cx, env, trace, lower, upper, lreason, ureason, ft1, ft2, use_op,
            )
        },
    )
}

// Returns true if (lt, ut) is a method-shape pair (FunT/FunT, compatible
// PolyT/PolyT, or overload intersections of those shapes) and the bivariant
// subtyping was applied. Returns false to signal the caller to fall back to
// the standard subtyping path. The PolyT case mirrors the `PolyT ~> PolyT`
// instantiation logic (see flow_obj_to_obj callers below) so generic methods
// get the same TS-style bivariance treatment as monomorphic ones.
//
// For overloaded methods, preserve Flow's existing intersection semantics:
// every target overload must be covered, while a source overload set may
// satisfy a target signature with any compatible overload. The only thing we
// swap is the per-signature compatibility predicate, from contravariant params
// to TS-style bivariant method params.
fn try_method_bivariant<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    lt: &Type,
    ut: &Type,
) -> Result<bool, FlowJsException> {
    fn direct_method_bivariant<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: UseOp,
        lt: &Type,
        ut: &Type,
    ) -> Result<bool, FlowJsException> {
        match (lt.deref(), ut.deref()) {
            (DefT(lreason_f, deref!(FunT(_, ft1))), DefT(ureason_f, deref!(FunT(_, ft2)))) => {
                funt_to_funt_method_bivariant(
                    cx, env, trace, use_op, lt, ut, lreason_f, ft1, ureason_f, ft2,
                )?;
                Ok(true)
            }
            #[rustfmt::skip]
            (
                DefT(_, deref!(PolyT(box PolyTData {
                    tparams: ps1,
                    t_out: inner1 @ deref!(DefT(_, deref!(FunT(_, _)))),
                    ..
                }))),
                DefT(_, deref!(PolyT(box PolyTData {
                    tparams: ps2,
                    t_out: inner2 @ deref!(DefT(_, deref!(FunT(_, _)))),
                    ..
                }))),
            ) if ps1.len() == ps2.len() => {
                // Like TypeScript, compare the methods at the type arguments a caller of the
                // upper method passes, which are within its bounds, rather than within the
                // bounds of the lower method.
                let (map2, generics) = flow_js_utils::mk_tparams(cx, ps2);
                let map1 = ps1.iter().zip(generics).fold(
                    flow_data_structure_wrapper::ord_map::FlowOrdMap::new(),
                    |mut map, (param, generic)| {
                        map.insert(param.name.dupe(), generic);
                        map
                    },
                );
                for (p1, p2) in ps1.iter().zip(ps2.iter()) {
                    let bound1 = flow_typing_flow_common::type_subst::subst(
                        cx,
                        Some(use_op.dupe()),
                        true,
                        false,
                        flow_typing_flow_common::type_subst::Purpose::Normal,
                        &map1,
                        p1.bound.dupe(),
                    );
                    let bound2 = flow_typing_flow_common::type_subst::subst(
                        cx,
                        Some(use_op.dupe()),
                        true,
                        false,
                        flow_typing_flow_common::type_subst::Purpose::Normal,
                        &map2,
                        p2.bound.dupe(),
                    );
                    FlowJs::rec_flow_with_env(
                        cx,
                        env,
                        trace,
                        &bound2,
                        &UseT::new(UseT(use_op.dupe(), bound1.dupe())),
                    )?;
                    if p1.is_const != p2.is_const {
                        flow_js_utils::add_output_with_env(
                            cx,
                            env,
                            ErrorMessage::ETypeParamConstIncompatibility(Box::new(
                                ETypeParamConstIncompatibilityData {
                                    use_op: use_op.dupe(),
                                    lower: NamedReferenceData {
                                        loc: p1.reason.loc().dupe(),
                                        name: p1.name.string_of_subst_name().dupe(),
                                    },
                                    upper: NamedReferenceData {
                                        loc: p2.reason.loc().dupe(),
                                        name: p2.name.string_of_subst_name().dupe(),
                                    },
                                },
                            )),
                        )?;
                    }
                }
                // Substitution preserves the outer DefT/FunT constructor (the pattern
                // guard above requires t_out is FunT, and Type_subst only rewrites
                // tparam references inside), so the assertive match is exhaustive in
                // practice. If it ever falls through, report an internal error rather
                // than crashing — matches the OCaml behavior of reporting an `EInternal`
                // `MethodBivariantInvariant` error.
                let inner1_subst = flow_typing_flow_common::type_subst::subst(
                    cx,
                    Some(use_op.dupe()),
                    true,
                    false,
                    flow_typing_flow_common::type_subst::Purpose::Normal,
                    &map1,
                    inner1.dupe(),
                );
                let inner2_subst = flow_typing_flow_common::type_subst::subst(
                    cx,
                    Some(use_op.dupe()),
                    true,
                    false,
                    flow_typing_flow_common::type_subst::Purpose::Normal,
                    &map2,
                    inner2.dupe(),
                );
                match (inner1_subst.deref(), inner2_subst.deref()) {
                    (DefT(lreason_f, deref!(FunT(_, ft1))), DefT(ureason_f, deref!(FunT(_, ft2)))) => {
                        funt_to_funt_method_bivariant(
                            cx,
                            env,
                            trace,
                            use_op,
                            &inner1_subst,
                            &inner2_subst,
                            lreason_f,
                            ft1,
                            ureason_f,
                            ft2,
                        )?;
                    }
                    _ => {
                        flow_js_utils::add_output_with_env(
                            cx,
                            env,
                            ErrorMessage::EInternal(Box::new((
                                type_util::loc_of_t(lt).dupe(),
                                InternalError::MethodBivariantInvariant(FlowSmolStr::from(
                                    "PolyT with FunT inner produced non-FunT after subst",
                                )),
                            ))),
                        )?;
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn can_try_method_bivariant(lt: &Type, ut: &Type) -> bool {
        match (lt.deref(), ut.deref()) {
            (_, IntersectionT(_, rep)) => rep
                .members_iter()
                .all(|ut| can_try_method_bivariant(lt, ut)),
            (IntersectionT(_, rep), _) => rep
                .members_iter()
                .any(|lt| can_try_method_bivariant(lt, ut)),
            (DefT(_, deref!(FunT(_, _))), DefT(_, deref!(FunT(_, _)))) => true,
            #[rustfmt::skip]
            (
                DefT(_, deref!(PolyT(box PolyTData {
                    tparams: ps1,
                    t_out: deref!(DefT(_, deref!(FunT(_, _)))),
                    ..
                }))),
                DefT(_, deref!(PolyT(box PolyTData {
                    tparams: ps2,
                    t_out: deref!(DefT(_, deref!(FunT(_, _)))),
                    ..
                }))),
            ) if ps1.len() == ps2.len() => true,
            _ => false,
        }
    }

    fn apply_method_bivariant<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: UseOp,
        lt: &Type,
        ut: &Type,
    ) -> Result<(), FlowJsException> {
        match (lt.deref(), ut.deref()) {
            (_, IntersectionT(_, rep)) => {
                for ut in rep.members_iter() {
                    apply_method_bivariant(cx, env, trace, use_op.dupe(), lt, ut)?;
                }
                Ok(())
            }
            (IntersectionT(_, rep), _) => {
                let mut cases: Vec<
                    Box<dyn FnOnce(&mut Context<'cx>, &FlowJsEnv) -> Result<(), FlowJsException>>,
                > = vec![];
                for lt in rep
                    .members_iter()
                    .filter(|lt| can_try_method_bivariant(lt, ut))
                {
                    let lt = lt.dupe();
                    let ut = ut.dupe();
                    let use_op = use_op.dupe();
                    cases.push(Box::new(move |cx: &mut Context<'cx>, env: &FlowJsEnv| {
                        apply_method_bivariant(cx, env, trace, use_op, &lt, &ut)
                    }));
                }
                let use_t = UseT::new(UseT(use_op.dupe(), ut.dupe()));
                speculation_kit::try_custom(
                    cx,
                    env,
                    Some(use_op),
                    Some(use_t),
                    None,
                    type_util::loc_of_t(lt).dupe(),
                    cases,
                )
            }
            _ => direct_method_bivariant(cx, env, trace, use_op, lt, ut).map(|_| ()),
        }
    }

    if can_try_method_bivariant(lt, ut) {
        apply_method_bivariant(cx, env, trace, use_op, lt, ut)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn recover_missing_props<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: &UseOp,
    reason_upper: &Reason,
    strictness_kind: TypeStrictnessKind,
    missing_props: &[Property],
) -> Result<(), FlowJsException> {
    for up in missing_props {
        let any = any_t::error_of_kind(AnyErrorKind::UnresolvedName, reason_upper.dupe());
        match property::property_type(up) {
            PropertyType::OrdinaryField {
                type_: ut,
                polarity: Polarity::Neutral,
            } if strictness_kind.is_typescript_loose() => {
                // This no-op flow intentionally avoids the invariant constraint used for Flow
                // object subtyping while preserving the missing-property recovery path.
                FlowJs::flow_opt_with_env(
                    cx,
                    env,
                    Some(trace),
                    &any,
                    &UseT::new(UseT(use_op.dupe(), ut)),
                )?;
            }
            PropertyType::OrdinaryField {
                type_: ut,
                polarity: Polarity::Neutral,
            } => {
                FlowJs::unify_opt_with_env(
                    cx,
                    env,
                    Some(trace),
                    use_op.dupe(),
                    UnifyCause::Uncategorized,
                    None,
                    &any,
                    &ut,
                )?;
            }
            up_pt => {
                if let Some(ut) = property::read_t_of_property_type(&up_pt) {
                    FlowJs::flow_opt_with_env(
                        cx,
                        env,
                        Some(trace),
                        &any,
                        &UseT::new(UseT(use_op.dupe(), ut)),
                    )?;
                }
                if let Some(ut) = property::write_t_of_property_type(&up_pt, None) {
                    FlowJs::flow_opt_with_env(
                        cx,
                        env,
                        Some(trace),
                        &ut,
                        &UseT::new(UseT(use_op.dupe(), any.dupe())),
                    )?;
                }
            }
        }
    }
    Ok(())
}

/// Report the upper properties a subtyping lookup found nowhere in the lower
/// type or its prototype chain. Properties that look like a typo of an existing
/// one get their own error carrying the suggestion; the rest are named together
/// in a single error.
pub(crate) fn add_output_missing_props_from_lookup<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    data: &LookupPropsForSubtypingData,
    ids: Option<&properties::Set>,
) -> Result<(), FlowJsException> {
    let ids = ids.map(|ids| ids.iter().duped().collect::<Vec<_>>());
    let mut missing = vec![];
    let add_output_one = |cx: &mut Context<'cx>,
                          prop_name: Option<Name>,
                          suggestion: Option<FlowSmolStr>|
     -> Result<(), FlowJsException> {
        flow_js_utils::add_output_with_env(
            cx,
            env,
            ErrorMessage::EPropNotFoundInSubtyping(Box::new(EPropNotFoundInSubtypingData {
                lower: flow_js_utils::type_reference_with_reason_for_error(
                    &data.lower,
                    type_util::reason_of_t(&data.lower).dupe(),
                ),
                upper: flow_js_utils::type_reference_with_reason_for_error(
                    &data.upper,
                    type_util::reason_of_t(&data.upper).dupe(),
                ),
                prop_name,
                use_op: data.use_op.dupe(),
                suggestion,
            })),
        )
    };
    for (propref, _) in data.props.iter() {
        let name = name_of_propref(propref);
        let suggestion = name
            .as_ref()
            .zip(ids.as_ref())
            .and_then(|(name, ids)| prop_typo_suggestion_for_name(cx, ids, name));
        match (&name, suggestion) {
            (Some(name), None) => missing.push(name.dupe()),
            (_, suggestion) => add_output_one(cx, name.dupe(), suggestion)?,
        }
    }
    match Vec1::try_from_vec(missing) {
        Err(_) => Ok(()),
        // A lone missing property reads better as its own error, and that is all
        // an unbatched lookup ever has.
        Ok(prop_names) if prop_names.len() == 1 => {
            add_output_one(cx, Some(prop_names.first().dupe()), None)
        }
        Ok(prop_names) => flow_js_utils::add_output_with_env(
            cx,
            env,
            ErrorMessage::EPropsNotFoundInSubtyping(Box::new(EPropsNotFoundInSubtypingData {
                prop_names,
                lower: flow_js_utils::type_reference_with_reason_for_error(
                    &data.lower,
                    type_util::reason_of_t(&data.lower).dupe(),
                ),
                upper: flow_js_utils::type_reference_with_reason_for_error(
                    &data.upper,
                    type_util::reason_of_t(&data.upper).dupe(),
                ),
                use_op: data.use_op.dupe(),
            })),
        ),
    }
}

pub(super) struct PropsToIndexerContext<'a> {
    pub(super) env: &'a FlowJsEnv,
    pub(super) trace: DepthTrace,
    pub(super) use_op: UseOp,
    pub(super) lower: Type,
    pub(super) upper: Type,
    pub(super) strictness_kind: TypeStrictnessKind,
    pub(super) lit: bool,
    pub(super) lower_upper_subtyping_obj_ts: Option<(Type, Type)>,
}

impl PropsToIndexerContext<'_> {
    pub(super) fn flow_props_to_indexer<'cx>(
        &self,
        cx: &mut Context<'cx>,
        lower_props: &properties::PropertiesMap,
        upper_props: &[&properties::PropertiesMap],
        upper_dict: &DictType,
    ) -> Result<(), FlowJsException> {
        let env = self.env;
        let trace = self.trace;
        let use_op = &self.use_op;
        let lreason = type_util::reason_of_t(&self.lower);
        let ureason = type_util::reason_of_t(&self.upper);
        let strictness_kind = self.strictness_kind;
        let lit = self.lit;
        let DictType {
            key,
            value,
            dict_polarity,
            ..
        } = upper_dict;
        let flow_prop_to_indexer =
            |cx: &mut Context<'cx>, lp: &Property, name: &Name| -> Result<(), FlowJsException> {
                let use_op = VirtualUseOp::Frame(
                    Arc::new(VirtualFrameUseOp::PropertyCompatibility(Box::new(
                        PropertyCompatibilityData {
                            prop: Some(name.dupe()),
                            lower: lreason.dupe(),
                            upper: ureason.dupe(),
                            lower_t: flow_js_utils::type_or_type_desc_for_error(&self.lower),
                            upper_t: flow_js_utils::type_or_type_desc_for_error(&self.upper),
                        },
                    ))),
                    Arc::new(use_op.dupe()),
                );
                let lp_type = match lp.deref() {
                    PropertyInner::Field(box FieldData {
                        type_: deref!(OptionalT { type_: lt, .. }),
                        polarity,
                        ..
                    }) => PropertyType::OrdinaryField {
                        type_: lt.dupe(),
                        polarity: *polarity,
                    },
                    _ => property_type_for_subtyping(lp, strictness_kind),
                };
                let up_type = PropertyType::OrdinaryField {
                    type_: value.dupe(),
                    polarity: *dict_polarity,
                };
                if lit {
                    match (
                        property::read_t_of_property_type(&lp_type),
                        property::read_t_of_property_type(&up_type),
                    ) {
                        (Some(lt), Some(ut)) => {
                            FlowJs::rec_flow_with_env(
                                cx,
                                env,
                                trace,
                                &lt,
                                &UseT::new(UseT(use_op.dupe(), ut)),
                            )?;
                        }
                        _ => {}
                    }
                } else {
                    let propref = type_util::mk_named_prop(
                        lreason
                            .dupe()
                            .replace_desc(VirtualReasonDesc::RProperty(Some(name.dupe()))),
                        false,
                        name.dupe(),
                    );
                    let lower_upper_subtyping_obj_ts = self
                        .lower_upper_subtyping_obj_ts
                        .as_ref()
                        .map(|(lower, upper)| (lower, upper));
                    let errs = rec_flow_p_inner(
                        cx,
                        env,
                        Some(trace),
                        use_op.dupe(),
                        None,
                        lower_upper_subtyping_obj_ts,
                        strictness_kind,
                        true,
                        &propref,
                        &lp_type,
                        &up_type,
                    )?;
                    add_output_prop_polarity_mismatch(
                        cx,
                        env,
                        use_op,
                        PropPolarityMismatchObject::Type(&self.lower),
                        PropPolarityMismatchObject::Type(&self.upper),
                        errs,
                        strictness_kind,
                    )?;
                }
                Ok(())
            };
        if !env.in_implicit_instantiation() {
            for (name, lp) in lower_props.iter() {
                if upper_props.iter().any(|props| props.contains_key(name)) {
                    continue;
                }
                let key_type = flow_js_utils::type_of_key_name_with_env(env, name.dupe(), lreason);
                if FlowJs::speculative_subtyping_succeeds_with_flow_errors(cx, env, &key_type, key)?
                {
                    flow_prop_to_indexer(cx, lp, name)?;
                } else {
                    // Without object types (`inst_structural_subtype`), the caller only passes
                    // props whose key already passed this check.
                    let Some((lower_obj_t, upper_obj_t)) = &self.lower_upper_subtyping_obj_ts
                    else {
                        continue;
                    };
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EIndexerCheckFailed(Box::new(EIndexerCheckFailedData {
                            prop_name: name.dupe(),
                            lower: flow_js_utils::type_reference_with_reason_for_error(
                                upper_obj_t,
                                ureason.dupe(),
                            ),
                            upper: flow_js_utils::type_reference_with_reason_for_error(
                                lower_obj_t,
                                lreason.dupe(),
                            ),
                            indexer_loc: type_util::ref_loc_of_t(key).dupe(),
                            indexer_desc: flow_js_utils::type_or_type_desc_for_error(key),
                            use_op: use_op.dupe(),
                        })),
                    )?;
                }
            }
        } else {
            let mut keys_list = Vec::new();
            for (name, lp) in lower_props.iter() {
                if upper_props.iter().any(|props| props.contains_key(name)) {
                    continue;
                }
                flow_prop_to_indexer(cx, lp, name)?;
                keys_list.push(flow_js_utils::type_of_key_name_with_env(
                    env,
                    name.dupe(),
                    lreason,
                ));
            }
            let keys = type_util::union_of_ts(lreason.dupe(), keys_list, None);
            FlowJs::rec_flow_with_env(
                cx,
                env,
                trace,
                &keys,
                &UseT::new(UseT(
                    VirtualUseOp::Frame(
                        Arc::new(VirtualFrameUseOp::IndexerKeyCompatibility(Box::new(
                            IndexerKeyCompatibilityData {
                                lower: lreason.dupe(),
                                upper: ureason.dupe(),
                                lower_t: flow_js_utils::type_or_type_desc_for_error(&self.lower),
                                upper_t: flow_js_utils::type_or_type_desc_for_error(&self.upper),
                            },
                        ))),
                        Arc::new(use_op.dupe()),
                    ),
                    key.dupe(),
                )),
            )?;
        }
        Ok(())
    }
}

fn flow_obj_to_obj<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    (l, u): (&Type, &Type),
    lreason: &Reason,
    l_obj: &Rc<ObjType>,
    ureason: &Reason,
    u_obj: &Rc<ObjType>,
) -> Result<(), FlowJsException> {
    let ObjType {
        flags: lflags,
        call_t: lcall,
        props_tmap: lflds,
        proto_t: lproto,
        reachable_targs: _,
        strictness_kind: lstrictness_kind,
        literal: _,
    } = &**l_obj;
    let ObjType {
        flags: rflags,
        call_t: ucall,
        props_tmap: uflds,
        proto_t: uproto,
        reachable_targs: _,
        strictness_kind: ustrictness_kind,
        literal: _,
    } = &**u_obj;
    let strictness_kind = lstrictness_kind.join(*ustrictness_kind);

    // if inflowing type is literal (thus guaranteed to be
    // unaliased), propertywise subtyping is sound
    let lit = l_obj.literal;
    // If both are dictionaries, ensure the keys and values are compatible
    // with each other.
    let ldict = obj_type::get_dict_opt(&lflags.obj_kind);
    let udict = obj_type::get_dict_opt(&rflags.obj_kind);

    match (ldict, udict) {
        (Some(ld), Some(ud)) => {
            let DictType {
                key: lk,
                value: lv,
                dict_polarity: lpolarity,
                ..
            } = ld;
            let DictType {
                key: uk,
                value: uv,
                dict_polarity: upolarity,
                ..
            } = ud;
            let use_op_k = VirtualUseOp::Frame(
                Arc::new(VirtualFrameUseOp::IndexerKeyCompatibility(Box::new(
                    IndexerKeyCompatibilityData {
                        lower: lreason.dupe(),
                        upper: ureason.dupe(),
                        lower_t: flow_js_utils::type_or_type_desc_for_error(l),
                        upper_t: flow_js_utils::type_or_type_desc_for_error(u),
                    },
                ))),
                Arc::new(use_op.dupe()),
            );
            if lit {
                FlowJs::rec_flow_t_with_env(cx, env, trace, use_op_k.dupe(), lk, uk)?;
            } else {
                let l_t = Type::new(DefT(lreason.dupe(), DefT::new(ObjT(l_obj.dupe()))));
                let u_t = Type::new(DefT(ureason.dupe(), DefT::new(ObjT(u_obj.dupe()))));
                // Don't report polarity errors when checking the indexer key. We would
                // report these errors again a second time when checking values.
                let errs = rec_flow_p_inner(
                    cx,
                    env,
                    Some(trace),
                    use_op_k.dupe(),
                    None,
                    Some((&l_t, &u_t)),
                    strictness_kind,
                    false,
                    &PropRef::Computed(uk.dupe()),
                    &PropertyType::OrdinaryField {
                        type_: lk.dupe(),
                        polarity: *lpolarity,
                    },
                    &PropertyType::OrdinaryField {
                        type_: uk.dupe(),
                        polarity: *upolarity,
                    },
                )?;
                add_output_prop_polarity_mismatch(
                    cx,
                    env,
                    use_op_k,
                    PropPolarityMismatchObject::Type(l),
                    PropPolarityMismatchObject::Type(u),
                    errs,
                    strictness_kind,
                )?;
            }
            let use_op_v = VirtualUseOp::Frame(
                Arc::new(VirtualFrameUseOp::PropertyCompatibility(Box::new(
                    PropertyCompatibilityData {
                        prop: None,
                        lower: lreason.dupe(),
                        upper: ureason.dupe(),
                        lower_t: flow_js_utils::type_or_type_desc_for_error(l),
                        upper_t: flow_js_utils::type_or_type_desc_for_error(u),
                    },
                ))),
                Arc::new(use_op.dupe()),
            );
            if lit {
                FlowJs::rec_flow_t_with_env(cx, env, trace, use_op_v, lv, uv)?;
            } else {
                let l_t = Type::new(DefT(lreason.dupe(), DefT::new(ObjT(l_obj.dupe()))));
                let u_t = Type::new(DefT(ureason.dupe(), DefT::new(ObjT(u_obj.dupe()))));
                let errs = rec_flow_p_inner(
                    cx,
                    env,
                    Some(trace),
                    use_op_v.dupe(),
                    None,
                    Some((&l_t, &u_t)),
                    strictness_kind,
                    true,
                    &PropRef::Computed(uv.dupe()),
                    &PropertyType::OrdinaryField {
                        type_: lv.dupe(),
                        polarity: *lpolarity,
                    },
                    &PropertyType::OrdinaryField {
                        type_: uv.dupe(),
                        polarity: *upolarity,
                    },
                )?;
                add_output_prop_polarity_mismatch(
                    cx,
                    env,
                    use_op_v,
                    PropPolarityMismatchObject::Type(l),
                    PropPolarityMismatchObject::Type(u),
                    errs,
                    strictness_kind,
                )?;
            }
        }
        _ => {}
    }

    // TS consumers need exact object types to behave structurally for non-literal
    // values, but this also suppresses Flow's usual excess-property check for
    // object literals like `{a: 1, b: 2} as {a: number}`. TypeScript still
    // reports those as excess properties, so this is a known compatibility gap.
    if rflags.obj_kind == ObjKind::Exact && !u_obj.literal && !strictness_kind.is_typescript_loose()
    {
        if !obj_type::is_exact(&lflags.obj_kind) {
            let l_t = Type::new(DefT(lreason.dupe(), DefT::new(ObjT(l_obj.dupe()))));
            flow_js_utils::exact_obj_error(cx, env, &lflags.obj_kind, use_op.dupe(), u, &l_t)?;
        }
        let missing_props: Vec<Name> = cx
            .fold_props(
                lflds.dupe(),
                |name, _prop, mut acc: Vec<Name>| {
                    if !cx.has_prop(uflds.dupe(), name) {
                        acc.push(name.dupe());
                    }
                    acc
                },
                Vec::new(),
            )
            .into_iter()
            .collect();
        if let Ok(missing_props) = Vec1::try_from_vec(missing_props) {
            let l_t = Type::new(DefT(lreason.dupe(), DefT::new(ObjT(l_obj.dupe()))));
            let u_t = Type::new(DefT(ureason.dupe(), DefT::new(ObjT(u_obj.dupe()))));
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::EPropsExtraAgainstExactObject(Box::new(
                    EPropsExtraAgainstExactObjectData {
                        prop_names: missing_props,
                        lower: flow_js_utils::type_reference_with_reason_for_error(
                            &l_t,
                            lreason.dupe(),
                        ),
                        upper: flow_js_utils::type_reference_with_reason_for_error(
                            &u_t,
                            ureason.dupe(),
                        ),
                        upper_is_record: flow_common::reason::is_record_reason(ureason),
                        use_op: use_op.dupe(),
                    },
                )),
            )?;
        }
        if let Some(_lcall) = lcall {
            if ucall.is_none() {
                let prop = Some(Name::new("$call"));
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::EPropNotFoundInSubtyping(Box::new(
                        EPropNotFoundInSubtypingData {
                            prop_name: prop,
                            lower: flow_js_utils::type_reference_with_reason_for_error(
                                u,
                                ureason.dupe(),
                            ),
                            upper: flow_js_utils::type_reference_with_reason_for_error(
                                l,
                                lreason.dupe(),
                            ),
                            use_op: use_op.dupe(),
                            suggestion: None,
                        },
                    )),
                )?;
            }
        }
    }

    match ucall {
        Some(ucall_id) => {
            let prop_name = Some(Name::new("$call"));
            match lcall {
                Some(lcall_id) => {
                    FlowJs::rec_flow_with_env(
                        cx,
                        env,
                        trace,
                        &cx.find_call(*lcall_id),
                        &UseT::new(UseT(use_op.dupe(), cx.find_call(*ucall_id))),
                    )?;
                }
                None => {
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EPropNotFoundInSubtyping(Box::new(
                            EPropNotFoundInSubtypingData {
                                lower: flow_js_utils::type_reference_with_reason_for_error(
                                    l,
                                    lreason.dupe(),
                                ),
                                upper: flow_js_utils::type_reference_with_reason_for_error(
                                    u,
                                    ureason.dupe(),
                                ),
                                prop_name,
                                use_op: use_op.dupe(),
                                suggestion: None,
                            },
                        )),
                    )?;
                }
            }
        }
        None => {}
    }

    // Properties in u must either exist in l, or match l's indexer.
    type InvSubFailed = Vec<(Name, Type, Type)>;
    type LhsMissing = Vec<(Name, Property)>;
    type PolMismatchErrs = Vec<(Option<Name>, (Polarity, Polarity))>;
    let (invariant_subtyping_failed_prop_names, lhs_missing_props, polarity_mismatch_errs): (
        InvSubFailed,
        LhsMissing,
        PolMismatchErrs,
    ) = {
        let mut invariant_subtyping_failed_prop_names: InvSubFailed = Vec::new();
        let mut lhs_missing_props: LhsMissing = Vec::new();
        let mut polarity_mismatch_errs: PolMismatchErrs = Vec::new();
        for (name, up) in cx.find_props(uflds.dupe()).iter() {
            // Lazily construct reason_prop and propref - only needed in non-literal
            // and non-invariant-subtyping branches.
            let mk_propref = || {
                let reason_prop = ureason
                    .dupe()
                    .replace_desc(VirtualReasonDesc::RProperty(Some(name.dupe())));
                type_util::mk_named_prop(reason_prop, false, name.dupe())
            };
            // Lazily construct the Frame use_op - two Arc allocations per property.
            // In the literal case where read_t returns None, this avoids wasted work.
            let mk_use_op = || {
                VirtualUseOp::Frame(
                    Arc::new(VirtualFrameUseOp::PropertyCompatibility(Box::new(
                        PropertyCompatibilityData {
                            prop: Some(name.dupe()),
                            lower: lreason.dupe(),
                            upper: ureason.dupe(),
                            lower_t: flow_js_utils::type_or_type_desc_for_error(l),
                            upper_t: flow_js_utils::type_or_type_desc_for_error(u),
                        },
                    ))),
                    Arc::new(use_op.dupe()),
                )
            };
            match (cx.get_prop(lflds.dupe(), name), &ldict) {
                (Some(lp), _) => {
                    if lit {
                        // prop from unaliased LB: check <:
                        let bivariant_handled = if strictness_kind.is_typescript_loose() {
                            try_property_method_bivariant(cx, env, trace, mk_use_op(), &lp, up)?
                        } else {
                            false
                        };
                        if !bivariant_handled {
                            match (
                                property::read_t_of_property_type(&property_type_for_subtyping(
                                    &lp,
                                    strictness_kind,
                                )),
                                property::read_t(up),
                            ) {
                                (Some(lt), Some(ut)) => {
                                    FlowJs::rec_flow_with_env(
                                        cx,
                                        env,
                                        trace,
                                        &lt,
                                        &UseT::new(UseT(mk_use_op(), ut)),
                                    )?;
                                }
                                _ => {}
                            }
                        }
                    } else {
                        // prop from aliased LB
                        let bivariant_handled = if strictness_kind.is_typescript_loose() {
                            try_property_method_bivariant(cx, env, trace, mk_use_op(), &lp, up)?
                        } else {
                            false
                        };
                        let additional_polarity_mismatch_errs = if bivariant_handled {
                            vec![]
                        } else {
                            match (
                                property_type_for_subtyping(&lp, strictness_kind),
                                property::property_type(up),
                            ) {
                                (
                                    PropertyType::OrdinaryField {
                                        type_: ref lt,
                                        polarity: Polarity::Neutral,
                                    },
                                    PropertyType::OrdinaryField {
                                        type_: ref ut,
                                        polarity: Polarity::Neutral,
                                    },
                                ) if strictness_kind.is_typescript_loose() => {
                                    // TS treats read-write-on-both-sides property type mismatch
                                    // in extends/implements/instantiation as covariant, not
                                    // invariant. Skip the try_unify path and do a covariant
                                    // subtype check; do not record this property as an
                                    // invariant-subtyping failure.
                                    FlowJs::rec_flow_with_env(
                                        cx,
                                        env,
                                        trace,
                                        lt,
                                        &UseT::new(UseT(mk_use_op(), ut.dupe())),
                                    )?;
                                    vec![]
                                }
                                (
                                    PropertyType::OrdinaryField {
                                        type_: ref lt,
                                        polarity: Polarity::Neutral,
                                    },
                                    PropertyType::OrdinaryField {
                                        type_: ref ut,
                                        polarity: Polarity::Neutral,
                                    },
                                ) if !env.speculating()
                                    && !flow_js_utils::tvar_visitors::has_unresolved_tvars(
                                        cx, lt,
                                    )
                                    && !flow_js_utils::tvar_visitors::has_unresolved_tvars(
                                        cx, ut,
                                    ) =>
                                {
                                    let failed = match speculation_kit::try_unify(
                                        cx,
                                        env,
                                        trace,
                                        lt.dupe(),
                                        mk_use_op(),
                                        ut.dupe(),
                                    ) {
                                        Ok(()) => false,
                                        Err(FlowJsException::SpeculationSingletonError) => true,
                                        Err(other) => return Err(other),
                                    };
                                    if failed {
                                        invariant_subtyping_failed_prop_names.push((
                                            name.dupe(),
                                            lt.dupe(),
                                            ut.dupe(),
                                        ));
                                    }
                                    vec![]
                                }
                                (lp_type, up_type) => {
                                    let l_t = Type::new(DefT(
                                        lreason.dupe(),
                                        DefT::new(ObjT(l_obj.dupe())),
                                    ));
                                    let u_t = Type::new(DefT(
                                        ureason.dupe(),
                                        DefT::new(ObjT(u_obj.dupe())),
                                    ));
                                    rec_flow_p_inner(
                                        cx,
                                        env,
                                        Some(trace),
                                        mk_use_op(),
                                        None,
                                        Some((&l_t, &u_t)),
                                        strictness_kind,
                                        true,
                                        &mk_propref(),
                                        &lp_type,
                                        &up_type,
                                    )?
                                }
                            }
                        };
                        polarity_mismatch_errs.extend(additional_polarity_mismatch_errs);
                    }
                }
                // TypeScript never checks a source index signature against an optional target
                // property that the source does not declare.
                (None, Some(_))
                    if ustrictness_kind.is_typescript_loose()
                        && matches!(
                            up.deref(),
                            PropertyInner::Field(box FieldData {
                                type_: deref!(OptionalT { .. }),
                                ..
                            })
                        ) => {}
                (None, Some(dict)) if !flow_js_utils::is_dictionary_exempt(name) => {
                    let DictType {
                        key,
                        value,
                        dict_polarity,
                        ..
                    } = *dict;
                    let lp_for_idx = PropertyType::OrdinaryField {
                        type_: value.dupe(),
                        polarity: *dict_polarity,
                    };
                    let up_for_idx = match up.deref() {
                        PropertyInner::Field(box FieldData {
                            type_: deref!(OptionalT { type_: ut, .. }),
                            polarity,
                            ..
                        }) => PropertyType::OrdinaryField {
                            type_: ut.dupe(),
                            polarity: *polarity,
                        },
                        _ => property::property_type(up),
                    };
                    let subtype_against_indexer =
                        |cx: &mut Context<'cx>,
                         polarity_mismatch_errs: &mut PolMismatchErrs|
                         -> Result<(), FlowJsException> {
                            if lit {
                                match (
                                    property::read_t_of_property_type(&lp_for_idx),
                                    property::read_t_of_property_type(&up_for_idx),
                                ) {
                                    (Some(lt), Some(ut)) => {
                                        FlowJs::rec_flow_with_env(
                                            cx,
                                            env,
                                            trace,
                                            &lt,
                                            &UseT::new(UseT(mk_use_op(), ut)),
                                        )?;
                                    }
                                    _ => {}
                                }
                            } else {
                                let l_t =
                                    Type::new(DefT(lreason.dupe(), DefT::new(ObjT(l_obj.dupe()))));
                                let u_t =
                                    Type::new(DefT(ureason.dupe(), DefT::new(ObjT(u_obj.dupe()))));
                                let additional = rec_flow_p_inner(
                                    cx,
                                    env,
                                    Some(trace),
                                    mk_use_op(),
                                    None,
                                    Some((&l_t, &u_t)),
                                    strictness_kind,
                                    true,
                                    &mk_propref(),
                                    &lp_for_idx,
                                    &up_for_idx,
                                )?;
                                polarity_mismatch_errs.extend(additional);
                            }
                            Ok(())
                        };
                    match up.deref() {
                        PropertyInner::Field(fd)
                            if matches!(fd.type_.deref(), OptionalT { .. })
                                && (lit || fd.polarity == Polarity::Positive)
                                && !env.in_implicit_instantiation() =>
                        {
                            // If the upper property is optional and readonly (or this is a lit check)
                            // then we only need to check the lower indexer type against the upper
                            // property type if the upper property key is covered by the lower property's
                            // indexer, otherwise we can omit the check. We already check elsewhere that
                            // the upper dictionary is compatible with the lower dictionary, so we are
                            // not risking any issues with exactness here.
                            //
                            // We only do this outside of implicit instantiation to avoid accidentally
                            // underconstraining tvars by avoiding flows.
                            let key_type = flow_js_utils::type_of_key_name_with_env(
                                env,
                                name.dupe(),
                                &ureason
                                    .dupe()
                                    .replace_desc(VirtualReasonDesc::RProperty(Some(name.dupe()))),
                            );
                            if FlowJs::speculative_subtyping_succeeds_with_flow_errors(
                                cx, env, &key_type, key,
                            )? {
                                subtype_against_indexer(cx, &mut polarity_mismatch_errs)?;
                            }
                        }
                        _ => {
                            let key_type = flow_js_utils::type_of_key_name_with_env(
                                env,
                                name.dupe(),
                                &ureason
                                    .dupe()
                                    .replace_desc(VirtualReasonDesc::RProperty(Some(name.dupe()))),
                            );
                            FlowJs::rec_flow_with_env(
                                cx,
                                env,
                                trace,
                                &key_type,
                                &UseT::new(UseT(
                                    VirtualUseOp::Frame(
                                        Arc::new(VirtualFrameUseOp::IndexerKeyCompatibility(
                                            Box::new(IndexerKeyCompatibilityData {
                                                lower: lreason.dupe(),
                                                upper: ureason.dupe(),
                                                lower_t: flow_js_utils::type_or_type_desc_for_error(
                                                    l,
                                                ),
                                                upper_t: flow_js_utils::type_or_type_desc_for_error(
                                                    u,
                                                ),
                                            }),
                                        )),
                                        Arc::new(use_op.dupe()),
                                    ),
                                    key.dupe(),
                                )),
                            )?;
                            subtype_against_indexer(cx, &mut polarity_mismatch_errs)?;
                        }
                    }
                }
                _ => {
                    // property doesn't exist in inflowing type
                    match up.deref() {
                        PropertyInner::Field(fd)
                            if matches!(fd.type_.deref(), OptionalT { .. }) && lit => {}
                        PropertyInner::Field(fd)
                            if fd.polarity == Polarity::Positive
                                && matches!(fd.type_.deref(), OptionalT { .. })
                                && obj_type::is_exact(&lflags.obj_kind) =>
                        {
                            let propref = mk_propref();
                            FlowJs::rec_flow_with_env(
                                cx,
                                env,
                                trace,
                                lproto,
                                &UseT::new(LookupT(Box::new(LookupTData {
                                    reason: ureason.dupe(),
                                    lookup_kind: Box::new(LookupKind::NonstrictReturning(
                                        Box::new(NonstrictReturningData(None, None)),
                                    )),
                                    indexer_fallback: None,
                                    try_ts_on_failure: vec![].into(),
                                    propref: Box::new(propref.clone()),
                                    lookup_action: Box::new(LookupAction::LookupPropsForSubtyping(
                                        Box::new(LookupPropsForSubtypingData {
                                            use_op: use_op.dupe(),
                                            props: Rc::from([(propref, up.dupe())]),
                                            strictness_kind,
                                            lower: l.dupe(),
                                            upper: u.dupe(),
                                        }),
                                    )),
                                    method_accessible: true,
                                    ids: None,
                                    ignore_dicts: false,
                                }))),
                            )?;
                        }
                        _ => match &ldict {
                            None if property_is_missing_from_lookup_terminal(lproto, name) => {
                                lhs_missing_props.push((name.dupe(), up.dupe()));
                            }
                            _ => {
                                let lookup_kind = match &ldict {
                                    None => LookupKind::Strict(l.dupe()),
                                    _ => LookupKind::NonstrictReturning(Box::new(
                                        NonstrictReturningData(None, None),
                                    )),
                                };
                                let propref = mk_propref();
                                FlowJs::rec_flow_with_env(
                                    cx,
                                    env,
                                    trace,
                                    lproto,
                                    &UseT::new(LookupT(Box::new(LookupTData {
                                        reason: ureason.dupe(),
                                        lookup_kind: Box::new(lookup_kind),
                                        indexer_fallback: None,
                                        try_ts_on_failure: vec![].into(),
                                        propref: Box::new(propref.clone()),
                                        lookup_action: Box::new(
                                            LookupAction::LookupPropsForSubtyping(Box::new(
                                                LookupPropsForSubtypingData {
                                                    use_op: use_op.dupe(),
                                                    props: Rc::from([(propref, up.dupe())]),
                                                    strictness_kind,
                                                    lower: l.dupe(),
                                                    upper: u.dupe(),
                                                },
                                            )),
                                        ),
                                        method_accessible: true,
                                        ids: None,
                                        ignore_dicts: false,
                                    }))),
                                )?;
                            }
                        },
                    }
                }
            }
        }
        (
            invariant_subtyping_failed_prop_names,
            lhs_missing_props,
            polarity_mismatch_errs,
        )
    };

    match invariant_subtyping_failed_prop_names.as_slice() {
        [] => {}
        [(name, l_prop_t, u_prop_t)] => {
            let lower_loc = match l_prop_t.deref() {
                _ if let Some(node) = type_util::constraint_node_id(l_prop_t) => {
                    let reason = type_util::reason_of_t(l_prop_t);
                    match flow_js_utils::merge_tvar_opt(
                        cx,
                        false,
                        union_rep::UnionKind::ResolvedKind,
                        reason,
                        node.id(),
                    ) {
                        Some(t) => type_util::loc_of_t(&t).dupe(),
                        None => reason.loc().dupe(),
                    }
                }
                _ => type_util::loc_of_t(l_prop_t).dupe(),
            };
            let upper_loc = type_util::loc_of_t(u_prop_t).dupe();
            let explanation = Some(
                intermediate_error_types::ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableProperty {
                    lower_obj_loc: lreason.def_loc().dupe(),
                    upper_obj_loc: ureason.def_loc().dupe(),
                    lower_obj_desc: type_or_type_desc::TypeOrTypeDescT::Type(
                        Type::new(DefT(
                            lreason.dupe(),
                            DefT::new(ObjT(l_obj.dupe())),
                        )),
                    ),
                    upper_obj_desc: type_or_type_desc::TypeOrTypeDescT::Type(
                        Type::new(DefT(
                            ureason.dupe(),
                            DefT::new(ObjT(u_obj.dupe())),
                        )),
                    ),
                    upper_object_loc: ureason.annot_loc().unwrap_or(ureason.def_loc()).dupe(),
                    property_name: Some(name.display_smol_str()),
                },
            );
            let use_op = VirtualUseOp::Frame(
                Arc::new(VirtualFrameUseOp::PropertyCompatibility(Box::new(
                    PropertyCompatibilityData {
                        prop: Some(name.dupe()),
                        lower: lreason.dupe(),
                        upper: ureason.dupe(),
                        lower_t: flow_js_utils::type_or_type_desc_for_error(l),
                        upper_t: flow_js_utils::type_or_type_desc_for_error(u),
                    },
                ))),
                Arc::new(use_op.dupe()),
            );
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::EInvariantSubtypingWithUseOp(Box::new(
                    EInvariantSubtypingWithUseOpData {
                        sub_component: None,
                        lower_loc,
                        upper_loc,
                        lower_desc: type_or_type_desc::TypeOrTypeDescT::Type(l_prop_t.dupe()),
                        upper_desc: type_or_type_desc::TypeOrTypeDescT::Type(u_prop_t.dupe()),
                        use_op,
                        explanation,
                    },
                )),
            )?;
        }
        properties => {
            let t1 = Type::new(DefT(lreason.dupe(), DefT::new(ObjT(l_obj.dupe()))));
            let t2 = Type::new(DefT(ureason.dupe(), DefT::new(ObjT(u_obj.dupe()))));
            let lower_loc = match t1.deref() {
                OpenT(tvar) => {
                    match flow_js_utils::merge_tvar_opt(
                        cx,
                        false,
                        union_rep::UnionKind::ResolvedKind,
                        tvar.reason(),
                        tvar.id() as i32,
                    ) {
                        Some(t) => type_util::loc_of_t(&t).dupe(),
                        None => tvar.reason().loc().dupe(),
                    }
                }
                _ => type_util::loc_of_t(&t1).dupe(),
            };
            let upper_loc = type_util::loc_of_t(&t2).dupe();
            let properties: Vec<Name> = properties.iter().map(|(p, _, _)| p.dupe()).collect();
            let explanation =
                intermediate_error_types::ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableProperties {
                    lower_obj_loc: lreason.def_loc().dupe(),
                    upper_obj_loc: ureason.def_loc().dupe(),
                    lower_obj_desc: type_or_type_desc::TypeOrTypeDescT::Type(t1.dupe()),
                    upper_obj_desc: type_or_type_desc::TypeOrTypeDescT::Type(t2.dupe()),
                    upper_object_loc: type_util::ref_loc_of_t(&t2).dupe(),
                    properties: properties.clone(),
                };
            flow_js_utils::add_output_with_env(
                cx,env,
                ErrorMessage::EInvariantSubtypingWithUseOp(Box::new(EInvariantSubtypingWithUseOpData {
                    sub_component: Some(
                        intermediate_error_types::SubComponentOfInvariantSubtypingError::ObjectProps(
                            properties,
                        ),
                    ),
                    lower_loc,
                    upper_loc,
                    lower_desc: type_or_type_desc::TypeOrTypeDescT::Type(t1),
                    upper_desc: type_or_type_desc::TypeOrTypeDescT::Type(t2),
                    use_op: use_op.dupe(),
                    explanation: Some(explanation),
                })),
            )
            ?;
        }
    }

    let (regular_missing, rhs_neutral_optional, ups_to_flow_any) =
        lhs_missing_props.into_iter().fold(
            (vec![], vec![], vec![]),
            |(mut regular_missing, mut rhs_neutral_optional, mut ups_to_flow_any): (
                Vec<Name>,
                Vec<Name>,
                Vec<Property>,
            ),
             (name, up)| {
                match up.deref() {
                    PropertyInner::Field(fd)
                        if fd.polarity == Polarity::Neutral
                            && matches!(fd.type_.deref(), OptionalT { .. }) =>
                    {
                        rhs_neutral_optional.push(name);
                        ups_to_flow_any.push(up);
                    }
                    _ => {
                        regular_missing.push(name);
                        ups_to_flow_any.push(up);
                    }
                }
                (regular_missing, rhs_neutral_optional, ups_to_flow_any)
            },
        );
    if let Ok(props) = Vec1::try_from_vec(regular_missing) {
        flow_js_utils::add_output_with_env(
            cx,
            env,
            ErrorMessage::EPropsNotFoundInSubtyping(Box::new(EPropsNotFoundInSubtypingData {
                prop_names: props,
                lower: flow_js_utils::type_reference_with_reason_for_error(l, lreason.dupe()),
                upper: flow_js_utils::type_reference_with_reason_for_error(u, ureason.dupe()),
                use_op: use_op.dupe(),
            })),
        )?;
    }
    if !strictness_kind.is_typescript_loose() {
        if let Ok(props) = Vec1::try_from_vec(rhs_neutral_optional) {
            let t1 = Type::new(DefT(lreason.dupe(), DefT::new(ObjT(l_obj.dupe()))));
            let t2 = Type::new(DefT(ureason.dupe(), DefT::new(ObjT(u_obj.dupe()))));
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::EPropsNotFoundInInvariantSubtyping(Box::new(
                    EPropsNotFoundInInvariantSubtypingData {
                        prop_names: props,
                        lower: flow_js_utils::type_reference_with_reason_for_error(
                            &t1,
                            lreason.dupe(),
                        ),
                        upper: flow_js_utils::type_reference_with_reason_for_error(
                            &t2,
                            ureason.dupe(),
                        ),
                        use_op: use_op.dupe(),
                    },
                )),
            )?;
        }
    }
    recover_missing_props(
        cx,
        env,
        trace,
        &use_op,
        ureason,
        strictness_kind,
        &ups_to_flow_any,
    )?;
    add_output_prop_polarity_mismatch(
        cx,
        env,
        use_op.dupe(),
        PropPolarityMismatchObject::Type(l),
        PropPolarityMismatchObject::Type(u),
        polarity_mismatch_errs,
        strictness_kind,
    )?;

    // Any properties in l but not u must match indexer
    match &udict {
        None => {}
        Some(ud) => {
            let lower = Type::new(DefT(lreason.dupe(), DefT::new(ObjT(l_obj.dupe()))));
            let upper = Type::new(DefT(ureason.dupe(), DefT::new(ObjT(u_obj.dupe()))));
            let lower_props = cx.find_props(lflds.dupe());
            let upper_props = cx.find_props(uflds.dupe());
            PropsToIndexerContext {
                env,
                trace,
                use_op: use_op.dupe(),
                lower: l.dupe(),
                upper: u.dupe(),
                strictness_kind,
                lit,
                lower_upper_subtyping_obj_ts: Some((lower, upper)),
            }
            .flow_props_to_indexer(cx, &lower_props, &[&upper_props], ud)?;
            // If the left is inexact and the right is indexed, Flow mixed to the indexer
            // value. Mixed represents the possibly unknown properties on the inexact object
            match &lflags.obj_kind {
                ObjKind::Inexact => {
                    let r = flow_common::reason::mk_reason(
                        lreason.desc(true).cloned().map(|desc| {
                            VirtualReasonDesc::RUnknownUnspecifiedProperty(Arc::new(desc))
                        }),
                        lreason.loc().dupe(),
                    );
                    let mixed = Type::new(DefT(r, DefT::new(MixedT(MixedFlavor::MixedEverything))));
                    FlowJs::rec_flow_t_with_env(cx, env, trace, use_op.dupe(), &mixed, &ud.value)?;
                }
                _ => {}
            }
            // Previously, call properties were stored in the props map, and were
            // checked against dictionary upper bounds. This is wrong, but useful for
            // distinguishing between thunk-like types found in graphql-js.
            //
            // Now that call properties are stored separately, it is particularly
            // egregious to emit this constraint. This only serves to maintain buggy
            // behavior, which should be fixed, and this code removed.
            match (lcall, ucall) {
                (Some(lcall_id), None) => {
                    let name = Name::new("$call");
                    let use_op = VirtualUseOp::Frame(
                        Arc::new(VirtualFrameUseOp::PropertyCompatibility(Box::new(
                            PropertyCompatibilityData {
                                prop: Some(name.dupe()),
                                lower: lreason.dupe(),
                                upper: ureason.dupe(),
                                lower_t: flow_js_utils::type_or_type_desc_for_error(l),
                                upper_t: flow_js_utils::type_or_type_desc_for_error(u),
                            },
                        ))),
                        Arc::new(use_op.dupe()),
                    );
                    let call_t = cx.find_call(*lcall_id);
                    let lp_t = match call_t.deref() {
                        OptionalT { type_: t, .. } => t.dupe(),
                        _ => call_t.dupe(),
                    };
                    let lp_type = PropertyType::OrdinaryField {
                        type_: lp_t,
                        polarity: Polarity::Positive,
                    };
                    let up_type = PropertyType::OrdinaryField {
                        type_: ud.value.dupe(),
                        polarity: ud.dict_polarity,
                    };
                    if lit {
                        match (
                            property::read_t_of_property_type(&lp_type),
                            property::read_t_of_property_type(&up_type),
                        ) {
                            (Some(lt), Some(ut)) => {
                                FlowJs::rec_flow_with_env(
                                    cx,
                                    env,
                                    trace,
                                    &lt,
                                    &UseT::new(UseT(use_op, ut)),
                                )?;
                            }
                            _ => {}
                        }
                    } else {
                        let reason_prop = lreason
                            .dupe()
                            .replace_desc(VirtualReasonDesc::RProperty(Some(name.dupe())));
                        let propref = type_util::mk_named_prop(reason_prop, false, name);
                        let l_t = Type::new(DefT(lreason.dupe(), DefT::new(ObjT(l_obj.dupe()))));
                        let u_t = Type::new(DefT(ureason.dupe(), DefT::new(ObjT(u_obj.dupe()))));
                        let errs = rec_flow_p_inner(
                            cx,
                            env,
                            Some(trace),
                            use_op.dupe(),
                            None,
                            Some((&l_t, &u_t)),
                            strictness_kind,
                            true,
                            &propref,
                            &lp_type,
                            &up_type,
                        )?;
                        add_output_prop_polarity_mismatch(
                            cx,
                            env,
                            use_op,
                            PropPolarityMismatchObject::Type(l),
                            PropPolarityMismatchObject::Type(u),
                            errs,
                            l_obj.strictness_kind.join(u_obj.strictness_kind),
                        )?;
                    }
                }
                _ => {}
            }
        }
    }

    let l_t = Type::new(DefT(lreason.dupe(), DefT::new(ObjT(l_obj.dupe()))));
    FlowJs::rec_flow_with_env(
        cx,
        env,
        trace,
        uproto,
        &UseT::new(ReposUseT(Box::new(ReposUseTData {
            reason: ureason.dupe(),
            use_desc: false,
            use_op,
            type_: l_t,
        }))),
    )?;
    Ok(())
}

// mutable sites on parent values (i.e. object properties,
// array elements) must be typed invariantly when a value
// flows to the parent, unless the incoming value is fresh,
// in which case covariant typing is sound (since no alias
// will break if the subtyped child value is replaced by a
// non-subtyped value
fn flow_to_mutable_child<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    unify_cause: UnifyCause,
    fresh: bool,
    t1: &Type,
    t2: &Type,
) -> Result<(), FlowJsException> {
    if fresh {
        FlowJs::rec_flow_with_env(cx, env, trace, t1, &UseT::new(UseT(use_op, t2.dupe())))?;
    } else {
        FlowJs::rec_unify_with_env(cx, env, trace, use_op, unify_cause, None, t1, t2)?;
    }
    Ok(())
}

// Subtyping of arrays is complicated by tuples. Currently, there are three
// different kinds of types, all encoded by arrays:
//
//    1. Array<T> (array type)
//    2. [T1, T2] (tuple type)
//    3. "internal" Array<X>[T1, T2] where T1 | T2 ~> X (array literal type)
//
// We have the following rules:
//
// (1) When checking types against Array<U>, the rules are not surprising. Array
// literal types behave like array types in these checks.
//
//    * Array<T> ~> Array<U> checks T <~> U
//    * [T1, T2] ~> Array<U> checks T1 | T2 ~> U
//    * Array<X>[T1, T2] ~> Array<U> checks Array<X> ~> Array<U>
//
// (2) When checking types against [T1, T2], the rules are again not
// surprising. Array literal types behave like tuple types in these checks. We
// consider missing tuple elements to be undefined, following common usage (and
// consistency with missing call arguments).
//
//    * Array<T> ~> [U1, U2] checks T ~> U1, T ~> U2
//    * [T1, T2] ~> [U1, U2] checks T1 ~> U1 and T2 ~> U2
//    * [T1, T2] ~> [U1] checks T1 ~> U1
//    * [T1] ~> [U1, U2] checks T1 ~> U1 and void ~> U2
//    * Array<X>[T1, T2] ~> [U1, U2] checks [T1, T2] ~> [U1, U2]
//
// (3) When checking types against Array<Y>[U1, U2], the rules are a bit
// unsound. Array literal types were not designed to appear as upper bounds. In
// particular, their summary element types are often overly precise. Checking
// individual element types of one array literal type against the summary
// element type of another array literal type can lead to crazy errors, so we
// currently drop such checks.
//
// TODO: Make these rules great again by computing more reasonable summary
// element types for array literal types.
//
//    * Array<T> ~> Array<Y>[U1, U2] checks Array<T> ~> Array<Y>
//    * [T1, T2] ~> Array<Y>[U1, U2] checks T1 ~> U1, T2 ~> U2
//    * [T1, T2] ~> Array<Y>[U1] checks T1 ~> U1
//    * [T1] ~> Array<Y>[U1, U2] checks T1 ~> U1
//    * Array<X>[T1, T2] ~> Array<Y>[U1, U2] checks [T1, T2] ~> Array<Y>[U1, U2]
fn array_flow<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    lit1: bool,
    strictness_kind: TypeStrictnessKind,
    _r1: &Reason,
    l: &Type,
    u: &Type,
    ts1: &[Type],
    inexact1: bool,
    rest1: Option<&TupleElement>,
    e1: &Type,
    ts2: &[Type],
    rest2: Option<&TupleElement>,
    e2: &Type,
) -> Result<(), FlowJsException> {
    let lit = lit1 || strictness_kind.is_typescript_loose();
    // Against an upper rest element, the unknown elements of an inexact lower view are like a
    // rest element of its element type.
    let lower_rest_t = rest1
        .map(|rest| &rest.t)
        .or_else(|| (inexact1 && rest2.is_some()).then_some(e1));
    let mut iter1 = ts1.iter();
    let mut iter2 = ts2.iter();
    let mut index = 0usize;
    loop {
        match (iter1.next(), iter2.next()) {
            // The lower rest element fills the remaining upper elements.
            (None, t2_elem) if let Some(rest1_t) = lower_rest_t => {
                match rest2 {
                    Some(TupleElement { t: rest2_t, .. }) => {
                        for t2_elem in t2_elem.into_iter().chain(iter2).chain([rest2_t]) {
                            let cause = UnifyCause::Uncategorized;
                            flow_to_mutable_child(
                                cx,
                                env,
                                trace,
                                use_op.dupe(),
                                cause,
                                lit,
                                rest1_t,
                                t2_elem,
                            )?;
                        }
                    }
                    None => {
                        let cause = UnifyCause::MutableArray {
                            lower_array_t: l.dupe(),
                            upper_array_t: u.dupe(),
                        };
                        flow_to_mutable_child(cx, env, trace, use_op, cause, lit, rest1_t, e2)?;
                    }
                }
                break;
            }
            // The remaining lower elements go to the upper rest element.
            (Some(t1_elem), None) if let Some(TupleElement { t: rest2_t, .. }) = rest2 => {
                for t1_elem in std::iter::once(t1_elem).chain(iter1).chain(lower_rest_t) {
                    let cause = UnifyCause::Uncategorized;
                    flow_to_mutable_child(
                        cx,
                        env,
                        trace,
                        use_op.dupe(),
                        cause,
                        lit,
                        t1_elem,
                        rest2_t,
                    )?;
                }
                break;
            }
            (None, _) => {
                if index == 0 {
                    flow_to_mutable_child(
                        cx,
                        env,
                        trace,
                        use_op,
                        UnifyCause::MutableArray {
                            lower_array_t: l.dupe(),
                            upper_array_t: u.dupe(),
                        },
                        lit1 || strictness_kind.is_typescript_loose(),
                        e1,
                        e2,
                    )?;
                }
                // otherwise, lower bound is an empty tuple (nothing to do)
                break;
            }
            (_, None) => {
                flow_to_mutable_child(
                    cx,
                    env,
                    trace,
                    use_op,
                    UnifyCause::MutableArray {
                        lower_array_t: l.dupe(),
                        upper_array_t: u.dupe(),
                    },
                    lit1 || strictness_kind.is_typescript_loose(),
                    e1,
                    e2,
                )?;
                break;
            }
            (Some(t1_elem), Some(t2_elem)) => {
                flow_to_mutable_child(
                    cx,
                    env,
                    trace,
                    use_op.dupe(),
                    UnifyCause::Uncategorized,
                    lit1 || strictness_kind.is_typescript_loose(),
                    t1_elem,
                    t2_elem,
                )?;
                index += 1;
            }
        }
    }
    Ok(())
}

fn take_n_from_set(n: usize, set: &UnionEnumSet) -> Vec<UnionEnum> {
    set.iter().take(n).cloned().collect()
}

pub fn union_to_union<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    l: &Type,
    rep: &union_rep::UnionRep,
    u: &Type,
) -> Result<(), FlowJsException> {
    let guard_result = flow_js_utils::union_optimization_guard(
        cx,
        |t1, t2| type_util::quick_subtype(None::<&mut fn(&Type)>, t1, t2),
        l,
        u,
    );
    match guard_result {
        flow_js_utils::UnionOptimizationGuardResult::True => {
            if cx.is_verbose() {
                eprintln!("UnionT ~> UnionT fast path (True)");
            }
        }
        flow_js_utils::UnionOptimizationGuardResult::Maybe => {
            flow_all_in_union(cx, env, &trace, rep, &UseT::new(UseT(use_op, u.dupe())))?;
        }
        flow_js_utils::UnionOptimizationGuardResult::False { diff } => {
            if cx.is_verbose() {
                eprintln!("UnionT ~> UnionT fast path (False)");
            }
            let members: Vec<flow_data_structure_wrapper::smol_str::FlowSmolStr> =
                take_n_from_set(3, &diff)
                    .iter()
                    .map(|e| e.to_string().into())
                    .collect();
            let extra_number = std::cmp::max(0, diff.len() as i32 - 3);
            let explanation = Some(
                intermediate_error_types::Explanation::ExplanationAdditionalUnionMembers(Box::new(
                    intermediate_error_types::ExplanationAdditionalUnionMembersData {
                        left_loc: type_util::ref_loc_of_t(l).dupe(),
                        left_desc: flow_js_utils::type_or_type_desc_for_error(l),
                        right_loc: type_util::ref_loc_of_t(u).dupe(),
                        right_desc: flow_js_utils::type_or_type_desc_for_error(u),
                        members,
                        extra_number,
                    },
                )),
            );
            flow_js_utils::add_output_with_env(
                cx,
                env,
                flow_js_utils::incompatible_types_error(l, u, use_op, explanation),
            )?;
        }
    }
    Ok(())
}

pub fn rec_sub_t<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: UseOp,
    l: &Type,
    u: &Type,
    trace: DepthTrace,
) -> Result<(), FlowJsException> {
    match (l.deref(), u.deref()) {
        // The sink component of an annotation constrains values flowing
        // into the annotated site.
        (_, AnnotT(r, t, use_desc)) => FlowJs::rec_flow_with_env(
            cx, env,
            trace,
            t,
            &UseT::new(ReposUseT(Box::new(ReposUseTData {
                reason: r.dupe(),
                use_desc: *use_desc,
                use_op,
                type_: l.dupe(),
            }))),
        ),
        (AnnotT(r, t, use_desc), _) => {
            let t = FlowJs::reposition_reason(cx, env, Some(trace), r, Some(*use_desc), t)?;
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &t, u)
        }

        // *******************************
        // * common implicit conversions *
        // *******************************
        (DefT(_, deref!(NumGeneralT(_) | SingletonNumT { .. })), DefT(_, deref!(NumGeneralT(_)))) => {
            Ok(())
        }
        (DefT(r, deref!(NullT | VoidT)), MaybeT(_, tout)) => {
            let empty = Type::new(DefT(r.dupe(), DefT::new(EmptyT)));
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &empty, tout)
        }
        (DefT(r, deref!(MixedT(MixedFlavor::MixedEverything))), MaybeT(_, tout)) => {
            let mixed = Type::new(DefT(
                r.dupe(),
                DefT::new(MixedT(MixedFlavor::MixedNonMaybe)),
            ));
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &mixed, tout)
        }
        (MaybeT(r, t), MaybeT(_, _)) => {
            let t = type_util::push_type_alias_reason(r, t.dupe());
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &t, u)
        }
        (MaybeT(reason, t), _) => {
            let reason = reason.dupe().replace_desc(VirtualReasonDesc::RNullOrVoid);
            let t = type_util::push_type_alias_reason(&reason, t.dupe());
            let null = Type::new(DefT(reason.dupe(), DefT::new(NullT)));
            let void = Type::new(DefT(reason.dupe(), DefT::new(VoidT)));
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op.dupe(), &null, u)?;
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op.dupe(), &void, u)?;
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &t, u)
        }
        (DefT(r, deref!(VoidT)), OptionalT { type_: tout, .. }) => {
            let empty = Type::new(DefT(r.dupe(), DefT::new(EmptyT)));
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &empty, tout)
        }
        (OptionalT { type_: t, .. }, OptionalT { .. })
        | (OptionalT { type_: t, .. }, MaybeT(_, _)) => {
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, t, u)
        }
        (
            OptionalT {
                reason,
                type_: t,
                use_desc,
            },
            _,
        ) => {
            let void = void::why_with_use_desc(*use_desc, reason.dupe());
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op.dupe(), &void, u)?;
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, t, u)
        }
        (
            ThisTypeAppT(box ThisTypeAppTData {
                reason,
                type_: c,
                this_t,
                targs,
            }),
            _,
        ) => {
            let reason_op = type_util::reason_of_t(u);
            FlowJs::instantiate_this_class_with_env(
                cx, env,
                trace,
                reason_op,
                reason,
                c,
                targs.dupe(),
                this_t,
                &Cont::Upper(Box::new(UseT::new(UseT(use_op, u.dupe())))),
            )?;
            Ok(())
        }
        (
            _,
            ThisTypeAppT(box ThisTypeAppTData {
                reason,
                type_: c,
                this_t,
                targs,
            }),
        ) => {
            let reason_op = type_util::reason_of_t(l);
            FlowJs::instantiate_this_class_with_env(
                cx, env,
                trace,
                reason_op,
                reason,
                c,
                targs.dupe(),
                this_t,
                &Cont::Lower(use_op, l.dupe()),
            )?;
            Ok(())
        }

        // When a subtyping question involves a union appearing on the right or an
        // intersection appearing on the left, the simplification rules are
        // imprecise: we split the union / intersection into cases and try to prove
        // that the subtyping question holds for one of the cases, but each of those
        // cases may be unprovable, which might lead to spurious errors. In
        // particular, obvious assertions such as (A | B) & C is a subtype of A | B
        // cannot be proved if we choose to split the union first (discharging
        // unprovable subgoals of (A | B) & C being a subtype of either A or B);
        // dually, obvious assertions such as A & B is a subtype of (A & B) | C
        // cannot be proved if we choose to simplify the intersection first
        // (discharging unprovable subgoals of either A or B being a subtype of (A &
        // B) | C). So instead, we try inclusion rules to handle such cases.
        //
        // An orthogonal benefit is that for large unions or intersections, checking
        // inclusion is significantly faster that splitting for proving simple
        // inequalities (O(n) instead of O(n^2) for n cases).
        (IntersectionT(_, rep), _)
            if rep
                .members_iter()
                .any(|member| flow_typing_flow_common::concrete_type_eq::eq_with_env(cx,env,  member, u)) =>
        {
            Ok(())
        }

        // If we have a TypeAppT ~> TypeAppT then we want to concretize both type
        // constructors (c, the `type_` field) to PolyTs so that we may
        // referentially compare them.
        // We cannot compare the non-concretized versions since they may have been
        // reposition, they may be two OpenTs from different locations, or any other
        // way you can access the same PolyT via different means that results in a
        // different c being passed to TypeAppT.
        //
        // We use the ConcretizeTypeAppsT use type to concretize both the c of our
        // upper and lower TypeAppT bound. We start by concretizing the upper bound
        // which we signal by setting the final element in ConcretizeTypeAppsT to
        // true.
        //
        // The next step happens back in flow_js.rs, at the cases for a
        // ConcretizeTypeAppsT use type.
        (
            TypeAppT(box TypeAppTData {
                reason: r1,
                use_op: op1,
                type_: c1,
                targs: ts1,
                from_value: fv1,
                use_desc: _,
            }),
            TypeAppT(box TypeAppTData {
                reason: r2,
                use_op: op2,
                type_: c2,
                targs: ts2,
                from_value: fv2,
                use_desc: _,
            }),
        ) => {
            if let Some(env) = instantiation_utils::type_app_expansion::push_unless_loop(
                cx,
                env,
                type_app_expansion::Bound::Lower,
                c1,
                ts1,
            ) && let Some(env) = instantiation_utils::type_app_expansion::push_unless_loop(
                cx,
                &env,
                type_app_expansion::Bound::Upper,
                c2,
                ts2,
            ) {
                FlowJs::rec_flow_with_env(
                    cx,
                    &env,
                    trace,
                    c2,
                    &UseT::new(ConcretizeTypeAppsT(
                        use_op.dupe(),
                        Box::new((ts2.dupe(), *fv2, op2.dupe(), r2.dupe())),
                        Box::new((c1.dupe(), ts1.dupe(), *fv1, op1.dupe(), r1.dupe())),
                        true,
                    )),
                )?;
            }
            Ok(())
        }
        (
            TypeAppT(box TypeAppTData {
                reason: reason_tapp,
                use_op: use_op_tapp,
                type_,
                targs,
                from_value,
                use_desc: type_app_use_desc,
            }),
            _,
        ) => {
            let reason_op = type_util::reason_of_t(u);
            if let Some(env) = instantiation_utils::type_app_expansion::push_unless_loop(
                cx,
                env,
                type_app_expansion::Bound::Lower,
                type_,
                targs,
            ) {
                let env = &env;
                let inst = FlowJs::mk_typeapp_instance_with_env(
                    cx, env,
                    Some(trace),
                    use_op_tapp.dupe(),
                    reason_op,
                    reason_tapp,
                    *from_value,
                    type_,
                    targs.dupe(),
                )?;
                let t = FlowJs::reposition_reason(
                    cx, env,
                    Some(trace),
                    reason_tapp,
                    Some(*type_app_use_desc),
                    &inst,
                )?;
                FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &t, u)?;
            }
            Ok(())
        }
        (
            _,
            TypeAppT(box TypeAppTData {
                reason: reason_tapp,
                use_op: use_op_tapp,
                type_,
                targs,
                from_value,
                use_desc: type_app_use_desc,
            }),
        ) => {
            let reason_op = type_util::reason_of_t(l);
            if let Some(env) = instantiation_utils::type_app_expansion::push_unless_loop(
                cx,
                env,
                type_app_expansion::Bound::Upper,
                type_,
                targs,
            ) {
                let env = &env;
                let t = FlowJs::mk_typeapp_instance_with_env(
                    cx, env,
                    Some(trace),
                    use_op_tapp.dupe(),
                    reason_op,
                    reason_tapp,
                    *from_value,
                    type_,
                    targs.dupe(),
                )?;
                // We do the slingshot trick here so that we flow l to the results of making the typeapp
                // instead of adding another lower bound to t. We can't use an Annot here, which would do
                // that for us, because ts may not be 0->1, so using them to make an Annot would break
                // invariants that we rely on. In particular, it would force us to traverse AnnotTs to
                // do any propagation, which is extremely costly.
                FlowJs::rec_flow_with_env(
                    cx, env,
                    trace,
                    &t,
                    &UseT::new(ReposUseT(Box::new(ReposUseTData {
                        reason: reason_tapp.dupe(),
                        use_desc: *type_app_use_desc,
                        use_op,
                        type_: l.dupe(),
                    }))),
                )?;
            }
            Ok(())
        }

        // *********************
        //    opaque types
        // *********************

        // If the ids are equal, we use flow_type_args to make sure that the type arguments of each
        // are compatible with each other. If there are no type args, this doesn't do anything
        (
            NominalT {
                reason: lreason,
                nominal_type: lnom,
            },
            NominalT {
                reason: ureason,
                nominal_type: unom,
            },
        ) if lnom.nominal_id == unom.nominal_id => {
            match &lnom.nominal_id {
                // When we are subtyping between two stuck EvalT, we just want to give a simple yes/no answer.
                // Complex subtyping on these stuck EvalTs is not supported
                nominal::Id::StuckEval(_) if !env.in_implicit_instantiation() => {
                    match speculation_kit::try_singleton_custom_throw_on_failure(
                        cx, env,
                        Box::new({
                            let use_op = use_op.dupe();
                            let lreason = lreason.dupe();
                            let ureason = ureason.dupe();
                            let ltargs = lnom.nominal_type_args.dupe();
                            let utargs = unom.nominal_type_args.dupe();
                            move |cx: &mut Context, env: &FlowJsEnv| {
                                FlowJs::flow_type_args_with_env(
                                    cx,
                                    env,
                                    trace,
                                    use_op,
                                    &lreason,
                                    &ureason,
                                    ltargs,
                                    utargs,
                                    TypeStrictnessKind::Flow,
                                )
                            }
                        }),
                    ) {
                        Ok(()) => {}
                        Err(FlowJsException::SpeculationSingletonError) => {
                            flow_js_utils::add_output_with_env(
                                cx,env,
                                flow_js_utils::incompatible_types_error(l, u, use_op, None),
                            )?;
                        }
                        Err(other) => return Err(other),
                    }
                }
                // An `ExactObjectType` carries the props it is known to have in its upper bound.
                nominal::Id::ExactObjectType
                    if let (Some(lupper), Some(uupper)) = (&lnom.upper_t, &unom.upper_t) =>
                {
                    FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, lupper, uupper)?;
                }
                _ => {
                    FlowJs::flow_type_args_with_env(
                        cx, env,
                        trace,
                        use_op,
                        lreason,
                        ureason,
                        lnom.nominal_type_args.dupe(),
                        unom.nominal_type_args.dupe(),
                        TypeStrictnessKind::Flow,
                    )?;
                }
            }
            Ok(())
        }
        // If the opaque type are from the same logical module, we need to do some structural validation
        // in additional to type_args check.
        (
            NominalT {
                reason: lreason,
                nominal_type: lnom,
            },
            NominalT {
                reason: ureason,
                nominal_type: unom,
            },
        ) if let (
            nominal::Id::UserDefinedOpaqueTypeId(box nominal::UserDefinedOpaqueTypeIdData(
                id1,
                name1,
            )),
            nominal::Id::UserDefinedOpaqueTypeId(box nominal::UserDefinedOpaqueTypeIdData(
                id2,
                name2,
            )),
        ) = (&lnom.nominal_id, &unom.nominal_id)
            && type_util::nominal_id_have_same_logical_module(
                &cx.file_options(),
                cx.projects_options(),
                (id1, Some(name1.as_str())),
                (id2, Some(name2.as_str())),
            )
            && lnom.nominal_type_args.len() == unom.nominal_type_args.len() =>
        {
            // Check super
            if type_util::is_in_common_interface_conformance_check(&use_op) {
                let lower_1 = lnom.lower_t.as_ref().duped().unwrap_or_else(|| {
                    Type::new(DefT(
                        lreason.dupe(),
                        DefT::new(EmptyT),
                    ))
                });
                let lower_2 = unom.lower_t.as_ref().duped().unwrap_or_else(|| {
                    Type::new(DefT(
                        ureason.dupe(),
                        DefT::new(EmptyT),
                    ))
                });
                let lower_1_reason = type_util::reason_of_t(&lower_1).dupe();
                FlowJs::rec_unify_with_env(
                    cx, env,
                    trace,
                    VirtualUseOp::Frame(
                        Arc::new(VirtualFrameUseOp::OpaqueTypeLowerBoundCompatibility {
                            lower: lower_1_reason.dupe(),
                            upper: lower_1_reason,
                        }),
                        Arc::new(use_op.dupe()),
                    ),
                    UnifyCause::Uncategorized,
                    None,
                    &lower_1,
                    &lower_2,
                )?;
                let upper_1 = lnom.upper_t.as_ref().duped().unwrap_or_else(|| {
                    Type::new(DefT(
                        lreason.dupe(),
                        DefT::new(MixedT(MixedFlavor::MixedEverything)),
                    ))
                });
                let upper_2 = unom.upper_t.as_ref().duped().unwrap_or_else(|| {
                    Type::new(DefT(
                        ureason.dupe(),
                        DefT::new(MixedT(MixedFlavor::MixedEverything)),
                    ))
                });
                FlowJs::rec_unify_with_env(
                    cx, env,
                    trace,
                    VirtualUseOp::Frame(
                        Arc::new(VirtualFrameUseOp::OpaqueTypeUpperBoundCompatibility {
                            lower: type_util::reason_of_t(&upper_1).dupe(),
                            upper: type_util::reason_of_t(&upper_2).dupe(),
                        }),
                        Arc::new(use_op.dupe()),
                    ),
                    UnifyCause::Uncategorized,
                    None,
                    &upper_1,
                    &upper_2,
                )?;
            }
            // Do not check underlying type even if we have access to them, because underlying types
            // are not visible across module boundaries.
            FlowJs::flow_type_args_with_env(
                cx, env,
                trace,
                use_op,
                lreason,
                ureason,
                lnom.nominal_type_args.dupe(),
                unom.nominal_type_args.dupe(),
                TypeStrictnessKind::Flow,
            )
        }
        // If the type is still in the same file it was defined, we allow it to
        // expose its underlying type information
        (
            NominalT {
                nominal_type: lnom, ..
            },
            _,
        ) if matches!(
            (&lnom.nominal_id, &lnom.underlying_t),
            (
                nominal::Id::UserDefinedOpaqueTypeId(box nominal::UserDefinedOpaqueTypeIdData(
                    _,
                    _,
                )),
                nominal::UnderlyingT::OpaqueWithLocal { .. },
            )
        ) && {
            if let nominal::Id::UserDefinedOpaqueTypeId(box nominal::UserDefinedOpaqueTypeIdData(
                nominal_id,
                _,
            )) = &lnom.nominal_id
            {
                nominal_id.0.source() == Some(cx.file())
            } else {
                false
            }
        } =>
        {
            if let nominal::UnderlyingT::OpaqueWithLocal { t } = &lnom.underlying_t {
                FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, t, u)?;
            }
            Ok(())
        }
        // If the lower bound is in the same file as where the opaque type was defined,
        // we expose the underlying type information
        (
            _,
            NominalT {
                nominal_type: unom, ..
            },
        ) if matches!(
            (&unom.nominal_id, &unom.underlying_t),
            (
                nominal::Id::UserDefinedOpaqueTypeId(box nominal::UserDefinedOpaqueTypeIdData(
                    _,
                    _,
                )),
                nominal::UnderlyingT::OpaqueWithLocal { .. },
            )
        ) && {
            if let nominal::Id::UserDefinedOpaqueTypeId(box nominal::UserDefinedOpaqueTypeIdData(
                nominal_id,
                _,
            )) = &unom.nominal_id
            {
                nominal_id.0.source() == Some(cx.file())
            } else {
                false
            }
        } =>
        {
            if let nominal::UnderlyingT::OpaqueWithLocal { t } = &unom.underlying_t {
                FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, l, t)?;
            }
            Ok(())
        }
        // Opaque type for custom error types are always fully transparent
        (
            NominalT {
                nominal_type: lnom, ..
            },
            _,
        ) if let nominal::UnderlyingT::CustomError(box nominal::CustomErrorData { t, .. }) =
            &lnom.underlying_t =>
        {
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, t, u)
        }
        (
            _,
            NominalT {
                reason: ru,
                nominal_type: unom,
            },
        ) if let (
            nominal::Id::UserDefinedOpaqueTypeId(box nominal::UserDefinedOpaqueTypeIdData(_, name)),
            nominal::UnderlyingT::CustomError(box nominal::CustomErrorData {
                t,
                custom_error_loc,
            }),
        ) = (&unom.nominal_id, &unom.underlying_t) =>
        {
            let use_op = VirtualUseOp::Frame(
                Arc::new(VirtualFrameUseOp::OpaqueTypeCustomErrorCompatibility(
                    Box::new(OpaqueTypeCustomErrorCompatibilityData {
                        lower: type_util::reason_of_t(l).dupe(),
                        upper: ru.dupe(),
                        lower_t: type_or_type_desc::TypeOrTypeDescT::Type(l.dupe()),
                        upper_t: type_or_type_desc::TypeOrTypeDescT::Type(u.dupe()),
                        name: name.dupe(),
                        custom_error_loc: custom_error_loc.dupe(),
                    }),
                )),
                Arc::new(use_op),
            );
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, l, t)
        }

        // **********************
        // Numeric string keys
        // **********************

        // This type is only to be used to represent numeric-like object keys in
        // the context of object-to-object subtyping.
        // It is a subtype of both string and number, so both of these are OK:
        // ```js
        // const o = {1: true};
        // o as {[number]: boolean}: // OK
        // o as {[string]: boolean}: // OK
        // ```
        (
            DefT(rl, deref!(NumericStrKeyT(actual_lit))),
            DefT(ru, deref!(SingletonNumT { value: expected_lit, .. })),
        ) => {
            if actual_lit.0 != expected_lit.0 {
                flow_js_utils::add_output_with_env(
                    cx,env,
                    ErrorMessage::EExpectedNumberLit(Box::new(EExpectedNumberLitData {
                        lower: flow_js_utils::type_reference_with_reason_for_error(l, rl.dupe()),
                        upper: flow_js_utils::type_reference_with_reason_for_error(u, ru.dupe()),
                        use_op,
                    })),
                )?;
            }
            Ok(())
        }
        (
            DefT(rl, deref!(NumericStrKeyT(actual_lit))),
            DefT(ru, deref!(SingletonStrT { value: expected, .. })),
        ) => {
            if actual_lit.1 != *expected {
                flow_js_utils::add_output_with_env(
                    cx,env,
                    ErrorMessage::EExpectedStringLit(Box::new(EExpectedStringLitData {
                        lower: flow_js_utils::type_reference_with_reason_for_error(l, rl.dupe()),
                        upper: flow_js_utils::type_reference_with_reason_for_error(u, ru.dupe()),
                        use_op,
                    })),
                )?;
            }
            Ok(())
        }
        (_, DefT(r, deref!(NumericStrKeyT(num_lit)))) => {
            let u_new = Type::new(DefT(
                r.dupe(),
                DefT::new(SingletonStrT {
                    value: num_lit.1.dupe(),
                    from_annot: false,
                }),
            ));
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, l, &u_new)
        }

        // **********************
        // Singletons and keys
        // **********************

        // Finite keysets over arbitrary objects can be represented by KeysT. While
        // it is possible to also represent singleton string types using KeysT (by
        // taking the keyset of an object with a single property whose key is that
        // string and whose value is ignored), we can model them more directly
        // using SingletonStrT. Specifically, SingletonStrT models a type
        // annotation that looks like a string literal, which describes a singleton
        // set containing that string literal. Going further, other uses of KeysT
        // where the underlying object is created solely for the purpose of
        // describing a keyset can be modeled using unions of singleton strings.
        //
        // One may also legitimately wonder why a SingletonStrT with value `key`
        // cannot always be replaced by the literal string type of `key`. The
        // reason is that types of the latter form (string literal types) are
        // inferred to be the type of string
        // literals appearing as values, and we don't want to prematurely narrow
        // down the type of the location where such values may appear, since that
        // would preclude other strings to be stored in that location. Thus, by
        // necessity we allow all string types to flow to SingletonStrT (whereas only
        // exactly matching string literal types may flow to SingletonStrT).
        (
            DefT(rl, deref!(SingletonStrT { value: actual, .. })),
            DefT(ru, deref!(SingletonStrT { value: expected, .. })),
        ) => {
            if expected == actual {
                flow_js_utils::update_lit_type_from_annot(cx,env,  l);
            } else {
                // TODO: ordered_reasons should not be necessary
                let (rl, ru) = flow_js_utils::ordered_reasons(cx, (rl.dupe(), ru.dupe()));
                flow_js_utils::add_output_with_env(
                    cx,env,
                    ErrorMessage::EExpectedStringLit(Box::new(EExpectedStringLitData {
                        lower: flow_js_utils::type_reference_with_reason_for_error(l, rl),
                        upper: flow_js_utils::type_reference_with_reason_for_error(u, ru),
                        use_op,
                    })),
                )?;
            }
            Ok(())
        }
        (DefT(rl, deref!(StrGeneralT(_))), DefT(ru, deref!(SingletonStrT { .. }))) => {
            // TODO: ordered_reasons should not be necessary
            let (rl, ru) = flow_js_utils::ordered_reasons(cx, (rl.dupe(), ru.dupe()));
            flow_js_utils::add_output_with_env(
                cx,env,
                ErrorMessage::EExpectedStringLit(Box::new(EExpectedStringLitData {
                    lower: flow_js_utils::type_reference_with_reason_for_error(l, rl),
                    upper: flow_js_utils::type_reference_with_reason_for_error(u, ru),
                    use_op,
                })),
            )?;
            Ok(())
        }
        (
            DefT(rl, deref!(SingletonNumT { value: actual, .. })),
            DefT(ru, deref!(SingletonNumT { value: expected, .. })),
        ) => {
            if expected.0 == actual.0 {
                flow_js_utils::update_lit_type_from_annot(cx,env,  l);
            } else {
                // TODO: ordered_reasons should not be necessary
                let (rl, ru) = flow_js_utils::ordered_reasons(cx, (rl.dupe(), ru.dupe()));
                flow_js_utils::add_output_with_env(
                    cx,env,
                    ErrorMessage::EExpectedNumberLit(Box::new(EExpectedNumberLitData {
                        lower: flow_js_utils::type_reference_with_reason_for_error(l, rl),
                        upper: flow_js_utils::type_reference_with_reason_for_error(u, ru),
                        use_op,
                    })),
                )?;
            }
            Ok(())
        }
        (DefT(rl, deref!(NumGeneralT(_))), DefT(ru, deref!(SingletonNumT { .. }))) => {
            // TODO: ordered_reasons should not be necessary
            let (rl, ru) = flow_js_utils::ordered_reasons(cx, (rl.dupe(), ru.dupe()));
            flow_js_utils::add_output_with_env(
                cx,env,
                ErrorMessage::EExpectedNumberLit(Box::new(EExpectedNumberLitData {
                    lower: flow_js_utils::type_reference_with_reason_for_error(l, rl),
                    upper: flow_js_utils::type_reference_with_reason_for_error(u, ru),
                    use_op,
                })),
            )?;
            Ok(())
        }
        (
            DefT(rl, deref!(SingletonBoolT { value: actual, .. })),
            DefT(ru, deref!(SingletonBoolT { value: expected, .. })),
        ) => {
            if expected != actual {
                // TODO: ordered_reasons should not be necessary
                let (rl, ru) = flow_js_utils::ordered_reasons(cx, (rl.dupe(), ru.dupe()));
                flow_js_utils::add_output_with_env(
                    cx,env,
                    ErrorMessage::EExpectedBooleanLit(Box::new(EExpectedBooleanLitData {
                        lower: flow_js_utils::type_reference_with_reason_for_error(l, rl),
                        upper: flow_js_utils::type_reference_with_reason_for_error(u, ru),
                        use_op,
                    })),
                )?;
            }
            Ok(())
        }
        (DefT(rl, deref!(BoolGeneralT)), DefT(ru, deref!(SingletonBoolT { .. }))) => {
            // TODO: ordered_reasons should not be necessary
            let (rl, ru) = flow_js_utils::ordered_reasons(cx, (rl.dupe(), ru.dupe()));
            flow_js_utils::add_output_with_env(
                cx,env,
                ErrorMessage::EExpectedBooleanLit(Box::new(EExpectedBooleanLitData {
                    lower: flow_js_utils::type_reference_with_reason_for_error(l, rl),
                    upper: flow_js_utils::type_reference_with_reason_for_error(u, ru),
                    use_op,
                })),
            )
        }
        (
            DefT(rl, deref!(SingletonBigIntT { value: actual, .. })),
            DefT(ru, deref!(SingletonBigIntT { value: expected, .. })),
        ) => {
            if expected.0 != actual.0 {
                // TODO: ordered_reasons should not be necessary
                let (rl, ru) = flow_js_utils::ordered_reasons(cx, (rl.dupe(), ru.dupe()));
                flow_js_utils::add_output_with_env(
                    cx,env,
                    ErrorMessage::EExpectedBigIntLit(Box::new(EExpectedBigIntLitData {
                        lower: flow_js_utils::type_reference_with_reason_for_error(l, rl),
                        upper: flow_js_utils::type_reference_with_reason_for_error(u, ru),
                        use_op,
                    })),
                )?;
            }
            Ok(())
        }
        (DefT(rl, deref!(BigIntGeneralT(_))), DefT(ru, deref!(SingletonBigIntT { .. }))) => {
            // TODO: ordered_reasons should not be necessary
            let (rl, ru) = flow_js_utils::ordered_reasons(cx, (rl.dupe(), ru.dupe()));
            flow_js_utils::add_output_with_env(
                cx,env,
                ErrorMessage::EExpectedBigIntLit(Box::new(EExpectedBigIntLitData {
                    lower: flow_js_utils::type_reference_with_reason_for_error(l, rl),
                    upper: flow_js_utils::type_reference_with_reason_for_error(u, ru),
                    use_op,
                })),
            )
        }

        // **********************************
        // * unique symbol ~> unique symbol *
        // **********************************
        (DefT(_, deref!(UniqueSymbolT(data1))), DefT(_, deref!(UniqueSymbolT(data2))))
            if data1.symbol == data2.symbol =>
        {
            Ok(())
        }
        (DefT(_, deref!(UniqueSymbolT(_))), DefT(_, deref!(UniqueSymbolT(_)))) => {
            flow_js_utils::add_output_with_env(
                cx,env,
                flow_js_utils::incompatible_types_error(l, u, use_op, None),
            )
        }
        // symbol ~> unique symbol: ERROR
        (DefT(_, deref!(SymbolT)), DefT(_, deref!(UniqueSymbolT(_)))) => {
            flow_js_utils::add_output_with_env(
                cx,env,
                flow_js_utils::incompatible_types_error(l, u, use_op, None),
            )
        }
        // ****************************************************
        // keys
        // ****************************************************
        (DefT(reason_s, deref!(SingletonStrT { value: x, .. })), KeysT(reason_op, o)) => {
            flow_js_utils::update_lit_type_from_annot(cx,env,  l);
            let reason_next = reason_s
                .dupe()
                .replace_desc_new(VirtualReasonDesc::RProperty(Some(Name::new(x.dupe()))));
            // check that o has key x
            let u_use = UseT::new(HasOwnPropT(Box::new(HasOwnPropTData {
                use_op: use_op.dupe(),
                reason: reason_next,
                type_: l.dupe(),
            })));
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                o,
                &UseT::new(ReposLowerT {
                    reason: reason_op.dupe(),
                    use_desc: false,
                    use_t: Box::new(u_use),
                }),
            )
        }
        (DefT(reason_s, ld @ deref!(UniqueSymbolT(_) | SymbolT)), KeysT(reason_op, o)) => {
            // A `unique symbol` names one property, so `HasOwnPropT` can find it
            // by nominal identity. A general `symbol` names none, so it holds
            // only where an indexer accepts a symbol key.
            let desc = match ld.deref() {
                UniqueSymbolT(data) => {
                    VirtualReasonDesc::RProperty(Some(Name::symbol(data.symbol.dupe())))
                }
                _ => VirtualReasonDesc::RProperty(None),
            };
            let reason_next = reason_s.dupe().replace_desc_new(desc);
            // check that o has the symbol key
            let u_use = UseT::new(HasOwnPropT(Box::new(HasOwnPropTData {
                use_op: use_op.dupe(),
                reason: reason_next,
                type_: l.dupe(),
            })));
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                o,
                &UseT::new(ReposLowerT {
                    reason: reason_op.dupe(),
                    use_desc: false,
                    use_t: Box::new(u_use),
                }),
            )
        }
        (
            GenericT(box GenericTData {
                reason: reason_s,
                bound: deref!(DefT(_, deref!(SingletonStrT { value: x, .. }))),
                ..
            }),
            KeysT(reason_op, o),
        ) => {
            flow_js_utils::update_lit_type_from_annot(cx,env,  l);
            let reason_next = reason_s
                .dupe()
                .replace_desc_new(VirtualReasonDesc::RProperty(Some(Name::new(x.dupe()))));
            // check that o has key x
            let u_use = UseT::new(HasOwnPropT(Box::new(HasOwnPropTData {
                use_op: use_op.dupe(),
                reason: reason_next,
                type_: l.dupe(),
            })));
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                o,
                &UseT::new(ReposLowerT {
                    reason: reason_op.dupe(),
                    use_desc: false,
                    use_t: Box::new(u_use),
                }),
            )
        }
        (DefT(reason_s, deref!(StrGeneralT(_))), KeysT(reason_op, o)) => {
            let reason_next = reason_s.dupe().replace_desc_new(None);
            let u_use = UseT::new(HasOwnPropT(Box::new(HasOwnPropTData {
                use_op: use_op.dupe(),
                reason: reason_next,
                type_: l.dupe(),
            })));
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                o,
                &UseT::new(ReposLowerT {
                    reason: reason_op.dupe(),
                    use_desc: false,
                    use_t: Box::new(u_use),
                }),
            )
        }
        (
            GenericT(box GenericTData {
                reason: reason_s,
                bound: deref!(DefT(_, deref!(StrGeneralT(_)))),
                ..
            }),
            KeysT(reason_op, o),
        ) => {
            let reason_next = reason_s.dupe().replace_desc_new(None);
            let u_use = UseT::new(HasOwnPropT(Box::new(HasOwnPropTData {
                use_op: use_op.dupe(),
                reason: reason_next,
                type_: l.dupe(),
            })));
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                o,
                &UseT::new(ReposLowerT {
                    reason: reason_op.dupe(),
                    use_desc: false,
                    use_t: Box::new(u_use),
                }),
            )
        }
        (DefT(reason_s, deref!(NumericStrKeyT(num_lit))), KeysT(reason_op, o)) => {
            let reason_next = reason_s
                .dupe()
                .replace_desc_new(VirtualReasonDesc::RProperty(Some(Name::new(
                    num_lit.1.as_str(),
                ))));
            let l_new = Type::new(DefT(
                reason_s.dupe(),
                DefT::new(SingletonStrT {
                    value: num_lit.1.dupe(),
                    from_annot: false,
                }),
            ));
            let u_use = UseT::new(HasOwnPropT(Box::new(HasOwnPropTData {
                use_op: use_op.dupe(),
                reason: reason_next,
                type_: l_new,
            })));
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                o,
                &UseT::new(ReposLowerT {
                    reason: reason_op.dupe(),
                    use_desc: false,
                    use_t: Box::new(u_use),
                }),
            )
        }
        (
            GenericT(box GenericTData {
                reason: reason_s,
                bound: deref!(DefT(_, deref!(NumericStrKeyT(num_lit)))),
                ..
            }),
            KeysT(reason_op, o),
        ) => {
            let reason_next = reason_s
                .dupe()
                .replace_desc_new(VirtualReasonDesc::RProperty(Some(Name::new(
                    num_lit.1.as_str(),
                ))));
            let l_new = Type::new(DefT(
                reason_s.dupe(),
                DefT::new(SingletonStrT {
                    value: num_lit.1.dupe(),
                    from_annot: false,
                }),
            ));
            let u_use = UseT::new(HasOwnPropT(Box::new(HasOwnPropTData {
                use_op: use_op.dupe(),
                reason: reason_next,
                type_: l_new,
            })));
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                o,
                &UseT::new(ReposLowerT {
                    reason: reason_op.dupe(),
                    use_desc: false,
                    use_t: Box::new(u_use),
                }),
            )
        }
        (
            TemplateLiteralT {
                reason: reason_s,
                from_annot,
                quasis,
                types,
            },
            KeysT(reason_op, o),
        ) => {
            let l_new = Type::new(TemplateLiteralT {
                reason: reason_s.dupe(),
                from_annot: *from_annot,
                quasis: quasis.to_vec(),
                types: types.to_vec(),
            });
            let u_use = UseT::new(HasOwnPropT(Box::new(HasOwnPropTData {
                use_op: use_op.dupe(),
                reason: reason_s.dupe(),
                type_: l_new,
            })));
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                o,
                &UseT::new(ReposLowerT {
                    reason: reason_op.dupe(),
                    use_desc: false,
                    use_t: Box::new(u_use),
                }),
            )
        }
        (
            GenericT(box GenericTData {
                reason: reason_s,
                bound: deref!(TemplateLiteralT { from_annot, quasis, types, .. }),
                ..
            }),
            KeysT(reason_op, o),
        ) => {
            let l_new = Type::new(TemplateLiteralT {
                reason: reason_s.dupe(),
                from_annot: *from_annot,
                quasis: quasis.to_vec(),
                types: types.to_vec(),
            });
            let u_use = UseT::new(HasOwnPropT(Box::new(HasOwnPropTData {
                use_op: use_op.dupe(),
                reason: reason_s.dupe(),
                type_: l_new,
            })));
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                o,
                &UseT::new(ReposLowerT {
                    reason: reason_op.dupe(),
                    use_desc: false,
                    use_t: Box::new(u_use),
                }),
            )
        }
        (KeysT(_, o1), KeysT(_, o2))
            if flow_js_utils::is_same_non_generic_instance_annot(cx, o1, o2) =>
        {
            Ok(())
        }
        (KeysT(reason1, o1), _) => {
            // flow all keys of o1 to u
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                o1,
                &UseT::new(GetKeysT {
                    reason: reason1.dupe(),
                    t_out: Box::new(UseT::new(UseT(use_op, u.dupe()))),
                    include_symbols: true,
                }),
            )
        }

        // ********************************************
        // Using predicate functions as regular ones
        // ********************************************
        (UnionT(reason, rep), _)
            if rep.members_iter().any(flow_js_utils::is_union_resolvable) =>
        {
            flow_js_utils::iter_resolve_union(
                |cx, trace, (t, u)| FlowJs::rec_flow_with_env(cx, env, *trace, &t, &u),
                cx,
                &trace,
                reason.dupe(),
                rep,
                UseT::new(UseT(use_op, u.dupe())),
            )
        }
        (UnionT(_, rep), UnionT(_, _)) => {
            union_to_union(cx, env, trace, use_op, l, rep, u)
        }
        (
            NominalT {
                nominal_type: lnom, ..
            },
            UnionT(_, _),
        ) if lnom.upper_t.as_ref().is_some_and(|upper| {
            matches!(upper.deref(), UnionT(_, _))
                && matches!(
                    flow_js_utils::union_optimization_guard(
                        cx,
                        |t1, t2| type_util::quick_subtype(None::<&mut fn(&Type)>, t1, t2),
                        upper,
                        u,
                    ),
                    flow_js_utils::UnionOptimizationGuardResult::True
                )
        }) =>
        {
            if cx.is_verbose() {
                eprintln!("UnionT ~> UnionT fast path (via an opaque type)");
            }
            Ok(())
        }
        // Optimization to treat maybe and optional types as special unions for subset comparision
        (UnionT(reason, rep), MaybeT(r, maybe)) => {
            let void = Type::new(DefT(r.dupe(), DefT::new(VoidT)));
            let null = Type::new(DefT(r.dupe(), DefT::new(NullT)));
            let maybe = type_util::push_type_alias_reason(r, maybe.dupe());
            // if the union doesn't contain void or null,
            // then everything in it must be upper-bounded by maybe
            match (
                union_rep::quick_mem_enum(
                    cx,
                    |_, t1, t2| type_util::quick_subtype(None::<&mut fn(&Type)>, t1, t2),
                    &void,
                    rep,
                ),
                union_rep::quick_mem_enum(
                    cx,
                    |_, t1, t2| type_util::quick_subtype(None::<&mut fn(&Type)>, t1, t2),
                    &null,
                    rep,
                ),
            ) {
                (union_rep::QuickMemResult::No, union_rep::QuickMemResult::No) => {
                    FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, l, &maybe)
                }
                (union_rep::QuickMemResult::Yes, union_rep::QuickMemResult::No) => {
                    let filtered = flow_js_utils::remove_predicate_from_union(
                        reason.dupe(),
                        cx,
                        |t| type_util::quick_subtype(None::<&mut fn(&Type)>, t, &void),
                        rep,
                    );
                    FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &filtered, &maybe)
                }
                (union_rep::QuickMemResult::No, union_rep::QuickMemResult::Yes) => {
                    let filtered = flow_js_utils::remove_predicate_from_union(
                        reason.dupe(),
                        cx,
                        |t| type_util::quick_subtype(None::<&mut fn(&Type)>, t, &null),
                        rep,
                    );
                    FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &filtered, &maybe)
                }
                (union_rep::QuickMemResult::Yes, union_rep::QuickMemResult::Yes) => {
                    let filtered = flow_js_utils::remove_predicate_from_union(
                        reason.dupe(),
                        cx,
                        |t| {
                            type_util::quick_subtype(None::<&mut fn(&Type)>, t, &void)
                                || type_util::quick_subtype(None::<&mut fn(&Type)>, t, &null)
                        },
                        rep,
                    );
                    FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &filtered, &maybe)
                }
                _ => flow_all_in_union(
                    cx, env,
                    &trace,
                    rep,
                    &UseT::new(UseT(use_op, u.dupe())),
                ),
            }
        }
        (
            UnionT(reason, rep),
            OptionalT {
                reason: r,
                type_: opt,
                use_desc: opt_use_desc,
            },
        ) => {
            let void = void::why_with_use_desc(*opt_use_desc, r.dupe());
            // if the union doesn't contain void, then everything in it must be upper-bounded by u
            match union_rep::quick_mem_enum(
                cx,
                |_, t1, t2| type_util::quick_subtype(None::<&mut fn(&Type)>, t1, t2),
                &void,
                rep,
            ) {
                union_rep::QuickMemResult::No => FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, l, opt),
                union_rep::QuickMemResult::Yes => {
                    let filtered = flow_js_utils::remove_predicate_from_union(
                        reason.dupe(),
                        cx,
                        |t| type_util::quick_subtype(None::<&mut fn(&Type)>, t, &void),
                        rep,
                    );
                    FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &filtered, opt)
                }
                _ => flow_all_in_union(
                    cx, env,
                    &trace,
                    rep,
                    &UseT::new(UseT(use_op, u.dupe())),
                ),
            }
        }
        (UnionT(_, _), IntersectionT(_, rep)) => {
            if cx.is_verbose() {
                eprintln!("UnionT ~> IntersectionT slow case");
            }
            for t in rep.members_iter() {
                FlowJs::rec_flow_with_env(
                    cx, env,
                    trace,
                    l,
                    &UseT::new(UseT(use_op.dupe(), t.dupe())),
                )?;
            }
            Ok(())
        }
        (UnionT(_, rep), _) => {
            if cx.is_verbose() {
                match u.deref() {
                    UnionT(..) => eprintln!("UnionT ~> UnionT slow case"),
                    IntersectionT(..) => {
                        eprintln!("UnionT ~> IntersectionT slow case")
                    }
                    _ => {}
                }
            }
            if !rep.is_optimized_finally() {
                rep.optimize_enum_only(cx, |cx, t| type_mapper::union_flatten(cx, t.duped()));
            }
            match rep.check_enum_with_tag() {
                Some((enums, Some(tag))) => {
                    let mut members = rep.members_iter();
                    match members.next() {
                        None => {}
                        Some(representative) => {
                            let representative = representative.dupe();
                            drop(members);
                            match speculation_kit::try_singleton_throw_on_failure(
                                cx, env,
                                trace,
                                representative.dupe(),
                                UseT::new(UseT(use_op.dupe(), u.dupe())),
                            ) {
                                Err(FlowJsException::SpeculationSingletonError) => {
                                    let use_op = flow_js_utils::union_representative_use_op(
                                        cx,
                                        l,
                                        &representative,
                                        use_op,
                                    );
                                    FlowJs::rec_flow_with_env(
                                        cx, env,
                                        trace,
                                        &representative,
                                        &UseT::new(UseT(use_op, u.dupe())),
                                    )?;
                                }
                                Ok(()) => {
                                    let singleton_check =
                                        |cx: &mut Context<'cx>, elt: UnionEnum| -> Result<(), FlowJsException> {
                                            if Some(tag) == union_rep::tag_of_member(u) {
                                                if !enums.iter().all(|e| *e == elt) {
                                                    flow_js_utils::add_output_with_env(
                                                        cx,env,
                                                        flow_js_utils::incompatible_types_error(
                                                            l,
                                                            u,
                                                            use_op.dupe(),
                                                            None,
                                                        ),
                                                    )?;
                                                }
                                            } else {
                                                flow_all_in_union(
                                                    cx, env,
                                                    &trace,
                                                    rep,
                                                    &UseT::new(UseT(
                                                        use_op.dupe(),
                                                        u.dupe(),
                                                    )),
                                                )?;
                                            }
                                            Ok(())
                                        };
                                    match u.deref() {
                                        DefT(_, deref!(SingletonStrT { value: x, .. })) => {
                                            singleton_check(cx, UnionEnum::Str(x.dupe()))?;
                                        }
                                        DefT(_, deref!(SingletonBoolT { value: x, .. })) => {
                                            singleton_check(cx, UnionEnum::Bool(*x))?;
                                        }
                                        DefT(_, deref!(SingletonNumT { value: x, .. })) => {
                                            singleton_check(cx, UnionEnum::Num(x.dupe()))?;
                                        }
                                        _ => {
                                            flow_all_in_union(
                                                cx, env,
                                                &trace,
                                                rep,
                                                &UseT::new(UseT(use_op, u.dupe())),
                                            )?;
                                        }
                                    }
                                }
                                Err(e) => return Err(e),
                            }
                        }
                    }
                    Ok(())
                }
                _ => flow_all_in_union(
                    cx, env,
                    &trace,
                    rep,
                    &UseT::new(UseT(use_op, u.dupe())),
                ),
            }
        }
        (_, IntersectionT(_, rep)) => {
            if cx.is_verbose() {
                if let UnionT(..) = l.deref() {
                    eprintln!("IntersectionT ~> UnionT slow case");
                }
            }
            for t in rep.members_iter() {
                FlowJs::rec_flow_with_env(
                    cx, env,
                    trace,
                    l,
                    &UseT::new(UseT(use_op.dupe(), t.dupe())),
                )?;
            }
            Ok(())
        }
        // String enum sets can be handled in logarithmic time by just
        // checking for membership in the set.
        (DefT(_reason_l, deref!(SingletonStrT { value: x, .. })), UnionT(_reason_u, rep))
            if {
                    if let Some(enums) = rep.check_enum() {
                        if !enums.contains(&UnionEnum::Str(x.dupe())) {
                            flow_js_utils::add_output_with_env(
                                cx,env,
                                flow_js_utils::incompatible_types_error(
                                    l,
                                    u,
                                    use_op.dupe(),
                                    None,
                                ),
                            )?;
                        }
                        true
                    } else {
                        false
                    }
                } =>
        {
            flow_js_utils::update_lit_type_from_annot(cx,env,  l);
            Ok(())
        }
        // Number, boolean and bigint literals in an enum union are found by membership in the
        // set, like string literals above, instead of by comparing against every member below.
        // Unlike strings, a miss falls through rather than erroring: the set compares bigints
        // by source text as well as value, so `0x1n` misses `1n` but is still a subtype of it.
        (
            DefT(_, deref!(SingletonNumT { .. } | SingletonBoolT { .. } | SingletonBigIntT { .. })),
            UnionT(_, rep),
        ) if let Some(member) = union_rep::canon(l)
            && rep.check_enum().is_some_and(|enums| enums.contains(&member)) =>
        {
            flow_js_utils::update_lit_type_from_annot(cx, env, l);
            Ok(())
        }
        (_, UnionT(_, rep))
            if {
                let ts: Vec<Type> = type_mapper::union_flatten(cx, rep.members_iter().duped());
                ts.iter().any(|t| {
                    type_util::quick_subtype(
                        Some(&mut |t: &Type| flow_js_utils::update_lit_type_from_annot(cx,env,  t)),
                        l,
                        t,
                    )
                })
            } =>
        {
            Ok(())
        }
        (DefT(renders_r, deref!(RendersT(_))), UnionT(r, rep)) => {
            // This is a tricky case because there are multiple ways that it could pass. Either
            //  the union contains a supertype of the LHS, or the Union itself is a super type of
            //  React.Node, in which case we can pass without splitting the union. Crucially, if the
            //  union is a super type of React.Node then splitting the union too early will cause
            //  spurious errors.
            //
            //  This is further complicated during implicit instantiation, where the union may contain
            //  implicitly instantiated tvars that should be constrained by the LHS.
            //
            //  To handle these cases, we first check to see if the union contains any implicitly instantiated
            //  tvars. If so, we start speculation. If not, we try to see if the RHS is a supertype of React.Node
            //  before kicking off regular speculation
            let union_contains_instantiable_tvars = if env.in_implicit_instantiation() {
                rep.members_iter().any(|t| {
                    if let Some(node) = type_util::constraint_node_id(t) {
                        use constraint::Constraints;
                        match cx.find_constraints(node.id()).1 {
                            Constraints::Resolved(_) | Constraints::FullyResolved(_) => false,
                            Constraints::Unresolved(_) => {
                                flow_common::reason::is_instantiable_reason(
                                    type_util::reason_of_t(t),
                                )
                            }
                        }
                    } else {
                        false
                    }
                })
            } else {
                false
            };
            let node = FlowJs::get_builtin_react_type_with_env(
                cx, env,
                None,
                renders_r,
                Some(true),
                intermediate_error_types::ExpectedModulePurpose::ReactModuleForReactNodeType,
            )?;
            if union_contains_instantiable_tvars
                || !FlowJs::speculative_subtyping_succeeds_with_flow_errors(cx, env, &node, u)?
            {
                speculation_kit::try_union(cx, env, None, trace, use_op, l.dupe(), r.dupe(), rep)?;
            }
            Ok(())
        }
        // The following case distributes the opaque constructor over a union/maybe/optional
        // type in the super type position of the opaque type.
        (
            NominalT {
                reason: lreason,
                nominal_type: lnom,
            },
            UnionT(r, rep),
        ) if lnom.upper_t.is_some() => {
            let upper_t = lnom.upper_t.as_ref().unwrap();
            let ts = FlowJs::possible_concrete_types_for_inspection_with_env(
                cx, env,
                type_util::reason_of_t(upper_t),
                upper_t,
            )?;
            match ts.as_slice() {
                [] | [_] => {
                    // Same as `_ ~> UnionT` case below
                    speculation_kit::try_union(cx, env, None, trace, use_op, l.dupe(), r.dupe(), rep)
                }
                [lt1, lt2, lts @ ..] => {
                    let make_opaque = |t: Type| -> Type {
                        Type::new(NominalT {
                            reason: lreason.dupe(),
                            nominal_type: Rc::new(NominalType::new(NominalTypeInner {
                                nominal_id: lnom.nominal_id.clone(),
                                underlying_t: lnom.underlying_t.clone(),
                                lower_t: lnom.lower_t.clone(),
                                upper_t: Some(t),
                                nominal_type_args: lnom.nominal_type_args.dupe(),
                            })),
                        })
                    };
                    let members: Rc<[Type]> =
                        lts.iter().map(|t: &Type| make_opaque(t.dupe())).collect();
                    let union_of_opaques = union_rep::make(
                        None,
                        union_rep::UnionKind::UnknownKind,
                        make_opaque(lt1.dupe()),
                        make_opaque(lt2.dupe()),
                        members,
                    );
                    let union_t = Type::new(UnionT(lreason.dupe(), union_of_opaques));
                    FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &union_t, u)
                }
            }
        }
        (TemplateLiteralT { quasis, types, .. }, UnionT(_, _))
            if template_literal_type::try_resolve_to_strings(quasis, types, true).is_some() =>
        {
            template_literal_type::subtype_template_into_union(
                cx, env, trace, use_op, l, u, quasis, types,
            )
        }
        (_, UnionT(r, rep)) => {
            // Try the branches of the union in turn, with the goal of selecting the
            // correct branch. This process is reused for intersections as well. See
            // comments on try_union and try_intersection.
            let l_for_annot = l.dupe();
            let on_success: Box<dyn FnOnce(&mut Context<'cx>, &FlowJsEnv)> =
                Box::new(move |cx, env| {
                    flow_js_utils::update_lit_type_from_annot(cx, env, &l_for_annot);
                });
            speculation_kit::try_union(cx, env, Some(on_success), trace, use_op, l.dupe(), r.dupe(), rep)
        }
        // maybe types are special union types
        // This target arm otherwise strips nullishness before opaque upper-bound
        // handling runs. Preserve explicit nullish upper bounds against the original
        // nullable target while retaining the stripped-target path as a fallback.
        (
            NominalT {
                reason: opaque_t_reason,
                nominal_type: lnom,
            },
            MaybeT(r2, t2),
        ) if let Some(upper_t) = &lnom.upper_t
            && has_syntactic_nullish_member(upper_t) =>
        {
            OpaqueUpperBoundSpeculation {
                trace,
                use_op,
                opaque_t_reason,
                upper_t,
                upper_u: u,
                fallback_l: l,
                fallback_t: type_util::push_type_alias_reason(r2, t2.dupe()),
            }
            .flow(cx, env)
        }
        (_, MaybeT(r2, t2)) => {
            let t2 = type_util::push_type_alias_reason(r2, t2.dupe());
            FlowJs::rec_flow_with_env(cx, env, trace, l, &UseT::new(UseT(use_op, t2)))
        }
        (
            NominalT {
                reason: opaque_t_reason,
                nominal_type: lnom,
            },
            OptionalT { type_: t2, .. },
        ) if let Some(upper_t) = &lnom.upper_t
            && has_syntactic_nullish_member(upper_t) =>
        {
            OpaqueUpperBoundSpeculation {
                trace,
                use_op,
                opaque_t_reason,
                upper_t,
                upper_u: u,
                fallback_l: l,
                // Match the existing OptionalT target path, which forwards its inner type directly.
                fallback_t: t2.dupe(),
            }
            .flow(cx, env)
        }
        (_, OptionalT { type_: t2, .. }) => {
            FlowJs::rec_flow_with_env(cx, env, trace, l, &UseT::new(UseT(use_op, t2.dupe())))
        }
        // object types: an intersection may satisfy an object UB without
        // any particular member of the intersection doing so completely.
        // Here we trap object UBs with more than one property, and
        // decompose them into singletons.
        //
        // This trap is skipped for exact objects, since intersections of inexact objects
        // can never satisfy exact objects, but it might cause spurious errors.
        //
        // Note: should be able to do this with LookupT rather than
        // slices, but that approach behaves in nonobvious ways. TODO why?
        (IntersectionT(_, _), DefT(r, deref!(ObjT(obj))))
            if cx.find_props(obj.props_tmap.dupe()).len() > 1
                && obj.flags.obj_kind != ObjKind::Exact =>
        {
            if cx.is_verbose() {
                eprintln!("trapped");
            }
            for (x, p) in cx.find_props(obj.props_tmap.dupe()).iter() {
                let pmap =
                    properties::PropertiesMap::from_btree_map(std::collections::BTreeMap::from([
                        (x.dupe(), p.clone()),
                    ]));
                let id = cx.generate_property_map(pmap);
                let new_obj = ObjType {
                    flags: obj.flags.clone(),
                    props_tmap: id,
                    proto_t: dummy_prototype(),
                    call_t: obj.call_t,
                    reachable_targs: Rc::from([]),
                    strictness_kind: obj.strictness_kind,
                    literal: obj.literal,
                };
                FlowJs::rec_flow_with_env(
                    cx, env,
                    trace,
                    l,
                    &UseT::new(UseT(
                        use_op.dupe(),
                        Type::new(DefT(
                            r.dupe(),
                            DefT::new(ObjT(Rc::new(new_obj))),
                        )),
                    )),
                )?;
            }
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                l,
                &UseT::new(UseT(use_op, obj.proto_t.dupe())),
            )
        }

        // (After the above preprocessing step, try the branches of the intersection
        // in turn, with the goal of selecting the correct branch. This process is
        // reused for unions as well. See comments on try_union and
        // try_intersection.)
        (IntersectionT(_, rep), _) => speculation_kit::try_intersection(
            cx, env,
            trace,
            UseT::new(UseT(use_op, u.dupe())),
            l.dupe(),
            rep,
        ),
        (NullProtoT(reason), _) => {
            let null = Type::new(DefT(reason.dupe(), DefT::new(NullT)));
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &null, u)
        }

        // Subtyping arm bodies live in template_literal_type; patterns and
        // guards stay here so dispatch order remains visible.
        (DefT(_, deref!(SingletonStrT { value, .. })), TemplateLiteralT { quasis, types, .. }) => {
            template_literal_type::subtype_str_lit_into_template(
                Some(&|cx, env: &FlowJsEnv, r, t| FlowJs::possible_concrete_types_for_inspection_with_env(cx, env, r, t)),
                cx,
                env,
                trace,
                use_op,
                l,
                u,
                value.as_str(),
                quasis,
                types,
            )
        }
        // TemplateLiteralT <: TemplateLiteralT: dispatch through the combined
        // helper, which folds once and tries (in order) pairwise / prefix-
        // extension / suffix-extension / wide-string. Falls through to
        // subtype_template_to_other when none apply.
        (
            TemplateLiteralT {
                reason: l_reason,
                from_annot: l_from_annot,
                quasis: lq,
                types: lt,
            },
            TemplateLiteralT {
                quasis: rq,
                types: rt,
                ..
            },
        ) => match template_literal_type::try_subtype_template_to_template(
            cx, env,
            trace,
            use_op.dupe(),
            l_reason,
            *l_from_annot,
            u,
            lq,
            lt,
            rq,
            rt,
        )? {
            template_literal_type::TlToTlResult::Handled => {
                // The template was checked against a matching template
                // annotation: keep its precision through generalization,
                // like a singleton checked against a matching literal.
                flow_js_utils::update_lit_type_from_annot(cx, env, l);
                Ok(())
            }
            template_literal_type::TlToTlResult::NotApplicable => {
                template_literal_type::subtype_template_to_other(
                    cx, env, trace, use_op, l_reason, u, lq, lt,
                )
            }
        },
        (
            TemplateLiteralT {
                reason,
                quasis,
                types,
                ..
            },
            _,
        ) => template_literal_type::subtype_template_to_other(
            cx, env, trace, use_op, reason, u, quasis, types,
        ),
        // StringMappingT<kind, arg> on the right: we get here when `arg` couldn't
        //    be eagerly reduced by `string_case_transform::resolve` (generic,
        //    `string`, unresolved tvar, EvalT for indexed/property access, OpenT,
        //    AnnotT, ...). We first try to concretize `arg`: if it reduces to a
        //    shape `resolve` can enumerate, re-resolve and re-flow against the
        //    eager result. This catches cases hidden behind generic substitution
        //    or sig-merge that weren't eagerly resolved at annotation time. If
        //    concretization yields nothing useful, fall back to the canonical-form
        //    check: the only way a string can satisfy `Kind<arg>` for opaque `arg`
        //    is to (a) already be in canonical form for `kind` and (b) flow to `arg`.
        (DefT(_, deref!(SingletonStrT { value, .. })), StringMappingT { reason, kind, arg }) => {
            flow_js_utils::update_lit_type_from_annot(cx,env,  l);
            let arg_loc = reason.loc().dupe();
            let ts = FlowJs::possible_concrete_types_for_inspection_with_env(
                cx, env,
                type_util::reason_of_t(arg),
                arg,
            )?;
            let arg_concretized: Option<Type> = match ts.as_slice() {
                [] => None,
                [t] if Type::ptr_eq(t, arg) => None,
                [t] => Some(t.dupe()),
                [t0, t1, rest @ ..] => {
                    let rest_rc: Rc<[Type]> = rest.iter().cloned().collect();
                    Some(Type::new(UnionT(
                        type_util::reason_of_t(arg).dupe(),
                        union_rep::make(
                            Some(cx.make_aloc_id(&arg_loc)),
                            union_rep::UnionKind::UnknownKind,
                            t0.dupe(),
                            t1.dupe(),
                            rest_rc,
                        ),
                    )))
                }
            };
            match arg_concretized {
                Some(arg_prime) => {
                    let resolved = string_case_transform::resolve(
                        Some(&|cx: &mut Context<'_>, r: &Reason, t: &Type| {
                            FlowJs::possible_concrete_types_for_inspection_with_env(cx, env, r, t)
                        }),
                        cx,
                        *kind,
                        arg_loc,
                        arg_prime,
                    );
                    FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, l, &resolved)
                }
                None => {
                    if string_case_transform::is_canonical(*kind, value.as_str()) {
                        FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, l, arg)
                    } else {
                        flow_js_utils::add_output_with_env(
                            cx,env,
                            flow_js_utils::incompatible_types_error(
                                l,
                                u,
                                use_op,
                                Some(
                                    flow_typing_errors::intermediate_error_types::Explanation::ExplanationStringCasingMustBeCanonical {
                                        kind_name: FlowSmolStr::new_inline(
                                            string_case_transform::name_of_kind(*kind),
                                        ),
                                    },
                                ),
                            ),
                        )?;
                        Ok(())
                    }
                }
            }
        }
        // StringMappingT on the left: every string the deferred form could produce
        //    is a string, so flow `string` outward. The post-inference `arg <: string`
        //    check on the original annotation guarantees this is safe.
        (StringMappingT { reason, .. }, _) => {
            let str_t = Type::new(DefT(
                reason.dupe(),
                DefT::new(StrGeneralT(Literal::AnyLiteral)),
            ));
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &str_t, u)
        }

        // When do we consider a polymorphic type <X:U> T to be a subtype of another
        // polymorphic type <X:U'> T'? This is the subject of a long line of
        // research. A rule that works (Cardelli/Wegner) is: force U = U', and prove
        // that T is a subtype of T' for any X:U'. A more general rule that proves
        // that U' is a subtype of U instead of forcing U = U' is known to cause
        // undecidable subtyping (Pierce): the counterexamples are fairly
        // pathological, but can be reliably constructed by exploiting the "switch"
        // of bounds from U' to U (and back, with sufficient trickery), in ways that
        // are difficult to detect statically.
        //
        // However, these results are somewhat tricky to interpret in Flow, since we
        // are not proving stuff inductively: instead we are co-inductively assuming
        // what we want to prove, and checking consistency.
        //
        // Separately, none of these rules capture the logical interpretation of the
        // original subtyping question (interpreting subtyping as implication, and
        // polymorphism as universal quantification). What we really want to show is
        // that, for all X:U', there is some X:U such that T is a subtype of T'. But
        // we already deal with statements of this form when checking polymorphic
        // definitions! In particular, statements such as "there is some X:U...")
        // correspond to "create a type variable with that constraint and ...", and
        // statements such as "show that for all X:U" correspond to "introduce a
        // GenericT with bound U".
        //
        // Thus, all we need to do when checking that any type flows to a
        // polymorphic type is to follow the same principles used when checking that
        // a polymorphic definition has a polymorphic type. This has the pleasant
        // side effect that the type we're checking does not itself need to be a
        // polymorphic type at all! For example, we can let a non-generic method be
        // overridden with a generic method, as long as the non-generic signature
        // can be derived as a specialization of the generic signature.

        // some shortcuts
        (
            DefT(_, deref!(PolyT(box PolyTData { id: id1, .. }))),
            DefT(_, deref!(PolyT(box PolyTData { id: id2, .. }))),
        ) if id1 == id2 => {
            if cx.is_verbose() {
                eprintln!("PolyT ~> PolyT fast path");
            }
            Ok(())
        }
        (
            DefT(_, deref!(PolyT(box PolyTData {
                tparams: params1,
                t_out: t1,
                ..
            }))),
            DefT(_, deref!(PolyT(box PolyTData {
                tparams: params2,
                t_out: t2,
                ..
            }))),
        ) if params1.len() == params2.len() => {
            // A description of this subtyping rule can be found in "Decidable Bounded
            // Quantifcation" by G. Castagna B. Pierce.
            //
            //   G |- T1 <: S1   G, { X: S1 } |- S2 <: T2
            //   ------------------------------------------ (All-local)
            //   G |- forall X:S1 . S2 <: forall X:T1 . T2
            //
            // The code below implements this rule for the slightly more general case of
            // subtyping between two polymorphic types:
            //
            // forall a1:b1  , ... ai:bi  , ..., an:bn   . t
            // forall a1':b1', ... ai':bi', ..., an':bn' . t'
            //
            // 1st Premise
            // -----------
            // For each type parameter pair (ai:bi, ai':bi') we create the constraints:
            //
            // bi[ai, ..., a_(i-1)] <: bi'[ai'/ai, ..., a_(i-1)'/a_(i-1)]
            //
            // where ai is a GenericT
            let (map1, generics) = flow_js_utils::mk_tparams(cx, params1);
            let map2 = params2.iter().zip(generics).fold(
                flow_data_structure_wrapper::ord_map::FlowOrdMap::new(),
                |mut map, (param, generic)| {
                    map.insert(param.name.dupe(), generic);
                    map
                },
            );
            for (param1, param2) in params1.iter().zip(params2.iter()) {
                let bound1 = flow_typing_flow_common::type_subst::subst(
                    cx,
                    Some(use_op.dupe()),
                    true,
                    false,
                    flow_typing_flow_common::type_subst::Purpose::Normal,
                    &map1,
                    param1.bound.dupe(),
                );
                let bound2 = flow_typing_flow_common::type_subst::subst(
                    cx,
                    Some(use_op.dupe()),
                    true,
                    false,
                    flow_typing_flow_common::type_subst::Purpose::Normal,
                    &map2,
                    param2.bound.dupe(),
                );
                FlowJs::rec_flow_with_env(
                    cx, env,
                    trace,
                    &bound2,
                    &UseT::new(UseT(use_op.dupe(), bound1.dupe())),
                )?;
                if param1.is_const != param2.is_const {
                    flow_js_utils::add_output_with_env(
                        cx,env,
                        ErrorMessage::ETypeParamConstIncompatibility(Box::new(
                            ETypeParamConstIncompatibilityData {
                                use_op: use_op.dupe(),
                                lower: NamedReferenceData {
                                    loc: param1.reason.loc().dupe(),
                                    name: param1.name.string_of_subst_name().dupe(),
                                },
                                upper: NamedReferenceData {
                                    loc: param2.reason.loc().dupe(),
                                    name: param2.name.string_of_subst_name().dupe(),
                                },
                            },
                        )),
                    )?;
                }
            }
            // 2nd Premise
            // -----------
            // We check t <: t' after substituting ai' for ai
            let t1_subst = flow_typing_flow_common::type_subst::subst(
                cx,
                Some(use_op.dupe()),
                true,
                false,
                flow_typing_flow_common::type_subst::Purpose::Normal,
                &map1,
                t1.dupe(),
            );
            let t2_subst = flow_typing_flow_common::type_subst::subst(
                cx,
                Some(use_op.dupe()),
                true,
                false,
                flow_typing_flow_common::type_subst::Purpose::Normal,
                &map2,
                t2.dupe(),
            );
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &t1_subst, &t2_subst)?;
            Ok(())
        }
        // general case
        (_, DefT(_, deref!(PolyT(box PolyTData { t_out: t, .. })))) => {
            FlowJs::rec_flow_with_env(cx, env, trace, l, &UseT::new(UseT(use_op, t.dupe())))
        }
        // TODO: ideally we'd do the same when lower bounds flow to a
        // this-abstracted class, but fixing the class is easier; might need to
        // revisit
        #[rustfmt::skip]
        (
            _,
            DefT(class_r, deref!(ClassT(deref!(ThisInstanceT(box ThisInstanceTData {
                reason: inst_r, instance: i, is_this: this, subst_name: this_name,
            }))))),
        ) => {
            let reason = type_util::reason_of_t(l);
            let fixed = flow_js_utils::fix_this_instance(
                cx,
                reason.dupe(),
                inst_r.dupe(),
                i,
                *this,
                this_name.dupe(),
            );
            let class_t = Type::new(DefT(
                class_r.dupe(),
                DefT::new(ClassT(fixed)),
            ));
            FlowJs::rec_flow_with_env(cx, env, trace, l, &UseT::new(UseT(use_op, class_t)))
        }
        (
            _,
            ThisInstanceT(box ThisInstanceTData {
                reason: r,
                instance: i,
                is_this: this,
                subst_name: this_name,
            }),
        ) => {
            let reason = type_util::reason_of_t(l);
            let fixed = flow_js_utils::fix_this_instance(
                cx,
                reason.dupe(),
                r.dupe(),
                i,
                *this,
                this_name.dupe(),
            );
            FlowJs::rec_flow_with_env(cx, env, trace, l, &UseT::new(UseT(use_op, fixed)))
        }
        // This rule is hit when a polymorphic type appears outside a
        // type application expression - i.e. not followed by a type argument list
        // delimited by angle brackets.
        // We want to require full expressions in type positions like annotations,
        // but allow use of polymorphically-typed values - for example, in class
        // extends clauses and at function call sites - without explicit type
        // arguments, since typically they're easily inferred from context.
        #[rustfmt::skip]
        (
            DefT(reason_tapp, deref!(PolyT(box PolyTData {
                tparams_loc, tparams: ids, t_out: t, ..
            }))),
            _,
        ) => {
            let reason_op = type_util::reason_of_t(u);
            let ids = Vec1::try_from_vec(ids.to_vec()).unwrap();
            // Like a type application that keeps expanding, a check of a generic value that
            // recurs through the value's own type is assumed to hold after the limit. A
            // `Thenable<number>` checked against `Promise<number>` checks its generic `then`,
            // whose return type is a `Thenable` checked against a `Promise` again.
            let Some(env) = &env.push_polymorphic_lower_unless_loop(
                cx.type_expansion_recursion_limit(),
                tparams_loc,
            ) else {
                return Ok(());
            };
            let (t_, _) = if env.in_implicit_instantiation() {
                let Some(targs) =
                    implicit_instantiation::kit::polymorphic_lower_targs(cx, env, l, &ids, u)?
                else {
                    return Ok(());
                };
                FlowJs::instantiate_poly_with_targs_with_env(
                    cx,
                    env,
                    trace,
                    use_op.dupe(),
                    reason_op,
                    reason_tapp,
                    None,
                    None,
                    (tparams_loc.dupe(), ids, t.dupe()),
                    targs,
                )?
            } else {
                let check = ImplicitInstantiationCheck {
                    lhs: l.dupe(),
                    poly_t: (tparams_loc.dupe(), ids, t.dupe()),
                    operation: (
                        use_op.dupe(),
                        reason_op.dupe(),
                        Operation::SubtypeLowerPoly(u.dupe()),
                    ),
                };
                implicit_instantiation::kit::run_call(
                    cx,
                    env,
                    &check,
                    &hint_unavailable(),
                    trace,
                    use_op.dupe(),
                    reason_op,
                    reason_tapp,
                )?
            };
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &t_, u)
        }
        // when a this-abstracted class flows to upper bounds, fix the class
        #[rustfmt::skip]
        (
            DefT(class_r, deref!(ClassT(deref!(ThisInstanceT(box ThisInstanceTData {
                reason: inst_r, instance: i, is_this: this, subst_name: this_name,
            }))))),
            _,
        ) => {
            let reason = type_util::reason_of_t(u);
            let fixed = flow_js_utils::fix_this_instance(
                cx,
                reason.dupe(),
                inst_r.dupe(),
                i,
                *this,
                this_name.dupe(),
            );
            let class_t = Type::new(DefT(
                class_r.dupe(),
                DefT::new(ClassT(fixed)),
            ));
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &class_t, u)
        }
        (
            ThisInstanceT(box ThisInstanceTData {
                reason: r,
                instance: i,
                is_this: this,
                subst_name: this_name,
            }),
            _,
        ) => {
            let reason = type_util::reason_of_t(u);
            let fixed = flow_js_utils::fix_this_instance(
                cx,
                reason.dupe(),
                r.dupe(),
                i,
                *this,
                this_name.dupe(),
            );
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &fixed, u)
        }

        // ****************************
        // React Abstract Components
        // ****************************
        //
        // In all of these cases, we check:
        //  1. configu <: configl
        //  2. default_propsl = default_propsu
        //  3. instancel <: instanceu
        //  4. rendersl <: rendersu
        //
        //  2. is necessary because we allow the default props of a component to be read and
        //  written.
        //
        //  1. Is necessary because we need to ensure that any config object that is passed to u
        //  is compatible with the config of l. This also is sufficient; unification is not required.
        //  We can think of AbstractComponents as some sort of callable that accepts a config object.
        //  The only place that the config object type would appear is in the callable signature, which
        //  is contravariant.
        //
        //  In reality, a component is turned into an element via createElement, which accepts a
        //  component and a config object. From there, it creates an object that will become the
        //  props of a component by combining the config object with the component's default props.
        //  This process creates a new fresh unaliased props object, which is passed to the component.
        //
        //  3. Is necessary because we need to ensure the ref passed in is compatible with the instance
        //  type of the component. React will assign ref.current to the instance of the component, so we
        //  need to ensure that the type we assign is compatible with the type ref.current.
        //
        //  4. Is necessary because we need to ensure the element returned from the render function is
        //  compatible with any places that the component can be rendered. This is often used by
        //  components that only accept specific components as children.
        //

        // Class component ~> AbstractComponent
        #[rustfmt::skip]
        (
            DefT(reasonl, deref!(ClassT(this))),
            DefT(_, deref!(ReactAbstractComponentT(box ReactAbstractComponentTData {
                config, renders, component_kind: ComponentKind::Structural,
            }))),
        ) => {
            // Contravariant config check
            FlowJs::react_get_config(
                cx, env,
                trace,
                l,
                use_op.dupe(),
                reasonl,
                Polarity::Negative,
                config,
            )?;
            // check rendersl <: rendersu
            FlowJs::react_subtype_class_component_render(
                cx, env, trace, use_op, this, reasonl, renders,
            )?;
            Ok(())
        }
        // Function Component ~> AbstractComponent
        #[rustfmt::skip]
        (
            DefT(reasonl, deref!(FunT(_, ft))),
            DefT(_, deref!(ReactAbstractComponentT(box ReactAbstractComponentTData {
                config, renders, component_kind: ComponentKind::Structural,
            }))),
        ) => {
            // Function components will not always have an annotation, so the config may
            // never resolve. To determine config compatibility, we instead
            // call createElement on the function with the given component to determine
            // the compatibility.
            //
            // We use ConfigCheck instead of CreateElement because:
            //  1. We can't perform the key check. If config is mixed, which can happen in
            //  polymorphic HOCs then the [string]: mixed indexer causes spurious errors.
            //  2. We check the ref here, so we don't need to check it in the config as well.
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                l,
                &UseT::new(ReactKitT(Box::new(ReactKitTData {
                    use_op: use_op.dupe(),
                    reason: reasonl.dupe(),
                    tool: Box::new(react::Tool::<Cx>::ConfigCheck {
                        props: config.dupe(),
                    }),
                }))),
            )?;
            // check rendered elements are covariant
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &ft.return_t, renders)
        }

        // Object Component ~> AbstractComponent
        #[rustfmt::skip]
        (
            DefT(reasonl, deref!(ObjT(deref!(ObjType { call_t: Some(call_t), .. })))),
            DefT(reasonu, deref!(ReactAbstractComponentT(box ReactAbstractComponentTData {
                config, renders, component_kind: ComponentKind::Structural,
            }))),
        ) => {
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                l,
                &UseT::new(ReactKitT(Box::new(ReactKitTData {
                    use_op: use_op.dupe(),
                    reason: reasonl.dupe(),
                    tool: Box::new(react::Tool::<Cx>::ConfigCheck {
                        props: config.dupe(),
                    }),
                }))),
            )?;
            // Ensure the callable signature's return type is compatible with the rendered element (renders). We
            // do this by flowing it to (...empty): renders
            let funtype = mk_functiontype(
                reasonu.dupe(),
                None,
                None,
                vec![],
                Some(FunRestParam(
                    None,
                    reasonu.loc().dupe(),
                    empty_t::why(reasonu.dupe()),
                )),
                reasonl.dupe(),
                flow_common::error_ref::FunctionReferenceKind::FunctionType,
                None,
                None,
                renders.dupe(),
            );
            let mixed = mixed_t::why(reasonu.dupe());
            let call_t = cx.find_call(*call_t);
            let fun_t = Type::new(DefT(
                reasonu.dupe(),
                DefT::new(FunT(mixed, Rc::new(funtype))),
            ));
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &call_t, &fun_t)
        }
        // AbstractComponent ~> AbstractComponent
        #[rustfmt::skip]
        (
            DefT(reasonl, deref!(ReactAbstractComponentT(box ReactAbstractComponentTData {
                config: configl,
                renders: rendersl,
                component_kind,
            }))),
            DefT(_, deref!(ReactAbstractComponentT(box ReactAbstractComponentTData {
                config: configu,
                renders: rendersu,
                component_kind: ComponentKind::Structural,
            }))),
        ) => {
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op.dupe(), configu, configl)?;
            let rendersl = match component_kind {
                ComponentKind::Nominal(renders_id, renders_name, _) => {
                    let reason = reasonl.dupe().replace_desc(VirtualReasonDesc::RReactElement {
                        name_opt: Some(Name::new(renders_name.dupe())),
                        from_component_syntax: true,
                    });
                    Type::new(DefT(
                        reason,
                        DefT::new(RendersT(Rc::new(
                            CanonicalRendersForm::NominalRenders {
                                renders_id: renders_id.dupe(),
                                renders_name: renders_name.dupe(),
                                renders_super: rendersl.dupe(),
                            },
                        ))),
                    ))
                }
                ComponentKind::Structural => rendersl.dupe(),
            };
            let use_op = UseOp::Frame(
                Arc::new(VirtualFrameUseOp::RendersCompatibility),
                Arc::new(use_op),
            );
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &rendersl, rendersu)
        }
        #[rustfmt::skip]
        (
            DefT(_, deref!(ReactAbstractComponentT(box ReactAbstractComponentTData {
                component_kind: ComponentKind::Nominal(idl, name_l, _),
                config: configl,
                renders: rendersl,
                ..
            }))),
            DefT(_, deref!(ReactAbstractComponentT(box ReactAbstractComponentTData {
                component_kind: ComponentKind::Nominal(idu, name_u, _),
                config: configu,
                renders: rendersu,
                ..
            }))),
        ) if idl == idu || type_util::nominal_id_have_same_logical_module(
            &cx.file_options(),
            cx.projects_options(),
            (idl, Some(name_l.as_str())),
            (idu, Some(name_u.as_str())),
        ) => {
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op.dupe(), configu, configl)?;
            let use_op = UseOp::Frame(
                Arc::new(VirtualFrameUseOp::RendersCompatibility),
                Arc::new(use_op),
            );
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, rendersl, rendersu)
        }
        (DefT(reasonl, deref!(RendersT(r1))), DefT(reasonu, deref!(RendersT(r2)))) => {
            renders_kit::rec_renders_to_renders(cx, env, trace, use_op, (reasonl, r1), (reasonu, r2))
        }
        (_, DefT(renders_r, deref!(RendersT(upper_renders)))) => {
            renders_kit::non_renders_to_renders(cx, env, trace, use_op, l, renders_r, upper_renders)
        }
        // Exiting the renders world
        #[rustfmt::skip]
        (
            DefT(r, deref!(RendersT(deref!(
                CanonicalRendersForm::IntrinsicRenders(_) | CanonicalRendersForm::NominalRenders { .. }
            )))),
            _,
        ) => {
            let mixed_element = FlowJs::get_builtin_react_type_with_env(
                cx, env,
                None,
                r,
                None,
                intermediate_error_types::ExpectedModulePurpose::ReactModuleForReactMixedElementType,
            )?;
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &mixed_element, u)
        }
        #[rustfmt::skip]
        (
            DefT(r, deref!(RendersT(deref!(CanonicalRendersForm::StructuralRenders {
                renders_structural_type: t, ..
            })))),
            _,
        ) => {
            let u_prime = UseT::new(ExitRendersT {
                renders_reason: r.dupe(),
                u: Box::new(UseT::new(UseT(use_op, u.dupe()))),
            });
            FlowJs::rec_flow_with_env(cx, env, trace, t, &u_prime)
        }
        (DefT(r, deref!(RendersT(deref!(CanonicalRendersForm::DefaultRenders)))), _) => {
            let u_prime = UseT::new(ExitRendersT {
                renders_reason: r.dupe(),
                u: Box::new(UseT::new(UseT(use_op, u.dupe()))),
            });
            FlowJs::rec_flow_with_env(cx, env, trace, l, &u_prime)
        }

        // ***********************************************
        // * function types deconstruct into their parts *
        // ***********************************************

        // FunT ~> FunT
        (DefT(lreason, deref!(FunT(_, ft1))), DefT(ureason, deref!(FunT(_, ft2)))) => {
            funt_to_funt_check(
                cx, env,
                trace,
                use_op,
                l,
                u,
                lreason,
                ft1,
                ureason,
                ft2,
                &|cx, use_op| {
                    funt_to_funt_check_this_contravariant(
                        cx, env, trace, l, u, lreason, ft1, ureason, ft2, use_op,
                    )
                },
                &|cx, use_op| {
                    funt_to_funt_check_params_contravariant(cx, env, trace, ureason, ft1, ft2, use_op)
                },
            )
        }

        // unwrap namespace type into object type, drop all information about types in the namespace
        (NamespaceT(ns), _) => FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &ns.values_type, u),
        (_, NamespaceT(ns)) => FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, l, &ns.values_type),

        // ObjT -> ObjT
        (DefT(lreason, deref!(ObjT(l_obj))), DefT(ureason, deref!(ObjT(u_obj)))) => {
            let u_deft = u;
            type_inference_hooks_js::dispatch_obj_to_obj_hook(l, u_deft);
            let print_fast_path = cx.is_verbose();
            if l_obj.props_tmap == u_obj.props_tmap && l_obj.call_t == u_obj.call_t {
                if print_fast_path {
                    eprintln!("ObjT ~> ObjT fast path: yes");
                }
            } else {
                if print_fast_path {
                    eprintln!("ObjT ~> ObjT fast path: no");
                }
                flow_obj_to_obj(cx, env, trace, use_op, (l, u), lreason, l_obj, ureason, u_obj)?;
            }
            Ok(())
        }
        (DefT(_, deref!(ObjT(_))), NullProtoT(_)) => {
            Ok(())
        }
        // InstanceT -> ObjT
        (DefT(lreason, deref!(InstanceT(inst))), DefT(ureason, deref!(ObjT(obj))))
            if matches!(
                    inst.inst.inst_kind,
                    InstanceKind::ClassKind | InstanceKind::InterfaceKind { .. }
                )
                && obj.flags.obj_kind == ObjKind::Exact
                && !inst
                    .inst
                    .strictness_kind
                    .join(obj.strictness_kind)
                    .is_typescript_loose() =>
        {
            let (lower_t, upper_t) = flow_js_utils::ordered_types(cx, (l, u));
            let (lower_reason, upper_reason) =
                flow_js_utils::ordered_reasons(cx, (lreason.dupe(), ureason.dupe()));
            flow_js_utils::add_output_with_env(
                cx,env,
                ErrorMessage::EIncompatibleWithExact(
                    (
                        flow_js_utils::type_reference_with_reason_for_error(lower_t, lower_reason),
                        flow_js_utils::type_reference_with_reason_for_error(upper_t, upper_reason),
                    ),
                    use_op,
                    intermediate_error_types::ExactnessErrorKind::UnexpectedInexact,
                ),
            )?;
            Ok(())
        }
        (DefT(lreason, deref!(InstanceT(inst_t))), DefT(ureason, deref!(ObjT(u_obj)))) => {
            let super_ = &inst_t.super_;
            let inst = &inst_t.inst;
            let lown = inst.own_props.dupe();
            let lproto = inst.proto_props.dupe();
            let lcall = inst.inst_call_t;
            let inst_kind = &inst.inst_kind;
            let uflds = u_obj.props_tmap.dupe();
            let uproto = &u_obj.proto_t;
            let ucall = u_obj.call_t;
            let strictness_kind = inst.strictness_kind.join(u_obj.strictness_kind);

            let suppress_class_to_object_error = strictness_kind.is_typescript_loose()
                && match inst_kind {
                    InstanceKind::ClassKind | InstanceKind::InterfaceKind { .. } => true,
                    InstanceKind::RecordKind { .. } => false,
                };
            if !suppress_class_to_object_error {
                let error_kind = match inst_kind {
                    InstanceKind::ClassKind | InstanceKind::InterfaceKind { .. } => {
                        intermediate_error_types::ClassKind::Class
                    }
                    InstanceKind::RecordKind { .. } => intermediate_error_types::ClassKind::Record,
                };
                flow_js_utils::add_output_with_env(
                    cx,env,
                    ErrorMessage::EClassToObject(Box::new(EClassToObjectData {
                        lower: flow_js_utils::type_reference_with_reason_for_error(
                            l,
                            lreason.dupe(),
                        ),
                        upper: flow_js_utils::type_reference_with_reason_for_error(
                            u,
                            ureason.dupe(),
                        ),
                        use_op: use_op.dupe(),
                        kind: error_kind,
                    })),
                )?;
            }
            let own_props = cx.find_props(lown.dupe());
            let proto_props = cx.find_props(lproto.dupe());
            let mut lflds_map = own_props.clone();
            for (k, v) in proto_props.iter() {
                if !lflds_map.contains_key(k) {
                    lflds_map.insert(k.dupe(), v.clone());
                }
            }
            if let Some(ucall) = ucall {
                let prop_name = Some(Name::new("$call"));
                match lcall {
                    Some(lcall) => {
                        FlowJs::rec_flow_with_env(
                            cx, env,
                            trace,
                            &cx.find_call(lcall),
                            &UseT::new(UseT(use_op.dupe(), cx.find_call(ucall))),
                        )?;
                    }
                    None => {
                        let error_message = ErrorMessage::EPropNotFoundInSubtyping(Box::new(
                            EPropNotFoundInSubtypingData {
                                lower: flow_js_utils::type_reference_with_reason_for_error(
                                    l,
                                    lreason.dupe(),
                                ),
                                upper: flow_js_utils::type_reference_with_reason_for_error(
                                    u,
                                    ureason.dupe(),
                                ),
                                prop_name,
                                use_op: use_op.dupe(),
                                suggestion: None,
                            },
                        ));
                        flow_js_utils::add_output_with_env(cx,env, error_message)?;
                    }
                }
            }
            let mut missing_props = vec![];
            let errs = {
                let mut acc: Vec<_> = vec![];
                for (name, up) in cx.find_props(uflds.dupe()).iter() {
                    let propref = type_util::mk_named_prop(
                        ureason
                            .dupe()
                            .replace_desc(VirtualReasonDesc::RProperty(Some(name.dupe()))),
                        false,
                        name.dupe(),
                    );
                    match lflds_map.get(name) {
                        Some(lp) => {
                            let prop_use_op = VirtualUseOp::Frame(
                                Arc::new(VirtualFrameUseOp::PropertyCompatibility(Box::new(
                                    PropertyCompatibilityData {
                                        prop: Some(name.dupe()),
                                        lower: lreason.dupe(),
                                        upper: ureason.dupe(),
                                        lower_t: flow_js_utils::type_or_type_desc_for_error(l),
                                        upper_t: flow_js_utils::type_or_type_desc_for_error(u),
                                    },
                                ))),
                                Arc::new(use_op.dupe()),
                            );
                            let bivariant_handled = if strictness_kind.is_typescript_loose() {
                                try_property_method_bivariant(
                                    cx,
                                    env,
                                    trace,
                                    prop_use_op.dupe(),
                                    lp,
                                    up,
                                )?
                            } else {
                                false
                            };
                            if !bivariant_handled {
                                let lower_prop_type = property_type_for_subtyping(
                                    lp,
                                    strictness_kind,
                                );
                                let new_errs = rec_flow_p_inner(
                                    cx, env,
                                    Some(trace),
                                    prop_use_op,
                                    None,
                                    Some((l, u)),
                                    strictness_kind,
                                    true,
                                    &propref,
                                    &lower_prop_type,
                                    &property::property_type(up),
                                )?;
                                acc.extend(new_errs);
                            }
                        }
                        None => {
                            let optional = matches!(
                                up.deref(),
                                PropertyInner::Field(box FieldData {
                                    type_: deref!(OptionalT { .. }), ..
                                })
                            );
                            if !optional && strictness_kind.is_typescript_loose() {
                                missing_props.push((propref.clone(), up.dupe()));
                                continue;
                            }
                            let lookup_kind = if optional {
                                LookupKind::NonstrictReturning(Box::new(NonstrictReturningData(
                                    None, None,
                                )))
                            } else {
                                LookupKind::Strict(l.dupe())
                            };
                            FlowJs::rec_flow_with_env(
                                cx, env,
                                trace,
                                super_,
                                &UseT::new(ReposLowerT {
                                    reason: lreason.dupe(),
                                    use_desc: false,
                                    use_t: Box::new(UseT::new(LookupT(Box::new(
                                        LookupTData {
                                            reason: ureason.dupe(),
                                            lookup_kind: Box::new(lookup_kind),
                                            indexer_fallback: None,
                                            try_ts_on_failure: vec![].into(),
                                            propref: Box::new(propref.clone()),
                                            lookup_action: Box::new(
                                                LookupAction::LookupPropsForSubtyping(Box::new(
                                                    LookupPropsForSubtypingData {
                                                        use_op: use_op.dupe(),
                                                        props: Rc::from([(
                                                            propref.clone(),
                                                            up.dupe(),
                                                        )]),
                                                        strictness_kind,
                                                        lower: l.dupe(),
                                                        upper: u.dupe(),
                                                    },
                                                )),
                                            ),
                                            method_accessible: false,
                                            ids: Some(
                                                [lown.dupe(), lproto.dupe()].into_iter().collect(),
                                            ),
                                            ignore_dicts: false,
                                        },
                                    )))),
                                }),
                            )?;
                        }
                    }
                }
                acc
            };
            if let Some((first_propref, _)) = missing_props.first() {
                let propref = first_propref.clone();
                FlowJs::rec_flow_with_env(
                    cx, env,
                    trace,
                    super_,
                    &UseT::new(ReposLowerT {
                        reason: lreason.dupe(),
                        use_desc: false,
                        use_t: Box::new(UseT::new(LookupT(Box::new(LookupTData {
                            reason: ureason.dupe(),
                            lookup_kind: Box::new(LookupKind::Strict(l.dupe())),
                            indexer_fallback: None,
                            try_ts_on_failure: vec![].into(),
                            propref: Box::new(propref),
                            lookup_action: Box::new(LookupAction::LookupPropsForSubtyping(
                                Box::new(LookupPropsForSubtypingData {
                                    use_op: use_op.dupe(),
                                    props: missing_props.into(),
                                    strictness_kind,
                                    lower: l.dupe(),
                                    upper: u.dupe(),
                                }),
                            )),
                            method_accessible: false,
                            ids: Some([lown.dupe(), lproto.dupe()].into_iter().collect()),
                            ignore_dicts: false,
                        })))),
                    }),
                )?;
            }
            add_output_prop_polarity_mismatch(
                cx, env,
                use_op.dupe(),
                PropPolarityMismatchObject::Type(l),
                PropPolarityMismatchObject::Type(u),
                errs,
                strictness_kind,
            )?;
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                l,
                &UseT::new(UseT(use_op, uproto.dupe())),
            )
        }
        // If passing in an object literal where a record is expected,
        // provide an additional explanation.
        (DefT(lreason, deref!(ObjT(_))), DefT(_, deref!(InstanceT(inst))))
            if matches!(
                    lreason.desc(true),
                    Some(VirtualReasonDesc::RObjectLit)
                )
                && matches!(inst.inst.inst_kind, InstanceKind::RecordKind { .. })
                && let Some(record_name) = &inst.inst.class_name =>
        {
            flow_js_utils::add_output_with_env(
                cx,env,
                flow_js_utils::incompatible_types_error(
                    l,
                    u,
                    use_op,
                    Some(
                        intermediate_error_types::Explanation::ExplanationObjectLiteralNeedsRecordSyntax {
                            record_name: record_name.dupe(),
                            obj_loc: type_util::ref_loc_of_t(l).dupe(),
                        },
                    ),
                ),
            )
        }

        // For some object `x` and constructor `C`, if `x instanceof C`, then the
        // object is a subtype. We use `ExtendsUseT` to walk the proto chain of the
        // object, in case it includes a nominal type.
        (DefT(_, ld @ deref!(ObjT(_) | ClassT(_))), DefT(_, deref!(InstanceT(inst))))
            if matches!(ld.deref(), ObjT(_))
                || matches!(inst.inst.inst_kind, InstanceKind::ClassKind) =>
        {
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                l,
                &type_util::extends_use_type(use_op, l.dupe(), u.dupe()),
            )
        }

        // ****************************************
        // You can cast an object to a function
        // ****************************************
        (DefT(reason, deref!(ObjT(_) | InstanceT(_))), DefT(reason_op, deref!(FunT(_, _)))) => {
            let fun_t = match l.deref() {
                DefT(_, deref!(ObjT(deref!(ObjType { call_t: Some(call_t), .. })))) => {
                    cx.find_call(*call_t)
                }
                DefT(_, deref!(InstanceT(inst)))
                    if let Some(call_t) = &inst.inst.inst_call_t =>
                {
                    cx.find_call(*call_t)
                }
                _ => {
                    let error_message =
                        flow_js_utils::incompatible_types_error(l, u, use_op.dupe(), None);
                    flow_js_utils::add_output_with_env(cx,env, error_message)?;
                    any_t::error(reason_op.dupe())
                }
            };
            let use_op = VirtualUseOp::Frame(
                Arc::new(VirtualFrameUseOp::PropertyCompatibility(Box::new(
                    PropertyCompatibilityData {
                        prop: Some(Name::new("$call")),
                        lower: reason.dupe(),
                        upper: reason_op.dupe(),
                        lower_t: flow_js_utils::type_or_type_desc_for_error(l),
                        upper_t: flow_js_utils::type_or_type_desc_for_error(u),
                    },
                ))),
                Arc::new(use_op),
            );
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &fun_t, u)
        }

        // ********************************************
        // array types deconstruct into their parts
        // ********************************************
        #[rustfmt::skip]
        (
            DefT(r1, deref!(ArrT(deref!(ArrayAT(box ArrayATData {
                elem_t: t1, tuple_view: tv1, strictness_kind: strictness_kind1, fresh: lit1, ..
            }))))),
            DefT(r2, deref!(ArrT(deref!(ArrayAT(box ArrayATData {
                elem_t: t2, tuple_view: tv2, strictness_kind: strictness_kind2, ..
            }))))),
        ) => {
            let use_op = VirtualUseOp::Frame(
                Arc::new(VirtualFrameUseOp::ArrayElementCompatibility(Box::new(
                    ArrayElementCompatibilityData {
                        lower: r1.dupe(),
                        upper: r2.dupe(),
                        lower_t: flow_js_utils::type_or_type_desc_for_error(l),
                        upper_t: flow_js_utils::type_or_type_desc_for_error(u),
                    },
                ))),
                Arc::new(use_op),
            );
            let ts1 = tv1
                .as_ref()
                .map(|tv| type_util::tuple_ts_of_elements(&tv.elements))
                .unwrap_or_default();
            let ts2 = tv2
                .as_ref()
                .map(|tv| type_util::tuple_ts_of_elements(&tv.elements))
                .unwrap_or_default();
            array_flow(
                cx,
                env,
                trace,
                use_op,
                *lit1,
                strictness_kind1.join(*strictness_kind2),
                r1,
                l,
                u,
                &ts1,
                tv1.as_ref().is_some_and(|tv| tv.inexact),
                tv1.as_ref().and_then(|tv| tv.rest.as_ref()),
                t1,
                &ts2,
                tv2.as_ref().and_then(|tv| tv.rest.as_ref()),
                t2,
            )
        }
        // Tuples can flow to tuples with the same arity
        #[rustfmt::skip]
        (
            DefT(r1, deref!(ArrT(deref!(TupleAT(box TupleATData {
                elements: elements1, arity: lower_arity, inexact: lower_inexact,
                rest: rest1, literal: fresh, ..
            }))))),
            DefT(r2, deref!(ArrT(deref!(TupleAT(box TupleATData {
                elements: elements2, arity: upper_arity, inexact: upper_inexact, rest: rest2, ..
            }))))),
        ) => {
            let (num_req1, num_total1) = *lower_arity;
            let (num_req2, num_total2) = *upper_arity;
            // A tuple with a rest element has no maximum length, like an inexact tuple.
            let lower_open = *lower_inexact || rest1.is_some();
            let upper_open = *upper_inexact || rest2.is_some();
            if !((*upper_inexact || !lower_inexact)
                && (upper_open || !lower_open)
                && num_req1 >= num_req2
                && (num_total1 <= num_total2 || upper_open))
            {
                flow_js_utils::add_output_with_env(
                    cx,env,
                    ErrorMessage::ETupleArityMismatch(Box::new(ETupleArityMismatchData {
                        use_op: use_op.dupe(),
                        lower: flow_js_utils::type_reference_with_reason_for_error(l, r1.dupe()),
                        lower_arity: *lower_arity,
                        lower_inexact: *lower_inexact,
                        lower_rest: rest1.is_some(),
                        upper: flow_js_utils::type_reference_with_reason_for_error(u, r2.dupe()),
                        upper_arity: *upper_arity,
                        upper_inexact: *upper_inexact,
                        upper_rest: rest2.is_some(),
                        unify: false,
                    })),
                )?;
            } else {
                let mut n: i32 = 0;
                let mut tuple_element_compat = |n: i32,
                                            t1: &Type,
                                            t2: &Type,
                                            p1: Polarity,
                                            p2: Polarity,
                                            optional1: bool,
                                            optional2: bool|
                 -> Result<(), FlowJsException> {
                    if !(*fresh || Polarity::compat(p1, p2)) {
                        flow_js_utils::add_output_with_env(
                            cx,env,
                            ErrorMessage::ETupleElementPolarityMismatch(Box::new(
                                ETupleElementPolarityMismatchData {
                                    index: n,
                                    lower: flow_js_utils::type_reference_with_reason_for_error(
                                        l,
                                        r1.dupe(),
                                    ),
                                    polarity_lower: p1,
                                    upper: flow_js_utils::type_reference_with_reason_for_error(
                                        u,
                                        r2.dupe(),
                                    ),
                                    polarity_upper: p2,
                                    use_op: use_op.dupe(),
                                },
                            )),
                        )?;
                    }
                    let elem_use_op = VirtualUseOp::Frame(
                        Arc::new(VirtualFrameUseOp::TupleElementCompatibility(Box::new(
                            TupleElementCompatibilityData {
                                n,
                                lower: r1.dupe(),
                                upper: r2.dupe(),
                                lower_t: flow_js_utils::type_or_type_desc_for_error(l),
                                upper_t: flow_js_utils::type_or_type_desc_for_error(u),
                            },
                        ))),
                        Arc::new(use_op.dupe()),
                    );
                    // We don't want to allow `undefined` when an element is marked as optional:
                    //
                    // ```js
                    // type T = [number, b?: string];
                    // ([0, undefined]: T); // Should error
                    // ([0]: T); // Should be ok
                    // ([0, 's']: T); // Should be ok
                    // ```
                    //
                    // A user can always add `| void` to the element type if they want to denote this.
                    let t2_inner = if !optional1 {
                        if let (
                            true,
                            OptionalT {
                                type_: inner_t2, ..
                            },
                        ) = (optional2, t2.deref())
                        {
                            inner_t2
                        } else {
                            t2
                        }
                    } else {
                        t2
                    };
                    match (*fresh, p2) {
                        (true, _) | (_, Polarity::Positive) => {
                            FlowJs::rec_flow_t_with_env(cx, env, trace, elem_use_op, t1, t2_inner)?;
                        }
                        (_, Polarity::Negative) => {
                            FlowJs::rec_flow_t_with_env(cx, env, trace, elem_use_op, t2_inner, t1)?;
                        }
                        (_, Polarity::Neutral) => {
                            FlowJs::rec_unify_with_env(
                                cx, env,
                                trace,
                                elem_use_op,
                                UnifyCause::Uncategorized,
                                None,
                                t1,
                                t2_inner,
                            )?;
                        }
                    }
                    Ok(())
                };
                let mut i1 = elements1.iter();
                let mut i2 = elements2.iter();
                loop {
                    let e1 = i1.next();
                    let e2 = i2.next();
                    match (e1, e2) {
                        (None, None) => break,
                        (
                            Some(TupleElement {
                                t: t1,
                                polarity: p1,
                                optional: optional1,
                                ..
                            }),
                            Some(TupleElement {
                                t: t2,
                                polarity: p2,
                                optional: optional2,
                                ..
                            }),
                        ) => {
                            tuple_element_compat(n, t1, t2, *p1, *p2, *optional1, *optional2)?;
                            n += 1;
                        }
                        // The lower rest element supplies the remaining optional elements.
                        (
                            None,
                            Some(TupleElement {
                                t: t2,
                                polarity: p2,
                                optional: optional2,
                                ..
                            }),
                        ) if let Some(TupleElement { t: t1, polarity: p1, .. }) = rest1 => {
                            tuple_element_compat(n, t1, t2, *p1, *p2, false, *optional2)?;
                            n += 1;
                        }
                        (
                            None,
                            Some(TupleElement {
                                t: t2,
                                polarity: p2,
                                optional: optional2,
                                ..
                            }),
                        ) => {
                            let p1 = Polarity::Neutral;
                            let t1 =
                                void::make(r1.clone().replace_desc_new(
                                    VirtualReasonDesc::RTupleOutOfBoundsAccess(n),
                                ));
                            let optional1 = true;
                            tuple_element_compat(n, &t1, t2, p1, *p2, optional1, *optional2)?;
                            n += 1;
                        }
                        // The remaining lower elements go to the upper rest element.
                        (
                            Some(TupleElement {
                                t: t1,
                                polarity: p1,
                                optional: optional1,
                                ..
                            }),
                            None,
                        ) if let Some(TupleElement { t: t2, polarity: p2, .. }) = rest2 => {
                            tuple_element_compat(n, t1, t2, *p1, *p2, *optional1, false)?;
                            n += 1;
                        }
                        _ => {}
                    }
                }
                if let (
                    Some(TupleElement { t: t1, polarity: p1, .. }),
                    Some(TupleElement { t: t2, polarity: p2, .. }),
                ) = (rest1, rest2)
                {
                    tuple_element_compat(n, t1, t2, *p1, *p2, false, false)?;
                }
            }
            Ok(())
        }

        // TypeScript treats mutable arrays covariantly, including tuples used as arrays.
        #[rustfmt::skip]
        (
            DefT(r1, deref!(ArrT(deref!(TupleAT(box TupleATData {
                elem_t: t1, strictness_kind: strictness_kind1, ..
            }))))),
            DefT(r2, deref!(ArrT(deref!(ArrayAT(box ArrayATData {
                elem_t: t2, strictness_kind: strictness_kind2, ..
            }))))),
        ) if strictness_kind1
                    .join(*strictness_kind2)
                    .is_typescript_loose() =>
        {
            let use_op = VirtualUseOp::Frame(
                Arc::new(VirtualFrameUseOp::ArrayElementCompatibility(Box::new(
                    ArrayElementCompatibilityData {
                        lower: r1.dupe(),
                        upper: r2.dupe(),
                        lower_t: flow_js_utils::type_or_type_desc_for_error(l),
                        upper_t: flow_js_utils::type_or_type_desc_for_error(u),
                    },
                ))),
                Arc::new(use_op),
            );
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, t1, t2)
        }

        // Arrays with known elements can flow to tuples
        #[rustfmt::skip]
        (
            DefT(r1, deref!(ArrT(deref!(ArrayAT(box ArrayATData {
                elem_t: t1, tuple_view, react_dro, literal, fresh, ..
            }))))),
            DefT(r2, deref!(ArrT(deref!(TupleAT(box TupleATData { strictness_kind, .. }))))),
        ) => {
            match tuple_view {
                None => {
                    flow_js_utils::add_output_with_env(
                        cx,env,
                        ErrorMessage::ENonLitArrayToTuple(
                            (
                                flow_js_utils::type_reference_with_reason_for_error(l, r1.dupe()),
                                flow_js_utils::type_reference_with_reason_for_error(u, r2.dupe()),
                            ),
                            use_op,
                        ),
                    )?;
                }
                Some(tv) => {
                    let tuple_t = Type::new(DefT(
                        r1.dupe(),
                        DefT::new(ArrT(Rc::new(TupleAT(Box::new(
                            TupleATData {
                                elem_t: t1.dupe(),
                                elements: tv.elements.dupe(),
                                arity: tv.arity,
                                inexact: tv.inexact,
                                rest: tv.rest.dupe(),
                                react_dro: react_dro.clone(),
                                strictness_kind: *strictness_kind,
                                literal: *literal,
                                fresh: *fresh,
                            },
                        ))))),
                    ));
                    FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &tuple_t, u)?;
                }
            }
            Ok(())
        }

        // Read only arrays are the super type of all tuples and arrays
        #[rustfmt::skip]
        (
            DefT(r1, deref!(ArrT(deref!(
                ArrayAT(box ArrayATData { elem_t: t1, .. })
                | TupleAT(box TupleATData { elem_t: t1, .. })
                | ROArrayAT(box ROArrayATData { elem_t: t1, .. })
            )))),
            DefT(r2, deref!(ArrT(deref!(ROArrayAT(box ROArrayATData { elem_t: t2, .. }))))),
        ) => {
            let use_op = VirtualUseOp::Frame(
                Arc::new(VirtualFrameUseOp::ArrayElementCompatibility(Box::new(
                    ArrayElementCompatibilityData {
                        lower: r1.dupe(),
                        upper: r2.dupe(),
                        lower_t: flow_js_utils::type_or_type_desc_for_error(l),
                        upper_t: flow_js_utils::type_or_type_desc_for_error(u),
                    },
                ))),
                Arc::new(use_op),
            );
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                t1,
                &UseT::new(UseT(use_op, t2.dupe())),
            )
        }
        (
            DefT(_, deref!(InstanceT(_))),
            DefT(r2, deref!(ArrT(deref!(ArrayAT(box ArrayATData { elem_t, .. }))))),
        ) => {
            let arrt = FlowJs::get_builtin_typeapp_with_env(cx, env, r2, None, "Array", vec![elem_t.dupe()]);
            FlowJs::rec_flow_with_env(cx, env, trace, l, &UseT::new(UseT(use_op, arrt)))
        }
        (
            DefT(_, deref!(InstanceT(_))),
            DefT(r2, deref!(ArrT(deref!(ROArrayAT(box ROArrayATData { elem_t: elemt, .. }))))),
        ) => {
            let arrt = FlowJs::get_builtin_typeapp_with_env(
                cx,
                env,
                r2,
                None,
                "ReadonlyArray",
                vec![elemt.dupe()],
            );
            FlowJs::rec_flow_with_env(cx, env, trace, l, &UseT::new(UseT(use_op, arrt)))
        }
        // An instance of the builtin `Array` (`ReadonlyArray`) class is the array type `T[]`
        // (`readonly T[]`), so an array is checked against it as one.
        #[rustfmt::skip]
        (
            DefT(_, deref!(ArrT(_))),
            DefT(r2, deref!(InstanceT(inst))),
        ) if let [(_, _, elem_t, _)] = &*inst.inst.type_args
                && flow_js_utils::is_builtin_class_id("Array", inst.inst.class_id.dupe(), cx) =>
        {
            let arrt = Type::new(DefT(
                r2.dupe().update_desc_new(|desc| desc),
                DefT::new(ArrT(Rc::new(ArrayAT(Box::new(ArrayATData {
                    react_dro: inst.inst.inst_react_dro.clone(),
                    elem_t: elem_t.dupe(),
                    tuple_view: None,
                    strictness_kind: inst.inst.strictness_kind,
                    literal: false,
                    fresh: false,
                }))))),
            ));
            FlowJs::rec_flow_with_env(cx, env, trace, l, &UseT::new(UseT(use_op, arrt)))
        }
        #[rustfmt::skip]
        (
            DefT(_, deref!(ArrT(_))),
            DefT(r2, deref!(InstanceT(inst))),
        ) if let [(_, _, elem_t, _)] = &*inst.inst.type_args
                && flow_js_utils::is_builtin_class_id("ReadonlyArray", inst.inst.class_id.dupe(), cx) =>
        {
            let arrt = Type::new(DefT(
                r2.dupe().update_desc_new(|desc| desc),
                DefT::new(ArrT(Rc::new(ROArrayAT(Box::new(ROArrayATData {
                    elem_t: elem_t.dupe(),
                    react_dro: inst.inst.inst_react_dro.clone(),
                    literal: false,
                }))))),
            ));
            FlowJs::rec_flow_with_env(cx, env, trace, l, &UseT::new(UseT(use_op, arrt)))
        }

        // **************************************************
        // instances of classes follow declared hierarchy
        // **************************************************
        (DefT(_, deref!(InstanceT(_))), DefT(_, deref!(InstanceT(_)))) => {
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                l,
                &type_util::extends_use_type(use_op, l.dupe(), u.dupe()),
            )
        }

        // ********************************************************
        // runtime types derive static types through annotation
        // ********************************************************
        (DefT(rl, deref!(ClassT(l_inner))), DefT(_, deref!(ClassT(u_inner)))) => {
            let repositioned = FlowJs::reposition_with_env(
                cx, env,
                Some(trace),
                rl.loc().clone(),
                None,
                None,
                l_inner.dupe(),
            )?;
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                &repositioned,
                &UseT::new(UseT(use_op, u_inner.dupe())),
            )
        }

        (DefT(reason_l, deref!(InstanceT(instance))), DefT(reason_u, deref!(ClassT(class_instance))))
            if matches!(instance.inst.inst_kind, InstanceKind::InterfaceKind { .. }) =>
        {
            let concretize = |cx: &mut Context<'_>, t: &Type| {
                FlowJs::possible_concrete_types_for_inspection_with_env(cx, env, type_util::reason_of_t(t), t)
            };
            match flow_js_utils::combine_construct_ts(flow_js_utils::collect_construct_ts(
                &concretize,
                cx,
                l,
            )?) {
                Some(construct_t) => {
                    let target = mk_functiontype(
                        reason_u.dupe(),
                        None,
                        None,
                        vec![],
                        Some(FunRestParam(
                            None,
                            reason_u.loc().dupe(),
                            empty_t::why(reason_u.dupe()),
                        )),
                        reason_l.dupe(),
                        flow_common::error_ref::FunctionReferenceKind::FunctionType,
                        None,
                        None,
                        class_instance.dupe(),
                    );
                    let target = Type::new(DefT(
                        reason_u.dupe(),
                        DefT::new(FunT(
                            mixed_t::why(reason_u.dupe()),
                            Rc::new(target),
                        )),
                    ));
                    FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &construct_t, &target)
                }
                None => flow_js_utils::add_output_with_env(
                    cx,env,
                    ErrorMessage::EConstructSignatureMissingInSubtyping(Box::new(
                        EConstructSignatureMissingInSubtypingData {
                            lower: flow_js_utils::type_reference_with_reason_for_error(
                                l,
                                reason_l.dupe(),
                            ),
                            upper: flow_js_utils::type_reference_with_reason_for_error(
                                u,
                                reason_u.dupe(),
                            ),
                            use_op,
                        },
                    )),
                ),
            }
        }

        // ***********************************************
        // You can use a function as a callable object
        // ***********************************************
        (
            DefT(_, deref!(FunT(_, _))),
            DefT(reason, deref!(ObjT(o @ deref!(ObjType { call_t: Some(call_t), .. })))),
        ) => {
            let t = cx.find_call(*call_t);
            FlowJs::rec_flow_with_env(cx, env, trace, l, &UseT::new(UseT(use_op.dupe(), t)))?;
            let new_obj = ObjType {
                flags: o.flags.clone(),
                props_tmap: o.props_tmap.dupe(),
                proto_t: o.proto_t.dupe(),
                call_t: None,
                reachable_targs: o.reachable_targs.dupe(),
                strictness_kind: o.strictness_kind,
                literal: o.literal,
            };
            let obj_t = Type::new(DefT(
                reason.dupe(),
                DefT::new(ObjT(Rc::new(new_obj))),
            ));
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, l, &obj_t)
        }

        (DefT(_, deref!(FunT(_, _))), DefT(reason, deref!(InstanceT(inst_t))))
            if let Some(call_t) = &inst_t.inst.inst_call_t =>
        {
            let t = cx.find_call(*call_t);
            FlowJs::rec_flow_with_env(cx, env, trace, l, &UseT::new(UseT(use_op.dupe(), t)))?;
            let inst_type = {
                let mut new_inst_inner: InstTypeInner = inst_t.inst.deref().clone();
                new_inst_inner.inst_call_t = None;
                let new_instance_t = InstanceT::new(InstanceTInner {
                    inst: InstType::new(new_inst_inner),
                    static_: inst_t.static_.dupe(),
                    super_: inst_t.super_.dupe(),
                    implements: inst_t.implements.clone(),
                });
                Type::new(DefT(
                    reason.dupe(),
                    DefT::new(InstanceT(Rc::new(new_instance_t))),
                ))
            };
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, l, &inst_type)
        }

        // FunT ~> ObjT
        //
        // Previously, call properties were stored in the props map, and were
        // checked against dictionary upper bounds. This is wrong, but useful for
        // distinguishing between thunk-like types found in graphql-js.
        //
        // Now that call properties are stored separately, it is particularly
        // egregious to emit this constraint. This only serves to maintain buggy
        // behavior, which should be fixed, and this code removed.
        (DefT(lreason, deref!(FunT(_, fun))), DefT(ureason, deref!(ObjT(obj))))
            if matches!(obj.flags.obj_kind, ObjKind::Exact | ObjKind::Indexed(_))
                && (!fun
                    .strictness_kind
                    .join(obj.strictness_kind)
                    .is_typescript_loose()
                    || obj.flags.obj_kind != ObjKind::Exact) =>
        {
            let reasons = flow_js_utils::ordered_reasons(cx, (lreason.dupe(), ureason.dupe()));
            match &obj.flags.obj_kind {
                ObjKind::Exact => {
                    let (lower_t, upper_t) = flow_js_utils::ordered_types(cx, (l, u));
                    let (lower_reason, upper_reason) = reasons;
                    flow_js_utils::add_output_with_env(
                        cx,env,
                        ErrorMessage::EIncompatibleWithExact(
                            (
                                flow_js_utils::type_reference_with_reason_for_error(
                                    lower_t,
                                    lower_reason,
                                ),
                                flow_js_utils::type_reference_with_reason_for_error(
                                    upper_t,
                                    upper_reason,
                                ),
                            ),
                            use_op,
                            intermediate_error_types::ExactnessErrorKind::UnexpectedInexact,
                        ),
                    )?;
                }
                ObjKind::Indexed(_) => {
                    let (lower_t, upper_t) = flow_js_utils::ordered_types(cx, (l, u));
                    let (lower_reason, upper_reason) = reasons;
                    flow_js_utils::add_output_with_env(
                        cx,env,
                        ErrorMessage::EFunctionIncompatibleWithIndexer(
                            (
                                flow_js_utils::type_reference_with_reason_for_error(
                                    lower_t,
                                    lower_reason,
                                ),
                                flow_js_utils::type_reference_with_reason_for_error(
                                    upper_t,
                                    upper_reason,
                                ),
                            ),
                            use_op,
                        ),
                    )?;
                }
                _ => {}
            }
            Ok(())
        }
        // TODO: This rule doesn't interact very well with union-type checking. It
        // looks up Function.prototype, which currently doesn't appear structurally
        // in the function type, and thus may not be fully resolved when the
        // function type is checked with a union containing the object
        // type. Ideally, we should either add Function.prototype to function types
        // or fully resolve them when resolving function types, but either way we
        // might bomb perf without additional work. Meanwhile, we need an immediate
        // fix for the common case where this bug shows up. So leaving this comment
        // here as a marker for future work, while going with a band-aid solution
        // for now, as motivated below.
        //
        // Fortunately, it is quite hard for a function type to successfully
        // check against an object type, and even more unlikely when the latter
        // is part of a union: the object type must only contain
        // Function.prototype methods or statics. Quickly confirming that the
        // check would fail before looking up Function.prototype (while falling
        // back to the general rule when we cannot guarantee failure) is a safe
        // optimization in any case, and fixes the commonly observed case where
        // the union type contains both a function type and a object type as
        // members, clearly intending for function types to match the former
        // instead of the latter.
        (DefT(_, deref!(FunT(statics, _))), DefT(_, deref!(ObjT(obj)))) => {
            if !flow_js_utils::quick_error_fun_as_obj(
                cx,
                env,
                &use_op,
                l,
                statics,
                u,
                &cx.find_props(obj.props_tmap.dupe()),
            )? {
                FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, statics, u)?;
            }
            Ok(())
        }
        // TODO: similar concern as above
        (DefT(_, deref!(FunT(statics, _))), DefT(_, deref!(InstanceT(inst_t))))
            if matches!(inst_t.inst.inst_kind, InstanceKind::InterfaceKind { .. }) =>
        {
            let mut filtered = cx.find_props(inst_t.inst.own_props.dupe());
            filtered.remove(&Name::new("constructor"));
            if !flow_js_utils::quick_error_fun_as_obj(
                cx,
                env,
                &use_op,
                l,
                statics,
                u,
                &filtered,
            )? {
                FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, statics, u)?;
            }
            Ok(())
        }

        (DefT(_, deref!(FunT(statics, _))), DefT(_, deref!(InstanceT(_)))) => {
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, statics, u)
        }

        // ***************************************************
        // classes and arrays can implement some interface
        // ***************************************************
        (DefT(_, deref!(ClassT(_) | ArrT(_))), DefT(_, deref!(InstanceT(inst))))
            if matches!(inst.inst.inst_kind, InstanceKind::InterfaceKind { .. }) =>
        {
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                u,
                &UseT::new(ImplementsT(use_op, l.dupe())),
            )
        }
        #[rustfmt::skip]
        (
            DefT(reason, deref!(
                BoolGeneralT
                    | SingletonBoolT { .. }
                    | NumGeneralT(_)
                    | SingletonNumT { .. }
                    | StrGeneralT(_)
                    | SingletonStrT { .. }
                    | BigIntGeneralT(_)
                    | SingletonBigIntT { .. }
                    | SymbolT
                    | UniqueSymbolT(_)
            )),
            DefT(_, ud @ deref!(InstanceT(_) | ObjT(_))),
        ) if matches!(ud.deref(), InstanceT(inst)
            if matches!(inst.inst.inst_kind, InstanceKind::InterfaceKind { reject_primitives: false, .. })
                && inst.inst.strictness_kind.is_typescript_loose()
        ) || matches!(ud.deref(), ObjT(obj) if obj.strictness_kind.is_typescript_loose()) => {
            let builtin_name = match l.deref() {
                DefT(_, deref!(BoolGeneralT | SingletonBoolT { .. })) => {
                    "Boolean"
                }
                DefT(_, deref!(NumGeneralT(_) | SingletonNumT { .. })) => {
                    "Number"
                }
                DefT(_, deref!(BigIntGeneralT(_) | SingletonBigIntT { .. })) => {
                    "BigInt"
                }
                DefT(_, deref!(SymbolT | UniqueSymbolT(_))) => {
                    "Symbol"
                }
                _ => "String",
            };
            let builtin = FlowJs::get_builtin_type_with_env(cx, env, Some(trace), reason, None, builtin_name)?;
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &builtin, u)
        }
        (
            DefT(reason, deref!(BoolGeneralT | SingletonBoolT { .. })),
            DefT(interface_reason, deref!(InstanceT(inst))),
        ) if matches!(inst.inst.inst_kind, InstanceKind::InterfaceKind { .. }) =>
        {
            flow_js_utils::add_output_with_env(
                cx,env,
                ErrorMessage::EPrimitiveAsInterface(Box::new(EPrimitiveAsInterfaceData {
                    use_op,
                    lower: flow_js_utils::type_reference_with_reason_for_error(l, reason.dupe()),
                    upper: flow_js_utils::type_reference_with_reason_for_error(
                        u,
                        interface_reason.dupe(),
                    ),
                    kind: intermediate_error_types::PrimitiveKind::Boolean,
                })),
            )
        }
        (
            DefT(reason, deref!(NumGeneralT(_) | SingletonNumT { .. })),
            DefT(interface_reason, deref!(InstanceT(inst))),
        ) if matches!(inst.inst.inst_kind, InstanceKind::InterfaceKind { .. }) =>
        {
            flow_js_utils::add_output_with_env(
                cx,env,
                ErrorMessage::EPrimitiveAsInterface(Box::new(EPrimitiveAsInterfaceData {
                    use_op,
                    lower: flow_js_utils::type_reference_with_reason_for_error(l, reason.dupe()),
                    upper: flow_js_utils::type_reference_with_reason_for_error(
                        u,
                        interface_reason.dupe(),
                    ),
                    kind: intermediate_error_types::PrimitiveKind::Number,
                })),
            )
        }
        (
            DefT(reason, deref!(StrGeneralT(_) | SingletonStrT { .. })),
            DefT(interface_reason, deref!(InstanceT(inst))),
        ) if matches!(inst.inst.inst_kind, InstanceKind::InterfaceKind { .. }) =>
        {
            flow_js_utils::add_output_with_env(
                cx,env,
                ErrorMessage::EPrimitiveAsInterface(Box::new(EPrimitiveAsInterfaceData {
                    use_op,
                    lower: flow_js_utils::type_reference_with_reason_for_error(l, reason.dupe()),
                    upper: flow_js_utils::type_reference_with_reason_for_error(
                        u,
                        interface_reason.dupe(),
                    ),
                    kind: intermediate_error_types::PrimitiveKind::String,
                })),
            )
        }

        // ************************************
        // opaque types lower & upper bound
        // ************************************

        // When both bounds are available, we need to do a speculative check, since only one of them
        // needs to pass.
        (
            NominalT {
                reason: opaque_l_reason,
                nominal_type: lnom,
            },
            NominalT {
                reason: opaque_u_reason,
                nominal_type: unom,
            },
        ) if lnom.upper_t.is_some() && unom.lower_t.is_some() => {
            let lower_upper = lnom.upper_t.as_ref().unwrap().dupe();
            let upper_lower = unom.lower_t.as_ref().unwrap().dupe();
            let l_clone = l.dupe();
            let u_clone = u.dupe();
            let opaque_u_reason_c = opaque_u_reason.dupe();
            let opaque_l_reason_c = opaque_l_reason.dupe();
            let use_op_c1 = use_op.dupe();
            let use_op_c2 = use_op.dupe();
            speculation_kit::try_custom(
                cx, env,
                Some(use_op),
                None,
                None,
                opaque_l_reason.loc().clone(),
                vec![
                    Box::new(move |cx: &mut Context, env: &FlowJsEnv| {
                        let use_op = VirtualUseOp::Frame(
                            Arc::new(VirtualFrameUseOp::OpaqueTypeLowerBound {
                                opaque_t_reason: opaque_u_reason_c,
                            }),
                            Arc::new(use_op_c1),
                        );
                        FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &l_clone, &upper_lower)
                    }),
                    Box::new(move |cx: &mut Context, env: &FlowJsEnv| {
                        let use_op = VirtualUseOp::Frame(
                            Arc::new(VirtualFrameUseOp::OpaqueTypeUpperBound {
                                opaque_t_reason: opaque_l_reason_c,
                            }),
                            Arc::new(use_op_c2),
                        );
                        FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &lower_upper, &u_clone)
                    }),
                ],
            )
        }
        // Opaque types may be treated as their upper bound when they are a lower bound for a use
        (
            NominalT {
                reason: opaque_t_reason,
                nominal_type: lnom,
            },
            _,
        ) if let Some(t) = &lnom.upper_t => {
            let use_op = VirtualUseOp::Frame(
                Arc::new(VirtualFrameUseOp::OpaqueTypeUpperBound {
                    opaque_t_reason: opaque_t_reason.dupe(),
                }),
                Arc::new(use_op),
            );
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, t, u)
        }

        // Similar to the NominalT ~> NominalT case where the lower bound has an
        // `upper_t` and the upper bound has a `lower_t`.
        // We need to do the same for GenericT.
        (
            GenericT(box GenericTData {
                reason,
                bound: lower_upper,
                ..
            }),
            NominalT {
                reason: opaque_u_reason,
                nominal_type: unom,
            },
        ) if unom.lower_t.is_some() => {
            let upper_lower = unom.lower_t.as_ref().unwrap().dupe();
            let l_clone = l.dupe();
            let u_clone = u.dupe();
            let lower_upper = lower_upper.dupe();
            let opaque_u_reason_c = opaque_u_reason.dupe();
            let reason_c = reason.dupe();
            let use_op_c1 = use_op.dupe();
            let use_op_c2 = use_op.dupe();
            speculation_kit::try_custom(
                cx, env,
                Some(use_op),
                None,
                None,
                reason.loc().clone(),
                vec![
                    Box::new(move |cx: &mut Context, env: &FlowJsEnv| {
                        let use_op = VirtualUseOp::Frame(
                            Arc::new(VirtualFrameUseOp::OpaqueTypeLowerBound {
                                opaque_t_reason: opaque_u_reason_c,
                            }),
                            Arc::new(use_op_c1),
                        );
                        FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &l_clone, &upper_lower)
                    }),
                    Box::new(move |cx: &mut Context, env: &FlowJsEnv| {
                        let repositioned = FlowJs::reposition_reason(
                            cx, env,
                            Some(trace),
                            &reason_c,
                            None,
                            &lower_upper,
                        )?;
                        FlowJs::rec_flow_t_with_env(cx, env, trace, use_op_c2, &repositioned, &u_clone)
                    }),
                ],
            )
        }

        // Opaque types may be treated as their lower bound when they are a upper bound for a use
        (
            _,
            NominalT {
                reason: opaque_t_reason,
                nominal_type: unom,
            },
        ) if let Some(t) = &unom.lower_t => {
            let use_op = VirtualUseOp::Frame(
                Arc::new(VirtualFrameUseOp::OpaqueTypeLowerBound {
                    opaque_t_reason: opaque_t_reason.dupe(),
                }),
                Arc::new(use_op),
            );
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, l, t)
        }

        // `ExactObjectType` is the supertype of every exact object type, a lower bound no type
        // can express.
        (DefT(_, deref!(ObjT(obj))), NominalT { nominal_type: unom, .. })
            if unom.nominal_id == nominal::Id::ExactObjectType =>
        {
            if obj_type::is_exact(&obj.flags.obj_kind) {
                Ok(())
            } else {
                flow_js_utils::exact_obj_error(cx, env, &obj.flags.obj_kind, use_op, u, l)
            }
        }

        // *********************
        // functions statics
        // *********************
        (DefT(reason, deref!(FunT(static_, _))), AnyT(_, _)) => {
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                static_,
                &UseT::new(ReposLowerT {
                    reason: reason.dupe(),
                    use_desc: false,
                    use_t: Box::new(UseT::new(UseT(use_op, u.dupe()))),
                }),
            )
        }

        // *****************
        // class statics
        // *****************
        (DefT(reason, deref!(ClassT(instance))), DefT(_, deref!(ObjT(_))) | AnyT(_, _)) => {
            let collector = TypeCollector::create();
            FlowJs::rec_flow_with_env(
                cx, env, trace, instance,
                &UseT::new(GetStaticsT(reason.dupe(), collector.dupe())),
            )?;
            for static_ in collector.collect_to_vec() {
                FlowJs::rec_flow_t_with_env(cx, env, trace, use_op.dupe(), &static_, u)?;
            }
            Ok(())
        }

        // ************************
        // classes as functions
        // ************************

        // When a class value flows to a function annotation or call site, check for
        // the presence of a call property in the former (as a static) compatible
        // with the latter.
        //
        // TODO: Call properties are excluded from the subclass compatibility
        // checks, which makes it unsafe to call a Class<T> type like this.
        // For example:
        //
        // ```js
        // declare class A { static (): string };
        // declare class B extends A { static (): number }
        // var klass: Class<A> = B;
        // var foo: string = klass(); // passes, but `foo` is a number
        // ```
        //
        // The same issue is also true for constructors, which are similarly
        // excluded from subclass compatibility checks, but are allowed on ClassT
        // types.
        (DefT(reason, deref!(ClassT(instance))), DefT(_, deref!(FunT(_, _)))) => {
            let collector = TypeCollector::create();
            FlowJs::rec_flow_with_env(
                cx, env, trace, instance,
                &UseT::new(GetStaticsT(reason.dupe(), collector.dupe())),
            )?;
            for static_ in collector.collect_to_vec() {
                FlowJs::rec_flow_t_with_env(cx, env, trace, use_op.dupe(), &static_, u)?;
            }
            Ok(())
        }

        // *********
        // Enums
        // *********
        (
            DefT(_, deref!(EnumObjectT { enum_info: ei1, .. })),
            DefT(_, deref!(EnumObjectT { enum_info: ei2, .. })),
        ) if let (
                EnumInfoInner::ConcreteEnum(c1),
                EnumInfoInner::ConcreteEnum(c2),
            ) = (ei1.deref().deref(), ei2.deref().deref())
                && c1.enum_id == c2.enum_id =>
        {
            Ok(())
        }
        (DefT(_, deref!(EnumValueT(ei1))), DefT(_, deref!(EnumValueT(ei2))))
            if let (
                EnumInfoInner::ConcreteEnum(c1),
                EnumInfoInner::ConcreteEnum(c2),
            ) = (ei1.deref().deref(), ei2.deref().deref())
                && c1.enum_id == c2.enum_id =>
        {
            Ok(())
        }
        (
            DefT(enum_reason_l, deref!(EnumObjectT { enum_info: ei1, .. })),
            DefT(enum_reason_u, deref!(EnumObjectT { enum_info: ei2, .. })),
        )
        | (
            DefT(enum_reason_l, deref!(EnumValueT(ei1))),
            DefT(enum_reason_u, deref!(EnumValueT(ei2))),
        ) if let (EnumInfoInner::ConcreteEnum(c1), EnumInfoInner::ConcreteEnum(c2)) =
            (ei1.deref().deref(), ei2.deref().deref())
            && type_util::nominal_id_have_same_logical_module(
                &cx.file_options(),
                cx.projects_options(),
                (&c1.enum_id, Some(c1.enum_name.as_str())),
                (&c2.enum_id, Some(c2.enum_name.as_str())),
            ) && c1.members.keys().collect::<std::collections::BTreeSet<_>>()
                == c2.members.keys().collect::<std::collections::BTreeSet<_>>()
                && c1.has_unknown_members == c2.has_unknown_members =>
        {
            if type_util::is_in_common_interface_conformance_check(&use_op) {
                let use_op = VirtualUseOp::Frame(
                    Arc::new(VirtualFrameUseOp::EnumRepresentationTypeCompatibility {
                        lower: enum_reason_l.dupe(),
                        upper: enum_reason_u.dupe(),
                    }),
                    Arc::new(use_op),
                );
                FlowJs::rec_flow_t_with_env(
                    cx, env, trace, use_op, &c1.representation_t, &c2.representation_t,
                )?;
            }
            Ok(())
        }

        (
            DefT(_, deref!(EnumObjectT { enum_value_t: ev1, .. })),
            DefT(_, deref!(EnumObjectT { enum_value_t: ev2, .. })),
        ) => {
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, ev1, ev2)
        }
        (DefT(enum_reason_l, deref!(EnumValueT(ei_l))), DefT(enum_reason_u, deref!(EnumValueT(ei_u))))
            if let EnumInfoInner::AbstractEnum {
                    representation_t: repr_u,
                } = ei_u.deref().deref() =>
        {
            let repr_l = match ei_l.deref().deref() {
                EnumInfoInner::ConcreteEnum(c) => &c.representation_t,
                EnumInfoInner::AbstractEnum { representation_t } => representation_t,
            };
            let use_op = VirtualUseOp::Frame(
                Arc::new(VirtualFrameUseOp::EnumRepresentationTypeCompatibility {
                    lower: enum_reason_l.dupe(),
                    upper: enum_reason_u.dupe(),
                }),
                Arc::new(use_op),
            );
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, repr_l, repr_u)
        }
        (DefT(enum_reason, deref!(EnumValueT(ei))), _)
            if let repr = match ei.deref().deref() {
                    EnumInfoInner::ConcreteEnum(c) => &c.representation_t,
                    EnumInfoInner::AbstractEnum { representation_t } => representation_t,
                }
                && type_util::quick_subtype(None::<&mut fn(&Type)>, repr, u) =>
        {
            let representation_type = match repr.deref() {
                DefT(_, deref!(BoolGeneralT | SingletonBoolT { .. })) => {
                    Some(FlowSmolStr::new_inline("boolean"))
                }
                DefT(_, deref!(NumGeneralT(_) | SingletonNumT { .. })) => {
                    Some(FlowSmolStr::new_inline("number"))
                }
                DefT(_, deref!(StrGeneralT(_) | SingletonStrT { .. })) => {
                    Some(FlowSmolStr::new_inline("string"))
                }
                DefT(_, deref!(SymbolT | UniqueSymbolT(_))) => {
                    Some(FlowSmolStr::new_inline("symbol"))
                }
                DefT(_, deref!(BigIntGeneralT(_) | SingletonBigIntT { .. })) => {
                    Some(FlowSmolStr::new_inline("bigint"))
                }
                _ => None,
            };
            let in_type_arg_position = {
                fn loop_(use_op: &UseOp) -> bool {
                    match use_op {
                        VirtualUseOp::Op(_) => false,
                        VirtualUseOp::Frame(
                            deref!(VirtualFrameUseOp::TypeArgCompatibility(_)
                                | VirtualFrameUseOp::TypeParamBound { .. }),
                            _,
                        ) => {
                            true
                        }
                        VirtualUseOp::Frame(_, parent) => loop_(parent),
                    }
                }
                loop_(&use_op)
            };
            let explanation = if in_type_arg_position {
                None
            } else {
                match ei.deref().deref() {
                    EnumInfoInner::ConcreteEnum(_) => representation_type.map(|representation_type| {
                        intermediate_error_types::Explanation::ExplanationConcreteEnumCasting {
                            representation_type,
                        }
                    }),
                    EnumInfoInner::AbstractEnum { .. } => Some(
                        intermediate_error_types::Explanation::ExplanationAbstractEnumCasting,
                    ),
                }
            };
            let mut error_data = flow_js_utils::incompatible_types_error_data(
                l,
                u,
                use_op,
                explanation,
                vec![],
            );
            if let Some(desc) = enum_reason.desc(false) {
                error_data.lower_desc = type_or_type_desc::TypeOrTypeDescT::TypeDesc(
                    ErrorTypeDesc::ReasonDesc(desc.clone()),
                );
            }
            flow_js_utils::add_output_with_env(
                cx,env,
                ErrorMessage::EEnumError(EnumErrorKind::EnumIncompatible(Box::new(error_data))),
            )
        }

        (
            GenericT(box GenericTData {
                bound: bound1,
                id: id1,
                reason: reason1,
                name: name1,
                no_infer: no_infer1,
            }),
            GenericT(box GenericTData {
                bound: bound2,
                id: id2,
                reason: reason2,
                name: name2,
                no_infer: no_infer2,
            }),
        ) => {
            use flow_typing_generics::GenericSatResult;
            let result = id1.satisfies(
                &|msgs| {
                    flow_typing_debug::verbose::print_if_verbose_lazy(
                        cx,
                        Some(&trace),
                        None,
                        None,
                        || msgs.to_vec(),
                    );
                },
                id2,
            );
            match result {
                GenericSatResult::Satisfied => {
                    let t1 = FlowJs::reposition_reason(cx, env, Some(trace), reason1, None, bound1)?;
                    let t2 = FlowJs::reposition_reason(cx, env, Some(trace), reason2, None, bound2)?;
                    FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &t1, &t2)
                }
                GenericSatResult::Lower(id) => {
                    let new_l = Type::new(GenericT(Box::new(GenericTData {
                        reason: reason1.dupe(),
                        name: name1.dupe(),
                        bound: bound1.dupe(),
                        no_infer: *no_infer1,
                        id,
                    })));
                    let t2 = FlowJs::reposition_reason(cx, env, Some(trace), reason2, None, bound2)?;
                    FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &new_l, &t2)
                }
                GenericSatResult::Upper(id) => {
                    let t1 = FlowJs::reposition_reason(cx, env, Some(trace), reason1, None, bound1)?;
                    let new_u = Type::new(GenericT(Box::new(GenericTData {
                        reason: reason2.dupe(),
                        name: name2.dupe(),
                        bound: bound2.dupe(),
                        no_infer: *no_infer2,
                        id,
                    })));
                    FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &t1, &new_u)
                }
            }
        }

        (GenericT(box GenericTData { reason, bound, .. }), _) => {
            let t = FlowJs::reposition_reason(cx, env, Some(trace), reason, None, bound)?;
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &t, u)
        }
        (ImplicitInstantiationTvar(data), _) => {
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &env.frozen_tvar_bound(data), u)
        }
        (_, GenericT(box GenericTData { reason, .. })) => {
            let bot = Type::new(DefT(
                reason.dupe().replace_desc(None),
                DefT::new(EmptyT),
            ));
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, l, &bot)
        }
        (ObjProtoT(reason), _) => {
            let obj_proto =
                FlowJs::get_builtin_type_with_env(cx, env, Some(trace), reason, Some(true), "Object")?;
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &obj_proto, u)
        }
        (_, ObjProtoT(reason)) => {
            let obj_proto =
                FlowJs::get_builtin_type_with_env(cx, env, Some(trace), reason, Some(true), "Object")?;
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, l, &obj_proto)
        }
        (FunProtoT(reason), _) => {
            let fun_proto =
                FlowJs::get_builtin_type_with_env(cx, env, Some(trace), reason, Some(true), "Function")?;
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &fun_proto, u)
        }
        (_, FunProtoT(reason)) => {
            let fun_proto =
                FlowJs::get_builtin_type_with_env(cx, env, Some(trace), reason, Some(true), "Function")?;
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, l, &fun_proto)
        }
        (DefT(lreason, deref!(MixedT(MixedFlavor::MixedFunction))), DefT(ureason, deref!(FunT(_, _)))) => {
            flow_js_utils::add_output_with_env(
                cx,env,
                flow_js_utils::incompatible_type_error(
                    l,
                    IncompatibleUpperData {
                        loc: ureason.loc().dupe(),
                        kind: flow_typing_errors::error_message::UpperKind::IncompatibleMixedCallT,
                    },
                    Some(use_op.dupe()),
                ),
            )?;
            let any = any_t::make(AnySource::AnyError(None), lreason.dupe());
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &any, u)
        }
        (
            NominalT {
                reason,
                nominal_type: lnom,
            },
            _,
        ) if lnom.nominal_id == nominal::Id::InternalEnforceUnionOptimized => {
            flow_js_utils::add_output_with_env(
                cx,env,
                ErrorMessage::EUnionOptimizationOnNonUnion(Box::new(
                    EUnionOptimizationOnNonUnionData {
                        loc: reason.loc().dupe(),
                        arg_loc: type_util::ref_loc_of_t(u).dupe(),
                        arg_desc: flow_js_utils::type_or_type_desc_for_error(u),
                    },
                )),
            )
        }

        // Every remaining lower bound asks `o` whether it is one of its keys,
        // the same way the key rules above do. `o` is not necessarily concrete
        // -- it can be an alias, an import or a type application -- and only
        // the constraint system can resolve it. `keyof any` accepting every key
        // falls out of this: `any` has every prop.
        // NOTE: this has to stay below the union, intersection and generic
        // rules, which own their lower bound whatever the upper bound is.
        (_, KeysT(reason_op, o)) => {
            let u_use = UseT::new(HasOwnPropT(Box::new(HasOwnPropTData {
                use_op,
                reason: type_util::reason_of_t(l).dupe(),
                type_: l.dupe(),
            })));
            FlowJs::rec_flow_with_env(
                cx, env,
                trace,
                o,
                &UseT::new(ReposLowerT {
                    reason: reason_op.dupe(),
                    use_desc: false,
                    use_t: Box::new(u_use),
                }),
            )
        }

        _ => flow_js_utils::add_output_with_env(
            cx,env,
            flow_js_utils::incompatible_types_error(l, u, use_op, None),
        ),
    }
}

pub fn rec_flow_p<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    use_op: UseOp,
    report_polarity: bool,
    lower: PropPolarityMismatchObject<&Type, ALoc>,
    upper: PropPolarityMismatchObject<&Type, ALoc>,
    propref: &PropRef,
    lp: &PropertyType,
    up: &PropertyType,
) -> Result<(), FlowJsException> {
    rec_flow_p_with_lower_upper_property(
        cx,
        env,
        trace,
        use_op,
        None,
        TypeStrictnessKind::Flow,
        report_polarity,
        lower,
        upper,
        propref,
        lp,
        up,
    )
}

pub fn rec_flow_p_with_lower_upper_property<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: Option<DepthTrace>,
    use_op: UseOp,
    lower_upper_property: Option<(&Property, &Property)>,
    strictness_kind: TypeStrictnessKind,
    report_polarity: bool,
    lower: PropPolarityMismatchObject<&Type, ALoc>,
    upper: PropPolarityMismatchObject<&Type, ALoc>,
    propref: &PropRef,
    lp: &PropertyType,
    up: &PropertyType,
) -> Result<(), FlowJsException> {
    let errs = rec_flow_p_inner(
        cx,
        env,
        trace,
        use_op.dupe(),
        lower_upper_property,
        None,
        strictness_kind,
        report_polarity,
        propref,
        lp,
        up,
    )?;
    add_output_prop_polarity_mismatch(cx, env, use_op, lower, upper, errs, strictness_kind)?;
    Ok(())
}
