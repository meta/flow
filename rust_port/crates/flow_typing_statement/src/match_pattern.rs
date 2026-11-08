/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeSet;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::enclosing_context::EnclosingContext;
use flow_common::js_number;
use flow_common::reason::VirtualReasonDesc;
use flow_common::reason::mk_expression_reference;
use flow_common::reason::mk_reason;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_env_builder::env_api::DefLocType;
use flow_parser::ast;
use flow_parser::ast::VariableKind;
use flow_parser::ast::expression;
use flow_parser::ast::match_pattern;
use flow_parser::ast::match_pattern::MemberPattern;
use flow_parser::polymorphic_ast_mapper;
use flow_typing_context::Context;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::MatchDuplicateObjectPropertyData;
use flow_typing_errors::error_message::MatchErrorKind;
use flow_typing_errors::error_message::MatchInvalidObjectShorthandData;
use flow_typing_errors::intermediate_error_types::MatchObjPatternKind;
use flow_typing_flow_js::flow_js;
use flow_typing_type::type_::AssignVarInit;
use flow_typing_type::type_::AssignVarTarget;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::UnsoundnessKind;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::unsoundness;
use flow_typing_utils::abnormal::CheckExprError;
use flow_typing_utils::type_env;
use flow_typing_utils::typed_ast_utils::ErrorMapper;

// Callback types matching the OCaml signatures
pub type OnIdentifier<'a> = dyn Fn(
        /*encl_ctx:*/ EnclosingContext,
        /*cx:*/ &mut Context<'a>,
        /*id:*/ &ast::IdentifierInner<ALoc, ALoc>,
        /*loc:*/ ALoc,
    ) -> Result<Type, flow_utils_concurrency::job_error::JobError>
    + 'a;

pub type OnExpression<'a> = dyn Fn(
        /*cx:*/ &mut Context<'a>,
        /*expr:*/ &expression::Expression<ALoc, ALoc>,
    ) -> Result<expression::Expression<ALoc, (ALoc, Type)>, CheckExprError>
    + 'a;

pub type OnBinding<'a> = dyn Fn(
        &mut Context<'_>,
        /*use_op:*/ &UseOp,
        /*name_loc:*/ ALoc,
        /*kind:*/ VariableKind,
        /*name:*/ &FlowSmolStr,
        /*t:*/ Type,
    ) -> Result<Type, flow_utils_concurrency::job_error::JobError>
    + 'a;

fn try_map_vec<T, U, E>(items: &[T], mut f: impl FnMut(&T) -> Result<U, E>) -> Result<Vec<U>, E> {
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        result.push(f(item)?);
    }
    Ok(result)
}

