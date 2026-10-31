/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Destructuring visitor for tree-shaped patterns, parameteric over an action f
//! to perform at the leaves. A type for the pattern is passed, which is taken
//! apart as the visitor goes deeper.

use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::reason::mk_reason;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_env_builder::env_api::DefLocType;
use flow_parser::ast;
use flow_parser::ast::expression;
use flow_parser::ast::pattern;
use flow_parser::ast_utils;
use flow_parser::loc_sig::LocSig;
use flow_parser::polymorphic_ast_mapper;
use flow_parser::polymorphic_ast_mapper::LocMapper;
use flow_typing_context::Context;
use flow_typing_default as default;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::intermediate_error_types::UnsupportedSyntax;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_js::flow_js;
use flow_typing_type::type_;
use flow_typing_type::type_::AssignVarDefaultData;
use flow_typing_type::type_::AssignVarInit;
use flow_typing_type::type_::AssignVarTarget;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::NumberLiteral;
use flow_typing_type::type_::RootUseOp;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::any_t;
use flow_typing_type::type_util;
use flow_typing_utils::abnormal::CheckExprError;
use flow_typing_utils::type_env;
use flow_typing_utils::typed_ast_utils;
use flow_typing_utils::typed_ast_utils::ErrorMapper;

use crate::statement;
pub struct State {
    has_parent: bool,
    init: Option<expression::Expression<ALoc, ALoc>>,
    default: Option<default::Default<Type>>,
}

pub type Callback<'a> = dyn Fn(
        &mut Context<'_>,
        /*use_op:*/ &UseOp,
        /*name_loc:*/ ALoc,
        /*name:*/ &FlowSmolStr,
        /*default:*/ Option<&default::Default<Type>>,
        /*current:*/ Type,
    ) -> Result<Type, flow_utils_concurrency::job_error::JobError>
    + 'a;

pub fn empty(
    init: Option<expression::Expression<ALoc, ALoc>>,
    default: Option<default::Default<Type>>,
) -> State {
    State {
        has_parent: false,
        init,
        default,
    }
}

fn pattern_default<'a>(
    cx: &mut Context<'a>,
    acc: &mut State,
    default_expr: Option<&expression::Expression<ALoc, ALoc>>,
) -> Result<Option<expression::Expression<ALoc, (ALoc, Type)>>, CheckExprError> {
    match default_expr {
        None => Ok(None),
        Some(e) => {
            let prev_default = acc.default.take();
            let e = statement::expression(None, None, None, cx, e)?;
            let t = e.loc().1.dupe();
            let new_default = Some(default::expr(t, prev_default));
            acc.default = new_default;
            Ok(Some(e))
        }
    }
}

fn array_element<'a>(
    cx: &mut Context<'a>,
    acc: &State,
    i: i32,
    loc: ALoc,
) -> Result<State, flow_utils_concurrency::job_error::JobError> {
    let init = &acc.init;
    let default_val = &acc.default;
    let key = Type::new(TypeInner::DefT(
        mk_reason(VirtualReasonDesc::RNumber, loc.dupe()),
        DefT::new(DefTInner::SingletonNumT {
            from_annot: false,
            value: NumberLiteral(i as f64, FlowSmolStr::new(i.to_string())),
        }),
    ));
    let reason = mk_reason(None, loc.dupe());
    let new_init = init.as_ref().map(|init| {
        expression::Expression::new(expression::ExpressionInner::Member {
            loc: loc.dupe(),
            inner: Arc::new(expression::Member {
                object: init.dupe(),
                property: expression::member::Property::PropertyExpression(
                    expression::Expression::new(expression::ExpressionInner::NumberLiteral {
                        loc: loc.dupe(),
                        inner: Arc::new(ast::NumberLiteral {
                            value: i as f64,
                            raw: FlowSmolStr::new(i.to_string()),
                            comments: None,
                        }),
                    }),
                ),
                comments: None,
            }),
        })
    });
    let refinement = match new_init.as_ref() {
        Some(init) => crate::refinement::get(true, cx, init, loc.dupe())?,
        None => None,
    };
    let has_parent = refinement.is_none();
    let new_default = default_val
        .as_ref()
        .map(|d| default::elem(key, reason, d.clone()));
    Ok(State {
        has_parent,
        init: new_init,
        default: new_default,
    })
}

fn array_rest_element(acc: &State, i: i32, loc: ALoc) -> State {
    let default_val = &acc.default;
    let reason = mk_reason(VirtualReasonDesc::RArrayPatternRestProp, loc.dupe());
    let new_default = default_val
        .as_ref()
        .map(|d| default::arr_rest(i, reason, d.clone()));
    State {
        has_parent: true,
        init: acc.init.dupe(),
        default: new_default,
    }
}

