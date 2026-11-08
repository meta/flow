/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::borrow::Cow;
use std::ops::Deref;
use std::sync::Arc;

use dupe::Dupe;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;

use crate::ast::expression::ExpressionInner;
use crate::ast::*;

pub fn fold_bindings_of_pattern<A, M: Dupe, T: Dupe, F: FnMut(A, &Identifier<M, T>) -> A>(
    mut acc: A,
    pattern: &pattern::Pattern<M, T>,
    f: &mut F,
) -> A {
    match pattern {
        pattern::Pattern::Object { loc: _, inner } => {
            for p in inner.properties.iter() {
                acc = match p {
                    pattern::object::Property::NormalProperty(
                        pattern::object::NormalProperty { pattern, .. },
                    ) => fold_bindings_of_pattern(acc, pattern, f),
                    pattern::object::Property::RestElement(pattern::RestElement {
                        argument,
                        ..
                    }) => fold_bindings_of_pattern(acc, argument, f),
                }
            }
            acc
        }
        pattern::Pattern::Array { loc: _, inner } => {
            for e in inner.elements.iter() {
                acc = match e {
                    pattern::array::Element::Hole(_) => acc,
                    pattern::array::Element::NormalElement(pattern::array::NormalElement {
                        argument,
                        ..
                    }) => fold_bindings_of_pattern(acc, argument, f),
                    pattern::array::Element::RestElement(pattern::RestElement {
                        argument, ..
                    }) => fold_bindings_of_pattern(acc, argument, f),
                }
            }
            acc
        }
        pattern::Pattern::Identifier { loc: _, inner } => f(acc, &inner.name),
        pattern::Pattern::Expression { .. } => acc,
    }
}

pub fn fold_bindings_of_variable_declarations<
    A,
    M: Dupe,
    T: Dupe,
    F: FnMut(bool, A, &Identifier<M, T>) -> A,
>(
    mut acc: A,
    declarations: &[statement::variable::Declarator<M, T>],
    f: &mut F,
) -> A {
    for statement::variable::Declarator {
        loc: _,
        id: pattern,
        init: _,
    } in declarations
    {
        let has_annotation = match pattern {
            pattern::Pattern::Object { loc: _, inner } => {
                matches!(inner.annot, types::AnnotationOrHint::Available(_))
            }
            pattern::Pattern::Array { loc: _, inner } => {
                matches!(inner.annot, types::AnnotationOrHint::Available(_))
            }
            pattern::Pattern::Identifier { loc: _, inner } => {
                matches!(inner.annot, types::AnnotationOrHint::Available(_))
            }
            pattern::Pattern::Expression { .. } => false,
        };
        acc = fold_bindings_of_pattern(acc, pattern, &mut |acc, id| f(has_annotation, acc, id));
    }
    acc
}

pub fn pattern_has_binding<M: Dupe, T: Dupe>(pattern: &pattern::Pattern<M, T>) -> bool {
    match pattern {
        pattern::Pattern::Object { loc: _, inner } => inner.properties.iter().any(|p| match p {
            pattern::object::Property::NormalProperty(normal_property) => {
                pattern_has_binding(&normal_property.pattern)
            }
            pattern::object::Property::RestElement(rest_element) => {
                pattern_has_binding(&rest_element.argument)
            }
        }),
        pattern::Pattern::Array { loc: _, inner } => inner.elements.iter().any(|e| match e {
            pattern::array::Element::NormalElement(normal_element) => {
                pattern_has_binding(&normal_element.argument)
            }
            pattern::array::Element::RestElement(rest_element) => {
                pattern_has_binding(&rest_element.argument)
            }
            pattern::array::Element::Hole(_) => false,
        }),
        pattern::Pattern::Identifier { .. } => true,
        pattern::Pattern::Expression { .. } => false,
    }
}

pub fn pattern_has_type_annotation<M: Dupe, T: Dupe>(pattern: &pattern::Pattern<M, T>) -> bool {
    match pattern {
        pattern::Pattern::Identifier { loc: _, inner } => {
            matches!(inner.annot, types::AnnotationOrHint::Available(_))
        }
        pattern::Pattern::Object { loc: _, inner } => {
            matches!(inner.annot, types::AnnotationOrHint::Available(_))
        }
        pattern::Pattern::Array { loc: _, inner } => {
            matches!(inner.annot, types::AnnotationOrHint::Available(_))
        }
        pattern::Pattern::Expression { .. } => false,
    }
}

pub fn pattern_optional<M: Dupe, T: Dupe>(pattern: &pattern::Pattern<M, T>) -> bool {
    match pattern {
        pattern::Pattern::Object { loc: _, inner } => inner.optional,
        pattern::Pattern::Array { loc: _, inner } => inner.optional,
        pattern::Pattern::Identifier { loc: _, inner } => inner.optional,
        _ => false,
    }
}

pub fn pattern_annot<M: Dupe, T: Dupe>(
    pattern: &pattern::Pattern<M, T>,
) -> &types::AnnotationOrHint<M, T> {
    match pattern {
        pattern::Pattern::Object { inner, .. } => &inner.annot,
        pattern::Pattern::Array { inner, .. } => &inner.annot,
        pattern::Pattern::Identifier { inner, .. } => &inner.annot,
        pattern::Pattern::Expression { .. } => panic!("Expression patterns have no annotation"),
    }
}

pub fn function_type_param_parts<M: Dupe, T: Dupe>(
    param: &types::function::ParamKind<M, T>,
) -> (Option<&Identifier<M, T>>, &types::Type<M, T>, bool) {
    match param {
        types::function::ParamKind::Anonymous(annot) => (None, annot, false),
        types::function::ParamKind::Labeled {
            name,
            annot,
            optional,
        } => (Some(name), annot, *optional),
        types::function::ParamKind::Destructuring(pattern) => match pattern_annot(pattern) {
            types::AnnotationOrHint::Available(annot) => {
                (None, &annot.annotation, pattern_optional(pattern))
            }
            types::AnnotationOrHint::Missing(_) => {
                panic!("Destructuring function type param must have annotation")
            }
        },
    }
}

pub fn string_of_variable_kind(kind: VariableKind) -> &'static str {
    kind.as_str()
}

