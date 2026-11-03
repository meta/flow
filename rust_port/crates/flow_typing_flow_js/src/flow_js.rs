/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! This module describes the subtyping algorithm that forms the core of
//! typechecking. The algorithm (in its basic form) is described in Francois
//! Pottier's thesis. The main data structures maintained by the algorithm are:
//! (1) for every type variable, which type variables form its lower and upper
//! bounds (i.e., flow in and out of the type variable); and (2) for every type
//! variable, which concrete types form its lower and upper bounds. Every new
//! subtyping constraint added to the system is deconstructed into its subparts,
//! until basic flows between type variables and other type variables or concrete
//! types remain; these flows are then viewed as links in a chain, bringing
//! together further concrete types and type variables to participate in
//! subtyping. This process continues till a fixpoint is reached---which itself
//! is guaranteed to exist, and is usually reached in very few steps.

use std::ops::Deref;
use std::rc::Rc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_aloc::ALoc;
use flow_common::polarity::Polarity;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::reason::ReasonDesc;
use flow_common::reason::VirtualReasonDesc;
use flow_common::subst_name;
use flow_common::subst_name::SubstName;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_lazy::ContextFamily;
use flow_typing_context::Context;
use flow_typing_context::Cx;
use flow_typing_context::SubstCacheErr;
use flow_typing_errors::error_message::EInvariantSubtypingWithUseOpData;
use flow_typing_errors::error_message::EnumErrorKind;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::InternalError;
use flow_typing_errors::error_message::UpperKind;
use flow_typing_errors::intermediate_error_types::ExpectedModulePurpose;
use flow_typing_errors::intermediate_error_types::ExplanationWithLazyParts;
use flow_typing_errors::intermediate_error_types::PropPolarityMismatchObject;
use flow_typing_flow_common::flow_cache;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_common::flow_js_utils::UnionOptimizationGuardResult;
use flow_typing_flow_common::flow_js_utils::callee_recorder;
use flow_typing_flow_common::flow_js_utils::enum_proto;
use flow_typing_flow_common::flow_js_utils::tvar_visitors;
use flow_typing_flow_common::flow_js_utils::use_op_of_lookup_action;
use flow_typing_flow_common::instantiation_utils;
use flow_typing_flow_common::obj_type;
use flow_typing_flow_common::type_subst;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_flow_js_env::type_app_expansion;
use flow_typing_generics::GenericId;
use flow_typing_type::type_::AnyErrorKind;
use flow_typing_type::type_::AnySource;
use flow_typing_type::type_::ArrLiteral;
use flow_typing_type::type_::ArrType;
use flow_typing_type::type_::ArrayATData;
use flow_typing_type::type_::CallAction;
use flow_typing_type::type_::CallArg;
use flow_typing_type::type_::CallArgInner;
use flow_typing_type::type_::CanonicalRendersForm;
use flow_typing_type::type_::ClassBinding;
use flow_typing_type::type_::ConcretizationKind;
use flow_typing_type::type_::ConstructorTData;
use flow_typing_type::type_::Cont;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::DepthTrace;
use flow_typing_type::type_::DerivedType;
use flow_typing_type::type_::Destructor;
use flow_typing_type::type_::DictType;
use flow_typing_type::type_::DroType;
use flow_typing_type::type_::ElemAction;
use flow_typing_type::type_::EnumInfo;
use flow_typing_type::type_::EnumInfoInner;
use flow_typing_type::type_::FieldData;
use flow_typing_type::type_::Flags;
use flow_typing_type::type_::FrameUseOp;
use flow_typing_type::type_::FunParam;
use flow_typing_type::type_::FunRestParam;
use flow_typing_type::type_::FunType;
use flow_typing_type::type_::FuncallType;
use flow_typing_type::type_::GetEnumKind;
use flow_typing_type::type_::GetPrivatePropTData;
use flow_typing_type::type_::GetPropTData;
use flow_typing_type::type_::ImplicitInstantiationTvarData;
use flow_typing_type::type_::ImplicitInstantiationTvarKind;
use flow_typing_type::type_::InstType;
use flow_typing_type::type_::InstTypeInner;
use flow_typing_type::type_::InstanceKind;
use flow_typing_type::type_::InstanceT;
use flow_typing_type::type_::InstanceTInner;
use flow_typing_type::type_::LazyHintT;
use flow_typing_type::type_::Literal;
use flow_typing_type::type_::LookupAction;
use flow_typing_type::type_::LookupKind;
use flow_typing_type::type_::LookupPropsForSubtypingData;
use flow_typing_type::type_::MappedTypeHomomorphicFlag;
use flow_typing_type::type_::MethodAction;
use flow_typing_type::type_::MethodCallType;
use flow_typing_type::type_::MixedFlavor;
use flow_typing_type::type_::NominalType;
use flow_typing_type::type_::NominalTypeInner;
use flow_typing_type::type_::NumberLiteral;
use flow_typing_type::type_::ObjKind;
use flow_typing_type::type_::ObjLiteral;
use flow_typing_type::type_::ObjType;
use flow_typing_type::type_::OptionalIndexedAccessIndex;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::PredicateConcretetizerVariant;
use flow_typing_type::type_::PrivateMethodTData;
use flow_typing_type::type_::PropRef;
use flow_typing_type::type_::Property;
use flow_typing_type::type_::PropertyInner;
use flow_typing_type::type_::PropertySource;
use flow_typing_type::type_::PropertyType;
use flow_typing_type::type_::ReactAbstractComponentTData;
use flow_typing_type::type_::ReactDro;
use flow_typing_type::type_::ReactEffectType;
use flow_typing_type::type_::ReadPropData;
use flow_typing_type::type_::RendersVariant;
use flow_typing_type::type_::ResolveSpreadType;
use flow_typing_type::type_::ResolvedParam;
use flow_typing_type::type_::RootUseOp;
use flow_typing_type::type_::Selector;
use flow_typing_type::type_::SetMode;
use flow_typing_type::type_::SetPrivatePropTData;
use flow_typing_type::type_::SpreadResolve;
use flow_typing_type::type_::Targ;
use flow_typing_type::type_::TupleATData;
use flow_typing_type::type_::TupleElement;
use flow_typing_type::type_::TupleView;
use flow_typing_type::type_::Tvar;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeDestructorT;
use flow_typing_type::type_::TypeDestructorTInner;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::TypeMap;
use flow_typing_type::type_::TypeParam;
use flow_typing_type::type_::TypeParamInner;
use flow_typing_type::type_::TypeStrictnessKind;
use flow_typing_type::type_::TypeTKind;
use flow_typing_type::type_::UnifyCause;
use flow_typing_type::type_::UnresolvedParam;
use flow_typing_type::type_::UnsoundnessKind;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::UseT;
use flow_typing_type::type_::UseTInner;
use flow_typing_type::type_::VirtualFrameUseOp;
use flow_typing_type::type_::VirtualRootUseOp;
use flow_typing_type::type_::VirtualUseOp;
use flow_typing_type::type_::WriteCtx;
use flow_typing_type::type_::WritePropData;
use flow_typing_type::type_::annot;
use flow_typing_type::type_::any_t;
use flow_typing_type::type_::call_of_method_app;
use flow_typing_type::type_::concretize_seen;
use flow_typing_type::type_::constraint;
use flow_typing_type::type_::drop_generic;
use flow_typing_type::type_::dummy_static;
use flow_typing_type::type_::elemt_of_arrtype;
use flow_typing_type::type_::empty_t;
use flow_typing_type::type_::eval;
use flow_typing_type::type_::global_this;
use flow_typing_type::type_::hint_unavailable;
use flow_typing_type::type_::inter_rep;
use flow_typing_type::type_::mixed_t;
use flow_typing_type::type_::mk_boundfunctiontype;
use flow_typing_type::type_::name_of_propref;
use flow_typing_type::type_::nominal;
use flow_typing_type::type_::null;
use flow_typing_type::type_::num_module_t;
use flow_typing_type::type_::object;
use flow_typing_type::type_::open_tvar;
use flow_typing_type::type_::poly;
use flow_typing_type::type_::primitive_promoting_use_t;
use flow_typing_type::type_::properties;
use flow_typing_type::type_::property;
use flow_typing_type::type_::react;
use flow_typing_type::type_::root_of_use_op;
use flow_typing_type::type_::str_module_t;
use flow_typing_type::type_::type_collector;
use flow_typing_type::type_::type_or_type_desc::TypeOrTypeDescT;
use flow_typing_type::type_::union_rep;
use flow_typing_type::type_::unknown_use;
use flow_typing_type::type_::unsoundness;
use flow_typing_type::type_::void;
use flow_typing_type::type_util;
use flow_typing_type::type_util::loc_of_t;
use flow_typing_type::type_util::mk_named_prop;
use flow_typing_type::type_util::reason_of_propref;
use flow_typing_type::type_util::reason_of_t;
use flow_typing_type::type_util::subtype_this_of_function;
use flow_typing_type::type_util::tuple_ts_of_elements;
use flow_typing_type::type_util::use_op_of_use_t;
use flow_utils_concurrency::job_error::JobError;
use flow_utils_union_find::Node;
use flow_utils_union_find::Root;
use vec1::Vec1;

