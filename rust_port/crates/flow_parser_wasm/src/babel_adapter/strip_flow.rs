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

use super::builders;

struct StripFlow {
    // Import-equals declarations and export assignments do not record whether they are inside a
    // `declare module`, but that context determines whether lowering emits declarations or runtime
    // statements.
    ambient: bool,
}

impl StripFlow {
    fn any(loc: &Loc) -> types::Type<Loc, Loc> {
        types::Type::new(types::TypeInner::Any {
            loc: loc.dupe(),
            comments: None,
        })
    }

    fn simple_generic(loc: &Loc, name: &str) -> types::Type<Loc, Loc> {
        types::Type::new(types::TypeInner::Generic {
            loc: loc.dupe(),
            inner: Arc::new(types::Generic {
                id: types::generic::Identifier::Unqualified(ast::Identifier::new(
                    ast::IdentifierInner {
                        loc: loc.dupe(),
                        name: name.into(),
                        comments: None,
                    },
                )),
                targs: None,
                comments: None,
            }),
        })
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

    fn expression_typeof_target(
        expression: &expression::Expression<Loc, Loc>,
    ) -> Option<types::typeof_::Target<Loc, Loc>> {
        match &**expression {
            expression::ExpressionInner::Identifier { inner, .. } => {
                Some(types::typeof_::Target::Unqualified(inner.dupe()))
            }
            expression::ExpressionInner::Member { loc, inner } => {
                let expression::member::Property::PropertyIdentifier(id) = &inner.property else {
                    return None;
                };
                Some(types::typeof_::Target::Qualified(Arc::new(
                    types::typeof_::Qualified {
                        loc: loc.dupe(),
                        qualification: Self::expression_typeof_target(&inner.object)?,
                        id: id.dupe(),
                    },
                )))
            }
            _ => None,
        }
    }

    fn declare_export_variable(
        loc: &Loc,
        declaration_loc: &Loc,
        declaration: statement::DeclareVariable<Loc, Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
    ) -> statement::Statement<Loc, Loc> {
        statement::Statement::new(StatementInner::DeclareExportDeclaration {
            loc: loc.dupe(),
            inner: Arc::new(statement::DeclareExportDeclaration {
                default: None,
                declaration: Some(
                    statement::declare_export_declaration::Declaration::Variable {
                        loc: declaration_loc.dupe(),
                        declaration: Arc::new(declaration),
                    },
                ),
                specifiers: None,
                source: None,
                comments,
            }),
        })
    }

    fn declare_module_exports(
        loc: &Loc,
        annotation: types::Type<Loc, Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
    ) -> statement::Statement<Loc, Loc> {
        statement::Statement::new(StatementInner::DeclareModuleExports {
            loc: loc.dupe(),
            inner: Arc::new(statement::DeclareModuleExports {
                annot: types::Annotation {
                    loc: loc.dupe(),
                    annotation,
                },
                comments,
            }),
        })
    }

    fn import_equals_declaration(
        &mut self,
        loc: &Loc,
        declaration: &statement::ImportEqualsDeclaration<Loc, Loc>,
    ) -> statement::Statement<Loc, Loc> {
        if self.ambient {
            let mut variable = Self::declare_variable_inner(loc, &declaration.id);
            if declaration.is_export {
                return Self::declare_export_variable(
                    loc,
                    loc,
                    variable,
                    declaration.comments.dupe(),
                );
            }
            variable.comments = declaration.comments.dupe();
            return statement::Statement::new(StatementInner::DeclareVariable {
                loc: loc.dupe(),
                inner: Arc::new(variable),
            });
        }

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
                    builders::call(
                        reference_loc,
                        builders::identifier(reference_loc, "require"),
                        vec![literal],
                    ),
                )
            }
            statement::import_equals_declaration::ModuleReference::Identifier(identifier) => {
                let initializer = Self::generic_identifier_expression(identifier)
                    .unwrap_or_else(|| builders::identifier(loc, "undefined"));
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
                builders::identifier_pattern(&declaration.id),
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

    fn export_assignment(
        &mut self,
        loc: &Loc,
        assignment: &statement::ExportAssignment<Loc, Loc>,
    ) -> Vec<statement::Statement<Loc, Loc>> {
        match &assignment.rhs {
            statement::ExportAssignmentRhs::Expression(rhs) => {
                if self.ambient {
                    let annotation = Self::expression_typeof_target(rhs)
                        .map(|argument| {
                            types::Type::new(types::TypeInner::Typeof {
                                loc: loc.dupe(),
                                inner: Arc::new(types::Typeof {
                                    argument,
                                    targs: None,
                                    comments: None,
                                }),
                            })
                        })
                        .unwrap_or_else(|| Self::any(loc));
                    return vec![Self::declare_module_exports(
                        loc,
                        annotation,
                        assignment.comments.dupe(),
                    )];
                }

                let member = builders::member(loc, builders::identifier(loc, "module"), "exports");
                let left = pattern::Pattern::Expression {
                    loc: loc.dupe(),
                    inner: Arc::new(member),
                };
                let assignment_expression = ast_builder::expressions::assignment(
                    Some(loc.dupe()),
                    None,
                    left,
                    None,
                    self.map_expression(rhs),
                );
                vec![ast_builder::statements::expression(
                    Some(loc.dupe()),
                    None,
                    assignment.comments.dupe(),
                    assignment_expression,
                )]
            }
            statement::ExportAssignmentRhs::DeclareFunction(function_loc, function) => {
                let function = self.map_declare_function(function_loc, function);
                let export_type = match function.id.as_ref() {
                    Some(id) => types::Type::new(types::TypeInner::Typeof {
                        loc: loc.dupe(),
                        inner: Arc::new(types::Typeof {
                            argument: types::typeof_::Target::Unqualified(id.dupe()),
                            targs: None,
                            comments: None,
                        }),
                    }),
                    None => function.annot.annotation.dupe(),
                };
                let module_exports =
                    Self::declare_module_exports(loc, export_type, assignment.comments.dupe());
                match function.id {
                    Some(_) => vec![
                        statement::Statement::new(StatementInner::DeclareFunction {
                            loc: function_loc.dupe(),
                            inner: Arc::new(function),
                        }),
                        module_exports,
                    ],
                    None => vec![module_exports],
                }
            }
        }
    }

    fn split_type_export(
        loc: &Loc,
        declaration: &statement::ExportNamedDeclaration<Loc, Loc>,
    ) -> Option<Vec<statement::Statement<Loc, Loc>>> {
        let Some(statement::export_named_declaration::Specifier::ExportSpecifiers(specifiers)) =
            &declaration.specifiers
        else {
            return None;
        };
        if !specifiers
            .iter()
            .any(|specifier| specifier.export_kind == statement::ExportKind::ExportType)
        {
            return None;
        }

        let value_specifiers = specifiers
            .iter()
            .filter(|specifier| specifier.export_kind == statement::ExportKind::ExportValue)
            .cloned()
            .collect::<Vec<_>>();
        let type_specifiers = specifiers
            .iter()
            .filter(|specifier| specifier.export_kind == statement::ExportKind::ExportType)
            .map(|specifier| {
                let mut specifier = specifier.clone();
                specifier.export_kind = statement::ExportKind::ExportValue;
                specifier
            })
            .collect::<Vec<_>>();

        [
            (value_specifiers, statement::ExportKind::ExportValue),
            (type_specifiers, statement::ExportKind::ExportType),
        ]
        .into_iter()
        .filter(|(specifiers, _)| !specifiers.is_empty())
        .map(|(specifiers, export_kind)| {
            statement::Statement::new(StatementInner::ExportNamedDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(statement::ExportNamedDeclaration {
                    declaration: declaration.declaration.dupe(),
                    specifiers: Some(
                        statement::export_named_declaration::Specifier::ExportSpecifiers(
                            specifiers,
                        ),
                    ),
                    source: declaration.source.clone(),
                    attributes: declaration.attributes.clone(),
                    export_kind,
                    comments: declaration.comments.dupe(),
                }),
            })
        })
        .collect::<Vec<_>>()
        .into()
    }