fn array_element(
    acc: &expression::Expression<ALoc, ALoc>,
    i: i32,
    loc: ALoc,
) -> expression::Expression<ALoc, ALoc> {
    expression::Expression::new(expression::ExpressionInner::Member {
        loc: loc.dupe(),
        inner: Arc::new(expression::Member {
            object: acc.dupe(),
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
}

fn object_named_property(
    acc: &expression::Expression<ALoc, ALoc>,
    loc: ALoc,
    name: &FlowSmolStr,
) -> expression::Expression<ALoc, ALoc> {
    expression::Expression::new(expression::ExpressionInner::Member {
        loc: loc.dupe(),
        inner: Arc::new(expression::Member {
            object: acc.dupe(),
            property: expression::member::Property::PropertyIdentifier(ast::Identifier(Arc::new(
                ast::IdentifierInner {
                    loc: loc.dupe(),
                    name: name.dupe(),
                    comments: None,
                },
            ))),
            comments: None,
        }),
    })
}

fn object_property_key<'a>(
    cx: &mut Context<'a>,
    acc: &expression::Expression<ALoc, ALoc>,
    pattern_kind: MatchObjPatternKind,
    key: &match_pattern::object_pattern::Key<ALoc, ALoc>,
) -> (
    expression::Expression<ALoc, ALoc>,
    match_pattern::object_pattern::Key<ALoc, (ALoc, Type)>,
    Option<FlowSmolStr>,
) {
    match key {
        match_pattern::object_pattern::Key::Identifier(id) => {
            let loc = id.loc.dupe();
            let new_acc = object_named_property(acc, loc.dupe(), &id.name);
            let current = unsoundness::at(UnsoundnessKind::NonBindingPattern, loc.dupe());
            let typed_id = ast::Identifier(
                ast::IdentifierInner {
                    loc: (loc, current),
                    name: id.name.dupe(),
                    comments: id.comments.dupe(),
                }
                .into(),
            );
            (
                new_acc,
                match_pattern::object_pattern::Key::Identifier(typed_id),
                Some(id.name.dupe()),
            )
        }
        match_pattern::object_pattern::Key::StringLiteral((loc, lit)) => {
            let new_acc = object_named_property(acc, loc.dupe(), &lit.value);
            (
                new_acc,
                match_pattern::object_pattern::Key::StringLiteral((loc.dupe(), lit.clone())),
                Some(lit.value.dupe()),
            )
        }
        match_pattern::object_pattern::Key::NumberLiteral((loc, lit)) => {
            if let Some(prop) = js_number::numeric_property_name(lit.value) {
                let new_acc = object_named_property(acc, loc.dupe(), &prop);
                (
                    new_acc,
                    match_pattern::object_pattern::Key::NumberLiteral((loc.dupe(), lit.clone())),
                    Some(prop),
                )
            } else {
                flow_js::add_output_non_speculating(
                    cx,
                    ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidObjectPropertyLiteral {
                        loc: loc.dupe(),
                        pattern_kind,
                    }),
                );
                (
                    acc.dupe(),
                    match_pattern::object_pattern::Key::NumberLiteral((loc.dupe(), lit.clone())),
                    None,
                )
            }
        }
        match_pattern::object_pattern::Key::BigIntLiteral((loc, _)) => {
            flow_js::add_output_non_speculating(
                cx,
                ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidObjectPropertyLiteral {
                    loc: loc.dupe(),
                    pattern_kind,
                }),
            );
            let mut mapper = ErrorMapper;
            let typed_key = {
                let Ok(v) =
                    polymorphic_ast_mapper::match_object_pattern_property_key(&mut mapper, key);
                v
            };
            (acc.dupe(), typed_key, None)
        }
    }
}

fn binding<'a>(
    cx: &mut Context<'a>,
    on_binding: &OnBinding<'_>,
    kind: VariableKind,
    acc: &expression::Expression<ALoc, ALoc>,
    name_loc: ALoc,
    name: &FlowSmolStr,
) -> Result<Type, flow_utils_concurrency::job_error::JobError> {
    let reason = mk_reason(VirtualReasonDesc::RIdentifier(name.dupe()), name_loc.dupe());
    let current = type_env::find_write(cx, DefLocType::OrdinaryNameLoc, reason.dupe());
    let use_op = UseOp::Op(std::sync::Arc::new(
        flow_typing_type::type_::RootUseOp::AssignVar {
            var: Some(Box::new(AssignVarTarget::Variable(reason.dupe()))),
            init: AssignVarInit::Expression(Box::new(mk_expression_reference(acc))),
        },
    ));
    on_binding(cx, &use_op, name_loc, kind, name, current)
}

fn binding_identifier<'a>(
    cx: &mut Context<'a>,
    on_binding: &OnBinding<'_>,
    in_or_pattern: bool,
    kind: VariableKind,
    acc: &expression::Expression<ALoc, ALoc>,
    id: &ast::Identifier<ALoc, ALoc>,
) -> Result<ast::Identifier<ALoc, (ALoc, Type)>, flow_utils_concurrency::job_error::JobError> {
    let loc = id.loc.dupe();
    let name = &id.name;
    let comments = &id.comments;
    if in_or_pattern {
        flow_js::add_output_non_speculating(
            cx,
            ErrorMessage::EMatchError(MatchErrorKind::MatchBindingInOrPattern { loc: loc.dupe() }),
        );
        let mut mapper = ErrorMapper;
        Ok({
            let Ok(v) = polymorphic_ast_mapper::t_identifier(&mut mapper, id);
            v
        })
    } else {
        let t = binding(cx, on_binding, kind, acc, loc.dupe(), name)?;
        Ok(ast::Identifier(
            ast::IdentifierInner {
                loc: (loc, t),
                name: name.dupe(),
                comments: comments.clone(),
            }
            .into(),
        ))
    }
}