use crate::implicit_instantiation;
use crate::react_kit;
use crate::subtyping_kit;
use crate::tvar_resolver;

mod any_helpers;
pub(crate) mod const_fold_expansion;
mod constraint_helpers;
mod dispatch;
mod enum_helpers;
mod eval_helpers;
mod get_prop_helpers;
pub(crate) use get_prop_helpers::prop_typo_suggestion_for_name;
mod helpers;
mod inheritance_helpers;
mod instantiation_helpers;
mod multi_arg_helpers;
pub mod recursion_check;
mod unification_helpers;

pub struct FlowJs;

impl FlowJs {
    // Base methods
    pub fn flow_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        t: &Type,
        use_t: &UseT<Cx>,
    ) -> Result<(), FlowJsException> {
        helpers::flow(cx, env, (t, use_t))
    }

    pub(super) fn flow_opt_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: Option<DepthTrace>,
        t: &Type,
        use_t: &UseT<Cx>,
    ) -> Result<(), FlowJsException> {
        helpers::flow_opt(cx, env, trace, (t, use_t))
    }

    fn flow_p_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        use_op: UseOp,
        lower: PropPolarityMismatchObject<&Type, ALoc>,
        upper: PropPolarityMismatchObject<&Type, ALoc>,
        propref: &PropRef,
        prop1: &PropertyType,
        prop2: &PropertyType,
    ) -> Result<(), FlowJsException> {
        helpers::flow_p(cx, env, use_op, lower, upper, propref, (prop1, prop2))
    }

    pub fn flow_t_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        t1: &Type,
        t2: &Type,
    ) -> Result<(), FlowJsException> {
        helpers::flow_t(cx, env, (t1, t2))
    }

    pub fn reposition_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: Option<DepthTrace>,
        loc: ALoc,
        desc: Option<Option<&ReasonDesc>>,
        annot_loc: Option<ALoc>,
        t: Type,
    ) -> Result<Type, FlowJsException> {
        helpers::reposition(cx, env, trace, loc, desc, annot_loc, t)
    }

    pub(super) fn rec_flow_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        t: &Type,
        use_t: &UseT<Cx>,
    ) -> Result<(), FlowJsException> {
        helpers::rec_flow(cx, env, trace, (t, use_t))
    }

    pub(super) fn rec_flow_t_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: UseOp,
        t1: &Type,
        t2: &Type,
    ) -> Result<(), FlowJsException> {
        helpers::rec_flow_t(cx, env, trace, use_op, (t1, t2))
    }

    pub(super) fn rec_unify_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: UseOp,
        unify_cause: UnifyCause,
        unify_any: Option<bool>,
        t1: &Type,
        t2: &Type,
    ) -> Result<(), FlowJsException> {
        helpers::rec_unify(cx, env, trace, use_op, unify_cause, unify_any, t1, t2)
    }

    fn unify_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        use_op: Option<UseOp>,
        unify_cause: UnifyCause,
        t1: &Type,
        t2: &Type,
    ) -> Result<(), FlowJsException> {
        helpers::unify(cx, env, use_op, unify_cause, t1, t2)
    }

    pub(super) fn unify_opt_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: Option<DepthTrace>,
        use_op: UseOp,
        unify_cause: UnifyCause,
        unify_any: Option<bool>,
        t1: &Type,
        t2: &Type,
    ) -> Result<(), FlowJsException> {
        helpers::unify_opt(cx, env, trace, use_op, unify_cause, unify_any, t1, t2)
    }

    pub(super) fn filter_optional_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: Option<DepthTrace>,
        reason: &Reason,
        t: &Type,
    ) -> Result<u32, FlowJsException> {
        helpers::filter_optional(cx, env, trace, reason, t)
    }

    pub(super) fn mk_typeapp_instance_annot_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: Option<DepthTrace>,
        use_op: UseOp,
        reason_op: &Reason,
        reason_tapp: &Reason,
        from_value: bool,
        use_desc: Option<bool>,
        t: &Type,
        targs: Rc<[Type]>,
    ) -> Result<Type, FlowJsException> {
        helpers::mk_typeapp_instance_annot(
            cx,
            env,
            trace,
            use_op,
            reason_op,
            reason_tapp,
            from_value,
            use_desc,
            t,
            targs,
        )
    }

    pub(super) fn mk_typeapp_instance_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: Option<DepthTrace>,
        use_op: UseOp,
        reason_op: &Reason,
        reason_tapp: &Reason,
        from_value: bool,
        t: &Type,
        targs: Rc<[Type]>,
    ) -> Result<Type, FlowJsException> {
        helpers::mk_typeapp_instance(
            cx,
            env,
            trace,
            use_op,
            reason_op,
            reason_tapp,
            from_value,
            t,
            targs,
        )
    }

    // Builtins methods
    pub fn get_builtin_type_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: Option<DepthTrace>,
        reason: &Reason,
        use_desc: Option<bool>,
        name: &str,
    ) -> Result<Type, FlowJsException> {
        helpers::get_builtin_type(cx, env, trace, reason, use_desc, name)
    }

    pub(super) fn get_builtin_react_type_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: Option<DepthTrace>,
        reason: &Reason,
        use_desc: Option<bool>,
        purpose: ExpectedModulePurpose,
    ) -> Result<Type, FlowJsException> {
        helpers::get_builtin_react_type(cx, env, trace, reason, use_desc, purpose)
    }

    pub fn get_builtin_typeapp_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason: &Reason,
        use_desc: Option<bool>,
        name: &str,
        targs: Vec<Type>,
    ) -> Type {
        helpers::get_builtin_typeapp(cx, env, reason, use_desc, name, targs)
    }

    pub(super) fn get_builtin_react_typeapp_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason: &Reason,
        use_desc: Option<bool>,
        purpose: ExpectedModulePurpose,
        targs: Vec<Type>,
    ) -> Result<Type, FlowJsException> {
        helpers::get_builtin_react_typeapp(cx, env, reason, use_desc, purpose, targs)
    }

    pub(crate) fn perform_read_prop_action<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: VirtualUseOp<ALoc>,
        propref: &PropRef,
        property_type: &PropertyType,
        reason: &Reason,
        dro: Option<(ALoc, DroType)>,
        tvar: &Tvar,
    ) -> Result<(), FlowJsException> {
        let react_dro = dro.map(|(loc, dro_type)| ReactDro(loc, dro_type));
        (flow_js_utils::get_prop_t_kit::perform_read_prop_action::<FlowJs>(
            cx,
            env,
            &trace,
            use_op,
            propref,
            property_type.clone(),
            reason,
            &react_dro,
        )?)(cx, tvar.dupe())
    }

    // Subtyping methods
    pub fn speculative_subtyping_succeeds_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        t1: &Type,
        t2: &Type,
    ) -> Result<bool, JobError> {
        helpers::speculative_subtyping_succeeds_non_speculating(cx, env, t1, t2)
    }

    pub fn speculative_subtyping_succeeds_with_flow_errors<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        t1: &Type,
        t2: &Type,
    ) -> Result<bool, FlowJsException> {
        helpers::speculative_subtyping_succeeds(cx, env, t1, t2)
    }

    pub(super) fn possible_concrete_types_for_optional_chain_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        helpers::possible_concrete_types_for_optional_chain(cx, env, reason, t)
    }

    pub fn possible_concrete_types_for_inspection_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        helpers::possible_concrete_types_for_inspection(cx, env, reason, t)
    }

    pub fn possible_concrete_types_for_enum_exhaustive_check_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        helpers::possible_concrete_types_for_enum_exhaustive_check(cx, env, reason, t)
    }

    pub fn possible_concrete_types_for_operators_checking_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        helpers::possible_concrete_types_for_operators_checking(cx, env, reason, t)
    }

    pub fn possible_concrete_types_for_object_assign<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        helpers::possible_concrete_types_for_object_assign(cx, env, reason, t)
    }

    fn possible_concrete_types_for_destructuring_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        helpers::possible_concrete_types_for_destructuring(cx, env, reason, t)
    }

    fn possible_concrete_types_for_computed_object_keys_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        helpers::possible_concrete_types_for_computed_object_keys(cx, env, reason, t)
    }

    pub fn reposition_reason<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: Option<DepthTrace>,
        reason: &Reason,
        use_desc: Option<bool>,
        t: &Type,
    ) -> Result<Type, FlowJsException> {
        helpers::reposition_reason(cx, env, trace, reason, use_desc.unwrap_or(false), t)
    }

    pub(super) fn multiflow_subtype_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: VirtualUseOp<ALoc>,
        reason: &Reason,
        call_args: &[CallArg],
        funtype: &FunType,
    ) -> Result<(), FlowJsException> {
        multi_arg_helpers::multiflow_subtype(cx, env, trace, use_op, reason, call_args, funtype)
    }

    pub(super) fn flow_type_args_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: UseOp,
        reason1: &Reason,
        reason2: &Reason,
        targs1: Rc<[(SubstName, Reason, Type, Option<Polarity>)]>,
        targs2: Rc<[(SubstName, Reason, Type, Option<Polarity>)]>,
        strictness_kind: TypeStrictnessKind,
    ) -> Result<(), FlowJsException> {
        inheritance_helpers::flow_type_args(
            cx,
            env,
            trace,
            use_op,
            reason1,
            reason2,
            targs1,
            targs2,
            strictness_kind,
        )
    }

    pub(super) fn instantiate_this_class_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        reason_op: &Reason,
        reason_tapp: &Reason,
        this_t: &Type,
        targs: Option<Rc<[Type]>>,
        t: &Type,
        cont: &Cont<Cx>,
    ) -> Result<(), FlowJsException> {
        instantiation_helpers::instantiate_this_class(
            cx,
            env,
            trace,
            reason_op,
            reason_tapp,
            this_t,
            targs,
            t,
            cont,
        )
    }

    pub(super) fn instantiate_poly_with_targs_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: UseOp,
        reason_op: &Reason,
        reason_tapp: &Reason,
        mut errs_ref: Option<&mut Vec<SubstCacheErr>>,
        unify_bounds: Option<bool>,
        poly_t: (ALoc, Vec1<TypeParam>, Type),
        targs: Vec<Type>,
    ) -> Result<(Type, Vec<(Type, SubstName)>), FlowJsException> {
        let unify_bounds = unify_bounds.unwrap_or(false);
        let (tparams_loc, xs, t) = poly_t;
        let mut errs_opt = errs_ref.as_deref_mut().map(std::mem::take);
        let result = flow_js_utils::instantiation_kit::instantiate_poly_with_targs::<FlowJs>(
            cx,
            env,
            trace,
            use_op,
            reason_op,
            reason_tapp,
            &mut errs_opt,
            unify_bounds,
            tparams_loc,
            &xs,
            t,
            targs,
        );
        if let (Some(errs), Some(v)) = (errs_opt, errs_ref) {
            *v = errs;
        }
        result
    }

    pub(super) fn instantiate_poly_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: UseOp,
        reason_op: &Reason,
        reason_tapp: &Reason,
        unify_bounds: Option<bool>,
        poly_t: (ALoc, Vec1<TypeParam>, Type),
    ) -> Result<(Type, Vec<(Type, SubstName)>), FlowJsException> {
        let unify_bounds = unify_bounds.unwrap_or(false);
        let (tparams_loc, xs, t) = poly_t;
        flow_js_utils::instantiation_kit::instantiate_poly_with_env::<FlowJs>(
            cx,
            env,
            trace,
            use_op,
            reason_op,
            reason_tapp,
            unify_bounds,
            tparams_loc,
            &xs,
            t,
        )
    }

    pub fn instantiate_poly_call_or_new<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        poly_info: (Reason, ALoc, Vec1<TypeParam>, Type),
        call_info: (UseOp, Reason, Option<Rc<[Targ]>>, LazyHintT<Cx>),
        implicit_check: &dyn Fn() -> flow_typing_implicit_instantiation_check::ImplicitInstantiationCheck,
    ) -> Result<Type, FlowJsException> {
        instantiation_helpers::instantiate_poly_call_or_new(
            cx,
            env,
            trace,
            poly_info,
            call_info,
            implicit_check,
        )
    }

    fn mk_typeapp_of_poly_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: UseOp,
        reason_op: &Reason,
        reason_tapp: &Reason,
        poly_id: poly::Id,
        loc: ALoc,
        tparams: Vec1<TypeParam>,
        t: &Type,
        targs: Rc<[Type]>,
    ) -> Result<Type, FlowJsException> {
        flow_js_utils::instantiation_kit::mk_typeapp_of_poly_with_env::<FlowJs>(
            cx,
            env,
            trace,
            use_op,
            reason_op,
            reason_tapp,
            poly_id,
            loc,
            &tparams,
            t.dupe(),
            targs,
        )
    }

    // Eval methods
    fn eval_selector_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: Option<DepthTrace>,
        annot: bool,
        reason: &Reason,
        t: &Type,
        selector: &Selector,
        tvar: &Tvar,
        index: i32,
    ) -> Result<(), FlowJsException> {
        eval_helpers::eval_selector(cx, env, trace, annot, reason, t, selector, tvar, index)
    }

    pub(crate) fn mk_type_destructor_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: UseOp,
        reason: &Reason,
        t: &Type,
        destructor: &Destructor,
        eval_id: eval::Id,
    ) -> Result<Type, FlowJsException> {
        eval_helpers::mk_type_destructor(cx, env, trace, use_op, reason, t, destructor, eval_id)
    }

    pub fn mk_possibly_evaluated_destructor_for_annotations<'cx>(
        cx: &mut Context<'cx>,
        use_op: UseOp,
        reason: &Reason,
        t: &Type,
        destructor: &Destructor,
        eval_id: eval::Id,
    ) -> Result<Type, FlowJsException> {
        eval_helpers::mk_possibly_evaluated_destructor_for_annotations(
            cx, use_op, reason, t, destructor, eval_id,
        )
    }

    // React methods
    pub(crate) fn react_subtype_class_component_render<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: UseOp,
        t: &Type,
        reason_op: &Reason,
        render_t: &Type,
    ) -> Result<(), FlowJsException> {
        react_kit::subtype_class_component_render(cx, env, trace, use_op, t, reason_op, render_t)
    }

    pub(crate) fn react_get_config<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        t: &Type,
        use_op: VirtualUseOp<ALoc>,
        reason_op: &Reason,
        polarity: Polarity,
        tout: &Type,
    ) -> Result<(), FlowJsException> {
        react_kit::get_config(cx, env, trace, t, use_op, reason_op, polarity, tout)
    }

    // ImplicitInstantiationKit methods
    pub(super) fn run_render_extractor_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        use_op: UseOp,
        reason: &Reason,
        t: &Type,
    ) -> Result<Type, FlowJsException> {
        implicit_instantiation::kit::run_render_extractor(cx, env, use_op, reason, t)
    }

    pub fn run_await<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        use_op: UseOp,
        reason: &Reason,
        t: &Type,
    ) -> Result<Type, FlowJsException> {
        implicit_instantiation::kit::run_await(cx, env, use_op, reason, t)
    }

    // S methods
    fn resolve_spread_list_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        use_op: UseOp,
        reason_op: &Reason,
        unresolved_params: flow_data_structure_wrapper::list::FlowOcamlList<UnresolvedParam>,
        spread_resolve: SpreadResolve,
    ) -> Result<(), FlowJsException> {
        multi_arg_helpers::resolve_spread_list(
            cx,
            env,
            use_op,
            reason_op,
            unresolved_params,
            spread_resolve,
        )
    }

    pub fn possible_concrete_types_for_predicate<'cx>(
        predicate_concretizer_variant: PredicateConcretetizerVariant,
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        helpers::possible_concrete_types_for_predicate(
            predicate_concretizer_variant,
            cx,
            env,
            reason,
            t,
        )
    }

    pub fn possible_concrete_types_for_sentinel_prop_test<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        helpers::possible_concrete_types_for_sentinel_prop_test(cx, env, reason, t)
    }

    fn singleton_concrete_type_for_cjs_extract_named_exports_and_type_exports_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason: &Reason,
        t: &Type,
    ) -> Result<Type, FlowJsException> {
        helpers::singleton_concrete_type_for_cjs_extract_named_exports_and_type_exports(
            cx, env, reason, t,
        )
    }

    pub fn singleton_concretize_type_for_imports_exports_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason: &Reason,
        t: &Type,
    ) -> Result<Type, FlowJsException> {
        helpers::singleton_concretize_type_for_imports_exports(cx, env, reason, t)
    }

    fn possible_concrete_types_for_imports_exports_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        helpers::possible_concrete_types_for_imports_exports(cx, env, reason, t)
    }

    pub fn singleton_concrete_type_for_inspection_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason: &Reason,
        t: &Type,
    ) -> Result<Type, FlowJsException> {
        helpers::singleton_concrete_type_for_inspection(cx, env, reason, t)
    }

    pub fn singleton_concrete_type_for_type_cast<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason: &Reason,
        t: &Type,
    ) -> Result<Type, FlowJsException> {
        helpers::singleton_concrete_type_for_type_cast(cx, env, reason, t)
    }

    fn all_possible_concrete_types_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        helpers::all_possible_concrete_types(cx, env, reason, t)
    }

    fn possible_concrete_types_for_unused_promise_check_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        helpers::possible_concrete_types_for_unused_promise_check(cx, env, reason, t)
    }

    fn singleton_concrete_type_for_match_arg_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        keep_unions: bool,
        reason: &Reason,
        t: &Type,
    ) -> Result<Type, FlowJsException> {
        helpers::singleton_concrete_type_for_match_arg(cx, env, keep_unions, reason, t)
    }

    fn possible_concrete_types_for_match_arg_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        keep_unions: bool,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        helpers::possible_concrete_types_for_match_arg(cx, env, keep_unions, reason, t)
    }
}

