/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::HashMap;
use std::sync::Arc;

use dupe::Dupe;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::class;
use flow_parser::ast::expression;
use flow_parser::ast::expression::ExpressionInner;
use flow_parser::ast::pattern;
use flow_parser::ast::statement;
use flow_parser::ast::statement::StatementInner;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::loc::Loc;
use flow_parser_utils::ast_builder;

use super::builders;
use super::error::BabelLoweringError;
use super::gen_id::GenId;

type Expr = expression::Expression<Loc, Loc>;
type Stmt = statement::Statement<Loc, Loc>;

struct RecordLowerer {
    gen_id: GenId,
    error: Option<BabelLoweringError>,
}

impl RecordLowerer {
    fn syntax<T>(&mut self, loc: &Loc, message: impl Into<String>) -> Option<T> {
        if self.error.is_none() {
            self.error = Some(BabelLoweringError::syntax(loc, message));
        }
        None
    }

    fn is_reserved(name: &str) -> bool {
        matches!(
            name,
            "break"
                | "case"
                | "catch"
                | "class"
                | "const"
                | "continue"
                | "debugger"
                | "default"
                | "delete"
                | "do"
                | "else"
                | "enum"
                | "export"
                | "extends"
                | "false"
                | "finally"
                | "for"
                | "function"
                | "if"
                | "import"
                | "in"
                | "instanceof"
                | "new"
                | "null"
                | "return"
                | "super"
                | "switch"
                | "this"
                | "throw"
                | "true"
                | "try"
                | "typeof"
                | "var"
                | "void"
                | "while"
                | "with"
                | "await"
                | "yield"
        )
    }

    fn key_name(&mut self, key: &expression::object::Key<Loc, Loc>) -> Option<FlowSmolStr> {
        match key {
            expression::object::Key::Identifier(id) => Some(id.name.dupe()),
            expression::object::Key::StringLiteral((_, literal)) => Some(literal.value.dupe()),
            expression::object::Key::NumberLiteral((_, literal)) => {
                Some(FlowSmolStr::from(literal.value.to_string()))
            }
            expression::object::Key::BigIntLiteral((_, literal)) => Some(literal.raw.dupe()),
            expression::object::Key::PrivateName(name) => self.syntax(
                &name.loc,
                "Private names are not valid record property keys.",
            ),
            expression::object::Key::Computed(key) => self.syntax(
                &key.loc,
                "Computed keys are not valid record property keys.",
            ),
        }
    }

    fn pattern_key(
        &mut self,
        key: &expression::object::Key<Loc, Loc>,
    ) -> Option<pattern::object::Key<Loc, Loc>> {
        match key {
            expression::object::Key::Identifier(id) => {
                Some(pattern::object::Key::Identifier(id.dupe()))
            }
            expression::object::Key::StringLiteral(value) => {
                Some(pattern::object::Key::StringLiteral(value.clone()))
            }
            expression::object::Key::NumberLiteral(value) => {
                Some(pattern::object::Key::NumberLiteral(value.clone()))
            }
            expression::object::Key::BigIntLiteral(value) => {
                Some(pattern::object::Key::BigIntLiteral(value.clone()))
            }
            expression::object::Key::PrivateName(name) => self.syntax(
                &name.loc,
                "Private names are not valid record property keys.",
            ),
            expression::object::Key::Computed(key) => self.syntax(
                &key.loc,
                "Computed keys are not valid record property keys.",
            ),
        }
    }

    fn generated_identifier(&self, loc: &Loc, name: FlowSmolStr) -> ast::Identifier<Loc, Loc> {
        ast::Identifier::new(ast::IdentifierInner {
            loc: loc.dupe(),
            name,
            comments: None,
        })
    }

    fn identifier_binding_pattern(
        &self,
        loc: &Loc,
        id: ast::Identifier<Loc, Loc>,
    ) -> pattern::Pattern<Loc, Loc> {
        pattern::Pattern::Identifier {
            loc: loc.dupe(),
            inner: Arc::new(pattern::Identifier {
                name: id,
                annot: ast::types::AnnotationOrHint::Missing(loc.dupe()),
                optional: false,
            }),
        }
    }