fn object_named_property<'a>(
    has_default: bool,
    _parent_loc: ALoc,
    cx: &mut Context<'a>,
    acc: &State,
    loc: ALoc,
    x: &FlowSmolStr,
    comments: Option<ast::Syntax<ALoc, ()>>,
) -> Result<State, flow_utils_concurrency::job_error::JobError> {
    let init = &acc.init;
    let default_val = &acc.default;
    let reason = mk_reason(
        VirtualReasonDesc::RProperty(Some(Name::new(FlowSmolStr::new(x)))),
        loc.dupe(),
    );
    let new_init = init.as_ref().map(|init| {
        expression::Expression::new(expression::ExpressionInner::Member {
            loc: loc.dupe(),
            inner: Arc::new(expression::Member {
                object: init.dupe(),
                property: expression::member::Property::PropertyIdentifier(ast::Identifier::new(
                    ast::IdentifierInner {
                        loc: loc.dupe(),
                        name: x.dupe(),
                        comments,
                    },
                )),
                comments: None,
            }),
        })
    });
    let refinement = match new_init.as_ref() {
        Some(init) => crate::refinement::get(true, cx, init, loc.dupe())?,
        None => None,
    };
    let new_default = default_val.as_ref().map(|d| {
        let prop_d = default::prop(x.dupe(), reason.dupe(), has_default, d.clone());
        if has_default {
            default::default(reason.dupe(), prop_d)
        } else {
            prop_d
        }
    });
    let parent_loc_opt = match refinement {
        Some(_) => None,
        None => Some(_parent_loc),
    };
    let has_parent = parent_loc_opt.is_some();
    Ok(State {
        has_parent,
        init: new_init,
        default: new_default,
    })
}

fn object_computed_property<'a>(
    cx: &mut Context<'a>,
    acc: &State,
    e: &expression::Expression<ALoc, ALoc>,
) -> Result<(State, expression::Expression<ALoc, (ALoc, Type)>), CheckExprError> {
    let init = &acc.init;
    let default_val = &acc.default;
    let e_typed = statement::expression(None, None, None, cx, e)?;
    let (loc, t) = e_typed.loc().dupe();
    let reason = mk_reason(VirtualReasonDesc::RProperty(None), loc.dupe());
    let new_init = init.as_ref().map(|init| {
        expression::Expression::new(expression::ExpressionInner::Member {
            loc: loc.dupe(),
            inner: Arc::new(expression::Member {
                object: init.dupe(),
                property: expression::member::Property::PropertyExpression(e.dupe()),
                comments: None,
            }),
        })
    });
    let new_default = default_val
        .as_ref()
        .map(|d| default::elem(t, reason, d.clone()));
    Ok((
        State {
            has_parent: true,
            init: new_init,
            default: new_default,
        },
        e_typed,
    ))
}

fn object_rest_property(acc: &State, xs: &[FlowSmolStr], loc: ALoc) -> State {
    let default_val = &acc.default;
    let reason = mk_reason(VirtualReasonDesc::RObjectPatternRestProp, loc.dupe());
    let new_default = default_val
        .as_ref()
        .map(|d| default::obj_rest(xs, reason, d.clone()));
    State {
        has_parent: true,
        init: acc.init.dupe(),
        default: new_default,
    }
}