impl FlowJs {
    pub fn flow_p<'cx>(
        cx: &mut Context<'cx>,
        use_op: UseOp,
        lower: PropPolarityMismatchObject<&Type, ALoc>,
        upper: PropPolarityMismatchObject<&Type, ALoc>,
        propref: &PropRef,
        prop1: &PropertyType,
        prop2: &PropertyType,
    ) -> Result<(), FlowJsException> {
        Self::flow_p_with_env(
            cx,
            &FlowJsEnv::entry(),
            use_op,
            lower,
            upper,
            propref,
            prop1,
            prop2,
        )
    }

    pub fn get_builtin_typeapp<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        use_desc: Option<bool>,
        name: &str,
        targs: Vec<Type>,
    ) -> Type {
        Self::get_builtin_typeapp_with_env(cx, &FlowJsEnv::entry(), reason, use_desc, name, targs)
    }

    pub fn get_builtin_react_typeapp<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        use_desc: Option<bool>,
        purpose: ExpectedModulePurpose,
        targs: Vec<Type>,
    ) -> Result<Type, FlowJsException> {
        Self::get_builtin_react_typeapp_with_env(
            cx,
            &FlowJsEnv::entry(),
            reason,
            use_desc,
            purpose,
            targs,
        )
    }

    pub fn speculative_subtyping_succeeds<'cx>(
        cx: &mut Context<'cx>,
        t1: &Type,
        t2: &Type,
    ) -> Result<bool, JobError> {
        Self::speculative_subtyping_succeeds_with_env(cx, &FlowJsEnv::entry(), t1, t2)
    }

    pub fn possible_concrete_types_for_inspection<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        Self::possible_concrete_types_for_inspection_with_env(cx, &FlowJsEnv::entry(), reason, t)
    }

    pub fn possible_concrete_types_for_enum_exhaustive_check<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        Self::possible_concrete_types_for_enum_exhaustive_check_with_env(
            cx,
            &FlowJsEnv::entry(),
            reason,
            t,
        )
    }

    pub fn possible_concrete_types_for_operators_checking<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        Self::possible_concrete_types_for_operators_checking_with_env(
            cx,
            &FlowJsEnv::entry(),
            reason,
            t,
        )
    }

    pub fn possible_concrete_types_for_destructuring<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        Self::possible_concrete_types_for_destructuring_with_env(cx, &FlowJsEnv::entry(), reason, t)
    }

    pub fn possible_concrete_types_for_computed_object_keys<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        Self::possible_concrete_types_for_computed_object_keys_with_env(
            cx,
            &FlowJsEnv::entry(),
            reason,
            t,
        )
    }

    pub fn eval_selector<'cx>(
        cx: &mut Context<'cx>,
        trace: Option<DepthTrace>,
        annot: bool,
        reason: &Reason,
        t: &Type,
        selector: &Selector,
        tvar: &Tvar,
        index: i32,
    ) -> Result<(), FlowJsException> {
        Self::eval_selector_with_env(
            cx,
            &FlowJsEnv::entry(),
            trace,
            annot,
            reason,
            t,
            selector,
            tvar,
            index,
        )
    }

    pub fn resolve_spread_list<'cx>(
        cx: &mut Context<'cx>,
        use_op: UseOp,
        reason_op: &Reason,
        unresolved_params: flow_data_structure_wrapper::list::FlowOcamlList<UnresolvedParam>,
        spread_resolve: SpreadResolve,
    ) -> Result<(), FlowJsException> {
        Self::resolve_spread_list_with_env(
            cx,
            &FlowJsEnv::entry(),
            use_op,
            reason_op,
            unresolved_params,
            spread_resolve,
        )
    }

    pub fn singleton_concrete_type_for_cjs_extract_named_exports_and_type_exports<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        t: &Type,
    ) -> Result<Type, FlowJsException> {
        Self::singleton_concrete_type_for_cjs_extract_named_exports_and_type_exports_with_env(
            cx,
            &FlowJsEnv::entry(),
            reason,
            t,
        )
    }

    pub fn singleton_concretize_type_for_imports_exports<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        t: &Type,
    ) -> Result<Type, FlowJsException> {
        Self::singleton_concretize_type_for_imports_exports_with_env(
            cx,
            &FlowJsEnv::entry(),
            reason,
            t,
        )
    }

    pub fn possible_concrete_types_for_imports_exports<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        Self::possible_concrete_types_for_imports_exports_with_env(
            cx,
            &FlowJsEnv::entry(),
            reason,
            t,
        )
    }

    pub fn singleton_concrete_type_for_inspection<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        t: &Type,
    ) -> Result<Type, FlowJsException> {
        Self::singleton_concrete_type_for_inspection_with_env(cx, &FlowJsEnv::entry(), reason, t)
    }

    pub fn all_possible_concrete_types<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        Self::all_possible_concrete_types_with_env(cx, &FlowJsEnv::entry(), reason, t)
    }

    pub fn possible_concrete_types_for_unused_promise_check<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        Self::possible_concrete_types_for_unused_promise_check_with_env(
            cx,
            &FlowJsEnv::entry(),
            reason,
            t,
        )
    }

    pub fn singleton_concrete_type_for_match_arg<'cx>(
        cx: &mut Context<'cx>,
        keep_unions: bool,
        reason: &Reason,
        t: &Type,
    ) -> Result<Type, FlowJsException> {
        Self::singleton_concrete_type_for_match_arg_with_env(
            cx,
            &FlowJsEnv::entry(),
            keep_unions,
            reason,
            t,
        )
    }

    pub fn possible_concrete_types_for_match_arg<'cx>(
        cx: &mut Context<'cx>,
        keep_unions: bool,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        Self::possible_concrete_types_for_match_arg_with_env(
            cx,
            &FlowJsEnv::entry(),
            keep_unions,
            reason,
            t,
        )
    }
}

