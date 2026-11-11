/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::sync::Arc;

use dupe::Dupe;
use flow_parser::ast;
use flow_parser::ast::class;
use flow_parser::ast::expression;
use flow_parser::ast::function;
use flow_parser::ast::pattern;
use flow_parser::ast::statement;
use flow_parser::ast::statement::StatementInner;
use flow_parser::ast::types;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::loc::Loc;
use flow_parser_utils::ast_builder;

struct StripFlow;

impl StripFlow {
    fn keep_statement(statement: &statement::Statement<Loc, Loc>) -> bool {
        match &**statement {
            StatementInner::DeclareClass { .. }
            | StatementInner::DeclareComponent { .. }
            | StatementInner::DeclareEnum { .. }
            | StatementInner::DeclareExportDeclaration { .. }
            | StatementInner::DeclareFunction { .. }
            | StatementInner::DeclareInterface { .. }
            | StatementInner::DeclareModule { .. }
            | StatementInner::DeclareModuleExports { .. }
            | StatementInner::DeclareNamespace { .. }
            | StatementInner::DeclareTypeAlias { .. }
            | StatementInner::DeclareOpaqueType { .. }
            | StatementInner::DeclareVariable { .. }
            | StatementInner::InterfaceDeclaration { .. }
            | StatementInner::TypeAlias { .. }
            | StatementInner::OpaqueType { .. }
            | StatementInner::NamespaceExportDeclaration { .. } => false,
            StatementInner::ImportDeclaration { inner, .. } => {
                inner.import_kind == statement::ImportKind::ImportValue
                    && (inner.default.is_some()
                        || match &inner.specifiers {
                            Some(
                                statement::import_declaration::Specifier::ImportNamedSpecifiers(
                                    specifiers,
                                ),
                            ) => {
                                specifiers.is_empty()
                                    || specifiers.iter().any(|specifier| {
                                        specifier.kind.is_none_or(|kind| {
                                            kind == statement::ImportKind::ImportValue
                                        })
                                    })
                            }
                            _ => true,
                        })
            }
            StatementInner::ExportNamedDeclaration { inner, .. } => {
                inner.export_kind == statement::ExportKind::ExportValue
                    && inner.declaration.as_ref().is_none_or(Self::keep_statement)
                    && match &inner.specifiers {
                        Some(statement::export_named_declaration::Specifier::ExportSpecifiers(
                            specifiers,
                        )) => {
                            specifiers.is_empty()
                                || specifiers.iter().any(|specifier| {
                                    specifier.export_kind == statement::ExportKind::ExportValue
                                })
                        }
                        _ => true,
                    }
            }
            StatementInner::ExportDefaultDeclaration { inner, .. } => match &inner.declaration {
                statement::export_default_declaration::Declaration::Declaration(declaration) => {
                    Self::keep_statement(declaration)
                }
                statement::export_default_declaration::Declaration::Expression(_) => true,
            },
            StatementInner::ImportEqualsDeclaration { inner, .. } => {
                inner.import_kind == statement::ImportKind::ImportValue
            }
            StatementInner::ExportAssignment { inner, .. } => {
                matches!(inner.rhs, statement::ExportAssignmentRhs::Expression(_))
            }
            _ => true,
        }
    }

    fn generic_identifier_expression(
        id: &types::generic::Identifier<Loc, Loc>,
    ) -> Option<expression::Expression<Loc, Loc>> {
        match id {
            types::generic::Identifier::Unqualified(id) => Some(expression::Expression::new(
                expression::ExpressionInner::Identifier {
                    loc: id.loc.dupe(),
                    inner: id.dupe(),
                },
            )),
            types::generic::Identifier::Qualified(qualified) => {
                let object = Self::generic_identifier_expression(&qualified.qualification)?;
                Some(expression::Expression::new(
                    expression::ExpressionInner::Member {
                        loc: qualified.loc.dupe(),
                        inner: Arc::new(expression::Member {
                            object,
                            property: expression::member::Property::PropertyIdentifier(
                                qualified.id.dupe(),
                            ),
                            comments: None,
                        }),
                    },
                ))
            }
            types::generic::Identifier::ImportTypeAnnot(_) => None,
        }
    }

