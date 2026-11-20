/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::reason::VirtualReasonDesc;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::statement::component_params::ParamName;
use flow_typing_context::Context;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_js::flow_js;
use flow_typing_loc_env::component_sig_types::declaration_param_config::Param;
use flow_typing_loc_env::component_sig_types::declaration_param_config::ParamAst;
use flow_typing_loc_env::component_sig_types::declaration_param_config::Pattern;
use flow_typing_loc_env::component_sig_types::declaration_param_config::Rest;
use flow_typing_loc_env::component_sig_types::declaration_param_config::RestAst;
use flow_typing_type::type_::AssignVarDefaultData;
use flow_typing_type::type_::AssignVarInit;
use flow_typing_type::type_::AssignVarTarget;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_util;
use flow_typing_utils::abnormal::CheckExprError;
use flow_typing_utils::type_env;

use crate::destructuring;
use crate::statement;

pub fn param_type_with_name(param: &Param) -> (ALoc, FlowSmolStr, Type, bool) {
    let optional = match &param.pattern {
        Pattern::Id(id) => id.optional,
        Pattern::Object { optional, .. } | Pattern::Array { optional, .. } => *optional,
    } || param.default.is_some();
    let t = param.t.dupe();
    match &param.name {
        ParamName::Identifier(id) => (id.loc.dupe(), id.name.dupe(), t, optional),
        ParamName::StringLiteral((loc, lit)) => (loc.dupe(), lit.value.dupe(), t, optional),
    }
}