// ======================================================================
// Top-level re-exports
// ======================================================================

pub fn flow_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    (l, u): (&Type, &UseT<Cx>),
) -> Result<(), FlowJsException> {
    FlowJs::flow_with_env(cx, env, l, u)
}

pub fn flow_t_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    (t1, t2): (&Type, &Type),
) -> Result<(), FlowJsException> {
    FlowJs::flow_t_with_env(cx, env, t1, t2)
}

pub fn subst<'cx>(
    cx: &mut Context<'cx>,
    use_op: Option<UseOp>,
    force: Option<bool>,
    purpose: Option<type_subst::Purpose>,
    map: &FlowOrdMap<subst_name::SubstName, Type>,
    t: Type,
) -> Type {
    type_subst::subst(
        cx,
        use_op,
        force.unwrap_or(true),
        false,
        purpose.unwrap_or(type_subst::Purpose::Normal),
        map,
        t,
    )
}

fn mk_default<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    d: &flow_typing_default::Default<Type>,
) -> Result<Type, FlowJsException> {
    flow_typing_default::fold(
        d,
        &|t: &Type| -> Result<Type, FlowJsException> { Ok(t.dupe()) },
        &|t1: Result<Type, FlowJsException>,
          t2: Result<Type, FlowJsException>|
         -> Result<Type, FlowJsException> {
            let t1 = t1?;
            let t2 = t2?;
            let collector = type_collector::TypeCollector::create();
            collector.add(t1);
            collector.add(t2);
            Ok(collector
                .union_opt(reason.dupe())
                .unwrap_or_else(|| tvar_resolver::default_no_lowers(reason)))
        },
        &mut |r: Reason,
              t: Result<Type, FlowJsException>,
              sel: Selector|
         -> Result<Type, FlowJsException> {
            let t = t?;
            flow_typing_tvar::mk_no_wrap_where(cx, r.dupe(), |cx, _reason, tvar_id| {
                let tvar = Tvar::new(r.dupe(), tvar_id as u32);
                FlowJs::eval_selector_with_env(
                    cx,
                    env,
                    None,
                    false,
                    &r,
                    &t,
                    &sel,
                    &tvar,
                    flow_common::reason::mk_id() as i32,
                )
            })
        },
    )
}