    fn import_equals_declaration(
        &mut self,
        loc: &Loc,
        declaration: &statement::ImportEqualsDeclaration<Loc, Loc>,
    ) -> statement::Statement<Loc, Loc> {
        let (kind, initializer) = match &declaration.module_reference {
            statement::import_equals_declaration::ModuleReference::ExternalModuleReference(
                reference_loc,
                literal,
            ) => {
                let literal =
                    expression::Expression::new(expression::ExpressionInner::StringLiteral {
                        loc: reference_loc.dupe(),
                        inner: Arc::new(literal.clone()),
                    });
                (
                    ast::VariableKind::Const,
                    ast_builder::expressions::call(
                        Some(reference_loc.dupe()),
                        Some(ast_builder::expressions::arg_list(
                            Some(reference_loc.dupe()),
                            None,
                            vec![ast_builder::expressions::expression_or_spread(literal)],
                        )),
                        ast_builder::expressions::identifier(
                            Some(reference_loc.dupe()),
                            None,
                            "require",
                        ),
                    ),
                )
            }
            statement::import_equals_declaration::ModuleReference::Identifier(identifier) => {
                let initializer =
                    Self::generic_identifier_expression(identifier).unwrap_or_else(|| {
                        ast_builder::expressions::identifier(Some(loc.dupe()), None, "undefined")
                    });
                (ast::VariableKind::Var, initializer)
            }
        };
        let comments = declaration.comments.dupe();
        let variable = ast_builder::statements::variable_declaration(
            Some(kind),
            Some(loc.dupe()),
            if declaration.is_export {
                None
            } else {
                comments.dupe()
            },
            vec![ast_builder::statements::variable_declarator_generic(
                Some(loc.dupe()),
                pattern::Pattern::Identifier {
                    loc: declaration.id.loc.dupe(),
                    inner: Arc::new(pattern::Identifier {
                        name: declaration.id.dupe(),
                        annot: types::AnnotationOrHint::Missing(declaration.id.loc.dupe()),
                        optional: false,
                    }),
                },
                Some(initializer),
            )],
        );

        if declaration.is_export {
            statement::Statement::new(StatementInner::ExportNamedDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(statement::ExportNamedDeclaration {
                    declaration: Some(variable),
                    specifiers: None,
                    source: None,
                    attributes: None,
                    export_kind: statement::ExportKind::ExportValue,
                    comments,
                }),
            })
        } else {
            variable
        }
    }
}