    fn declare_variable_inner(
        loc: &Loc,
        id: &ast::Identifier<Loc, Loc>,
    ) -> statement::DeclareVariable<Loc, Loc> {
        let pattern = pattern::Pattern::Identifier {
            loc: id.loc.dupe(),
            inner: Arc::new(pattern::Identifier {
                name: id.dupe(),
                annot: types::AnnotationOrHint::Available(types::Annotation {
                    loc: loc.dupe(),
                    annotation: Self::any(loc),
                }),
                optional: false,
            }),
        };
        statement::DeclareVariable {
            declarations: Arc::from([statement::variable::Declarator {
                loc: loc.dupe(),
                id: pattern,
                init: None,
                definite: false,
            }]),
            kind: ast::VariableKind::Const,
            comments: None,
        }
    }

    fn declare_variable(
        loc: &Loc,
        id: &ast::Identifier<Loc, Loc>,
    ) -> statement::Statement<Loc, Loc> {
        statement::Statement::new(StatementInner::DeclareVariable {
            loc: loc.dupe(),
            inner: Arc::new(Self::declare_variable_inner(loc, id)),
        })
    }

    fn strip_this_param(
        mut function: function::Function<Loc, Loc>,
    ) -> function::Function<Loc, Loc> {
        function.params.this_ = None;
        function
    }
}

impl<'ast> AstVisitor<'ast, Loc> for StripFlow {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn map_type_(&mut self, type_: &'ast types::Type<Loc, Loc>) -> types::Type<Loc, Loc> {
        match &**type_ {
            types::TypeInner::Symbol { loc, .. } => Self::simple_generic(loc, "symbol"),
            types::TypeInner::BigInt { loc, .. } => Self::simple_generic(loc, "bigint"),
            types::TypeInner::Object { loc, inner }
                if inner
                    .properties
                    .iter()
                    .any(|property| matches!(property, types::object::Property::MappedType(_))) =>
            {
                Self::any(loc)
            }
            types::TypeInner::IndexedAccess { loc, .. }
            | types::TypeInner::OptionalIndexedAccess { loc, .. }
            | types::TypeInner::Keyof { loc, .. }
            | types::TypeInner::Conditional { loc, .. }
            | types::TypeInner::Infer { loc, .. }
            | types::TypeInner::Component { loc, .. }
            | types::TypeInner::Renders { loc, .. }
            | types::TypeInner::ReadOnly { loc, .. } => Self::any(loc),
            types::TypeInner::Function { loc, inner } if inner.effect == function::Effect::Hook => {
                Self::any(loc)
            }
            _ => ast_visitor::map_type_default(self, type_),
        }
    }