fn binding_pattern<'a>(
    cx: &mut Context<'a>,
    on_binding: &OnBinding<'_>,
    in_or_pattern: bool,
    loc: ALoc,
    acc: &expression::Expression<ALoc, ALoc>,
    bp: &match_pattern::BindingPattern<ALoc, ALoc>,
) -> Result<
    match_pattern::BindingPattern<ALoc, (ALoc, Type)>,
    flow_utils_concurrency::job_error::JobError,
> {
    let kind = bp.kind;
    let id = &bp.id;
    let comments = &bp.comments;
    let typed_id = match kind {
        VariableKind::Var | VariableKind::Let => {
            flow_js::add_output_non_speculating(
                cx,
                ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidBindingKind { loc, kind }),
            );
            let mut mapper = ErrorMapper;
            {
                let Ok(v) = polymorphic_ast_mapper::t_identifier(&mut mapper, id);
                v
            }
        }
        VariableKind::Const => binding_identifier(cx, on_binding, in_or_pattern, kind, acc, id)?,
    };
    Ok(match_pattern::BindingPattern {
        kind,
        id: typed_id,
        comments: comments.clone(),
    })
}

fn member<'a>(
    cx: &mut Context<'a>,
    on_identifier: &OnIdentifier<'a>,
    on_expression: &OnExpression<'a>,
    mem: &MemberPattern<ALoc, ALoc>,
) -> Result<
    (
        expression::Expression<ALoc, ALoc>,
        MemberPattern<ALoc, (ALoc, Type)>,
    ),
    CheckExprError,
