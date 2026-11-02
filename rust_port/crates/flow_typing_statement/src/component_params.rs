/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::polarity::Polarity;
use flow_common::subst_name::SubstName;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_typing_context::Context;
use flow_typing_loc_env::component_sig_types::declaration_param_config;
use flow_typing_loc_env::component_sig_types::param_types;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_utils::abnormal::CheckExprError;
use flow_typing_visitors::type_visitor;
use flow_typing_visitors::type_visitor::TypeVisitor;

use crate::component_declaration_config;
use crate::component_params_intf::*;

pub struct DeclarationConfig;

impl Config for DeclarationConfig {
    type Param = declaration_param_config::Param;
    type Rest = declaration_param_config::Rest;
    type ParamAst = declaration_param_config::ParamAst<(ALoc, Type)>;
    type RestAst = declaration_param_config::RestAst<(ALoc, Type)>;

    fn eval_param<'a>(
        cx: &mut Context<'a>,
        param: &Self::Param,
    ) -> Result<Self::ParamAst, CheckExprError> {
        component_declaration_config::eval_param(cx, param)
    }

    fn eval_rest<'a>(
        cx: &mut Context<'a>,
        rest: &Self::Rest,
    ) -> Result<Self::RestAst, CheckExprError> {
        component_declaration_config::eval_rest(cx, rest)
    }

    fn param_type_with_name(param: &Self::Param) -> (ALoc, FlowSmolStr, Type) {
        component_declaration_config::param_type_with_name(param)
    }

    fn rest_type(rest: &Self::Rest) -> Type {
        component_declaration_config::rest_type(rest)
    }
}

pub struct Found(pub FlowSmolStr);

pub struct TparamFinder;

impl TypeVisitor<Result<FlowOrdSet<SubstName>, Found>> for TparamFinder {
    fn type_<'a>(
        &mut self,
        cx: &mut Context<'a>,
        pole: Polarity,
        acc: Result<FlowOrdSet<SubstName>, Found>,
        t: &Type,
    ) -> Result<FlowOrdSet<SubstName>, Found> {
        let tparam_names = acc?;
        match t.deref() {
            TypeInner::GenericT(box GenericTData { name: s, .. }) => {
                if tparam_names.contains(s) {
                    Err(Found(s.string_of_subst_name().dupe()))
                } else {
                    type_visitor::type_default(self, cx, pole, Ok(tparam_names), t)
                }
            }
            TypeInner::DefT(_, def_t)
                if let DefTInner::PolyT(box PolyTData { tparams, .. }) = def_t.deref() =>
            {
                let mut tparam_names_prime = tparam_names.dupe();
                for tp in tparams.iter() {
                    tparam_names_prime.remove(&tp.name);
                }
                type_visitor::type_default(self, cx, pole, Ok(tparam_names_prime), t)?;
                Ok(tparam_names)
            }
            _ => type_visitor::type_default(self, cx, pole, Ok(tparam_names), t),
        }
    }
}

pub fn empty(reconstruct: param_types::Reconstruct) -> param_types::Params {
    param_types::Params {
        params: Vec::new(),
        rest: None,
        reconstruct,
    }
}

pub fn add_param(p: declaration_param_config::Param, x: &mut param_types::Params) {
    x.params.push(p);
}

pub fn add_rest(r: declaration_param_config::Rest, x: &mut param_types::Params) {
    x.rest = Some(r);
}