pub fn match_pattern_has_binding<M: Dupe, T: Dupe>(
    pattern: &match_pattern::MatchPattern<M, T>,
) -> bool {
    fn object_pattern_has_binding<M: Dupe, T: Dupe>(
        pattern: &match_pattern::ObjectPattern<M, T>,
    ) -> bool {
        rest_has_binding(&pattern.rest)
            || pattern.properties.iter().any(|p| match p {
                match_pattern::object_pattern::Property::Valid { loc: _, property } => {
                    match_pattern_has_binding(&property.pattern)
                }
                match_pattern::object_pattern::Property::InvalidShorthand { .. } => false,
            })
    }
    fn rest_has_binding<M: Dupe, T: Dupe>(rest: &Option<match_pattern::RestPattern<M, T>>) -> bool {
        matches!(
            rest,
            Some(match_pattern::RestPattern {
                loc: _,
                argument: Some(_),
                comments: _,
            })
        )
    }
    match pattern {
        match_pattern::MatchPattern::WildcardPattern { .. }
        | match_pattern::MatchPattern::NumberPattern { .. }
        | match_pattern::MatchPattern::BigIntPattern { .. }
        | match_pattern::MatchPattern::StringPattern { .. }
        | match_pattern::MatchPattern::BooleanPattern { .. }
        | match_pattern::MatchPattern::NullPattern { .. }
        | match_pattern::MatchPattern::UnaryPattern { .. }
        | match_pattern::MatchPattern::IdentifierPattern { .. }
        | match_pattern::MatchPattern::MemberPattern { .. } => false,
        match_pattern::MatchPattern::BindingPattern { .. } => true,
        match_pattern::MatchPattern::ObjectPattern { loc: _, inner } => {
            object_pattern_has_binding(inner)
        }
        match_pattern::MatchPattern::ArrayPattern { loc: _, inner } => {
            rest_has_binding(&inner.rest)
                || inner
                    .elements
                    .iter()
                    .any(|e| match_pattern_has_binding(&e.pattern))
        }
        match_pattern::MatchPattern::InstancePattern { loc: _, inner } => {
            object_pattern_has_binding(&inner.as_ref().properties.1)
        }
        match_pattern::MatchPattern::OrPattern { loc: _, inner } => {
            inner.patterns.iter().any(match_pattern_has_binding)
        }
        match_pattern::MatchPattern::AsPattern { .. } => true,
    }
}

pub fn partition_directives<M: Dupe, T: Dupe>(
    statements: Vec<statement::Statement<M, T>>,
) -> (
    Vec<statement::Statement<M, T>>,
    Vec<statement::Statement<M, T>>,
) {
    let mut directives = Vec::new();
    let mut non_directives = Vec::new();
    for statement in statements {
        match statement.deref() {
            statement::StatementInner::Expression { loc: _, inner }
                if inner.directive.is_some() =>
            {
                directives.push(statement)
            }
            _ => {
                non_directives.push(statement);
            }
        }
    }
    (directives, non_directives)
}

pub fn hoist_function_and_component_declarations<M: Dupe, T: Dupe>(
    statements: &[statement::Statement<M, T>],
) -> Cow<'_, [statement::Statement<M, T>]> {
    fn should_hoist<M: Dupe, T: Dupe>(statement: &statement::Statement<M, T>) -> bool {
        match statement.deref() {
            // function f() {} / component F() {}
            statement::StatementInner::FunctionDeclaration { inner, .. } if inner.id.is_some() => {
                true
            }
            statement::StatementInner::ComponentDeclaration { .. } => true,
            // export function f() {} / export component F() {}
            statement::StatementInner::ExportNamedDeclaration { inner, .. } => {
                match &inner.declaration {
                    Some(decl) => match &**decl {
                        statement::StatementInner::FunctionDeclaration { inner, .. }
                            if inner.id.is_some() =>
                        {
                            true
                        }
                        statement::StatementInner::ComponentDeclaration { .. } => true,
                        _ => false,
                    },
                    None => false,
                }
            }
            // export default function f() {} / export default component F()
            statement::StatementInner::ExportDefaultDeclaration { inner, .. } => {
                match &inner.declaration {
                    statement::export_default_declaration::Declaration::Declaration(decl) => {
                        match &**decl {
                            statement::StatementInner::FunctionDeclaration { inner, .. }
                                if inner.id.is_some() =>
                            {
                                true
                            }
                            statement::StatementInner::ComponentDeclaration { .. } => true,
                            _ => false,
                        }
                    }
                    _ => false,
                }
            }
            // TODO(jmbrown): Hoist declared components
            // declare function f(): void;
            statement::StatementInner::DeclareFunction { .. } => true,
            // declare export function f(): void
            statement::StatementInner::DeclareExportDeclaration { inner, .. } => {
                matches!(
                    &inner.declaration,
                    Some(statement::declare_export_declaration::Declaration::Function { .. })
                )
            }
            _ => false,
        }
    }

    if !statements.iter().any(should_hoist) {
        return Cow::Borrowed(statements);
    }

    let (function_and_component_declarations, mut other_stmts): (Vec<_>, Vec<_>) =
        statements.iter().cloned().partition(should_hoist);
    let mut all = function_and_component_declarations;
    all.append(&mut other_stmts);
    Cow::Owned(all)
}

fn negate_raw_lit(raw: FlowSmolStr) -> String {
    let raw_len = raw.len();
    if raw_len > 0 && raw.starts_with('-') {
        raw[1..raw_len].to_string()
    } else {
        format!("-{}", raw)
    }
}

pub fn negate_number_literal((value, raw): (f64, FlowSmolStr)) -> (f64, FlowSmolStr) {
    (-value, FlowSmolStr::new(negate_raw_lit(raw)))
}

pub fn negate_bigint_literal(
    (value, raw): (Option<i64>, FlowSmolStr),
) -> (Option<i64>, FlowSmolStr) {
    match value {
        None => (None, raw),
        Some(value) => (Some(-value), FlowSmolStr::new(negate_raw_lit(raw))),
    }
}

pub fn is_number_literal<M: Dupe, T: Dupe>(node: &expression::Expression<M, T>) -> bool {
    match node.deref() {
        ExpressionInner::NumberLiteral { .. } => true,
        ExpressionInner::Unary { inner, .. }
            if matches!(inner.operator, expression::UnaryOperator::Minus)
                && matches!(&*inner.argument, ExpressionInner::NumberLiteral { .. }) =>
        {
            true
        }
        _ => false,
    }
}

