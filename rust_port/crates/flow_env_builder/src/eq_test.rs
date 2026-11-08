/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! This module provides equality test analysis for refinements.

use std::collections::BTreeMap;
use std::ops::Deref;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::hint::SentinelRefinement;
use flow_common::js_number::numeric_property_name;
use flow_common::reason::mk_expression_reason;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast::expression::Expression;
use flow_parser::ast::expression::ExpressionInner;
use flow_parser::ast::expression::object;
use flow_parser::ast::jsx;
use flow_parser::ast_utils;

use crate::env_api::RefinementKind;
use crate::refinement_key::Lookup;

#[derive(Debug)]
pub enum EqTestAction<'a> {
    /// on_type_of_test: (loc, argument, str_expr, typename, sense)
    TypeOf(
        ALoc,
        &'a Expression<ALoc, ALoc>,
        &'a Expression<ALoc, ALoc>,
        &'a str,
        bool,
    ),
    /// on_literal_test: (strict, sense, loc, expr, refinement, other)
    Literal(
        bool,
        bool,
        ALoc,
        &'a Expression<ALoc, ALoc>,
        RefinementKind<ALoc>,
        &'a Expression<ALoc, ALoc>,
    ),
    /// on_null_test: (sense, strict, loc, expr, other)
    Null(
        bool,
        bool,
        ALoc,
        &'a Expression<ALoc, ALoc>,
        &'a Expression<ALoc, ALoc>,
    ),
    /// on_void_test: (sense, strict, check_for_bound_undefined, loc, expr, other)
    Void(
        bool,
        bool,
        bool,
        ALoc,
        &'a Expression<ALoc, ALoc>,
        &'a Expression<ALoc, ALoc>,
    ),
    /// on_member_eq_other: (expr, other)
    MemberEqOther(&'a Expression<ALoc, ALoc>, &'a Expression<ALoc, ALoc>),
    /// on_other_eq_member: (other, expr)
    OtherEqMember(&'a Expression<ALoc, ALoc>, &'a Expression<ALoc, ALoc>),
    /// on_other_eq_test: (left, right)
    Other(&'a Expression<ALoc, ALoc>, &'a Expression<ALoc, ALoc>),
}

/// Dispatch version of visit_eq_test that returns an enum instead of calling callbacks.
/// This allows Rust code to call this function and then dispatch based on the result,
/// working around Rust's ownership constraints with closures.
#[allow(clippy::too_many_arguments)]
pub fn dispatch_eq_test<'a>(
    is_switch_cond_context: bool,
    strict: bool,
    sense: bool,
    loc: ALoc,
    left: &'a Expression<ALoc, ALoc>,
    right: &'a Expression<ALoc, ALoc>,
) -> EqTestAction<'a> {
    visit_eq_test(
        |loc, arg, str_expr, typename, sense| {
            EqTestAction::TypeOf(loc, arg, str_expr, typename, sense)
        },
        |strict, sense, loc, expr, refinement, other| {
            EqTestAction::Literal(strict, sense, loc, expr, refinement, other)
        },
        EqTestAction::Null,
        |sense, strict, check, loc, expr, other| {
            EqTestAction::Void(sense, strict, check, loc, expr, other)
        },
        EqTestAction::MemberEqOther,
        EqTestAction::OtherEqMember,
        is_switch_cond_context,
        EqTestAction::Other,
        strict,
        sense,
        loc,
        left,
        right,
    )
}

pub fn extract_number_literal(node: &Expression<ALoc, ALoc>) -> (f64, FlowSmolStr) {
    match ast_utils::extract_number_literal(node) {
        Some(lit) => lit,
        None => panic!("not a number literal"),
    }
}

pub fn extract_bigint_literal(node: &Expression<ALoc, ALoc>) -> (Option<i64>, FlowSmolStr) {
    match ast_utils::extract_bigint_literal(node) {
        Some(lit) => lit,
        None => panic!("not a bigint literal"),
    }
}