// Export some functions without the trace parameter

fn get_builtin_type_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    use_desc: Option<bool>,
    x: &str,
) -> Result<Type, FlowJsException> {
    FlowJs::get_builtin_type_with_env(cx, env, None, reason, use_desc, x)
}

fn get_builtin_react_type_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    use_desc: Option<bool>,
    purpose: ExpectedModulePurpose,
) -> Result<Type, FlowJsException> {
    FlowJs::get_builtin_react_type_with_env(cx, env, None, reason, use_desc, purpose)
}

pub(super) fn filter_optional_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    opt_t: &Type,
) -> Result<u32, FlowJsException> {
    FlowJs::filter_optional_with_env(cx, env, None, reason, opt_t)
}

pub fn unify_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: Option<UseOp>,
    t1: &Type,
    t2: &Type,
) -> Result<(), FlowJsException> {
    FlowJs::unify_with_env(cx, env, use_op, UnifyCause::Uncategorized, t1, t2)
}

pub fn reposition_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    loc: ALoc,
    t: Type,
) -> Result<Type, FlowJsException> {
    FlowJs::reposition_with_env(cx, env, None, loc, None, None, t)
}

fn mk_typeapp_instance_annot_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: UseOp,
    reason_op: &Reason,
    reason_tapp: &Reason,
    from_value: bool,
    c: &Type,
    ts: Rc<[Type]>,
) -> Result<Type, FlowJsException> {
    FlowJs::mk_typeapp_instance_annot_with_env(
        cx,
        env,
        None,
        use_op,
        reason_op,
        reason_tapp,
        from_value,
        None,
        c,
        ts,
    )
}
fn mk_type_destructor_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: UseOp,
    reason: &Reason,
    t: &Type,
    d: &Destructor,
    id: eval::Id,
) -> Result<Type, FlowJsException> {
    match FlowJs::mk_type_destructor_with_env(
        cx,
        env,
        DepthTrace::dummy_trace(),
        use_op,
        reason,
        t,
        d,
        id.dupe(),
    ) {
        Ok(result) => Ok(result),
        Err(FlowJsException::LimitExceeded) => {
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::ERecursionLimit(reason.loc().dupe()),
            )?;
            let result = any_t::why(AnySource::AnyError(None), reason.dupe());
            let mut evaluated = cx.evaluated();
            evaluated.insert(id, result.dupe());
            cx.set_evaluated(evaluated);
            Ok(result)
        }
        Err(e) => Err(e),
    }
}