pub fn extract_number_literal<M: Dupe, T: Dupe>(
    node: &expression::Expression<M, T>,
) -> Option<(f64, FlowSmolStr)> {
    match node.deref() {
        ExpressionInner::NumberLiteral { inner, .. } => Some((inner.value, inner.raw.to_owned())),
        ExpressionInner::Unary { inner, .. }
            if matches!(inner.operator, expression::UnaryOperator::Minus) =>
        {
            if let ExpressionInner::NumberLiteral {
                inner: num_inner, ..
            } = &*inner.argument
            {
                Some(negate_number_literal((
                    num_inner.value,
                    num_inner.raw.to_owned(),
                )))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn is_bigint_literal<M: Dupe, T: Dupe>(node: &expression::Expression<M, T>) -> bool {
    match node.deref() {
        ExpressionInner::BigIntLiteral { .. } => true,
        ExpressionInner::Unary { inner, .. }
            if matches!(inner.operator, expression::UnaryOperator::Minus)
                && matches!(
                    inner.argument.deref(),
                    ExpressionInner::BigIntLiteral { .. }
                ) =>
        {
            true
        }
        _ => false,
    }
}

pub fn extract_bigint_literal<M: Dupe, T: Dupe>(
    node: &expression::Expression<M, T>,
) -> Option<(Option<i64>, FlowSmolStr)> {
    match node.deref() {
        ExpressionInner::BigIntLiteral { inner, .. } => Some((inner.value, inner.raw.to_owned())),
        ExpressionInner::Unary { inner, .. }
            if matches!(inner.operator, expression::UnaryOperator::Minus) =>
        {
            if let ExpressionInner::BigIntLiteral {
                inner: bigint_inner,
                ..
            } = inner.argument.deref()
            {
                Some(negate_bigint_literal((
                    bigint_inner.value,
                    bigint_inner.raw.to_owned(),
                )))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn is_call_to_require<M: Dupe, T: Dupe>(callee: &expression::Expression<M, T>) -> bool {
    match callee.deref() {
        ExpressionInner::Identifier { inner, .. } => inner.name == "require",
        _ => false,
    }
}

pub fn is_call_to_is_array<M: Dupe, T: Dupe>(callee: &expression::Expression<M, T>) -> bool {
    match callee.deref() {
        ExpressionInner::Member { inner, .. } => {
            if let ExpressionInner::Identifier {
                inner: object_inner,
                ..
            } = inner.object.deref()
            {
                if let expression::member::Property::PropertyIdentifier(id) = &inner.property {
                    return object_inner.name == "Array" && id.name == "isArray";
                }
            }
            false
        }
        _ => false,
    }
}

pub fn is_call_to_object_dot_freeze<M: Dupe, T: Dupe>(
    callee: &expression::Expression<M, T>,
) -> bool {
    match callee.deref() {
        ExpressionInner::Member { inner, .. } => {
            if let ExpressionInner::Identifier {
                inner: object_inner,
                ..
            } = inner.object.deref()
            {
                if let expression::member::Property::PropertyIdentifier(id) = &inner.property {
                    return object_inner.name == "Object" && id.name == "freeze";
                }
            }
            false
        }
        _ => false,
    }
}

pub fn get_call_to_object_dot_freeze_arg<'a, M: Dupe, T: Dupe>(
    callee: &expression::Expression<M, T>,
    targs: &Option<expression::CallTypeArgs<M, T>>,
    args: &'a expression::ArgList<M, T>,
) -> Option<(&'a T, &'a expression::Object<M, T>)> {
    match (targs, args) {
        (
            None,
            expression::ArgList {
                loc: _,
                arguments,
                comments: _,
            },
        ) if is_call_to_object_dot_freeze(callee) => {
            if let Some(expression::ExpressionOrSpread::Expression(expr)) = arguments.first() {
                if let ExpressionInner::Object {
                    loc: obj_loc,
                    inner,
                } = expr.deref()
                {
                    Some((obj_loc, inner.as_ref()))
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Whether a call `Object.m(...)` is one that the checker types itself, instead of through the
/// library signature of `Object.m` (see `static_method_call_object`).
pub fn is_specially_typed_object_call<M: Dupe, T: Dupe>(
    callee: &expression::Expression<M, T>,
    has_targs: bool,
    arguments: &[expression::ExpressionOrSpread<M, T>],
) -> bool {
    use expression::ExpressionOrSpread::Expression;
    let ExpressionInner::Member { inner, .. } = callee.deref() else {
        return false;
    };
    let (
        ExpressionInner::Identifier {
            inner: object_inner,
            ..
        },
        expression::member::Property::PropertyIdentifier(id),
    ) = (inner.object.deref(), &inner.property)
    else {
        return false;
    };
    if object_inner.name != "Object" {
        return false;
    }
    let is_object_literal =
        |e: &expression::Expression<M, T>| matches!(e.deref(), ExpressionInner::Object { .. });
    match (id.name.as_str(), arguments) {
        (
            "assign"
            | "create"
            | "getOwnPropertyNames"
            | "getPrototypeOf"
            | "keys"
            | "defineProperty"
            | "defineProperties"
            | "freeze",
            _,
        ) if has_targs => true,
        (_, _) if has_targs => false,
        ("assign", _) => true,
        (
            "getPrototypeOf" | "create" | "getOwnPropertyNames" | "keys" | "values" | "entries",
            [Expression(_)],
        ) => true,
        ("create" | "defineProperties", [Expression(_), Expression(props)]) => {
            is_object_literal(props)
        }
        ("defineProperty", [Expression(_), Expression(key), Expression(_)]) => {
            matches!(key.deref(), ExpressionInner::StringLiteral { .. })
        }
        ("freeze", [Expression(o)]) => is_object_literal(o),
        _ => false,
    }
}

pub fn is_module_dot_exports<M: Dupe, T: Dupe>(callee: &expression::Expression<M, T>) -> bool {
    match callee.deref() {
        ExpressionInner::Member { inner, .. } => {
            if let ExpressionInner::Identifier {
                inner: object_inner,
                ..
            } = inner.object.deref()
            {
                if let expression::member::Property::PropertyIdentifier(id) = &inner.property {
                    return object_inner.name == "module" && id.name == "exports";
                }
            }
            false
        }
        _ => false,
    }
}

pub fn well_known_symbol_name<M: Dupe, T: Dupe>(
    expr: &expression::Expression<M, T>,
) -> Option<&'static str> {
    match expr.deref() {
        ExpressionInner::Member { inner, .. } => {
            if let ExpressionInner::Identifier {
                inner: object_inner,
                ..
            } = inner.object.deref()
            {
                if object_inner.name == "Symbol" {
                    if let expression::member::Property::PropertyIdentifier(id) = &inner.property {
                        return match id.name.as_str() {
                            "iterator" => Some("@@iterator"),
                            "asyncIterator" => Some("@@asyncIterator"),
                            "dispose" => Some("@@dispose"),
                            "asyncDispose" => Some("@@asyncDispose"),
                            _ => None,
                        };
                    }
                }
            }
            None
        }
        _ => None,
    }
}

pub fn get_call_to_jest_module_mocking_fn<'a, 'b, M: Dupe, T: Dupe>(
    callee: &'a expression::Expression<M, T>,
    arguments: &'b expression::ArgList<M, T>,
) -> Option<(&'a T, &'b T, &'b FlowSmolStr)> {
    let ExpressionInner::Member {
        inner: member_inner,
        ..
    } = callee.deref()
    else {
        return None;
    };
    let ExpressionInner::Identifier {
        loc: jest_loc,
        inner: object_inner,
    } = member_inner.object.deref()
    else {
        return None;
    };
    let expression::member::Property::PropertyIdentifier(property_id) = &member_inner.property
    else {
        return None;
    };
    if object_inner.name != "jest"
        || !matches!(
            property_id.name.as_str(),
            // See https://jestjs.io/docs/jest-object#mock-modules
            "createMockFromModule"
                | "mock"
                | "unmock"
                | "deepUnmock"
                | "doMock"
                | "dontMock"
                | "setMock"
                | "requireActual"
                | "requireMock"
        )
    {
        return None;
    }
    if let Some(expression::ExpressionOrSpread::Expression(arg)) = arguments.arguments.first() {
        match arg.deref() {
            ExpressionInner::StringLiteral {
                loc: source_loc,
                inner,
            } => Some((jest_loc, source_loc, &inner.value)),
            ExpressionInner::TemplateLiteral {
                loc: source_loc,
                inner,
            } if inner.expressions.is_empty() && inner.quasis.len() == 1 => {
                Some((jest_loc, source_loc, &inner.quasis[0].value.cooked))
            }
            _ => None,
        }
    } else {
        None
    }
}

pub fn is_super_member_access<M: Dupe, T: Dupe>(member: &expression::Member<M, T>) -> bool {
    matches!(member.object.deref(), ExpressionInner::Super { .. })
}

/// The declaration construct whose body is being validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationContext {
    DeclareModule,
    DeclareNamespace,
    DeclareGlobal,
}

pub fn is_ambient_variable_declaration<M: Dupe, T: Dupe>(
    declaration: &statement::VariableDeclaration<M, T>,
) -> bool {
    declaration
        .declarations
        .iter()
        .all(|decl| decl.init.is_none() && pattern_has_type_annotation(&decl.id))
}

pub fn acceptable_statement_in_declaration_context<M: Dupe, T: Dupe>(
    context: DeclarationContext,
    x: &statement::Statement<M, T>,
) -> Result<(), &'static str> {
    match x.deref() {
        statement::StatementInner::Block { .. } => Err("block"),
        statement::StatementInner::Break { .. } => Err("break"),
        statement::StatementInner::ClassDeclaration { inner, .. } => {
            use class::BodyElement;
            let is_ambient = inner.class_decorators.is_empty()
                && inner.body.body.iter().all(|element| match element {
                    BodyElement::Method(_) | BodyElement::StaticBlock(_) => false,
                    BodyElement::Property(property) => {
                        !matches!(property.value, class::property::Value::Initialized(_))
                    }
                    BodyElement::PrivateField(field) => {
                        !matches!(field.value, class::property::Value::Initialized(_))
                    }
                    BodyElement::DeclareMethod(_)
                    | BodyElement::AbstractMethod(_)
                    | BodyElement::AbstractProperty(_)
                    | BodyElement::IndexSignature(_) => true,
                });
            if is_ambient {
                Ok(())
            } else {
                Err("class declaration")
            }
        }
        statement::StatementInner::ComponentDeclaration { inner, .. } if inner.body.is_none() => {
            Ok(())
        }
        statement::StatementInner::ComponentDeclaration { .. } => Err("component declaration"),
        statement::StatementInner::Continue { .. } => Err("continue"),
        statement::StatementInner::Debugger { .. } => Err("debugger"),
        statement::StatementInner::DoWhile { .. } => Err("do while"),
        statement::StatementInner::ExportDefaultDeclaration { .. } => Err("export default"),
        statement::StatementInner::ExportNamedDeclaration { inner, .. } => match context {
            DeclarationContext::DeclareGlobal => Err("export declaration"),
            _ => {
                if let Some(declaration) = &inner.declaration {
                    acceptable_statement_in_declaration_context(context, declaration)
                } else if context == DeclarationContext::DeclareNamespace && inner.source.is_none()
                    || inner.export_kind == statement::ExportKind::ExportType
                    || matches!(
                        &inner.specifiers,
                        Some(statement::export_named_declaration::Specifier::ExportSpecifiers(specs))
                            if !export_specifiers_has_value_export(inner.export_kind, specs)
                    )
                {
                    Ok(())
                } else {
                    Err("value export")
                }
            }
        },
        statement::StatementInner::Expression { .. } => Err("expression"),
        statement::StatementInner::For { .. } => Err("for"),
        statement::StatementInner::ForIn { .. } => Err("for in"),
        statement::StatementInner::ForOf { .. } => Err("for of"),
        statement::StatementInner::FunctionDeclaration { .. } => Err("function declaration"),
        statement::StatementInner::If { .. } => Err("if"),
        statement::StatementInner::Labeled { .. } => Err("labeled"),
        statement::StatementInner::Match { .. } => Err("match"),
        statement::StatementInner::RecordDeclaration { .. } => Err("record declaration"),
        statement::StatementInner::Return { .. } => Err("return"),
        statement::StatementInner::Switch { .. } => Err("switch"),
        statement::StatementInner::Throw { .. } => Err("throw"),
        statement::StatementInner::Try { .. } => Err("try"),
        statement::StatementInner::VariableDeclaration { inner, .. } => {
            if is_ambient_variable_declaration(inner) {
                Ok(())
            } else {
                Err("variable declaration")
            }
        }
        statement::StatementInner::While { .. } => Err("while"),
        statement::StatementInner::With { .. } => Err("with"),
        statement::StatementInner::ImportDeclaration { .. } => {
            if context == DeclarationContext::DeclareModule {
                Ok(())
            } else {
                Err("import declaration")
            }
        }
        statement::StatementInner::DeclareModuleExports { .. } => {
            if context == DeclarationContext::DeclareModule {
                Ok(())
            } else {
                Err("declare module.exports")
            }
        }
        statement::StatementInner::DeclareExportDeclaration { .. }
            if context == DeclarationContext::DeclareGlobal =>
        {
            Err("declare export")
        }
        statement::StatementInner::ExportAssignment { .. }
            if context == DeclarationContext::DeclareGlobal =>
        {
            Err("export assignment")
        }
        statement::StatementInner::NamespaceExportDeclaration { .. }
            if context == DeclarationContext::DeclareGlobal =>
        {
            Err("namespace export")
        }
        statement::StatementInner::ImportEqualsDeclaration { .. }
            if context == DeclarationContext::DeclareGlobal =>
        {
            Err("import-equals declaration")
        }
        statement::StatementInner::DeclareModule { inner, .. }
            if context == DeclarationContext::DeclareGlobal
                && matches!(inner.id, statement::declare_module::Id::Literal(_)) =>
        {
            Err("string-named module declaration")
        }
        statement::StatementInner::DeclareNamespace { inner, .. }
            if context == DeclarationContext::DeclareGlobal
                && matches!(inner.id, statement::declare_namespace::Id::Global(_)) =>
        {
            Err("nested declare global")
        }
        statement::StatementInner::DeclareClass { .. }
        | statement::StatementInner::DeclareComponent { .. }
        | statement::StatementInner::DeclareEnum { .. }
        | statement::StatementInner::DeclareExportDeclaration { .. }
        | statement::StatementInner::DeclareFunction { .. }
        | statement::StatementInner::DeclareInterface { .. }
        | statement::StatementInner::DeclareModule { .. }
        | statement::StatementInner::DeclareNamespace { .. }
        | statement::StatementInner::DeclareOpaqueType { .. }
        | statement::StatementInner::DeclareTypeAlias { .. }
        | statement::StatementInner::DeclareVariable { .. }
        | statement::StatementInner::Empty { .. }
        | statement::StatementInner::EnumDeclaration { .. }
        | statement::StatementInner::ExportAssignment { .. }
        | statement::StatementInner::NamespaceExportDeclaration { .. }
        | statement::StatementInner::ImportEqualsDeclaration { .. }
        | statement::StatementInner::InterfaceDeclaration { .. }
        | statement::StatementInner::OpaqueType { .. }
        | statement::StatementInner::TypeAlias { .. } => Ok(()),
    }
}

pub fn is_type_only_declaration_statement<M: Dupe, T: Dupe>(
    stmt: &statement::Statement<M, T>,
) -> bool {
    match stmt.deref() {
        statement::StatementInner::DeclareInterface { .. }
        | statement::StatementInner::DeclareOpaqueType { .. }
        | statement::StatementInner::DeclareTypeAlias { .. }
        | statement::StatementInner::Empty { .. }
        | statement::StatementInner::InterfaceDeclaration { .. }
        | statement::StatementInner::OpaqueType { .. }
        | statement::StatementInner::TypeAlias { .. } => true,
        statement::StatementInner::DeclareExportDeclaration { inner, .. } => {
            match &inner.declaration {
                Some(
                    statement::declare_export_declaration::Declaration::NamedType { .. }
                    | statement::declare_export_declaration::Declaration::NamedOpaqueType { .. }
                    | statement::declare_export_declaration::Declaration::Interface { .. },
                ) => true,
                Some(statement::declare_export_declaration::Declaration::Namespace {
                    loc: _,
                    declaration,
                }) => declaration
                    .body
                    .1
                    .body
                    .iter()
                    .all(is_type_only_declaration_statement),
                _ => false,
            }
        }
        statement::StatementInner::DeclareNamespace { inner, .. } => inner
            .body
            .1
            .body
            .iter()
            .all(is_type_only_declaration_statement),
        statement::StatementInner::ExportNamedDeclaration { inner, .. } => match &inner.declaration
        {
            Some(declaration) => is_type_only_declaration_statement(declaration),
            None => match &inner.specifiers {
                Some(statement::export_named_declaration::Specifier::ExportSpecifiers(specs)) => {
                    !export_specifiers_has_value_export(inner.export_kind, specs)
                }
                _ => inner.export_kind == statement::ExportKind::ExportType,
            },
        },
        statement::StatementInner::Block { .. }
        | statement::StatementInner::Break { .. }
        | statement::StatementInner::ClassDeclaration { .. }
        | statement::StatementInner::ComponentDeclaration { .. }
        | statement::StatementInner::Continue { .. }
        | statement::StatementInner::Debugger { .. }
        | statement::StatementInner::DoWhile { .. }
        | statement::StatementInner::EnumDeclaration { .. }
        | statement::StatementInner::ExportAssignment { .. }
        | statement::StatementInner::NamespaceExportDeclaration { .. }
        | statement::StatementInner::ExportDefaultDeclaration { .. }
        | statement::StatementInner::Expression { .. }
        | statement::StatementInner::For { .. }
        | statement::StatementInner::ForIn { .. }
        | statement::StatementInner::ForOf { .. }
        | statement::StatementInner::FunctionDeclaration { .. }
        | statement::StatementInner::If { .. }
        | statement::StatementInner::ImportEqualsDeclaration { .. }
        | statement::StatementInner::Labeled { .. }
        | statement::StatementInner::Match { .. }
        | statement::StatementInner::RecordDeclaration { .. }
        | statement::StatementInner::Return { .. }
        | statement::StatementInner::Switch { .. }
        | statement::StatementInner::Throw { .. }
        | statement::StatementInner::Try { .. }
        | statement::StatementInner::VariableDeclaration { .. }
        | statement::StatementInner::While { .. }
        | statement::StatementInner::With { .. }
        | statement::StatementInner::ImportDeclaration { .. }
        | statement::StatementInner::DeclareClass { .. }
        | statement::StatementInner::DeclareComponent { .. }
        | statement::StatementInner::DeclareEnum { .. }
        | statement::StatementInner::DeclareFunction { .. }
        | statement::StatementInner::DeclareModule { .. }
        | statement::StatementInner::DeclareModuleExports { .. }
        | statement::StatementInner::DeclareVariable { .. } => false,
    }
}

pub fn loc_of_statement<M: Dupe, T: Dupe>(stmt: &statement::Statement<M, T>) -> &M {
    stmt.loc()
}

pub fn loc_of_expression<M: Dupe, T: Dupe>(expr: &expression::Expression<M, T>) -> &T {
    expr.loc()
}

pub fn loc_of_pattern<M: Dupe, T: Dupe>(pat: &pattern::Pattern<M, T>) -> &T {
    pat.loc()
}

pub fn loc_of_ident<M: Dupe, T: Dupe>(id: &Identifier<M, T>) -> &T {
    &id.loc
}

pub fn name_of_ident<M: Dupe, T: Dupe>(id: &Identifier<M, T>) -> &FlowSmolStr {
    &id.name
}

pub fn source_of_ident<M: Dupe, T: Dupe>(id: &Identifier<M, T>) -> (&T, &FlowSmolStr) {
    (&id.loc, &id.name)
}

pub fn ident_of_source<M: Dupe, T: Dupe>(
    comments: Option<Syntax<M, ()>>,
    loc: T,
    name: FlowSmolStr,
) -> Identifier<M, T> {
    Identifier::new(IdentifierInner {
        loc,
        name,
        comments,
    })
}

pub fn mk_comments<M: Dupe>(
    leading: Option<Arc<[Comment<M>]>>,
    trailing: Option<Arc<[Comment<M>]>>,
) -> Syntax<M, ()> {
    let leading = leading.unwrap_or_else(|| Arc::from([]));
    let trailing = trailing.unwrap_or_else(|| Arc::from([]));
    Syntax {
        leading,
        trailing,
        internal: (),
    }
}

pub fn mk_comments_opt<M: Dupe>(
    leading: Option<Arc<[Comment<M>]>>,
    trailing: Option<Arc<[Comment<M>]>>,
) -> Option<Syntax<M, ()>> {
    fn is_empty<M: Dupe>(comments: &Option<Arc<[Comment<M>]>>) -> bool {
        match comments {
            None => true,
            Some(comments) => comments.is_empty(),
        }
    }

    if is_empty(&leading) && is_empty(&trailing) {
        None
    } else {
        Some(mk_comments(leading, trailing))
    }
}

pub fn mk_comments_opt_from_vecs<M: Dupe>(
    leading: Vec<Comment<M>>,
    trailing: Vec<Comment<M>>,
) -> Option<Syntax<M, ()>> {
    if leading.is_empty() && trailing.is_empty() {
        None
    } else {
        mk_comments_opt(Some(leading.into()), Some(trailing.into()))
    }
}

pub fn mk_comments_with_internal_opt<M: Dupe>(
    leading: Option<Arc<[Comment<M>]>>,
    trailing: Option<Arc<[Comment<M>]>>,
    internal: Option<Arc<[Comment<M>]>>,
) -> Option<Syntax<M, Arc<[Comment<M>]>>> {
    let leading = leading.unwrap_or_else(|| Arc::from([]));
    let trailing = trailing.unwrap_or_else(|| Arc::from([]));
    let internal = internal.unwrap_or_else(|| Arc::from([]));
    match (leading.is_empty(), trailing.is_empty(), internal.is_empty()) {
        (true, true, true) => None,
        _ => Some(Syntax {
            leading,
            trailing,
            internal,
        }),
    }
}

pub fn merge_comments<M: Dupe>(
    inner: Option<Syntax<M, ()>>,
    outer: Option<Syntax<M, ()>>,
) -> Option<Syntax<M, ()>> {
    match (inner, outer) {
        (None, c) | (c, None) => c,
        (Some(inner), Some(outer)) => mk_comments_opt(
            Some(Arc::from(
                outer
                    .leading
                    .iter()
                    .cloned()
                    .chain(inner.leading.iter().cloned())
                    .collect::<Vec<_>>(),
            )),
            Some(Arc::from(
                inner
                    .trailing
                    .iter()
                    .cloned()
                    .chain(outer.trailing.iter().cloned())
                    .collect::<Vec<_>>(),
            )),
        ),
    }
}

pub fn merge_comments_with_internal<M: Dupe>(
    inner: Option<Syntax<M, Arc<[Comment<M>]>>>,
    outer: Option<Syntax<M, ()>>,
) -> Option<Syntax<M, Arc<[Comment<M>]>>> {
    match (inner, outer) {
        (inner, None) => inner,
        (
            None,
            Some(Syntax {
                leading, trailing, ..
            }),
        ) => mk_comments_with_internal_opt(Some(leading), Some(trailing), Some(Arc::from([]))),
        (
            Some(Syntax {
                leading: inner_leading,
                trailing: inner_trailing,
                internal,
            }),
            Some(Syntax {
                leading: outer_leading,
                trailing: outer_trailing,
                ..
            }),
        ) => mk_comments_with_internal_opt(
            Some(Arc::from(
                outer_leading
                    .iter()
                    .cloned()
                    .chain(inner_leading.iter().cloned())
                    .collect::<Vec<_>>(),
            )),
            Some(Arc::from(
                inner_trailing
                    .iter()
                    .cloned()
                    .chain(outer_trailing.iter().cloned())
                    .collect::<Vec<_>>(),
            )),
            Some(internal),
        ),
    }
}

pub fn split_comments<M: Dupe>(
    comments: Option<Syntax<M, ()>>,
) -> (Option<Syntax<M, ()>>, Option<Syntax<M, ()>>) {
    match comments {
        None => (None, None),
        Some(Syntax {
            leading, trailing, ..
        }) => (
            mk_comments_opt(Some(leading), None),
            mk_comments_opt(None, Some(trailing)),
        ),
    }
}

pub fn string_of_assignment_operator(op: expression::AssignmentOperator) -> &'static str {
    op.as_str()
}

pub fn string_of_binary_operator(op: expression::BinaryOperator) -> &'static str {
    op.as_str()
}

pub mod expression_sort {
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum Kind {
        Array,
        ArrowFunction,
        Assignment,
        Binary,
        Call,
        Class,
        Conditional,
        Function,
        Identifier,
        Import,
        JSXElement,
        JSXFragment,
        Literal,
        Logical,
        Match,
        Member,
        MetaProperty,
        New,
        Object,
        OptionalCall,
        OptionalMember,
        Record,
        Satisfies,
        Sequence,
        Super,
        TaggedTemplate,
        TemplateLiteral,
        This,
        TypeCast,
        Unary,
        Update,
        Yield,
    }

    impl Kind {
        pub fn as_str(&self) -> &'static str {
            match self {
                Self::Array => "array",
                Self::ArrowFunction => "arrow function",
                Self::Assignment => "assignment expression",
                Self::Binary => "binary expression",
                Self::Call => "call expression",
                Self::Class => "class",
                Self::Conditional => "conditional expression",
                Self::Function => "function",
                Self::Identifier => "identifier",
                Self::Import => "import expression",
                Self::JSXElement => "JSX element",
                Self::JSXFragment => "JSX fragment",
                Self::Literal => "literal",
                Self::Logical => "logical expression",
                Self::Match => "match expression",
                Self::Member => "member expression",
                Self::MetaProperty => "metaproperty expression",
                Self::New => "new expression",
                Self::Object => "object",
                Self::OptionalCall => "optional call expression",
                Self::OptionalMember => "optional member expression",
                Self::Record => "record expression",
                Self::Satisfies => "satisfies expression",
                Self::Sequence => "sequence",
                Self::Super => "`super` reference",
                Self::TaggedTemplate => "tagged template expression",
                Self::TemplateLiteral => "template literal",
                Self::This => "`this` reference",
                Self::TypeCast => "type cast",
                Self::Unary => "unary expression",
                Self::Update => "update expression",
                Self::Yield => "yield expression",
            }
        }
    }

    pub type ExpressionSort = Kind;
}

pub fn loc_of_annotation_or_hint<M: Dupe, T: Dupe>(annot: &types::AnnotationOrHint<M, T>) -> &T {
    match annot {
        types::AnnotationOrHint::Missing(loc) => loc,
        types::AnnotationOrHint::Available(types::Annotation { annotation, .. }) => {
            annotation.loc()
        }
    }
}

pub fn loc_of_return_annot<L: Dupe>(annot: &function::ReturnAnnot<L, L>) -> &L {
    match annot {
        function::ReturnAnnot::Missing(loc) => loc,
        function::ReturnAnnot::Available(types::Annotation { annotation, .. }) => annotation.loc(),
        function::ReturnAnnot::TypeGuard(types::TypeGuardAnnotation { loc, .. }) => loc,
    }
}

// Apply type [t] at the toplevel of expression [exp]. This is straightforward overall
// except for the case of Identifier and Member, where we push the type within the
// identifier and member property type position as well. This is to ensure that
// type-at-pos searcher will detect the updated type.
pub fn push_toplevel_type<M: Dupe, A: Dupe, B: Dupe>(
    t: B,
    exp: expression::Expression<M, (A, B)>,
) -> expression::Expression<M, (A, B)> {
    let push_toplevel_identifier = |id: &Identifier<M, (A, B)>, t: &B| -> Identifier<M, (A, B)> {
        Identifier::new(IdentifierInner {
            loc: (id.loc.0.dupe(), t.dupe()),
            name: id.name.dupe(),
            comments: id.comments.clone(),
        })
    };
    let push_to_member =
        |mem: &expression::Member<M, (A, B)>, t: &B| -> expression::Member<M, (A, B)> {
            match &mem.property {
                expression::member::Property::PropertyIdentifier(id) => expression::Member {
                    object: mem.object.dupe(),
                    property: expression::member::Property::PropertyIdentifier(
                        push_toplevel_identifier(id, t),
                    ),
                    comments: mem.comments.clone(),
                },
                _ => mem.clone(),
            }
        };
    let exp_loc = exp.loc().0.dupe();
    let e = match exp.deref() {
        ExpressionInner::Identifier { loc: _, inner } => ExpressionInner::Identifier {
            loc: (exp_loc, t.dupe()),
            inner: push_toplevel_identifier(inner, &t),
        },
        ExpressionInner::Member { loc: _, inner } => ExpressionInner::Member {
            loc: (exp_loc, t.dupe()),
            inner: Arc::new(push_to_member(inner, &t)),
        },
        ExpressionInner::OptionalMember { loc: _, inner } => ExpressionInner::OptionalMember {
            loc: (exp_loc, t.dupe()),
            inner: Arc::new(expression::OptionalMember {
                member: push_to_member(&inner.member, &t),
                filtered_out: inner.filtered_out.dupe(),
                optional: inner.optional.dupe(),
            }),
        },
        other => {
            let mut cloned = other.clone();
            *cloned.loc_mut() = (exp_loc, t);
            return expression::Expression::new(cloned);
        }
    };
    expression::Expression::new(e)
}

pub fn hook_name(s: &str) -> bool {
    // OCaml's is_cap checks that `c` equals its ASCII uppercase form, which is
    // true for uppercase letters, digits, underscores, and any non-lowercase
    // character.
    fn is_cap(c: char) -> bool {
        c == c.to_ascii_uppercase()
    }
    s.starts_with("use") && (s.len() == 3 || s.chars().nth(3).is_some_and(is_cap))
}

pub fn hook_function<M: Dupe, T: Dupe>(func: &function::Function<M, T>) -> Option<&T> {
    match &func.id {
        Some(id) if hook_name(&id.name) => Some(&id.loc),
        _ => None,
    }
}

pub fn hook_call<M: Dupe, T: Dupe>(call: &expression::Call<M, T>) -> bool {
    // A.B.C.useFoo() is a hook, A().useFoo() is not
    fn hook_callee<M: Dupe, T: Dupe>(top: bool, exp: &expression::Expression<M, T>) -> bool {
        match exp.deref() {
            ExpressionInner::Identifier { inner, .. } => hook_name(&inner.name) || !top,
            ExpressionInner::Member { inner, .. } => {
                if let expression::member::Property::PropertyIdentifier(id) = &inner.property {
                    (hook_name(&id.name) || !top) && hook_callee(false, &inner.object)
                } else {
                    false
                }
            }
            _ => false,
        }
    }
    hook_callee(true, &call.callee)
}

// Match
pub const MATCH_ROOT_NAME: &str = "<match_root>";

pub fn match_root_ident<M: Dupe, T: Dupe>(loc: T) -> Identifier<M, T> {
    Identifier::new(IdentifierInner {
        loc,
        name: FlowSmolStr::new_inline(MATCH_ROOT_NAME),
        comments: None,
    })
}

pub fn expression_of_match_member_pattern<L: Dupe>(
    visit_expression: &mut dyn FnMut(&expression::Expression<L, L>),
    pattern: &match_pattern::MemberPattern<L, L>,
) -> (expression::Expression<L, L>, Identifier<L, L>) {
    let (object, root_name) = match &pattern.base {
        match_pattern::member_pattern::Base::BaseIdentifier(id) => (
            expression::Expression::new(expression::ExpressionInner::Identifier {
                loc: id.loc.dupe(),
                inner: id.dupe(),
            }),
            id.dupe(),
        ),
        match_pattern::member_pattern::Base::BaseMember(mem) => {
            expression_of_match_member_pattern(visit_expression, mem)
        }
    };
    let property = match &pattern.property {
        match_pattern::member_pattern::Property::PropertyIdentifier(id) => {
            expression::member::Property::PropertyIdentifier(id.dupe())
        }
        match_pattern::member_pattern::Property::PropertyString { loc, literal } => {
            expression::member::Property::PropertyExpression(expression::Expression::new(
                expression::ExpressionInner::StringLiteral {
                    loc: loc.dupe(),
                    inner: Arc::new(literal.clone()),
                },
            ))
        }
        match_pattern::member_pattern::Property::PropertyNumber { loc, literal } => {
            expression::member::Property::PropertyExpression(expression::Expression::new(
                expression::ExpressionInner::NumberLiteral {
                    loc: loc.dupe(),
                    inner: Arc::new(literal.clone()),
                },
            ))
        }
        match_pattern::member_pattern::Property::PropertyBigInt { loc, literal } => {
            expression::member::Property::PropertyExpression(expression::Expression::new(
                expression::ExpressionInner::BigIntLiteral {
                    loc: loc.clone(),
                    inner: Arc::new(literal.clone()),
                },
            ))
        }
    };
    let exp = expression::Expression::new(expression::ExpressionInner::Member {
        loc: pattern.loc.dupe(),
        inner: Arc::new(expression::Member {
            object,
            property,
            comments: pattern.comments.dupe(),
        }),
    });
    visit_expression(&exp);
    (exp, root_name)
}

// Type Guards
pub fn get_inferred_type_guard_candidate<'a, M: Dupe, T: Dupe>(
    params: &'a function::Params<M, T>,
    body: &function::Body<M, T>,
    return_: &function::ReturnAnnot<M, T>,
) -> Option<(&'a T, &'a str)> {
    match (body, return_) {
        (function::Body::BodyExpression(_), function::ReturnAnnot::Missing(_)) => {
            if params.params.len() == 1 && params.rest.is_none() {
                if let Some(param) = params.params.first() {
                    match param {
                        function::Param::RegularParam { argument, .. } => match argument {
                            pattern::Pattern::Identifier { inner, .. } => {
                                Some((&inner.name.loc, &inner.name.name))
                            }
                            _ => None,
                        },
                        function::Param::ParamProperty { .. } => None,
                    }
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

pub struct NonnullReconstruct<M: Dupe, T: Dupe>(NonnullReconstructInner<M, T>);

enum NonnullReconstructInner<M: Dupe, T: Dupe> {
    Identity,
    Layer {
        loc: T,
        comments: Option<Syntax<M, ()>>,
        reconstruct: Box<NonnullReconstruct<M, T>>,
    },
}

impl<M: Dupe + Clone + std::hash::Hash + Eq + Ord, T: Dupe> NonnullReconstruct<M, T> {
    pub fn call<T2: Dupe + Clone + std::hash::Hash + Eq + Ord>(
        self,
        argument: expression::Expression<M, T2>,
        filter_nullish: &mut dyn FnMut(T) -> T2,
    ) -> expression::Expression<M, T2> {
        match self.0 {
            NonnullReconstructInner::Identity => argument,
            NonnullReconstructInner::Layer {
                loc,
                comments,
                reconstruct,
            } => expression::Expression::new(expression::ExpressionInner::Unary {
                loc: filter_nullish(loc),
                inner: Arc::new(expression::Unary {
                    operator: expression::UnaryOperator::Nonnull,
                    argument: reconstruct.call(argument, filter_nullish),
                    comments,
                }),
            }),
        }
    }
}

pub fn unwrap_nonnull_lhs_expr<M: Dupe, T: Dupe>(
    expr: &expression::Expression<M, T>,
) -> (
    &expression::Expression<M, T>,
    bool,
    NonnullReconstruct<M, T>,
) {
    match expr.deref() {
        ExpressionInner::Unary { loc, inner }
            if matches!(inner.operator, expression::UnaryOperator::Nonnull) =>
        {
            let (argument, _, reconstruct) = unwrap_nonnull_lhs_expr(&inner.argument);
            let reconstruct = NonnullReconstruct(NonnullReconstructInner::Layer {
                loc: loc.dupe(),
                comments: inner.comments.clone(),
                reconstruct: Box::new(reconstruct),
            });
            (argument, true, reconstruct)
        }
        _ => (
            expr,
            false,
            NonnullReconstruct(NonnullReconstructInner::Identity),
        ),
    }
}

pub fn unwrap_nonnull_lhs<M: Dupe, T: Dupe>(
    pat: &pattern::Pattern<M, T>,
) -> (Cow<'_, pattern::Pattern<M, T>>, bool) {
    match pat {
        pattern::Pattern::Expression { loc, inner } => {
            let (unwrapped_expr, optional, _) = unwrap_nonnull_lhs_expr(inner.as_ref());
            match unwrapped_expr.deref() {
                ExpressionInner::Identifier {
                    loc: e_loc,
                    inner: ident,
                } => {
                    assert!(optional);
                    let new_pat = pattern::Pattern::Identifier {
                        loc: loc.clone(),
                        inner: Arc::new(pattern::Identifier {
                            name: ident.dupe(),
                            optional: false,
                            annot: types::AnnotationOrHint::Missing(e_loc.dupe()),
                        }),
                    };
                    (Cow::Owned(new_pat), true)
                }
                _ => {
                    if optional {
                        let new_pat = pattern::Pattern::Expression {
                            loc: loc.dupe(),
                            inner: Arc::new(unwrapped_expr.dupe()),
                        };
                        (Cow::Owned(new_pat), optional)
                    } else {
                        (Cow::Borrowed(pat), false)
                    }
                }
            }
        }
        _ => (Cow::Borrowed(pat), false),
    }
}

pub fn string_of_bigint<M: Dupe>(bigint: &BigIntLiteral<M>) -> String {
    // https://github.com/estree/estree/blob/master/es2020.md#bigintliteral
    // `bigint` property is the string representation of the `BigInt` value.
    //  It must contain only decimal digits and not include numeric separators `_` or the suffix `n`.
    match bigint.value {
        Some(value) => value.to_string(),
        // Remove the 'n' suffix and the underscores.
        None => {
            let raw = &bigint.raw;
            raw[..raw.len() - 1].replace('_', "")
        }
    }
}

pub fn effective_export_kind(
    statement_export_kind: statement::ExportKind,
    specifier_export_kind: statement::ExportKind,
) -> statement::ExportKind {
    match specifier_export_kind {
        statement::ExportKind::ExportType => statement::ExportKind::ExportType,
        statement::ExportKind::ExportValue => statement_export_kind,
    }
}

pub fn export_specifiers_has_value_export<M: Dupe, T: Dupe>(
    statement_export_kind: statement::ExportKind,
    specs: &[statement::export_named_declaration::ExportSpecifier<M, T>],
) -> bool {
    specs.iter().any(|spec| {
        effective_export_kind(statement_export_kind, spec.export_kind)
            == statement::ExportKind::ExportValue
    })
}

pub fn string_of_enum_member_name<M: Dupe>(
    id: &statement::enum_declaration::MemberName<M>,
) -> &str {
    match id {
        statement::enum_declaration::MemberName::Identifier(ident) => &ident.name,
        statement::enum_declaration::MemberName::StringLiteral(_, lit) => &lit.value,
    }
}