pub fn literal_check_of_expr(e: &Expression<ALoc, ALoc>) -> Option<SentinelRefinement> {
    match e.deref() {
        ExpressionInner::StringLiteral { inner, .. } => {
            Some(SentinelRefinement::SingletonStr(inner.value.dupe()))
        }
        ExpressionInner::NumberLiteral { inner, .. } => {
            Some(SentinelRefinement::SingletonNum(inner.value))
        }
        ExpressionInner::BooleanLiteral { inner, .. } => {
            Some(SentinelRefinement::SingletonBool(inner.value))
        }
        ExpressionInner::Member { inner, .. } => {
            if Lookup::of_member(inner, false).is_some() {
                Some(SentinelRefinement::Member(mk_expression_reason(e)))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn jsx_attributes_possible_sentinel_refinements(
    attrs: &[jsx::OpeningAttribute<ALoc, ALoc>],
) -> BTreeMap<FlowSmolStr, SentinelRefinement> {
    let mut acc = BTreeMap::new();

    for attr in attrs {
        match attr {
            jsx::OpeningAttribute::Attribute(normal) => {
                let name = match &normal.name {
                    jsx::attribute::Name::Identifier(id) => id.name.dupe(),
                    jsx::attribute::Name::NamespacedName(_) => continue,
                };

                let check = match &normal.value {
                    None => Some(SentinelRefinement::SingletonBool(true)),
                    Some(jsx::attribute::Value::StringLiteral((_, lit))) => {
                        Some(SentinelRefinement::SingletonStr(lit.value.dupe()))
                    }
                    Some(jsx::attribute::Value::ExpressionContainer((_, container))) => {
                        match &container.expression {
                            jsx::expression_container::Expression::Expression(e) => {
                                literal_check_of_expr(e)
                            }
                            jsx::expression_container::Expression::EmptyExpression => None,
                        }
                    }
                };

                if let Some(check) = check {
                    acc.insert(name, check);
                }
            }
            jsx::OpeningAttribute::SpreadAttribute(_) => {}
        }
    }

    acc
}

pub fn object_properties_possible_sentinel_refinements(
    props: &[object::Property<ALoc, ALoc>],
) -> BTreeMap<FlowSmolStr, SentinelRefinement> {
    let mut acc = BTreeMap::new();

    for prop in props {
        match prop {
            object::Property::NormalProperty(normal) => {
                if let object::NormalProperty::Init { key, value, .. } = normal {
                    match key {
                        object::Key::StringLiteral((_, lit)) => {
                            let name = lit.value.dupe();
                            if let Some(check) = literal_check_of_expr(value) {
                                acc.insert(name, check);
                            }
                        }
                        object::Key::Identifier(id) => {
                            let name = id.name.dupe();
                            if let Some(check) = literal_check_of_expr(value) {
                                acc.insert(name, check);
                            }
                        }
                        object::Key::NumberLiteral((_, lit))
                            if let Some(name) = numeric_property_name(lit.value) =>
                        {
                            if let Some(check) = literal_check_of_expr(value) {
                                acc.insert(name, check);
                            }
                        }
                        _ => {}
                    }
                }
            }
            object::Property::SpreadProperty(_) => {}
        }
    }

    acc
}

#[allow(clippy::too_many_arguments)]
pub fn visit_eq_test<
    'a,
    R,
    OnTypeOf,
    OnLiteral,
    OnNull,
    OnVoid,
    OnMemberOther,
    OnOtherMember,
    OnOther,
>(
    on_type_of_test: OnTypeOf,
    on_literal_test: OnLiteral,
    on_null_test: OnNull,
    on_void_test: OnVoid,
    on_member_eq_other: OnMemberOther,
    on_other_eq_member: OnOtherMember,
    is_switch_cond_context: bool,
    on_other_eq_test: OnOther,
    strict: bool,
    sense: bool,
    loc: ALoc,
    left: &'a Expression<ALoc, ALoc>,
    right: &'a Expression<ALoc, ALoc>,
) -> R
where
    OnTypeOf:
        FnOnce(ALoc, &'a Expression<ALoc, ALoc>, &'a Expression<ALoc, ALoc>, &'a str, bool) -> R,
    OnLiteral: FnOnce(
        bool,
        bool,
        ALoc,
        &'a Expression<ALoc, ALoc>,
        RefinementKind<ALoc>,
        &'a Expression<ALoc, ALoc>,
    ) -> R,
    OnNull: FnOnce(bool, bool, ALoc, &'a Expression<ALoc, ALoc>, &'a Expression<ALoc, ALoc>) -> R,
    OnVoid:
        FnOnce(bool, bool, bool, ALoc, &'a Expression<ALoc, ALoc>, &'a Expression<ALoc, ALoc>) -> R,
    OnMemberOther: FnOnce(&'a Expression<ALoc, ALoc>, &'a Expression<ALoc, ALoc>) -> R,
    OnOtherMember: FnOnce(&'a Expression<ALoc, ALoc>, &'a Expression<ALoc, ALoc>) -> R,
    OnOther: FnOnce(&'a Expression<ALoc, ALoc>, &'a Expression<ALoc, ALoc>) -> R,
{
    use flow_parser::ast::expression::*;

    fn extract_string_value(expr: &Expression<ALoc, ALoc>) -> Option<&FlowSmolStr> {
        match expr.deref() {
            ExpressionInner::StringLiteral { inner, .. } => Some(&inner.value),
            ExpressionInner::TemplateLiteral { inner, .. }
                if inner.quasis.len() == 1 && inner.expressions.is_empty() =>
            {
                Some(&inner.quasis[0].value.cooked)
            }
            _ => None,
        }
    }

    fn is_typeof(expr: &Expression<ALoc, ALoc>) -> Option<&Expression<ALoc, ALoc>> {
        match expr.deref() {
            ExpressionInner::Unary { inner, .. }
                if matches!(inner.operator, UnaryOperator::Typeof) =>
            {
                Some(&inner.argument)
            }
            _ => None,
        }
    }

    match (left, right) {
        // typeof expr ==/=== string
        (l, r) if is_typeof(l).is_some() && extract_string_value(r).is_some() => {
            let argument = is_typeof(l).unwrap();
            let s = extract_string_value(r).unwrap();
            on_type_of_test(loc, argument, r, s, sense)
        }
        (l, r) if extract_string_value(l).is_some() && is_typeof(r).is_some() => {
            let argument = is_typeof(r).unwrap();
            let s = extract_string_value(l).unwrap();
            on_type_of_test(loc, argument, l, s, sense)
        }

        // bool equality
        (left_expr, right_expr)
            if matches!(left_expr.deref(), ExpressionInner::BooleanLiteral { .. })
                || matches!(right_expr.deref(), ExpressionInner::BooleanLiteral { .. }) =>
        {
            let (lit_loc, inner, expr, other) = if let ExpressionInner::BooleanLiteral {
                loc: lit_loc,
                inner,
            } = left_expr.deref()
            {
                (lit_loc, inner, right_expr, left_expr)
            } else if let ExpressionInner::BooleanLiteral {
                loc: lit_loc,
                inner,
            } = &**right_expr
            {
                (lit_loc, inner, left_expr, right_expr)
            } else {
                unreachable!()
            };
            let refi = RefinementKind::SingletonBoolR {
                loc: lit_loc.dupe(),
                sense,
                lit: inner.value,
            };
            on_literal_test(strict, sense, loc, expr, refi, other)
        }

        // string equality - StringLiteral
        (left_expr, right_expr)
            if matches!(&**left_expr, ExpressionInner::StringLiteral { .. })
                || matches!(&**right_expr, ExpressionInner::StringLiteral { .. }) =>
        {
            let (lit_loc, inner, expr, other) = if let ExpressionInner::StringLiteral {
                loc: lit_loc,
                inner,
            } = &**left_expr
            {
                (lit_loc, inner, right_expr, left_expr)
            } else if let ExpressionInner::StringLiteral {
                loc: lit_loc,
                inner,
            } = &**right_expr
            {
                (lit_loc, inner, left_expr, right_expr)
            } else {
                unreachable!()
            };
            let refi = RefinementKind::SingletonStrR {
                loc: lit_loc.dupe(),
                sense,
                lit: inner.value.dupe(),
            };
            on_literal_test(strict, sense, loc, expr, refi, other)
        }

        // string equality - TemplateLiteral (single quasi, no expressions)
        (left_expr, right_expr)
            if matches!(
                &**right_expr,
                ExpressionInner::TemplateLiteral { inner, .. }
                    if inner.quasis.len() == 1 && inner.expressions.is_empty()
            ) =>
        {
            let ExpressionInner::TemplateLiteral {
                loc: lit_loc,
                inner,
            } = &**right_expr
            else {
                unreachable!()
            };
            let refi = RefinementKind::SingletonStrR {
                loc: lit_loc.dupe(),
                sense,
                lit: inner.quasis[0].value.cooked.dupe(),
            };
            on_literal_test(strict, sense, loc, left_expr, refi, right_expr)
        }
        (left_expr, right_expr)
            if matches!(
                &**left_expr,
                ExpressionInner::TemplateLiteral { inner, .. }
                    if inner.quasis.len() == 1 && inner.expressions.is_empty()
            ) =>
        {
            let ExpressionInner::TemplateLiteral {
                loc: lit_loc,
                inner,
            } = &**left_expr
            else {
                unreachable!()
            };
            let refi = RefinementKind::SingletonStrR {
                loc: lit_loc.dupe(),
                sense,
                lit: inner.quasis[0].value.cooked.dupe(),
            };
            on_literal_test(strict, sense, loc, right_expr, refi, left_expr)
        }

        // number equality
        (other, expr) if ast_utils::is_number_literal(other) => {
            let lit_loc = other.loc().dupe();
            let raw = extract_number_literal(other);
            let refi = RefinementKind::SingletonNumR {
                loc: lit_loc,
                sense,
                lit: raw,
            };
            on_literal_test(strict, sense, loc, expr, refi, other)
        }
        (expr, other) if ast_utils::is_number_literal(other) => {
            let lit_loc = other.loc().dupe();
            let raw = extract_number_literal(other);
            let refi = RefinementKind::SingletonNumR {
                loc: lit_loc,
                sense,
                lit: raw,
            };
            on_literal_test(strict, sense, loc, expr, refi, other)
        }

        // bigint equality
        (other, expr) if ast_utils::is_bigint_literal(other) => {
            let lit_loc = other.loc().dupe();
            let raw = extract_bigint_literal(other);
            let refi = RefinementKind::SingletonBigIntR {
                loc: lit_loc,
                sense,
                lit: raw,
            };
            on_literal_test(strict, sense, loc, expr, refi, other)
        }
        (expr, other) if ast_utils::is_bigint_literal(other) => {
            let lit_loc = other.loc().dupe();
            let raw = extract_bigint_literal(other);
            let refi = RefinementKind::SingletonBigIntR {
                loc: lit_loc,
                sense,
                lit: raw,
            };
            on_literal_test(strict, sense, loc, expr, refi, other)
        }

        // expr op null
        (other, expr) if matches!(&**other, ExpressionInner::NullLiteral { .. }) => {
            on_null_test(sense, strict, loc, expr, other)
        }
        (expr, other) if matches!(&**other, ExpressionInner::NullLiteral { .. }) => {
            on_null_test(sense, strict, loc, expr, other)
        }

        // expr op undefined
        (undefined, expr) if matches!(&**undefined, ExpressionInner::Identifier { inner, .. } if inner.name == "undefined") => {
            on_void_test(sense, strict, true, loc, expr, undefined)
        }
        (expr, undefined) if matches!(&**undefined, ExpressionInner::Identifier { inner, .. } if inner.name == "undefined") => {
            on_void_test(sense, strict, true, loc, expr, undefined)
        }

        // expr op void(...)
        (other, expr) if matches!(&**other, ExpressionInner::Unary { inner, .. } if matches!(inner.operator, UnaryOperator::Void)) => {
            on_void_test(sense, strict, false, loc, expr, other)
        }
        (expr, other) if matches!(&**other, ExpressionInner::Unary { inner, .. } if matches!(inner.operator, UnaryOperator::Void)) => {
            on_void_test(sense, strict, false, loc, expr, other)
        }

        // member expression
        (expr, other) if matches!(&**expr, ExpressionInner::Member { .. }) => {
            on_member_eq_other(expr, other)
        }
        (other, expr)
            if matches!(&**expr, ExpressionInner::Member { .. }) && !is_switch_cond_context =>
        {
            on_other_eq_member(other, expr)
        }

        // fallthrough
        _ => on_other_eq_test(left, right),
    }
}