// exporting this for convenience
pub fn add_output_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    msg: ErrorMessage<ALoc>,
) -> Result<(), FlowJsException> {
    flow_js_utils::add_output_with_env(cx, env, msg)
}

pub fn flow<'cx>(cx: &mut Context<'cx>, pair: (&Type, &UseT<Cx>)) -> Result<(), FlowJsException> {
    flow_with_env(cx, &FlowJsEnv::entry(), pair)
}

pub fn get_builtin_type<'cx>(
    cx: &mut Context<'cx>,
    reason: &Reason,
    use_desc: Option<bool>,
    x: &str,
) -> Result<Type, FlowJsException> {
    get_builtin_type_with_env(cx, &FlowJsEnv::entry(), reason, use_desc, x)
}

pub fn unify<'cx>(
    cx: &mut Context<'cx>,
    use_op: Option<UseOp>,
    t1: &Type,
    t2: &Type,
) -> Result<(), FlowJsException> {
    unify_with_env(cx, &FlowJsEnv::entry(), use_op, t1, t2)
}

pub fn mk_type_destructor<'cx>(
    cx: &mut Context<'cx>,
    use_op: UseOp,
    reason: &Reason,
    t: &Type,
    d: &Destructor,
    id: eval::Id,
) -> Result<Type, FlowJsException> {
    mk_type_destructor_with_env(cx, &FlowJsEnv::entry(), use_op, reason, t, d, id)
}