> {
    let loc = mem.loc.dupe();
    let base = &mem.base;
    let property = &mem.property;
    let comments = &mem.comments;
    let (base_exp, typed_base) = match base {
        match_pattern::member_pattern::Base::BaseIdentifier(id) => {
            let id_loc = id.loc.dupe();
            let exp = expression::Expression::new(expression::ExpressionInner::Identifier {
                loc: id_loc.dupe(),
                inner: id.dupe(),
            });
            let t = on_identifier(EnclosingContext::OtherTestContext, cx, id, id_loc.dupe())?;
            let typed_id = ast::Identifier(
                ast::IdentifierInner {
                    loc: (id_loc, t),
                    name: id.name.dupe(),
                    comments: id.comments.dupe(),
                }
                .into(),
            );
            (
                exp,
                match_pattern::member_pattern::Base::BaseIdentifier(typed_id),
            )
        }
        match_pattern::member_pattern::Base::BaseMember(inner_mem) => {
            let (exp, typed_mem) = member(cx, on_identifier, on_expression, inner_mem)?;
            (
                exp,
                match_pattern::member_pattern::Base::BaseMember(typed_mem.into()),
            )
        }
    };
    enum PropertyMapper<'a> {
        Identifier(ALoc, &'a ast::IdentifierInner<ALoc, ALoc>),
        StringLiteral(ALoc, &'a ast::StringLiteral<ALoc>),
        NumberLiteral(ALoc, &'a ast::NumberLiteral<ALoc>),
        BigIntLiteral(ALoc, &'a ast::BigIntLiteral<ALoc>),
    }
    let (property_exp, prop_mapper) = match property {
        match_pattern::member_pattern::Property::PropertyIdentifier(id) => {
            let prop_loc = id.loc.dupe();
            let exp = expression::member::Property::PropertyIdentifier(id.dupe());
            (exp, PropertyMapper::Identifier(prop_loc, id))
        }
        match_pattern::member_pattern::Property::PropertyString {
            loc: prop_loc,
            literal,
        } => {
            let exp = expression::member::Property::PropertyExpression(
                expression::Expression::new(expression::ExpressionInner::StringLiteral {
                    loc: prop_loc.dupe(),
                    inner: Arc::new(literal.clone()),
                }),
            );
            (exp, PropertyMapper::StringLiteral(prop_loc.dupe(), literal))
        }
        match_pattern::member_pattern::Property::PropertyNumber {
            loc: prop_loc,
            literal,
        } => {
            let exp = expression::member::Property::PropertyExpression(
                expression::Expression::new(expression::ExpressionInner::NumberLiteral {
                    loc: prop_loc.dupe(),
                    inner: Arc::new(literal.clone()),
                }),
            );
            (exp, PropertyMapper::NumberLiteral(prop_loc.dupe(), literal))
        }
        match_pattern::member_pattern::Property::PropertyBigInt {
            loc: prop_loc,
            literal,
        } => {
            let exp = expression::member::Property::PropertyExpression(
                expression::Expression::new(expression::ExpressionInner::BigIntLiteral {
                    loc: prop_loc.dupe(),
                    inner: Arc::new(literal.clone()),
                }),
            );
            (exp, PropertyMapper::BigIntLiteral(prop_loc.dupe(), literal))
        }
    };
    let exp = expression::Expression::new(expression::ExpressionInner::Member {
        loc: loc.dupe(),
        inner: Arc::new(expression::Member {
            object: base_exp,
            property: property_exp,
            comments: comments.clone(),
        }),
    });
    let typed_exp = on_expression(cx, &exp)?;
    let (_, t) = typed_exp.loc().dupe();
    let typed_property = match prop_mapper {
        PropertyMapper::Identifier(prop_loc, id) => {
            match_pattern::member_pattern::Property::PropertyIdentifier(ast::Identifier(
                ast::IdentifierInner {
                    loc: (prop_loc, t.dupe()),
                    name: id.name.dupe(),
                    comments: id.comments.dupe(),
                }
                .into(),
            ))
        }
        PropertyMapper::StringLiteral(prop_loc, lit) => {
            match_pattern::member_pattern::Property::PropertyString {
                loc: prop_loc,
                literal: lit.clone(),
            }
        }
        PropertyMapper::NumberLiteral(prop_loc, lit) => {
            match_pattern::member_pattern::Property::PropertyNumber {
                loc: prop_loc,
                literal: lit.clone(),
            }
        }
        PropertyMapper::BigIntLiteral(prop_loc, lit) => {
            match_pattern::member_pattern::Property::PropertyBigInt {
                loc: prop_loc,
                literal: lit.clone(),
            }
        }
    };
    Ok((
        exp,
        MemberPattern {
            loc: (loc, t),
            base: typed_base,
            property: typed_property,
            comments: comments.clone(),
        },
    ))
}

fn rest_pattern<'a>(
    cx: &mut Context<'a>,
    on_binding: &OnBinding<'_>,
    in_or_pattern: bool,
    acc: &expression::Expression<ALoc, ALoc>,
    rest: &Option<match_pattern::RestPattern<ALoc, ALoc>>,
) -> Result<
    Option<match_pattern::RestPattern<ALoc, (ALoc, Type)>>,
    flow_utils_concurrency::job_error::JobError,
> {
    Ok(match rest.as_ref() {
        Some(rp) => {
            let rest_loc = rp.loc.dupe();
            let argument = &rp.argument;
            let comments = &rp.comments;
            let typed_argument = match argument.as_ref() {
                Some((arg_loc, arg)) => {
                    let typed_bp =
                        binding_pattern(cx, on_binding, in_or_pattern, arg_loc.dupe(), acc, arg)?;
                    Some((arg_loc.dupe(), typed_bp))
                }
                None => None,
            };
            Some(match_pattern::RestPattern {
                loc: rest_loc,
                argument: typed_argument,
                comments: comments.clone(),
            })
        }
        None => None,
    })
}

fn pattern_<'a>(
    cx: &mut Context<'a>,
    on_identifier: &OnIdentifier<'a>,
    on_expression: &OnExpression<'a>,
    on_binding: &OnBinding<'_>,
    in_or_pattern: bool,
    acc: &expression::Expression<ALoc, ALoc>,
    p: &match_pattern::MatchPattern<ALoc, ALoc>,
) -> Result<match_pattern::MatchPattern<ALoc, (ALoc, Type)>, CheckExprError> {
    let _loc = p.loc().dupe();
    Ok(match p {
        match_pattern::MatchPattern::NumberPattern { loc, inner } => {
            match_pattern::MatchPattern::NumberPattern {
                loc: loc.dupe(),
                inner: inner.clone(),
            }
        }
        match_pattern::MatchPattern::BigIntPattern { loc, inner } => {
            match_pattern::MatchPattern::BigIntPattern {
                loc: loc.dupe(),
                inner: inner.clone(),
            }
        }
        match_pattern::MatchPattern::StringPattern { loc, inner } => {
            match_pattern::MatchPattern::StringPattern {
                loc: loc.dupe(),
                inner: inner.clone(),
            }
        }
        match_pattern::MatchPattern::BooleanPattern { loc, inner } => {
            match_pattern::MatchPattern::BooleanPattern {
                loc: loc.dupe(),
                inner: inner.clone(),
            }
        }
        match_pattern::MatchPattern::NullPattern { loc, inner } => {
            match_pattern::MatchPattern::NullPattern {
                loc: loc.dupe(),
                inner: inner.clone(),
            }
        }
        match_pattern::MatchPattern::UnaryPattern { loc, inner } => {
            let operator = &inner.operator;
            let argument = &inner.argument;
            match (operator, argument) {
                (_, (_, match_pattern::unary_pattern::Argument::NumberLiteral(lit)))
                    if lit.value == 0.0 =>
                {
                    flow_js::add_output_non_speculating(
                        cx,
                        ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidUnaryZero {
                            loc: loc.dupe(),
                        }),
                    );
                }
                (
                    match_pattern::unary_pattern::Operator::Plus,
                    (_, match_pattern::unary_pattern::Argument::BigIntLiteral(_)),
                ) => {
                    flow_js::add_output_non_speculating(
                        cx,
                        ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidUnaryPlusBigInt {
                            loc: loc.dupe(),
                        }),
                    );
                }
                _ => {}
            }
            match_pattern::MatchPattern::UnaryPattern {
                loc: loc.dupe(),
                inner: inner.clone(),
            }
        }
        match_pattern::MatchPattern::MemberPattern { loc, inner } => {
            let (_, typed_mem) = member(cx, on_identifier, on_expression, inner)?;
            match_pattern::MatchPattern::MemberPattern {
                loc: loc.dupe(),
                inner: typed_mem.into(),
            }
        }
        match_pattern::MatchPattern::OrPattern { loc, inner } => {
            let typed_patterns: Vec<_> = try_map_vec(&inner.patterns, |pat| {
                pattern_(cx, on_identifier, on_expression, on_binding, true, acc, pat)
            })?;
            match_pattern::MatchPattern::OrPattern {
                loc: loc.dupe(),
                inner: match_pattern::OrPattern {
                    patterns: typed_patterns.into(),
                    comments: inner.comments.dupe(),
                }
                .into(),
            }
        }
        match_pattern::MatchPattern::AsPattern { loc, inner } => {
            if matches!(
                &inner.pattern,
                match_pattern::MatchPattern::BindingPattern { .. }
            ) {
                flow_js::add_output_non_speculating(
                    cx,
                    ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidAsPattern {
                        loc: loc.dupe(),
                    }),
                );
            }
            let typed_p = pattern_(
                cx,
                on_identifier,
                on_expression,
                on_binding,
                in_or_pattern,
                acc,
                &inner.pattern,
            )?;
            let typed_target = match &inner.target {
                match_pattern::as_pattern::Target::Binding {
                    loc: target_loc,
                    pattern: bp,
                } => {
                    let typed_bp =
                        binding_pattern(cx, on_binding, in_or_pattern, target_loc.dupe(), acc, bp)?;
                    match_pattern::as_pattern::Target::Binding {
                        loc: target_loc.dupe(),
                        pattern: typed_bp,
                    }
                }
                match_pattern::as_pattern::Target::Identifier(id) => {
                    let typed_id = binding_identifier(
                        cx,
                        on_binding,
                        in_or_pattern,
                        VariableKind::Const,
                        acc,
                        id,
                    )?;
                    match_pattern::as_pattern::Target::Identifier(typed_id)
                }
            };
            match_pattern::MatchPattern::AsPattern {
                loc: loc.dupe(),
                inner: match_pattern::AsPattern {
                    pattern: typed_p,
                    target: typed_target,
                    comments: inner.comments.dupe(),
                }
                .into(),
            }
        }
        match_pattern::MatchPattern::IdentifierPattern { loc, inner } => {
            let t = on_identifier(EnclosingContext::OtherTestContext, cx, inner, loc.dupe())?;
            match_pattern::MatchPattern::IdentifierPattern {
                loc: loc.dupe(),
                inner: Box::new(ast::Identifier(
                    ast::IdentifierInner {
                        loc: (loc.dupe(), t),
                        name: inner.name.dupe(),
                        comments: inner.comments.dupe(),
                    }
                    .into(),
                )),
            }
        }
        match_pattern::MatchPattern::BindingPattern { loc, inner } => {
            let typed_bp = binding_pattern(cx, on_binding, in_or_pattern, loc.dupe(), acc, inner)?;
            match_pattern::MatchPattern::BindingPattern {
                loc: loc.dupe(),
                inner: typed_bp.into(),
            }
        }
        match_pattern::MatchPattern::WildcardPattern { loc, inner } => {
            if inner.invalid_syntax_default_keyword {
                flow_js::add_output_non_speculating(
                    cx,
                    ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidWildcardSyntax(
                        loc.dupe(),
                    )),
                );
            }
            match_pattern::MatchPattern::WildcardPattern {
                loc: loc.dupe(),
                inner: inner.clone(),
            }
        }
        match_pattern::MatchPattern::ArrayPattern { loc, inner } => {
            let typed_ap = array_pattern(
                cx,
                on_identifier,
                on_expression,
                on_binding,
                in_or_pattern,
                acc,
                inner,
            )?;
            match_pattern::MatchPattern::ArrayPattern {
                loc: loc.dupe(),
                inner: typed_ap.into(),
            }
        }
        match_pattern::MatchPattern::ObjectPattern { loc, inner } => {
            let typed_op = object_pattern(
                cx,
                on_identifier,
                on_expression,
                on_binding,
                in_or_pattern,
                MatchObjPatternKind::Object,
                acc,
                inner,
            )?;
            match_pattern::MatchPattern::ObjectPattern {
                loc: loc.dupe(),
                inner: typed_op.into(),
            }
        }
        match_pattern::MatchPattern::InstancePattern { loc, inner } => {
            let typed_constructor = match &inner.constructor {
                match_pattern::InstancePatternConstructor::IdentifierConstructor(id) => {
                    let id_loc = id.loc.dupe();
                    let t =
                        on_identifier(EnclosingContext::OtherTestContext, cx, id, id_loc.dupe())?;
                    match_pattern::InstancePatternConstructor::IdentifierConstructor(
                        ast::Identifier(
                            ast::IdentifierInner {
                                loc: (id_loc, t),
                                name: id.name.dupe(),
                                comments: id.comments.dupe(),
                            }
                            .into(),
                        ),
                    )
                }
                match_pattern::InstancePatternConstructor::MemberConstructor(mem) => {
                    let (_, typed_mem) = member(cx, on_identifier, on_expression, mem)?;
                    match_pattern::InstancePatternConstructor::MemberConstructor(typed_mem)
                }
            };
            let (properties_loc, properties_inner) = &inner.properties;
            let typed_properties = object_pattern(
                cx,
                on_identifier,
                on_expression,
                on_binding,
                in_or_pattern,
                MatchObjPatternKind::Instance,
                acc,
                properties_inner,
            )?;
            match_pattern::MatchPattern::InstancePattern {
                loc: loc.dupe(),
                inner: match_pattern::InstancePattern {
                    constructor: typed_constructor,
                    properties: (properties_loc.dupe(), typed_properties),
                    comments: inner.comments.dupe(),
                }
                .into(),
            }
        }
    })
}