fn object_property<'a>(
    cx: &mut Context<'a>,
    has_default: bool,
    parent_loc: ALoc,
    current: Type,
    acc: &State,
    xs: &mut Vec<FlowSmolStr>,
    key: &pattern::object::Key<ALoc, ALoc>,
) -> Result<(State, pattern::object::Key<ALoc, (ALoc, Type)>), CheckExprError> {
    match key {
        pattern::object::Key::Identifier(id) => {
            let loc = id.loc.dupe();
            let x = id.name.dupe();
            let comments = id.comments.dupe();
            let new_acc = object_named_property(
                has_default,
                parent_loc,
                cx,
                acc,
                loc.dupe(),
                &x,
                comments.dupe(),
            )?;
            xs.push(x.dupe());
            Ok((
                new_acc,
                pattern::object::Key::Identifier(ast::Identifier::new(ast::IdentifierInner {
                    loc: (loc, current),
                    name: x,
                    comments,
                })),
            ))
        }
        pattern::object::Key::StringLiteral((loc, lit)) => {
            let x = &lit.value;
            let new_acc =
                object_named_property(has_default, parent_loc, cx, acc, loc.dupe(), x, None)?;
            xs.push(x.dupe());
            Ok((
                new_acc,
                pattern::object::Key::StringLiteral((loc.dupe(), lit.clone())),
            ))
        }
        pattern::object::Key::Computed(ck) => {
            let (new_acc, e) = object_computed_property(cx, acc, &ck.expression)?;
            Ok((
                new_acc,
                pattern::object::Key::Computed(ast::ComputedKey {
                    loc: ck.loc.dupe(),
                    expression: e,
                    comments: ck.comments.dupe(),
                }),
            ))
        }
        pattern::object::Key::NumberLiteral((loc, lit))
            if flow_common::js_number::is_float_safe_integer(lit.value) =>
        {
            let name = flow_common::js_number::ecma_string_of_float(lit.value);
            let name_smol = FlowSmolStr::new(&name);
            let new_acc = object_named_property(
                has_default,
                parent_loc,
                cx,
                acc,
                loc.dupe(),
                &name_smol,
                lit.comments.dupe(),
            )?;
            xs.push(name_smol);
            Ok((
                new_acc,
                pattern::object::Key::NumberLiteral((loc.dupe(), lit.clone())),
            ))
        }
        pattern::object::Key::NumberLiteral((loc, _))
        | pattern::object::Key::BigIntLiteral((loc, _)) => {
            flow_js::add_output_non_speculating(
                cx,
                ErrorMessage::EUnsupportedSyntax(Box::new((
                    loc.dupe(),
                    UnsupportedSyntax::DestructuringObjectPropertyInvalidLiteral,
                ))),
            );
            let mut mapper = ErrorMapper;
            let typed_key = match key {
                pattern::object::Key::NumberLiteral((loc, lit)) => {
                    pattern::object::Key::NumberLiteral((
                        {
                            let Ok(v) = mapper.on_loc_annot(loc);
                            v
                        },
                        lit.clone(),
                    ))
                }
                pattern::object::Key::BigIntLiteral((loc, lit)) => {
                    pattern::object::Key::BigIntLiteral((
                        {
                            let Ok(v) = mapper.on_loc_annot(loc);
                            v
                        },
                        lit.clone(),
                    ))
                }
                _ => unreachable!(),
            };
            Ok((
                State {
                    has_parent: acc.has_parent,
                    init: acc.init.dupe(),
                    default: acc.default.clone(),
                },
                typed_key,
            ))
        }
    }
}

fn identifier<'a>(
    cx: &mut Context<'a>,
    f: &Callback<'_>,
    acc: &State,
    name_loc: ALoc,
    name: &FlowSmolStr,
) -> Result<Type, flow_utils_concurrency::job_error::JobError> {
    let default_val = &acc.default;
    let reason = mk_reason(VirtualReasonDesc::RIdentifier(name.dupe()), name_loc.dupe());
    let write_t = type_env::find_write(cx, DefLocType::OrdinaryNameLoc, reason.dupe());
    let current = type_util::mod_reason_of_t(
        &|r: Reason| {
            let desc = r.desc(false);
            let new_desc = match desc {
                Some(
                    VirtualReasonDesc::RArrayPatternRestProp
                    | VirtualReasonDesc::RObjectPatternRestProp,
                ) => Some(VirtualReasonDesc::RIdentifier(name.dupe())),
                _ => desc.cloned(),
            };
            r.replace_desc(new_desc)
        },
        &write_t,
    );
    let init = match (default_val, &acc.init) {
        (Some(default::Default::Expr(t)), _) => {
            AssignVarInit::Default(Box::new(AssignVarDefaultData {
                loc: type_util::loc_of_t(t).dupe(),
                default: flow_js_utils::type_or_type_desc_for_error(t),
            }))
        }
        (_, Some(init)) => AssignVarInit::Reason(flow_common::reason::mk_expression_reason(init)),
        _ => AssignVarInit::Reason(type_util::reason_of_t(&current).dupe()),
    };
    let use_op = UseOp::Op(Arc::new(RootUseOp::AssignVar {
        var: Some(Box::new(AssignVarTarget::Variable(reason))),
        init,
    }));
    f(cx, &use_op, name_loc, name, default_val.as_ref(), current)
}