// Non-speculating entry points
//
// These wrappers are called from checker entry points (statement.rs, merge.rs,
// env_resolution.rs, ...), which by definition are not inside a speculation
// branch: taking no [FlowJsEnv] is what makes that true, so no error can be
// deferred to a branch. For WorkerCanceled, TimedOut and DebugThrow they
// propagate via JobError, so the type system enforces propagation up to the
// per-file boundary in mk_check. Anything else is a bug and panics.

pub fn flow_non_speculating<'cx>(
    cx: &mut Context<'cx>,
    (l, u): (&Type, &UseT<Cx>),
) -> Result<(), JobError> {
    match flow_with_env(cx, &FlowJsEnv::entry(), (l, u)) {
        Ok(()) => Ok(()),
        Err(FlowJsException::WorkerCanceled(c)) => Err(JobError::Canceled(c)),
        Err(FlowJsException::TimedOut(t)) => Err(JobError::TimedOut(t)),
        Err(FlowJsException::DebugThrow { loc }) => Err(JobError::DebugThrow { loc }),
        Err(err) => panic!("Non speculating: {:?}", err),
    }
}

pub fn flow_t_non_speculating<'cx>(
    cx: &mut Context<'cx>,
    (t1, t2): (&Type, &Type),
) -> Result<(), JobError> {
    match flow_t_with_env(cx, &FlowJsEnv::entry(), (t1, t2)) {
        Ok(()) => Ok(()),
        Err(FlowJsException::WorkerCanceled(c)) => Err(JobError::Canceled(c)),
        Err(FlowJsException::TimedOut(t)) => Err(JobError::TimedOut(t)),
        Err(FlowJsException::DebugThrow { loc }) => Err(JobError::DebugThrow { loc }),
        Err(err) => panic!("Non speculating: {:?}", err),
    }
}