    /// `construct_loc` is the span of the whole `record ... { ... }` statement. Nodes derived
    /// from a specific property take that property's span; the class shell, the constructor, and
    /// its parameter object are scaffolding with no source counterpart and use the construct.
    fn map_record_declaration(
        &mut self,
        construct_loc: &Loc,
        declaration: &statement::RecordDeclaration<Loc, Loc>,
    ) -> Option<Stmt> {
        let mut own_properties = Vec::new();
        let mut methods = Vec::new();
        let mut static_properties = Vec::new();
        let mut static_methods = Vec::new();
        for element in declaration.body.body.iter() {
            match element {
                statement::record_declaration::BodyElement::Property(property) => {
                    own_properties.push(property)
                }
                statement::record_declaration::BodyElement::StaticProperty(property) => {
                    static_properties.push(property)
                }
                statement::record_declaration::BodyElement::Method(method) if method.static_ => {
                    static_methods.push(class::BodyElement::Method(method.clone()))
                }
                statement::record_declaration::BodyElement::Method(method) => {
                    methods.push(class::BodyElement::Method(method.clone()))
                }
            }
        }

        let mut renamed = HashMap::new();
        let mut constructor_properties = Vec::new();
        for property in &own_properties {
            let name = self.key_name(&property.key)?;
            let needs_binding = Self::is_reserved(name.as_str())
                || !matches!(property.key, expression::object::Key::Identifier(_));
            let binding_name = if needs_binding {
                let generated = self.gen_id.id();
                renamed.insert(name, generated.dupe());
                generated
            } else {
                name
            };
            let binding = self.generated_identifier(&property.loc, binding_name);
            constructor_properties.push(pattern::object::Property::NormalProperty(
                pattern::object::NormalProperty {
                    loc: property.loc.dupe(),
                    key: self.pattern_key(&property.key)?,
                    pattern: self.identifier_binding_pattern(&property.loc, binding),
                    default: property.default_value.clone(),
                    shorthand: !needs_binding,
                },
            ));
        }
        let constructor_param = pattern::Pattern::Object {
            loc: construct_loc.dupe(),
            inner: Arc::new(pattern::Object {
                properties: constructor_properties.into(),
                annot: ast::types::AnnotationOrHint::Missing(construct_loc.dupe()),
                optional: false,
                comments: None,
            }),
        };

        let mut assignments = Vec::new();
        for property in &own_properties {
            let name = self.key_name(&property.key)?;
            let binding_name = renamed.get(&name).unwrap_or(&name);
            let object = ast_builder::expressions::this(Some(property.loc.dupe()), None);
            let member = match &property.key {
                expression::object::Key::Identifier(id) => ast_builder::expressions::member(
                    Some(property.loc.dupe()),
                    ast_builder::expressions::members::identifier(None, id.dupe(), object),
                ),
                _ => ast_builder::expressions::member(
                    Some(property.loc.dupe()),
                    ast_builder::expressions::members::expression(
                        None,
                        match &property.key {
                            expression::object::Key::StringLiteral((loc, literal)) => {
                                expression::Expression::new(ExpressionInner::StringLiteral {
                                    loc: loc.dupe(),
                                    inner: Arc::new(literal.clone()),
                                })
                            }
                            expression::object::Key::NumberLiteral((loc, literal)) => {
                                expression::Expression::new(ExpressionInner::NumberLiteral {
                                    loc: loc.dupe(),
                                    inner: Arc::new(literal.clone()),
                                })
                            }
                            expression::object::Key::BigIntLiteral((loc, literal)) => {
                                expression::Expression::new(ExpressionInner::BigIntLiteral {
                                    loc: loc.dupe(),
                                    inner: Arc::new(literal.clone()),
                                })
                            }
                            _ => {
                                return self.syntax(&property.loc, "Invalid record property key.");
                            }
                        },
                        object,
                    ),
                ),
            };
            let assignment = ast_builder::expressions::assignment(
                Some(property.loc.dupe()),
                None,
                pattern::Pattern::Expression {
                    loc: property.loc.dupe(),
                    inner: Arc::new(member),
                },
                None,
                builders::identifier(&property.loc, binding_name.as_str()),
            );
            assignments.push(ast_builder::statements::expression(
                Some(property.loc.dupe()),
                None,
                None,
                assignment,
            ));
        }

        let constructor_function = ast_builder::functions::make(
            None,
            Some(ast_builder::functions::params(
                Some(construct_loc.dupe()),
                None,
                None,
                None,
                vec![ast_builder::functions::param(
                    Some(construct_loc.dupe()),
                    None,
                    constructor_param,
                )],
            )),
            None,
            None,
            Some(false),
            None,
            Some(false),
            Some(ast_builder::functions::body(
                Some(construct_loc.dupe()),
                None,
                assignments,
            )),
        );
        let constructor = class::BodyElement::Method(class::Method {
            loc: construct_loc.dupe(),
            kind: class::MethodKind::Constructor,
            key: expression::object::Key::Identifier(ast_builder::identifiers::identifier(
                Some(construct_loc.dupe()),
                "constructor",
            )),
            value: (construct_loc.dupe(), constructor_function),
            static_: false,
            override_: false,
            ts_accessibility: None,
            decorators: Arc::from([]),
            comments: None,
        });

        let mut elements = vec![constructor];
        elements.extend(methods);
        elements.extend(static_properties.into_iter().map(|property| {
            class::BodyElement::Property(class::Property {
                loc: property.loc.dupe(),
                key: property.key.clone(),
                value: class::property::Value::Initialized(property.value.clone()),
                annot: ast::types::AnnotationOrHint::Missing(property.loc.dupe()),
                static_: true,
                override_: false,
                optional: false,
                definite: false,
                variance: None,
                ts_accessibility: None,
                decorators: Arc::from([]),
                comments: None,
            })
        }));
        elements.extend(static_methods);

        Some(statement::Statement::new(
            StatementInner::ClassDeclaration {
                loc: construct_loc.dupe(),
                inner: Arc::new(class::Class {
                    id: Some(declaration.id.dupe()),
                    body: class::Body {
                        loc: construct_loc.dupe(),
                        body: elements.into(),
                        comments: None,
                    },
                    tparams: None,
                    extends: None,
                    implements: None,
                    class_decorators: Arc::from([]),
                    abstract_: false,
                    comments: None,
                }),
            },
        ))
    }