fn current_type<'a>(cx: &mut Context<'a>, p: &pattern::Pattern<ALoc, ALoc>) -> Type {
    let loc = p.loc().dupe();
    match p {
        pattern::Pattern::Identifier { inner, .. } => {
            let name_loc = inner.name.loc.dupe();
            let name = &inner.name.name;
            type_env::find_write(
                cx,
                DefLocType::OrdinaryNameLoc,
                mk_reason(VirtualReasonDesc::RIdentifier(name.dupe()), name_loc),
            )
        }
        pattern::Pattern::Expression { .. } => {
            // Expression in pattern destructuring is unsupported syntax,
            // so we shouldn't read the environment.
            any_t::untyped(mk_reason(None, loc))
        }
        _ => {
            if ast_utils::pattern_has_binding(p) {
                type_env::find_write(cx, DefLocType::PatternLoc, mk_reason(None, loc))
            } else {
                type_::unsoundness::at(type_::UnsoundnessKind::NonBindingPattern, loc)
            }
        }
    }
}

pub fn pattern<'a>(
    cx: &mut Context<'a>,
    f: &Callback<'_>,
    acc: &mut State,
    p: &pattern::Pattern<ALoc, ALoc>,
) -> Result<pattern::Pattern<ALoc, (ALoc, Type)>, CheckExprError> {
    let check_for_invalid_annot =
        |cx: &mut Context<'a>,
         has_parent: bool,
         annot: &ast::types::AnnotationOrHint<ALoc, ALoc>| {
            if let (true, ast::types::AnnotationOrHint::Available(ann)) = (has_parent, annot) {
                flow_js::add_output_non_speculating(
                    cx,
                    ErrorMessage::EUnsupportedSyntax(Box::new((
                        ann.loc.dupe(),
                        UnsupportedSyntax::AnnotationInsideDestructuring,
                    ))),
                );
            }
        };

    let loc = p.loc().dupe();
    let ct = current_type(cx, p);

    match p {
        pattern::Pattern::Array { inner, .. } => {
            check_for_invalid_annot(cx, acc.has_parent, &inner.annot);
            let elements = array_elements(cx, f, acc, &inner.elements)?;
            let mut mapper = typed_ast_utils::UnimplementedMapper;
            let Ok(annot) = polymorphic_ast_mapper::type_annotation_hint(&mut mapper, &inner.annot);
            Ok(pattern::Pattern::Array {
                loc: (loc.dupe(), ct),
                inner: pattern::Array {
                    elements: elements.into(),
                    annot,
                    optional: inner.optional,
                    comments: inner.comments.dupe(),
                }
                .into(),
            })
        }
        pattern::Pattern::Object { inner, .. } => {
            check_for_invalid_annot(cx, acc.has_parent, &inner.annot);
            let properties = object_properties(cx, f, loc.dupe(), acc, &inner.properties)?;
            let mut mapper = typed_ast_utils::UnimplementedMapper;
            let Ok(annot) = polymorphic_ast_mapper::type_annotation_hint(&mut mapper, &inner.annot);
            Ok(pattern::Pattern::Object {
                loc: (loc.dupe(), ct),
                inner: pattern::Object {
                    properties: properties.into(),
                    annot,
                    optional: inner.optional,
                    comments: inner.comments.dupe(),
                }
                .into(),
            })
        }
        pattern::Pattern::Identifier { inner, .. } => {
            let id_loc = inner.name.loc.dupe();
            let name = &inner.name.name;
            let comments = &inner.name.comments;
            check_for_invalid_annot(cx, acc.has_parent, &inner.annot);
            let mut mapper = typed_ast_utils::UnimplementedMapper;
            let Ok(annot) = polymorphic_ast_mapper::type_annotation_hint(&mut mapper, &inner.annot);
            let id_ty = identifier(cx, f, acc, id_loc.dupe(), name)?;
            let id = ast::Identifier::new(ast::IdentifierInner {
                loc: (id_loc, id_ty),
                name: name.dupe(),
                comments: comments.dupe(),
            });
            Ok(pattern::Pattern::Identifier {
                loc: (loc.dupe(), ct),
                inner: pattern::Identifier {
                    name: id,
                    optional: inner.optional,
                    annot,
                }
                .into(),
            })
        }
        pattern::Pattern::Expression { inner, .. } => {
            flow_js::add_output_non_speculating(
                cx,
                ErrorMessage::EUnsupportedSyntax(Box::new((
                    loc.dupe(),
                    UnsupportedSyntax::DestructuringExpressionPattern,
                ))),
            );
            let mut mapper = ErrorMapper;
            let Ok(typed_expr) = polymorphic_ast_mapper::expression(&mut mapper, inner);
            Ok(pattern::Pattern::Expression {
                loc: (loc.dupe(), ct),
                inner: typed_expr.into(),
            })
        }
    }
}