pub fn mk_default_non_speculating<'cx>(
    cx: &mut Context<'cx>,
    reason: &Reason,
    d: &flow_typing_default::Default<Type>,
) -> Result<Type, JobError> {
    flow_js_utils::flow_js_result_to_job_error(mk_default(cx, &FlowJsEnv::entry(), reason, d))
}

pub fn get_builtin_type_non_speculating<'cx>(
    cx: &mut Context<'cx>,
    reason: &Reason,
    use_desc: Option<bool>,
    x: &str,
) -> Result<Type, JobError> {
    flow_js_utils::flow_js_result_to_job_error(get_builtin_type_with_env(
        cx,
        &FlowJsEnv::entry(),
        reason,
        use_desc,
        x,
    ))
}

pub fn get_builtin_react_type_non_speculating<'cx>(
    cx: &mut Context<'cx>,
    reason: &Reason,
    use_desc: Option<bool>,
    purpose: ExpectedModulePurpose,
) -> Result<Type, JobError> {
    flow_js_utils::flow_js_result_to_job_error(get_builtin_react_type_with_env(
        cx,
        &FlowJsEnv::entry(),
        reason,
        use_desc,
        purpose,
    ))
}

pub fn unify_non_speculating<'cx>(
    cx: &mut Context<'cx>,
    use_op: Option<UseOp>,
    t1: &Type,
    t2: &Type,
) -> Result<(), JobError> {
    match unify_with_env(cx, &FlowJsEnv::entry(), use_op, t1, t2) {
        Ok(()) => Ok(()),
        Err(FlowJsException::WorkerCanceled(c)) => Err(JobError::Canceled(c)),
        Err(FlowJsException::TimedOut(t)) => Err(JobError::TimedOut(t)),
        Err(FlowJsException::DebugThrow { loc }) => Err(JobError::DebugThrow { loc }),
        Err(err) => panic!("Non speculating: {:?}", err),
    }
}

pub fn reposition_non_speculating<'cx>(
    cx: &mut Context<'cx>,
    loc: ALoc,
    t: Type,
) -> Result<Type, JobError> {
    flow_js_utils::flow_js_result_to_job_error(reposition_with_env(cx, &FlowJsEnv::entry(), loc, t))
}

pub fn mk_typeapp_instance_annot_non_speculating<'cx>(
    cx: &mut Context<'cx>,
    use_op: UseOp,
    reason_op: &Reason,
    reason_tapp: &Reason,
    from_value: bool,
    c: &Type,
    ts: Rc<[Type]>,
) -> Result<Type, JobError> {
    flow_js_utils::flow_js_result_to_job_error(mk_typeapp_instance_annot_with_env(
        cx,
        &FlowJsEnv::entry(),
        use_op,
        reason_op,
        reason_tapp,
        from_value,
        c,
        ts,
    ))
}

pub fn add_output_non_speculating<'cx>(cx: &mut Context<'cx>, msg: ErrorMessage<ALoc>) {
    flow_js_utils::add_output_non_speculating(cx, msg)
}