    fn map_record_expression(&mut self, loc: &Loc, record: &expression::Record<Loc, Loc>) -> Expr {
        let object = ast_builder::expressions::object_(
            None,
            Some(record.properties.0.dupe()),
            record.properties.1.properties.to_vec(),
        );
        ast_builder::expressions::new_(
            Some(loc.dupe()),
            None,
            None,
            Some(ast_builder::expressions::arg_list(
                Some(record.properties.0.dupe()),
                None,
                vec![ast_builder::expressions::expression_or_spread(object)],
            )),
            record.constructor.clone(),
        )
    }
}

impl<'ast> AstVisitor<'ast, Loc> for RecordLowerer {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn map_identifier(
        &mut self,
        identifier: &'ast ast::Identifier<Loc, Loc>,
    ) -> ast::Identifier<Loc, Loc> {
        self.gen_id.add_usage(&identifier.name);
        identifier.dupe()
    }

    fn map_expression(&mut self, expression: &'ast Expr) -> Expr {
        let mapped = ast_visitor::map_expression_default(self, expression);
        if let ExpressionInner::Record { loc, inner } = &*mapped {
            self.map_record_expression(loc, inner)
        } else {
            mapped
        }
    }

    fn map_statement(&mut self, statement: &'ast Stmt) -> Stmt {
        let mapped = ast_visitor::map_statement_default(self, statement);
        if let StatementInner::RecordDeclaration { loc, inner } = &*mapped {
            self.map_record_declaration(loc, inner)
                .unwrap_or_else(|| mapped.dupe())
        } else {
            mapped
        }
    }
}

pub fn lower_program(
    program: &ast::Program<Loc, Loc>,
) -> Result<ast::Program<Loc, Loc>, BabelLoweringError> {
    let mut lowerer = RecordLowerer {
        gen_id: GenId::new("r"),
        error: None,
    };
    let program = lowerer.map_program(program);
    match lowerer.error {
        Some(error) => Err(error),
        None => Ok(program),
    }
}