pub fn config<'a, C: Config>(
    cx: &mut Context<'a>,
    in_annotation: bool,
    config_reason: &flow_common::reason::Reason,
    params: &[C::Param],
    rest: Option<&C::Rest>,
) -> Result<Type, flow_utils_concurrency::job_error::JobError> {
    use flow_common::polarity::Polarity;
    use flow_typing_type::type_::*;

    let mut ref_prop: Option<(ALoc, Type)> = None;
    let pmap: properties::PropertiesMap = params
        .iter()
        .map(|p| {
            let (key_loc, key, t) = C::param_type_with_name(p);
            if key.as_str() == "ref" {
                ref_prop = Some((key_loc.dupe(), t.dupe()));
            }
            (
                flow_common::reason::Name::new(key),
                Property::new(PropertyInner::Field(Box::new(FieldData {
                    preferred_def_locs: None,
                    key_loc: Some(key_loc),
                    type_: t,
                    polarity: Polarity::Positive,
                }))),
            )
        })
        .collect();
    let rest_t = match rest {
        None => flow_typing_flow_common::obj_type::mk_with_proto(
            cx,
            config_reason.dupe(),
            ObjKind::Exact,
            None,
            None,
            None,
            None,
            Type::new(TypeInner::ObjProtoT(config_reason.dupe())),
            ObjLiteral::NonLiteral,
        ),
        Some(rest) => {
            let t = C::rest_type(rest);
            let flags = Flags {
                obj_kind: ObjKind::Inexact,
                react_dro: Some(ReactDro(config_reason.loc().dupe(), DroType::Props)),
            };
            let call: Option<i32> = None;
            let empty_pmap = cx.generate_property_map(properties::PropertiesMap::new());
            let rest_param_reason = flow_typing_type::type_util::reason_of_t(&t);
            let use_op = UseOp::Op(std::sync::Arc::new(
                RootUseOp::ComponentRestParamCompatibility(Box::new(
                    ComponentRestParamCompatibilityData {
                        rest_param_loc: rest_param_reason.loc().dupe(),
                        rest_param: type_or_type_desc::TypeOrTypeDescT::Type(t.dupe()),
                    },
                )),
            ));
            // The reason doesn't matter because in our special use_op handling we do not
            // reference the object reason
            let inexact_empty_obj = Type::new(mk_object_def_type(
                config_reason.dupe(),
                Some(flags),
                call,
                empty_pmap,
                Type::new(TypeInner::ObjProtoT(rest_param_reason.dupe())),
            ));
            // The rest param must be an object type.
            cx.add_post_inference_subtyping_check(t.dupe(), use_op, inexact_empty_obj);
            t
        }
    };
    if let Some((key_loc, ref_prop_t)) = &ref_prop {
        if !in_annotation {
            let reason_op = flow_common::reason::mk_reason(None, key_loc.dupe());
            let u = match flow_typing_flow_js::flow_js::FlowJs::get_builtin_react_typeapp(
                cx,
                &reason_op,
                None,
                flow_typing_errors::intermediate_error_types::ExpectedModulePurpose::ReactModuleForReactRefSetterType,
                vec![any_t::error(reason_op.dupe())],
            ) {
                Ok(v) => v,
                Err(flow_typing_flow_common::flow_js_utils::FlowJsException::WorkerCanceled(c)) => {
                    return Err(flow_utils_concurrency::job_error::JobError::Canceled(c));
                }
                Err(flow_typing_flow_common::flow_js_utils::FlowJsException::TimedOut(t)) => {
                    return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t));
                }
                Err(err) => panic!("Should not be under speculation: {:?}", err),
            };
            cx.add_post_inference_subtyping_check(
                ref_prop_t.dupe(),
                UseOp::Op(std::sync::Arc::new(RootUseOp::DeclareComponentRef {
                    op: reason_op,
                })),
                u,
            );
        }
    }
    let destructor = Destructor::ReactCheckComponentConfig { props: pmap };
    match flow_typing_flow_js::flow_js::FlowJs::mk_possibly_evaluated_destructor_for_annotations(
        cx,
        unknown_use(),
        config_reason,
        &rest_t,
        &destructor,
        eval::Id::generate_id(),
    ) {
        Ok(v) => Ok(v),
        Err(flow_typing_flow_common::flow_js_utils::FlowJsException::WorkerCanceled(c)) => {
            Err(flow_utils_concurrency::job_error::JobError::Canceled(c))
        }
        Err(flow_typing_flow_common::flow_js_utils::FlowJsException::TimedOut(t)) => {
            Err(flow_utils_concurrency::job_error::JobError::TimedOut(t))
        }
        Err(err) => panic!("Should not be under speculation: {:?}", err),
    }
}

pub fn eval<'a, C: Config, R>(
    cx: &mut Context<'a>,
    params: &[C::Param],
    rest: Option<&C::Rest>,
    reconstruct: impl Fn(Vec<C::ParamAst>, Option<C::RestAst>) -> R,
) -> Result<R, CheckExprError> {
    let param_tasts: Vec<_> = params
        .iter()
        .map(|p| C::eval_param(cx, p))
        .collect::<Result<_, _>>()?;
    let rest_tast = rest.map(|r| C::eval_rest(cx, r)).transpose()?;
    Ok(reconstruct(param_tasts, rest_tast))
}