fn array_pattern<'a>(
    cx: &mut Context<'a>,
    on_identifier: &OnIdentifier<'a>,
    on_expression: &OnExpression<'a>,
    on_binding: &OnBinding<'_>,
    in_or_pattern: bool,
    acc: &expression::Expression<ALoc, ALoc>,
    ap: &match_pattern::ArrayPattern<ALoc, ALoc>,
) -> Result<match_pattern::ArrayPattern<ALoc, (ALoc, Type)>, CheckExprError> {
    let typed_rest = rest_pattern(cx, on_binding, in_or_pattern, acc, &ap.rest)?;
    let typed_elements = array_elements(
        cx,
        on_identifier,
        on_expression,
        on_binding,
        in_or_pattern,
        acc,
        &ap.elements,
    )?;
    Ok(match_pattern::ArrayPattern {
        elements: typed_elements.into(),
        rest: typed_rest,
        comments: ap.comments.dupe(),
    })
}

fn array_elements<'a>(
    cx: &mut Context<'a>,
    on_identifier: &OnIdentifier<'a>,
    on_expression: &OnExpression<'a>,
    on_binding: &OnBinding<'_>,
    in_or_pattern: bool,
    acc: &expression::Expression<ALoc, ALoc>,
    elements: &[match_pattern::array_pattern::Element<ALoc, ALoc>],
) -> Result<Vec<match_pattern::array_pattern::Element<ALoc, (ALoc, Type)>>, CheckExprError> {
    elements
        .iter()
        .enumerate()
        .map(|(i, elem)| {
            let loc = elem.pattern.loc().dupe();
            let new_acc = array_element(acc, i as i32, loc.dupe());
            let typed_p = pattern_(
                cx,
                on_identifier,
                on_expression,
                on_binding,
                in_or_pattern,
                &new_acc,
                &elem.pattern,
            )?;
            Ok(match_pattern::array_pattern::Element {
                pattern: typed_p,
                index: elem.index.dupe(),
            })
        })
        .collect()
}