    fn map_expression(
        &mut self,
        expression: &'ast expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        match &**expression {
            expression::ExpressionInner::TSSatisfies { loc, inner } => {
                expression::Expression::new(expression::ExpressionInner::AsExpression {
                    loc: loc.dupe(),
                    inner: Arc::new(expression::AsExpression {
                        expression: self.map_expression(&inner.expression),
                        annot: self.map_type_annotation(&inner.annot),
                        comments: self.map_syntax_opt(inner.comments.as_ref()),
                    }),
                })
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
                    _ => Some(self.map_class_element(element)),
                })
                .collect::<Vec<_>>()
                .into(),
            comments: self.map_syntax_opt(body.comments.as_ref()),
        }
    }

    fn map_tuple_type(
        &mut self,
        loc: &'ast Loc,
        tuple: &'ast types::Tuple<Loc, Loc>,
    ) -> types::Tuple<Loc, Loc> {
        let mut tuple = ast_visitor::map_tuple_type_default(self, loc, tuple);
        tuple.elements = tuple
            .elements
            .iter()
            .map(|element| match element {
                types::tuple::Element::UnlabeledElement { .. } => element.clone(),
                types::tuple::Element::LabeledElement { loc, .. }
                | types::tuple::Element::SpreadElement { loc, .. } => {
                    types::tuple::Element::UnlabeledElement {
                        loc: loc.dupe(),
                        annot: Self::any(loc),
                        optional: false,
                    }
                }
            })
            .collect::<Vec<_>>()
            .into();
        tuple
    }

    fn map_function_return_annotation(
        &mut self,
        return_: &'ast function::ReturnAnnot<Loc, Loc>,
    ) -> function::ReturnAnnot<Loc, Loc> {
        match return_ {
            function::ReturnAnnot::TypeGuard(guard) => {
                function::ReturnAnnot::Available(types::Annotation {
                    loc: guard.loc.dupe(),
                    annotation: Self::any(&guard.loc),
                })
            }
            _ => ast_visitor::map_function_return_annotation_default(self, return_),
        }
    }

    fn map_function_(
        &mut self,
        loc: &'ast Loc,
        function: &'ast function::Function<Loc, Loc>,
    ) -> function::Function<Loc, Loc> {
        Self::strip_this_param(ast_visitor::map_function_default(self, loc, function))
    }

    fn map_declare_module(
        &mut self,
        loc: &'ast Loc,
        declaration: &'ast statement::DeclareModule<Loc, Loc>,
    ) -> statement::DeclareModule<Loc, Loc> {
        let ambient = self.ambient;
        self.ambient = true;
        let declaration = ast_visitor::map_declare_module_default(self, loc, declaration);
        self.ambient = ambient;
        declaration
    }

    fn map_statement_list(
        &mut self,
        statements: &'ast Arc<[statement::Statement<Loc, Loc>]>,
    ) -> Arc<[statement::Statement<Loc, Loc>]> {
        statements
            .iter()
            .flat_map(|statement| match &**statement {
                StatementInner::ImportEqualsDeclaration { loc, inner } => {
                    if inner.import_kind == statement::ImportKind::ImportValue {
                        vec![self.import_equals_declaration(loc, inner)]
                    } else {
                        Vec::new()
                    }
                }
                StatementInner::ExportAssignment { loc, inner } => {
                    self.export_assignment(loc, inner)
                }
                _ => {
                    let statement = self.map_statement(statement);
                    match &*statement {
                        StatementInner::NamespaceExportDeclaration { .. } => Vec::new(),
                        StatementInner::ExportNamedDeclaration { loc, inner } => {
                            Self::split_type_export(loc, inner).unwrap_or_else(|| vec![statement])
                        }
                        _ => vec![statement],
                    }
                }
            })
            .collect::<Vec<_>>()
            .into()
    }

    fn map_statement(
        &mut self,
        statement: &'ast statement::Statement<Loc, Loc>,
    ) -> statement::Statement<Loc, Loc> {
        match &**statement {
            StatementInner::DeclareEnum { loc, inner } => Self::declare_variable(loc, &inner.id),
            StatementInner::DeclareNamespace { loc, inner } => {
                let id = match &inner.id {
                    statement::declare_namespace::Id::Global(id) => id,
                    statement::declare_namespace::Id::Local(id) => id,
                };
                Self::declare_variable(loc, id)
            }
            _ => ast_visitor::map_statement_default(self, statement),
        }
    }
}

pub fn lower_program(program: &ast::Program<Loc, Loc>) -> ast::Program<Loc, Loc> {
    StripFlow { ambient: false }.map_program(program)
}
