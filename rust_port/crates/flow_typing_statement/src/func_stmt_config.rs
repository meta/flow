/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::reason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::subst_name::SubstName;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_env_builder::env_api;
use flow_parser::ast;
use flow_parser::ast::function;
use flow_parser::ast::pattern;
use flow_typing_context::Context;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_js::flow_js;
use flow_typing_loc_env::func_stmt_config_types::Param;
use flow_typing_loc_env::func_stmt_config_types::Pattern;
use flow_typing_loc_env::func_stmt_config_types::Rest;
use flow_typing_loc_env::func_stmt_config_types::ThisParam;
use flow_typing_type::type_;
use flow_typing_type::type_::AssignVarTarget;
use flow_typing_type::type_::FunParam;
use flow_typing_type::type_::FunRestParam;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_util;
use flow_typing_utils::abnormal::CheckExprError;
use flow_typing_utils::type_env;

use crate::destructuring;
use crate::statement;

pub fn param_type(param: &Param) -> FunParam {
    match &param.pattern {
        Pattern::Id(id) => {
            let name = &id.name.name;
            let optional = id.optional;
            let t = if optional || param.default.is_some() {
                type_util::optional(param.t.dupe(), None, false)
            } else {
                param.t.dupe()
            };
            FunParam(Some(name.dupe()), t)
        }
        Pattern::Object { optional, .. } | Pattern::Array { optional, .. } => {
            let t = if *optional || param.default.is_some() {
                type_util::optional(param.t.dupe(), None, false)
            } else {
                param.t.dupe()
            };
            FunParam(None, t)
        }
        Pattern::ParamPropertyPattern(_) => {
            let t = if param.default.is_some() {
                type_util::optional(param.t.dupe(), None, false)
            } else {
                param.t.dupe()
            };
            FunParam(None, t)
        }
    }
}

pub fn rest_type(rest: &Rest) -> FunRestParam {
    let name = &rest.id.name.name;
    FunRestParam(Some(name.dupe()), rest.loc.dupe(), rest.t.dupe())
}

pub fn this_type(this: &ThisParam) -> Type {
    this.t.dupe()
}

pub fn is_param_type_annotated(param: &Param) -> bool {
    param.has_anno
}

pub fn is_rest_type_annotated(rest: &Rest) -> bool {
    rest.has_anno
}

pub fn subst_param<'a>(
    cx: &mut Context<'a>,
    map: &FlowOrdMap<SubstName, Type>,
    param: &Param,
) -> Param {
    let t = flow_js::subst(cx, None, None, None, map, param.t.dupe());
    Param {
        t,
        loc: param.loc.dupe(),
        ploc: param.ploc.dupe(),
        pattern: param.pattern.clone(),
        default: param.default.dupe(),
        has_anno: param.has_anno,
    }
}

pub fn subst_rest<'a>(
    cx: &mut Context<'a>,
    map: &FlowOrdMap<SubstName, Type>,
    rest: &Rest,
) -> Rest {
    let t = flow_js::subst(cx, None, None, None, map, rest.t.dupe());
    Rest {
        t,
        loc: rest.loc.dupe(),
        ploc: rest.ploc.dupe(),
        id: rest.id.clone(),
        has_anno: rest.has_anno,
    }
}

pub fn subst_this<'a>(
    cx: &mut Context<'a>,
    map: &FlowOrdMap<SubstName, Type>,
    this: &ThisParam,
) -> ThisParam {
    let t = flow_js::subst(cx, None, None, None, map, this.t.dupe());
    ThisParam {
        t,
        loc: this.loc.dupe(),
        annot: this.annot.clone(),
    }
}

fn destruct<'a>(
    cx: &mut Context<'a>,
    use_op: &UseOp,
    name_loc: ALoc,
    name: &str,
    default: Option<&flow_typing_default::Default<Type>>,
    t: Type,
) -> Result<Type, flow_utils_concurrency::job_error::JobError> {
    if let Some(d) = default {
        let reason = reason::mk_reason(VirtualReasonDesc::RIdentifier(name.into()), name_loc);
        let default_t = flow_js::mk_default_non_speculating(cx, &reason, d)?;
        flow_js::flow_non_speculating(
            cx,
            (
                &default_t,
                &type_::UseT::new(type_::UseTInner::UseT(use_op.dupe(), t.dupe())),
            ),
        )?;
    }
    Ok(t)
}

fn flow_default<'a>(
    cx: &mut Context<'a>,
    target: AssignVarTarget<ALoc>,
    annot_t: &Type,
    default_t: &Type,
) -> Result<(), flow_utils_concurrency::job_error::JobError> {
    let use_op = UseOp::Op(std::sync::Arc::new(type_::RootUseOp::AssignVar {
        var: Some(Box::new(target)),
        init: type_::AssignVarInit::Default(Box::new(type_::AssignVarDefaultData {
            loc: type_util::loc_of_t(default_t).dupe(),
            default: flow_js_utils::type_or_type_desc_for_error(default_t),
        })),
    }));
    flow_js::flow_non_speculating(
        cx,
        (
            default_t,
            &type_::UseT::new(type_::UseTInner::UseT(use_op, annot_t.dupe())),
        ),
    )?;
    Ok(())
}