impl<'ast> AstVisitor<'ast, Loc> for StripFlow {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn map_expression(
        &mut self,
        expression: &'ast expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        match &**expression {
            expression::ExpressionInner::AsExpression { inner, .. } => {
                self.map_expression(&inner.expression)
            }
            expression::ExpressionInner::AsConstExpression { inner, .. } => {
                self.map_expression(&inner.expression)
            }
            expression::ExpressionInner::TypeCast { inner, .. } => {
                self.map_expression(&inner.expression)
            }
            expression::ExpressionInner::TSSatisfies { inner, .. } => {
                self.map_expression(&inner.expression)
            }
            expression::ExpressionInner::Unary { inner, .. }
                if inner.operator == expression::UnaryOperator::Nonnull =>
            {
                self.map_expression(&inner.argument)
            }
            _ => ast_visitor::map_expression_default(self, expression),
        }
    }

    fn map_class_body(&mut self, body: &'ast class::Body<Loc, Loc>) -> class::Body<Loc, Loc> {
        class::Body {
            loc: body.loc.dupe(),
            body: body
                .body
                .iter()
                .filter_map(|element| match element {
                    class::BodyElement::AbstractMethod(_)
                    | class::BodyElement::AbstractProperty(_) => None,
                    class::BodyElement::DeclareMethod(_)
                    | class::BodyElement::IndexSignature(_) => None,
                    class::BodyElement::Property(property)
                        if matches!(property.value, class::property::Value::Declared) =>
                    {
                        None
                    }
                    class::BodyElement::PrivateField(property)
                        if matches!(property.value, class::property::Value::Declared) =>
                    {
                        None
                    }
                    _ => Some(self.map_class_element(element)),
                })
                .collect::<Vec<_>>()
                .into(),
            comments: self.map_syntax_opt(body.comments.as_ref()),
        }
    }

    fn map_function_return_annotation(
        &mut self,
        return_: &'ast function::ReturnAnnot<Loc, Loc>,
    ) -> function::ReturnAnnot<Loc, Loc> {
        let loc = match return_ {
            function::ReturnAnnot::Missing(loc) => loc,
            function::ReturnAnnot::Available(annotation) => &annotation.loc,
            function::ReturnAnnot::TypeGuard(guard) => &guard.loc,
        };
        function::ReturnAnnot::Missing(loc.dupe())
    }

    fn map_function_(
        &mut self,
        loc: &'ast Loc,
        function: &'ast function::Function<Loc, Loc>,
    ) -> function::Function<Loc, Loc> {
        let mut function = ast_visitor::map_function_default(self, loc, function);
        function.params.this_ = None;
        function.tparams = None;
        function.predicate = None;
        function
    }

    fn map_type_annotation_hint(
        &mut self,
        hint: &'ast types::AnnotationOrHint<Loc, Loc>,
    ) -> types::AnnotationOrHint<Loc, Loc> {
        let loc = match hint {
            types::AnnotationOrHint::Available(annotation) => &annotation.loc,
            types::AnnotationOrHint::Missing(loc) => loc,
        };
        types::AnnotationOrHint::Missing(loc.dupe())
    }

    fn map_pattern(
        &mut self,
        kind: Option<ast::VariableKind>,
        pattern: &'ast pattern::Pattern<Loc, Loc>,
    ) -> pattern::Pattern<Loc, Loc> {
        let mut pattern = ast_visitor::map_pattern_default(self, kind, pattern);
        match &mut pattern {
            pattern::Pattern::Identifier { inner, .. } => Arc::make_mut(inner).optional = false,
            pattern::Pattern::Object { inner, .. } => Arc::make_mut(inner).optional = false,
            pattern::Pattern::Array { inner, .. } => Arc::make_mut(inner).optional = false,
            pattern::Pattern::Expression { .. } => {}
        }
        pattern
    }

    fn map_class_(
        &mut self,
        loc: &'ast Loc,
        class: &'ast class::Class<Loc, Loc>,
    ) -> class::Class<Loc, Loc> {
        let mut class = ast_visitor::map_class_default(self, loc, class);
        class.tparams = None;
        class.implements = None;
        class.abstract_ = false;
        if let Some(extends) = &mut class.extends {
            extends.targs = None;
        }
        class
    }

    fn map_class_element(
        &mut self,
        element: &'ast class::BodyElement<Loc, Loc>,
    ) -> class::BodyElement<Loc, Loc> {
        let mut element = ast_visitor::map_class_element_default(self, element);
        match &mut element {
            class::BodyElement::Property(property) => {
                property.variance = None;
                property.optional = false;
                property.definite = false;
                property.override_ = false;
                property.ts_accessibility = None;
            }
            class::BodyElement::PrivateField(property) => {
                property.variance = None;
                property.optional = false;
                property.definite = false;
                property.override_ = false;
                property.ts_accessibility = None;
            }
            class::BodyElement::Method(method) => {
                method.override_ = false;
                method.ts_accessibility = None;
            }
            _ => {}
        }
        element
    }

    fn map_variable_declarator(
        &mut self,
        kind: ast::VariableKind,
        declarator: &'ast statement::variable::Declarator<Loc, Loc>,
    ) -> statement::variable::Declarator<Loc, Loc> {
        let mut declarator = ast_visitor::map_variable_declarator_default(self, kind, declarator);
        declarator.definite = false;
        declarator
    }

    fn map_call(
        &mut self,
        loc: &'ast Loc,
        call: &'ast expression::Call<Loc, Loc>,
    ) -> expression::Call<Loc, Loc> {
        let mut call = ast_visitor::map_call_default(self, loc, call);
        call.targs = None;
        call
    }

    fn map_new(
        &mut self,
        loc: &'ast Loc,
        new: &'ast expression::New<Loc, Loc>,
    ) -> expression::New<Loc, Loc> {
        let mut new = ast_visitor::map_new_default(self, loc, new);
        new.targs = None;
        new
    }

    fn map_optional_call(
        &mut self,
        loc: &'ast Loc,
        call: &'ast expression::OptionalCall<Loc, Loc>,
    ) -> expression::OptionalCall<Loc, Loc> {
        let mut call = ast_visitor::map_optional_call_default(self, loc, call);
        if matches!(
            call.optional,
            expression::OptionalCallKind::AssertNonnull { .. }
        ) {
            call.optional = expression::OptionalCallKind::NonOptional;
        }
        call
    }

    fn map_optional_member(
        &mut self,
        loc: &'ast Loc,
        member: &'ast expression::OptionalMember<Loc, Loc>,
    ) -> expression::OptionalMember<Loc, Loc> {
        let mut member = ast_visitor::map_optional_member_default(self, loc, member);
        if matches!(
            member.optional,
            expression::OptionalMemberKind::AssertNonnull { .. }
        ) {
            member.optional = expression::OptionalMemberKind::NonOptional;
        }
        member
    }

    fn map_import_declaration(
        &mut self,
        loc: &'ast Loc,
        import: &'ast statement::ImportDeclaration<Loc, Loc>,
    ) -> statement::ImportDeclaration<Loc, Loc> {
        let mut import = ast_visitor::map_import_declaration_default(self, loc, import);
        if let Some(statement::import_declaration::Specifier::ImportNamedSpecifiers(specifiers)) =
            &mut import.specifiers
        {
            specifiers.retain(|specifier| {
                specifier
                    .kind
                    .is_none_or(|kind| kind == statement::ImportKind::ImportValue)
            });
            if specifiers.is_empty() && import.default.is_some() {
                import.specifiers = None;
            }
        }
        import
    }

    fn map_export_named_declaration(
        &mut self,
        loc: &'ast Loc,
        export: &'ast statement::ExportNamedDeclaration<Loc, Loc>,
    ) -> statement::ExportNamedDeclaration<Loc, Loc> {
        let mut export = ast_visitor::map_export_named_declaration_default(self, loc, export);
        if let Some(statement::export_named_declaration::Specifier::ExportSpecifiers(specifiers)) =
            &mut export.specifiers
        {
            specifiers
                .retain(|specifier| specifier.export_kind == statement::ExportKind::ExportValue);
        }
        export
    }

    fn map_jsx_opening_element(
        &mut self,
        element: &'ast ast::jsx::Opening<Loc, Loc>,
    ) -> ast::jsx::Opening<Loc, Loc> {
        let mut element = ast_visitor::map_jsx_opening_element_default(self, element);
        element.targs = None;
        element
    }

    fn map_statement_list(
        &mut self,
        statements: &'ast Arc<[statement::Statement<Loc, Loc>]>,
    ) -> Arc<[statement::Statement<Loc, Loc>]> {
        statements
            .iter()
            .filter(|statement| Self::keep_statement(statement))
            .map(|statement| self.map_statement(statement))
            .collect::<Vec<_>>()
            .into()
    }

    fn map_statement(
        &mut self,
        statement: &'ast statement::Statement<Loc, Loc>,
    ) -> statement::Statement<Loc, Loc> {
        if !Self::keep_statement(statement) {
            return statement::Statement::new(StatementInner::Empty {
                loc: statement.loc().dupe(),
                inner: Arc::new(statement::Empty { comments: None }),
            });
        }
        match &**statement {
            StatementInner::ImportEqualsDeclaration { loc, inner } => {
                self.import_equals_declaration(loc, inner)
            }
            StatementInner::ExportAssignment { loc, inner }
                if let statement::ExportAssignmentRhs::Expression(rhs) = &inner.rhs =>
            {
                let member = ast_builder::expressions::member(
                    Some(loc.dupe()),
                    ast_builder::expressions::members::identifier(
                        None,
                        ast_builder::identifiers::identifier(Some(loc.dupe()), "exports"),
                        ast_builder::expressions::identifier(Some(loc.dupe()), None, "module"),
                    ),
                );
                let assignment = ast_builder::expressions::assignment(
                    Some(loc.dupe()),
                    None,
                    pattern::Pattern::Expression {
                        loc: loc.dupe(),
                        inner: Arc::new(member),
                    },
                    None,
                    self.map_expression(rhs),
                );
                ast_builder::statements::expression(
                    Some(loc.dupe()),
                    None,
                    inner.comments.dupe(),
                    assignment,
                )
            }
            _ => ast_visitor::map_statement_default(self, statement),
        }
    }
}

/// Erase type syntax from a program after its runtime syntax has been lowered.
pub fn lower_program(program: &ast::Program<Loc, Loc>) -> ast::Program<Loc, Loc> {
    StripFlow.map_program(program)
}

#[cfg(test)]
#[path = "strip_flow_tests.rs"]
mod tests;