fn object_pattern<'a>(
    cx: &mut Context<'a>,
    on_identifier: &OnIdentifier<'a>,
    on_expression: &OnExpression<'a>,
    on_binding: &OnBinding<'_>,
    in_or_pattern: bool,
    pattern_kind: MatchObjPatternKind,
    acc: &expression::Expression<ALoc, ALoc>,
    op: &match_pattern::ObjectPattern<ALoc, ALoc>,
) -> Result<match_pattern::ObjectPattern<ALoc, (ALoc, Type)>, CheckExprError> {
    let typed_rest = rest_pattern(cx, on_binding, in_or_pattern, acc, &op.rest)?;
    let typed_properties = object_properties(
        cx,
        on_identifier,
        on_expression,
        on_binding,
        in_or_pattern,
        pattern_kind,
        acc,
        &op.properties,
    )?;
    Ok(match_pattern::ObjectPattern {
        properties: typed_properties.into(),
        rest: typed_rest,
        comments: op.comments.dupe(),
    })
}

fn object_properties<'a>(
    cx: &mut Context<'a>,
    on_identifier: &OnIdentifier<'a>,
    on_expression: &OnExpression<'a>,
    on_binding: &OnBinding<'_>,
    in_or_pattern: bool,
    pattern_kind: MatchObjPatternKind,
    acc: &expression::Expression<ALoc, ALoc>,
    props: &[match_pattern::object_pattern::Property<ALoc, ALoc>],
) -> Result<Vec<match_pattern::object_pattern::Property<ALoc, (ALoc, Type)>>, CheckExprError> {
    let mut seen: BTreeSet<FlowSmolStr> = BTreeSet::new();
    let mut result: Vec<match_pattern::object_pattern::Property<ALoc, (ALoc, Type)>> = Vec::new();

    for prop in props {
        match prop {
            match_pattern::object_pattern::Property::Valid { loc, property } => {
                let key = &property.key;
                let p = &property.pattern;
                let shorthand = property.shorthand;
                let prop_comments = &property.comments;
                let (new_acc, typed_key, name) = object_property_key(cx, acc, pattern_kind, key);
                if let Some(name) = &name
                    && seen.contains(name)
                {
                    let key_loc = match &typed_key {
                        match_pattern::object_pattern::Key::StringLiteral((loc, _))
                        | match_pattern::object_pattern::Key::NumberLiteral((loc, _))
                        | match_pattern::object_pattern::Key::BigIntLiteral((loc, _)) => loc.dupe(),
                        match_pattern::object_pattern::Key::Identifier(id) => {
                            if shorthand {
                                if let match_pattern::MatchPattern::BindingPattern {
                                    inner: bp,
                                    ..
                                } = p
                                {
                                    bp.id.loc.dupe()
                                } else {
                                    id.loc.0.dupe()
                                }
                            } else {
                                id.loc.0.dupe()
                            }
                        }
                    };
                    flow_js::add_output_non_speculating(
                        cx,
                        ErrorMessage::EMatchError(MatchErrorKind::MatchDuplicateObjectProperty(
                            Box::new(MatchDuplicateObjectPropertyData {
                                loc: key_loc,
                                name: FlowSmolStr::new(name),
                                pattern_kind,
                            }),
                        )),
                    );
                }
                let typed_p = pattern_(
                    cx,
                    on_identifier,
                    on_expression,
                    on_binding,
                    in_or_pattern,
                    &new_acc,
                    p,
                )?;
                result.push(match_pattern::object_pattern::Property::Valid {
                    loc: loc.dupe(),
                    property: match_pattern::object_pattern::PropertyStruct {
                        key: typed_key,
                        pattern: typed_p,
                        shorthand,
                        comments: prop_comments.clone(),
                    },
                });
                if let Some(name) = name {
                    seen.insert(name);
                }
            }
            match_pattern::object_pattern::Property::InvalidShorthand {
                loc,
                identifier: id,
            } => {
                let name = &id.name;
                flow_js::add_output_non_speculating(
                    cx,
                    ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidObjectShorthand(
                        Box::new(MatchInvalidObjectShorthandData {
                            loc: loc.dupe(),
                            name: name.dupe(),
                            pattern_kind,
                        }),
                    )),
                );
                result.push(match_pattern::object_pattern::Property::InvalidShorthand {
                    loc: loc.dupe(),
                    identifier: id.clone(),
                });
                seen.insert(name.dupe());
            }
        }
    }

    Ok(result)
}

pub fn pattern<'a>(
    cx: &mut Context<'a>,
    on_identifier: &OnIdentifier<'a>,
    on_expression: &OnExpression<'a>,
    on_binding: &OnBinding<'_>,
    acc: &expression::Expression<ALoc, ALoc>,
    p: &match_pattern::MatchPattern<ALoc, ALoc>,
) -> Result<match_pattern::MatchPattern<ALoc, (ALoc, Type)>, CheckExprError> {
    pattern_(cx, on_identifier, on_expression, on_binding, false, acc, p)
}

pub fn type_of_member_pattern<'a>(
    cx: &mut Context<'a>,
    on_identifier: &OnIdentifier<'a>,
    on_expression: &OnExpression<'a>,
    mem: &MemberPattern<ALoc, ALoc>,
) -> Result<Type, CheckExprError> {
    let (_, typed_mem) = member(cx, on_identifier, on_expression, mem)?;
    Ok(typed_mem.loc.1.dupe())
}