pub fn rest_type(rest: &Rest) -> Type {
    let optional = match &rest.pattern {
        Pattern::Id(id) => id.optional,
        Pattern::Object { optional, .. } | Pattern::Array { optional, .. } => *optional,
    };
    type_util::slot_value_t(&rest.t, optional)
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
        let reason = flow_common::reason::mk_reason(
            VirtualReasonDesc::RIdentifier(FlowSmolStr::new(name)),
            name_loc,
        );
        let default_t = flow_js::mk_default_non_speculating(cx, &reason, d)?;
        flow_js::flow_non_speculating(
            cx,
            (
                &default_t,
                &flow_typing_type::type_::UseT::new(flow_typing_type::type_::UseTInner::UseT(
                    use_op.dupe(),
                    t.dupe(),
                )),
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
    let use_op = UseOp::Op(std::sync::Arc::new(
        flow_typing_type::type_::RootUseOp::AssignVar {
            var: Some(Box::new(target)),
            init: AssignVarInit::Default(Box::new(AssignVarDefaultData {
                loc: type_util::loc_of_t(default_t).dupe(),
                default: flow_js_utils::type_or_type_desc_for_error(default_t),
            })),
        },
    ));
    flow_js::flow_non_speculating(
        cx,
        (
            default_t,
            &flow_typing_type::type_::UseT::new(flow_typing_type::type_::UseTInner::UseT(
                use_op,
                annot_t.dupe(),
            )),
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
            let typed_e = statement::expression(None, None, None, cx, &e)?;
            let (loc, default_t) = typed_e.loc().dupe();
            if has_anno {
                flow_default(cx, target, annot_t, &default_t)?;
                let mut inner = (*typed_e.0).clone();
                *inner.loc_mut() = (loc, annot_t.dupe());
                Ok(ast::expression::Expression(inner.into()))
            } else {
                if always_flow_default {
                    flow_default(cx, target, annot_t, &default_t)?;
                }
                Ok(typed_e)
            }
        })
        .transpose()
}

pub fn eval_param<'a>(
    cx: &mut Context<'a>,
    param: &Param,
) -> Result<ParamAst<(ALoc, Type)>, CheckExprError> {
    let Param {
        ref t,
        ref loc,
        ref ploc,
        ref pattern,
        ref default,
        has_anno,
        shorthand,
        ref name,
    } = *param;
    let reconstruct_prop_name =
        |t: Type| -> ast::statement::component_params::ParamName<ALoc, (ALoc, Type)> {
            use ast::statement::component_params::ParamName;
            match name {
                ParamName::Identifier(id) => {
                    ParamName::Identifier(ast::Identifier::new(ast::IdentifierInner {
                        loc: (id.loc.dupe(), t),
                        name: id.name.dupe(),
                        comments: id.comments.dupe(),
                    }))
                }
                ParamName::StringLiteral(lit) => ParamName::StringLiteral(lit.clone()),
            }
        };
    match pattern {
        Pattern::Id(id) => {
            let id_name = &id.name.name;
            let name_loc = id.name.loc.0.dupe();
            let reason = flow_common::reason::mk_reason(
                VirtualReasonDesc::RIdentifier(id_name.dupe()),
                name_loc.dupe(),
            );
            let id_t = type_env::find_write(
                cx,
                flow_env_builder::env_api::DefLocType::OrdinaryNameLoc,
                reason,
            );
            let target = AssignVarTarget::Variable(type_util::reason_of_t(&id_t).dupe());
            let default = eval_default(cx, true, target, &id_t, has_anno, default.clone())?;
            Ok(ast::statement::component_params::Param {
                loc: (loc.dupe(), id_t.dupe()),
                local: ast::pattern::Pattern::Identifier {
                    loc: (ploc.dupe(), id_t.dupe()),
                    inner: id.clone().into(),
                },
                default,
                shorthand,
                name: reconstruct_prop_name(id_t),
            })
        }
        Pattern::Object {
            annot,
            properties,
            optional,
            comments,
        } => {
            let target = AssignVarTarget::Annotation(flow_js_utils::type_or_type_desc_for_error(t));
            let default = eval_default(cx, false, target, t, has_anno, default.clone())?;
            let mut init = destructuring::empty(None, None);
            let typed_properties = destructuring::object_properties(
                cx,
                &|cx, use_op, name_loc, name, default, t| {
                    destruct(cx, use_op, name_loc, name, default, t)
                },
                ploc.dupe(),
                &mut init,
                properties,
            )?;
            let t = t.dupe();
            Ok(ast::statement::component_params::Param {
                loc: (loc.dupe(), t.dupe()),
                local: ast::pattern::Pattern::Object {
                    loc: (ploc.dupe(), t.dupe()),
                    inner: ast::pattern::Object {
                        properties: typed_properties.into(),
                        annot: annot.clone(),
                        optional: *optional,
                        comments: comments.clone(),
                    }
                    .into(),
                },
                default,
                shorthand,
                name: reconstruct_prop_name(t),
            })
        }
        Pattern::Array {
            annot,
            elements,
            optional,
            comments,
        } => {
            let target = AssignVarTarget::Annotation(flow_js_utils::type_or_type_desc_for_error(t));
            let default = eval_default(cx, false, target, t, has_anno, default.clone())?;
            let mut init = destructuring::empty(None, None);
            let typed_elements = destructuring::array_elements(
                cx,
                &|cx, use_op, name_loc, name, default, t| {
                    destruct(cx, use_op, name_loc, name, default, t)
                },
                &mut init,
                elements,
            )?;
            let t = t.dupe();
            Ok(ast::statement::component_params::Param {
                loc: (loc.dupe(), t.dupe()),
                local: ast::pattern::Pattern::Array {
                    loc: (ploc.dupe(), t.dupe()),
                    inner: ast::pattern::Array {
                        elements: typed_elements.into(),
                        annot: annot.clone(),
                        optional: *optional,
                        comments: comments.clone(),
                    }
                    .into(),
                },
                default,
                shorthand,
                name: reconstruct_prop_name(t),
            })
        }
    }
}

pub fn eval_rest<'a>(
    cx: &mut Context<'a>,
    rest: &Rest,
) -> Result<RestAst<(ALoc, Type)>, CheckExprError> {
    let Rest {
        ref t,
        ref loc,
        ref ploc,
        ref pattern,
        has_anno: _,
        ref comments,
    } = *rest;
    match pattern {
        Pattern::Id(id) => Ok(ast::statement::component_params::RestParam {
            loc: (loc.dupe(), t.dupe()),
            argument: ast::pattern::Pattern::Identifier {
                loc: (ploc.dupe(), t.dupe()),
                inner: id.clone().into(),
            },
            comments: comments.clone(),
        }),
        Pattern::Object {
            annot,
            properties,
            optional,
            comments: obj_comments,
        } => {
            let mut init = destructuring::empty(None, None);
            let typed_properties = destructuring::object_properties(
                cx,
                &|cx, use_op, name_loc, name, default, dest_t| {
                    destruct(cx, use_op, name_loc, name, default, dest_t)
                },
                ploc.dupe(),
                &mut init,
                properties,
            )?;
            Ok(ast::statement::component_params::RestParam {
                loc: (loc.dupe(), t.dupe()),
                argument: ast::pattern::Pattern::Object {
                    loc: (ploc.dupe(), t.dupe()),
                    inner: ast::pattern::Object {
                        properties: typed_properties.into(),
                        annot: annot.clone(),
                        optional: *optional,
                        comments: obj_comments.clone(),
                    }
                    .into(),
                },
                comments: comments.clone(),
            })
        }
        Pattern::Array {
            annot,
            elements,
            optional,
            comments: arr_comments,
        } => {
            let mut init = destructuring::empty(None, None);
            let typed_elements = destructuring::array_elements(
                cx,
                &|cx, use_op, name_loc, name, default, dest_t| {
                    destruct(cx, use_op, name_loc, name, default, dest_t)
                },
                &mut init,
                elements,
            )?;
            Ok(ast::statement::component_params::RestParam {
                loc: (loc.dupe(), t.dupe()),
                argument: ast::pattern::Pattern::Array {
                    loc: (ploc.dupe(), t.dupe()),
                    inner: ast::pattern::Array {
                        elements: typed_elements.into(),
                        annot: annot.clone(),
                        optional: *optional,
                        comments: arr_comments.clone(),
                    }
                    .into(),
                },
                comments: comments.clone(),
            })
        }
    }
}