fn eval_default<'a>(
    cx: &mut Context<'a>,
    always_flow_default: bool,
    target: AssignVarTarget<ALoc>,
    annot_t: &Type,
    has_anno: bool,
    default: Option<ast::expression::Expression<ALoc, ALoc>>,
) -> Result<Option<ast::expression::Expression<ALoc, (ALoc, Type)>>, CheckExprError> {
    default
        .map(|e| {
            let (e, default_t) = statement::default_value(cx, &e)?;
            let loc = e.loc().0.dupe();
            if has_anno {
                flow_default(cx, target, annot_t, &default_t)?;
                let mut inner = (*e.0).clone();
                *inner.loc_mut() = (loc, annot_t.dupe());
                Ok(ast::expression::Expression(inner.into()))
            } else {
                if always_flow_default {
                    flow_default(cx, target, annot_t, &default_t)?;
                }
                Ok(e)
            }
        })
        .transpose()
}

pub fn eval_param<'a>(
    cx: &mut Context<'a>,
    param: &Param,
) -> Result<flow_typing_loc_env::func_stmt_config_types::ParamAst<(ALoc, Type)>, CheckExprError> {
    let Param {
        t,
        loc,
        ploc,
        pattern,
        default,
        has_anno,
    } = param;
    match pattern {
        Pattern::Id(id) => {
            let name_loc = id.name.loc.0.dupe();
            let name = &id.name.name;
            let reason = reason::mk_reason(VirtualReasonDesc::RIdentifier(name.dupe()), name_loc);
            let t = type_env::find_write(cx, env_api::DefLocType::OrdinaryNameLoc, reason);
            let target = AssignVarTarget::Variable(type_util::reason_of_t(&t).dupe());
            let default = eval_default(cx, true, target, &t, *has_anno, default.dupe())?;
            Ok(function::Param::RegularParam {
                loc: loc.dupe(),
                argument: pattern::Pattern::Identifier {
                    loc: (ploc.dupe(), t),
                    inner: id.clone().into(),
                },
                default,
            })
        }
        Pattern::Object {
            annot,
            properties,
            optional,
            comments,
        } => {
            let target = AssignVarTarget::Annotation(flow_js_utils::type_or_type_desc_for_error(t));
            let default = eval_default(cx, false, target, t, *has_anno, default.dupe())?;
            let mut init = destructuring::empty(None, None);
            let properties = destructuring::object_properties(
                cx,
                &|cx, use_op, name_loc, name, default, t| {
                    destruct(cx, use_op, name_loc, name, default, t)
                },
                ploc.dupe(),
                &mut init,
                properties,
            )?;
            Ok(function::Param::RegularParam {
                loc: loc.dupe(),
                argument: pattern::Pattern::Object {
                    loc: (ploc.dupe(), t.dupe()),
                    inner: pattern::Object {
                        properties: properties.into(),
                        annot: annot.clone(),
                        optional: *optional,
                        comments: comments.dupe(),
                    }
                    .into(),
                },
                default,
            })
        }
        Pattern::Array {
            annot,
            elements,
            optional,
            comments,
        } => {
            let target = AssignVarTarget::Annotation(flow_js_utils::type_or_type_desc_for_error(t));
            let default = eval_default(cx, false, target, t, *has_anno, default.dupe())?;
            let mut init = destructuring::empty(None, None);
            let elements = destructuring::array_elements(
                cx,
                &|cx, use_op, name_loc, name, default, t| {
                    destruct(cx, use_op, name_loc, name, default, t)
                },
                &mut init,
                elements,
            )?;
            Ok(function::Param::RegularParam {
                loc: loc.dupe(),
                argument: pattern::Pattern::Array {
                    loc: (ploc.dupe(), t.dupe()),
                    inner: pattern::Array {
                        elements: elements.into(),
                        annot: annot.clone(),
                        optional: *optional,
                        comments: comments.dupe(),
                    }
                    .into(),
                },
                default,
            })
        }
        Pattern::ParamPropertyPattern(prop) => Ok(function::Param::ParamProperty {
            loc: loc.dupe(),
            property: prop.clone(),
        }),
    }
}

pub fn eval_rest<'a>(
    cx: &mut Context<'a>,
    rest: &Rest,
) -> flow_typing_loc_env::func_stmt_config_types::RestAst<(ALoc, Type)> {
    let Rest {
        t: _,
        loc,
        ploc,
        id,
        has_anno: _,
    } = rest;
    let name_loc = id.name.loc.0.dupe();
    let name = &id.name.name;
    let reason = reason::mk_reason(VirtualReasonDesc::RIdentifier(name.dupe()), name_loc);
    let t = type_env::find_write(cx, env_api::DefLocType::OrdinaryNameLoc, reason);
    function::RestParam {
        loc: loc.dupe(),
        argument: pattern::Pattern::Identifier {
            loc: (ploc.dupe(), t),
            inner: id.clone().into(),
        },
        comments: None,
    }
}

pub fn eval_this<'a>(
    _cx: &mut Context<'a>,
    this: &ThisParam,
) -> flow_typing_loc_env::func_stmt_config_types::ThisAst<(ALoc, Type)> {
    ast::function::ThisParam {
        loc: this.loc.dupe(),
        annot: this.annot.clone(),
        comments: None,
    }
}