pub fn array_elements<'a>(
    cx: &mut Context<'a>,
    f: &Callback<'_>,
    acc: &mut State,
    elements: &[pattern::array::Element<ALoc, ALoc>],
) -> Result<Vec<pattern::array::Element<ALoc, (ALoc, Type)>>, CheckExprError> {
    let mut result = Vec::with_capacity(elements.len());
    for (i, elem) in elements.iter().enumerate() {
        result.push(match elem {
            pattern::array::Element::Hole(loc) => pattern::array::Element::Hole(loc.dupe()),
            pattern::array::Element::NormalElement(ne) => {
                let loc = ne.loc.dupe();
                let p = &ne.argument;
                let d = &ne.default;
                let mut elem_acc = array_element(cx, acc, i as i32, loc.dupe())?;
                let typed_d = pattern_default(cx, &mut elem_acc, d.as_ref())?;
                let typed_p = pattern(cx, f, &mut elem_acc, p)?;
                pattern::array::Element::NormalElement(pattern::array::NormalElement {
                    loc,
                    argument: typed_p,
                    default: typed_d,
                })
            }
            pattern::array::Element::RestElement(re) => {
                let loc = re.loc.dupe();
                let p = &re.argument;
                let arg_loc = p.loc().dupe();
                let mut rest_acc = array_rest_element(acc, i as i32, arg_loc);
                let typed_p = pattern(cx, f, &mut rest_acc, p)?;
                pattern::array::Element::RestElement(pattern::RestElement {
                    loc,
                    argument: typed_p,
                    comments: re.comments.dupe(),
                })
            }
        });
    }
    Ok(result)
}

pub fn object_properties<'a>(
    cx: &mut Context<'a>,
    f: &Callback<'_>,
    parent_loc: ALoc,
    acc: &mut State,
    properties: &[pattern::object::Property<ALoc, ALoc>],
) -> Result<Vec<pattern::object::Property<ALoc, (ALoc, Type)>>, CheckExprError> {
    let mut xs = Vec::new();
    let mut result = Vec::new();

    for p in properties {
        match p {
            pattern::object::Property::NormalProperty(np) => {
                let loc = np.loc.dupe();
                let key = &np.key;
                let pat = &np.pattern;
                let d = &np.default;
                let shorthand = np.shorthand;
                let has_default = d.is_some();
                let current = current_type(cx, pat);
                let (mut prop_acc, typed_key) = object_property(
                    cx,
                    has_default,
                    parent_loc.dupe(),
                    current,
                    acc,
                    &mut xs,
                    key,
                )?;
                let typed_d = pattern_default(cx, &mut prop_acc, d.as_ref())?;
                let typed_p = pattern(cx, f, &mut prop_acc, pat)?;
                result.push(pattern::object::Property::NormalProperty(
                    pattern::object::NormalProperty {
                        loc,
                        key: typed_key,
                        pattern: typed_p,
                        default: typed_d,
                        shorthand,
                    },
                ));
            }
            pattern::object::Property::RestElement(re) => {
                let loc = re.loc.dupe();
                let pat = &re.argument;
                let arg_loc = pat.loc().dupe();
                let mut rest_acc = object_rest_property(acc, &xs, arg_loc);
                let typed_p = pattern(cx, f, &mut rest_acc, pat)?;
                result.push(pattern::object::Property::RestElement(
                    pattern::RestElement {
                        loc,
                        argument: typed_p,
                        comments: re.comments.dupe(),
                    },
                ));
            }
        }
    }

    Ok(result)
}

pub fn type_of_pattern(
    p: &pattern::Pattern<ALoc, ALoc>,
) -> ast::types::AnnotationOrHint<ALoc, ALoc> {
    match p {
        pattern::Pattern::Array { inner, .. } => inner.annot.clone(),
        pattern::Pattern::Object { inner, .. } => inner.annot.clone(),
        pattern::Pattern::Identifier { inner, .. } => inner.annot.clone(),
        _ => ast::types::AnnotationOrHint::Missing(ALoc::none()),
    }
}

/// instantiate pattern visitor for assignments
pub fn assignment<'a>(
    cx: &mut Context<'a>,
    init: expression::Expression<ALoc, ALoc>,
    p: &pattern::Pattern<ALoc, ALoc>,
) -> Result<pattern::Pattern<ALoc, (ALoc, Type)>, CheckExprError> {
    let mut acc = empty(Some(init), None);
    pattern(
        cx,
        &|cx, use_op, name_loc, name, _default, t| {
            // TODO destructuring+defaults unsupported in assignment expressions
            type_env::set_var(cx, use_op, name, &t, name_loc.dupe())?;
            Ok(type_env::constraining_type(t, cx, name, name_loc))
        },
        &mut acc,
        p,
    )
}
