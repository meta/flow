/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::HashSet;
use std::ops::Deref;
use std::sync::Arc;

use dupe::Dupe;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::expression;
use flow_parser::ast::expression::ExpressionInner;
use flow_parser::ast::function;
use flow_parser::ast::pattern;
use flow_parser::ast::statement;
use flow_parser::ast::statement::StatementInner;
use flow_parser::ast::types;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::loc::Loc;
use flow_parser_utils::ast_builder;

use super::ReactRuntimeTarget;
use super::builders;
use super::error::BabelLoweringError;

type Expr = expression::Expression<Loc, Loc>;
type Stmt = statement::Statement<Loc, Loc>;

struct ComponentParams {
    props: Option<function::Param<Loc, Loc>>,
    ref_: Option<function::Param<Loc, Loc>>,
}

struct ComponentResult {
    function: Stmt,
    wrapper: Option<Stmt>,
    exported_id: ast::Identifier<Loc, Loc>,
}

struct ComponentLowerer {
    target: ReactRuntimeTarget,
    error: Option<BabelLoweringError>,
    metadata: ComponentMetadata,
}

#[derive(Default)]
pub(crate) struct ComponentMetadata {
    pub(crate) component_functions: HashSet<Loc>,
    pub(crate) hook_functions: HashSet<Loc>,
    pub(crate) component_patterns_with_optional: HashSet<Loc>,
    pub(crate) component_rests_with_type_annotation: HashSet<Loc>,
}

impl ComponentLowerer {
    fn syntax<T>(&mut self, loc: &Loc, message: impl Into<String>) -> Option<T> {
        if self.error.is_none() {
            self.error = Some(BabelLoweringError::syntax(loc, message));
        }
        None
    }

    fn any_type(loc: &Loc) -> types::Type<Loc, Loc> {
        types::Type::new(types::TypeInner::Any {
            loc: loc.dupe(),
            comments: None,
        })
    }

    fn pattern_annotation(
        pattern: &pattern::Pattern<Loc, Loc>,
    ) -> Option<types::Annotation<Loc, Loc>> {
        let annotation = match pattern {
            pattern::Pattern::Identifier { inner, .. } => &inner.annot,
            pattern::Pattern::Object { inner, .. } => &inner.annot,
            pattern::Pattern::Array { inner, .. } => &inner.annot,
            pattern::Pattern::Expression { .. } => return None,
        };
        match annotation {
            types::AnnotationOrHint::Available(annotation) => Some(annotation.clone()),
            types::AnnotationOrHint::Missing(_) => None,
        }
    }

    fn pattern_optional(pattern: &pattern::Pattern<Loc, Loc>) -> bool {
        match pattern {
            pattern::Pattern::Identifier { inner, .. } => inner.optional,
            pattern::Pattern::Object { inner, .. } => inner.optional,
            pattern::Pattern::Array { inner, .. } => inner.optional,
            pattern::Pattern::Expression { .. } => false,
        }
    }

    fn strip_pattern_annotation(
        pattern: &pattern::Pattern<Loc, Loc>,
    ) -> pattern::Pattern<Loc, Loc> {
        match pattern {
            pattern::Pattern::Identifier { loc, inner } => pattern::Pattern::Identifier {
                loc: loc.dupe(),
                inner: Arc::new(pattern::Identifier {
                    name: inner.name.dupe(),
                    annot: types::AnnotationOrHint::Missing(loc.dupe()),
                    optional: false,
                }),
            },
            pattern::Pattern::Object { loc, inner } => pattern::Pattern::Object {
                loc: loc.dupe(),
                inner: Arc::new(pattern::Object {
                    properties: inner.properties.clone(),
                    annot: types::AnnotationOrHint::Missing(loc.dupe()),
                    optional: false,
                    comments: inner.comments.dupe(),
                }),
            },
            pattern::Pattern::Array { loc, inner } => pattern::Pattern::Array {
                loc: loc.dupe(),
                inner: Arc::new(pattern::Array {
                    elements: inner.elements.clone(),
                    annot: types::AnnotationOrHint::Missing(loc.dupe()),
                    optional: false,
                    comments: inner.comments.dupe(),
                }),
            },
            pattern::Pattern::Expression { .. } => pattern.clone(),
        }
    }

    fn mark_component_parameter_pattern(&mut self, pattern: &pattern::Pattern<Loc, Loc>) {
        if matches!(
            pattern,
            pattern::Pattern::Object { .. } | pattern::Pattern::Array { .. }
        ) {
            self.metadata
                .component_patterns_with_optional
                .insert(pattern.loc().dupe());
        }
    }

    fn object_property_loc(property: &pattern::object::Property<Loc, Loc>) -> &Loc {
        match property {
            pattern::object::Property::NormalProperty(property) => &property.loc,
            pattern::object::Property::RestElement(rest) => &rest.loc,
        }
    }

    fn param_name(param: &statement::component_params::Param<Loc, Loc>) -> &FlowSmolStr {
        match &param.name {
            statement::component_params::ParamName::Identifier(id) => &id.name,
            statement::component_params::ParamName::StringLiteral((_, literal)) => &literal.value,
        }
    }

    fn expression_key(
        name: &statement::component_params::ParamName<Loc, Loc>,
    ) -> expression::object::Key<Loc, Loc> {
        match name {
            statement::component_params::ParamName::Identifier(id) => {
                expression::object::Key::Identifier(id.dupe())
            }
            statement::component_params::ParamName::StringLiteral(value) => {
                expression::object::Key::StringLiteral(value.clone())
            }
        }
    }

    fn pattern_key(
        name: &statement::component_params::ParamName<Loc, Loc>,
    ) -> pattern::object::Key<Loc, Loc> {
        match name {
            statement::component_params::ParamName::Identifier(id) => {
                pattern::object::Key::Identifier(id.dupe())
            }
            statement::component_params::ParamName::StringLiteral(value) => {
                pattern::object::Key::StringLiteral(value.clone())
            }
        }
    }

    fn props_type(
        &self,
        params: &[&statement::component_params::Param<Loc, Loc>],
        rest: Option<&statement::component_params::RestParam<Loc, Loc>>,
        loc: Loc,
    ) -> types::AnnotationOrHint<Loc, Loc> {
        let mut properties = params
            .iter()
            .map(|param| {
                let annotation = Self::pattern_annotation(&param.local)
                    .map(|annotation| annotation.annotation)
                    .unwrap_or_else(|| Self::any_type(param.local.loc()));
                types::object::Property::NormalProperty(types::object::NormalProperty {
                    loc: param.loc.dupe(),
                    key: Self::expression_key(&param.name),
                    value: types::object::PropertyValue::Init(Some(annotation)),
                    optional: param.default.is_some() || Self::pattern_optional(&param.local),
                    static_: false,
                    proto: false,
                    method: false,
                    abstract_: false,
                    override_: false,
                    variance: None,
                    ts_accessibility: None,
                    init: None,
                    comments: None,
                })
            })
            .collect::<Vec<_>>();
        let spread = rest.map(|rest| {
            Self::pattern_annotation(&rest.argument)
                .map(|annotation| annotation.annotation)
                .unwrap_or_else(|| Self::any_type(&rest.loc))
        });
        let annotation = if let Some((rest, spread)) = rest.zip(spread) {
            if params.is_empty() {
                spread
            } else {
                properties.insert(
                    0,
                    types::object::Property::SpreadProperty(types::object::SpreadProperty {
                        loc: rest.loc.dupe(),
                        argument: spread,
                        comments: None,
                    }),
                );
                Self::component_props_object(loc.dupe(), properties)
            }
        } else {
            Self::component_props_object(loc.dupe(), properties)
        };
        types::AnnotationOrHint::Available(types::Annotation { loc, annotation })
    }

    fn component_props_object(
        loc: Loc,
        properties: Vec<types::object::Property<Loc, Loc>>,
    ) -> types::Type<Loc, Loc> {
        let object = types::Type::new(types::TypeInner::Object {
            loc: loc.dupe(),
            inner: Arc::new(types::Object {
                exact: false,
                inexact: false,
                properties: properties.into(),
                comments: None,
            }),
        });
        types::Type::new(types::TypeInner::Generic {
            loc: loc.dupe(),
            inner: Arc::new(types::Generic {
                id: types::generic::Identifier::Unqualified(ast_builder::identifiers::identifier(
                    Some(loc.dupe()),
                    "$ReadOnly",
                )),
                targs: Some(types::TypeArgs {
                    loc,
                    arguments: Arc::from([object]),
                    comments: None,
                }),
                comments: None,
            }),
        })
    }

    fn component_params(
        &mut self,
        params: &statement::component_params::Params<Loc, Loc>,
    ) -> Option<ComponentParams> {
        if params.params.is_empty() && params.rest.is_none() {
            return Some(ComponentParams {
                props: None,
                ref_: None,
            });
        }
        if params.params.is_empty()
            && let Some(rest) = &params.rest
            && matches!(rest.argument, pattern::Pattern::Identifier { .. })
        {
            return Some(ComponentParams {
                props: Some(ast_builder::functions::param(
                    Some(rest.loc.dupe()),
                    None,
                    rest.argument.clone(),
                )),
                ref_: None,
            });
        }

        let ref_param = if matches!(self.target, ReactRuntimeTarget::React18) {
            params
                .params
                .iter()
                .find(|param| Self::param_name(param).as_str() == "ref")
        } else {
            None
        };
        let normal_params = params
            .params
            .iter()
            .filter(|param| ref_param.is_none_or(|ref_param| !std::ptr::eq(*param, ref_param)))
            .collect::<Vec<_>>();

        let mut properties = normal_params
            .iter()
            .map(|param| {
                self.mark_component_parameter_pattern(&param.local);
                pattern::object::Property::NormalProperty(pattern::object::NormalProperty {
                    loc: param.loc.dupe(),
                    key: Self::pattern_key(&param.name),
                    pattern: Self::strip_pattern_annotation(&param.local),
                    default: param.default.clone(),
                    shorthand: param.shorthand
                        && matches!(
                            param.name,
                            statement::component_params::ParamName::Identifier(_)
                        ),
                })
            })
            .collect::<Vec<_>>();
        if let Some(rest) = &params.rest {
            match &rest.argument {
                pattern::Pattern::Identifier { .. } => {
                    properties.push(pattern::object::Property::RestElement(
                        pattern::RestElement {
                            loc: rest.loc.dupe(),
                            argument: Self::strip_pattern_annotation(&rest.argument),
                            comments: rest.comments.dupe(),
                        },
                    ));
                }
                pattern::Pattern::Object { inner, .. } => {
                    for property in inner.properties.iter() {
                        if let pattern::object::Property::RestElement(rest) = property {
                            self.metadata
                                .component_rests_with_type_annotation
                                .insert(rest.loc.dupe());
                        }
                    }
                    properties.extend(inner.properties.iter().cloned());
                }
                other => {
                    return self.syntax(other.loc(), "Invalid component rest parameter pattern.");
                }
            }
        }

        let props = if properties.is_empty() {
            let ref_param = ref_param?;
            let empty_loc = Loc {
                source: ref_param.loc.source.clone(),
                start: ref_param.loc.start,
                end: ref_param.loc.start,
            };
            let id = ast::Identifier::new(ast::IdentifierInner {
                loc: empty_loc.dupe(),
                name: FlowSmolStr::from("_$$empty_props_placeholder$$"),
                comments: None,
            });
            let pattern = pattern::Pattern::Identifier {
                loc: empty_loc.dupe(),
                inner: Arc::new(pattern::Identifier {
                    name: id,
                    annot: self.props_type(&[], None, empty_loc.dupe()),
                    optional: false,
                }),
            };
            Some(ast_builder::functions::param(
                Some(empty_loc),
                None,
                pattern,
            ))
        } else {
            let first = Self::object_property_loc(
                properties
                    .first()
                    .expect("component props have a source location"),
            );
            let last = Self::object_property_loc(
                properties
                    .last()
                    .expect("component props have a source location"),
            );
            let object_loc = Loc::between(first, last);
            let annotation_loc = Loc {
                source: last.source.clone(),
                start: last.end,
                end: last.end,
            };
            let pattern = pattern::Pattern::Object {
                loc: object_loc.dupe(),
                inner: Arc::new(pattern::Object {
                    properties: properties.into(),
                    annot: self.props_type(&normal_params, params.rest.as_ref(), annotation_loc),
                    optional: false,
                    comments: None,
                }),
            };
            Some(ast_builder::functions::param(
                Some(object_loc),
                None,
                pattern,
            ))
        };
        let ref_ = ref_param.map(|param| {
            ast_builder::functions::param(
                Some(param.loc.dupe()),
                param.default.clone(),
                param.local.clone(),
            )
        });
        Some(ComponentParams { props, ref_ })
    }

    fn react_node_return(body_loc: &Loc) -> function::ReturnAnnot<Loc, Loc> {
        let loc = Loc {
            source: body_loc.source.clone(),
            start: body_loc.end,
            end: body_loc.end,
        };
        let react = ast_builder::identifiers::identifier(Some(loc.dupe()), "React");
        let node = ast_builder::identifiers::identifier(Some(loc.dupe()), "Node");
        let qualified = types::generic::Qualified {
            loc: loc.dupe(),
            qualification: types::generic::Identifier::Unqualified(react),
            id: node,
        };
        function::ReturnAnnot::Available(types::Annotation {
            loc: loc.dupe(),
            annotation: types::Type::new(types::TypeInner::Generic {
                loc,
                inner: Arc::new(types::Generic {
                    id: types::generic::Identifier::Qualified(Arc::new(qualified)),
                    targs: None,
                    comments: None,
                }),
            }),
        })
    }

    fn component_result(
        &mut self,
        loc: &Loc,
        component: &statement::ComponentDeclaration<Loc, Loc>,
    ) -> Option<ComponentResult> {
        let Some((body_loc, body)) = &component.body else {
            return Some(ComponentResult {
                function: self.declare_component_as_variable(loc, &component.id),
                wrapper: None,
                exported_id: component.id.dupe(),
            });
        };
        let params = self.component_params(&component.params)?;
        let has_ref = params.ref_.is_some();
        let internal_id = if has_ref {
            ast::Identifier::new(ast::IdentifierInner {
                loc: component.id.loc.dupe(),
                name: FlowSmolStr::from(format!("{}_withRef", component.id.name)),
                comments: component.id.comments.dupe(),
            })
        } else {
            component.id.dupe()
        };
        let mut function_params = Vec::new();
        function_params.extend(params.props);
        function_params.extend(params.ref_);
        let function = statement::Statement::new(StatementInner::FunctionDeclaration {
            loc: loc.dupe(),
            inner: Arc::new(function::Function {
                id: Some(internal_id.dupe()),
                params: function::Params {
                    loc: component.params.loc.dupe(),
                    this_: None,
                    params: function_params.into(),
                    rest: None,
                    comments: component.params.comments.dupe(),
                },
                body: function::Body::BodyBlock((body_loc.dupe(), body.clone())),
                async_: component.async_,
                generator: false,
                effect_: function::Effect::Arbitrary,
                predicate: None,
                return_: Self::react_node_return(body_loc),
                tparams: component.tparams.clone(),
                comments: component.comments.dupe(),
                sig_loc: component.sig_loc.dupe(),
            }),
        });
        self.metadata.component_functions.insert(loc.dupe());
        let wrapper = has_ref.then(|| {
            let react = expression::Expression::new(ExpressionInner::Identifier {
                loc: loc.dupe(),
                inner: ast_builder::identifiers::identifier(Some(loc.dupe()), "React"),
            });
            let call = expression::Expression::new(ExpressionInner::Call {
                loc: loc.dupe(),
                inner: Arc::new(expression::Call {
                    callee: builders::member(loc, react, "forwardRef"),
                    targs: None,
                    arguments: expression::ArgList {
                        loc: loc.dupe(),
                        arguments: Arc::from([expression::ExpressionOrSpread::Expression(
                            Self::identifier_expression(&internal_id),
                        )]),
                        comments: None,
                    },
                    comments: None,
                }),
            });
            ast_builder::statements::variable_declaration(
                Some(ast::VariableKind::Const),
                Some(loc.dupe()),
                component.comments.dupe(),
                vec![statement::variable::Declarator {
                    loc: loc.dupe(),
                    id: builders::identifier_pattern(&component.id),
                    init: Some(call),
                    definite: false,
                }],
            )
        });
        Some(ComponentResult {
            function,
            wrapper,
            exported_id: component.id.dupe(),
        })
    }

    fn identifier_expression(id: &ast::Identifier<Loc, Loc>) -> Expr {
        expression::Expression::new(ExpressionInner::Identifier {
            loc: id.loc.dupe(),
            inner: id.dupe(),
        })
    }

    fn declare_component_as_variable(&self, loc: &Loc, id: &ast::Identifier<Loc, Loc>) -> Stmt {
        statement::Statement::new(StatementInner::DeclareVariable {
            loc: loc.dupe(),
            inner: Arc::new(Self::declare_component_as_variable_declaration(loc, id)),
        })
    }

    fn declare_component_as_variable_declaration(
        loc: &Loc,
        id: &ast::Identifier<Loc, Loc>,
    ) -> statement::DeclareVariable<Loc, Loc> {
        let pattern = pattern::Pattern::Identifier {
            loc: id.loc.dupe(),
            inner: Arc::new(pattern::Identifier {
                name: id.dupe(),
                annot: types::AnnotationOrHint::Available(types::Annotation {
                    loc: loc.dupe(),
                    annotation: Self::any_type(loc),
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

    fn statement_references(statement: &Stmt, name: &FlowSmolStr) -> bool {
        struct Finder<'a> {
            name: &'a FlowSmolStr,
            found: bool,
        }
        impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, std::convert::Infallible> for Finder<'_> {
            fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
                loc
            }

            fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
                type_
            }

            fn identifier(
                &mut self,
                identifier: &'ast ast::Identifier<Loc, Loc>,
            ) -> Result<(), std::convert::Infallible> {
                self.found |= &identifier.name == self.name;
                Ok(())
            }
        }
        let mut finder = Finder { name, found: false };
        let Ok(()) = finder.statement(statement);
        finder.found
    }

    fn map_component_into(
        &mut self,
        output: &mut Vec<Stmt>,
        loc: &Loc,
        component: &statement::ComponentDeclaration<Loc, Loc>,
        export: Option<ExportKind<'_>>,
    ) {
        let Some(mut result) = self.component_result(loc, component) else {
            return;
        };
        if let Some(wrapper) = result.wrapper.take() {
            let insert_at = output
                .iter()
                .position(|statement| {
                    Self::statement_references(statement, &result.exported_id.name)
                })
                .unwrap_or(output.len());
            output.insert(insert_at, wrapper);
            output.push(result.function);
            match export {
                Some(ExportKind::Named {
                    loc: export_loc,
                    inner: original,
                }) => {
                    let id = result.exported_id;
                    output.push(statement::Statement::new(StatementInner::ExportNamedDeclaration {
                        loc: export_loc.dupe(),
                        inner: Arc::new(statement::ExportNamedDeclaration {
                            declaration: None,
                            specifiers: Some(
                                statement::export_named_declaration::Specifier::ExportSpecifiers(
                                    vec![statement::export_named_declaration::ExportSpecifier {
                                        loc: loc.dupe(),
                                        local: id.dupe(),
                                        exported: Some(id),
                                        export_kind: statement::ExportKind::ExportValue,
                                        from_remote: false,
                                        imported_name_def_loc: None,
                                    }],
                                ),
                            ),
                            source: None,
                            export_kind: statement::ExportKind::ExportValue,
                            attributes: None,
                            comments: original.comments.dupe(),
                        }),
                    }));
                }
                Some(ExportKind::Default {
                    loc: export_loc,
                    inner: original,
                }) => {
                    output.push(statement::Statement::new(
                        StatementInner::ExportDefaultDeclaration {
                            loc: export_loc.dupe(),
                            inner: Arc::new(statement::ExportDefaultDeclaration {
                                default: original.default.dupe(),
                                declaration:
                                    statement::export_default_declaration::Declaration::Expression(
                                        Self::identifier_expression(&result.exported_id),
                                    ),
                                comments: original.comments.dupe(),
                            }),
                        },
                    ));
                }
                None => {}
            }
        } else {
            let function = match export {
                Some(ExportKind::Named {
                    loc: export_loc,
                    inner: original,
                }) => statement::Statement::new(StatementInner::ExportNamedDeclaration {
                    loc: export_loc.dupe(),
                    inner: Arc::new(statement::ExportNamedDeclaration {
                        declaration: Some(result.function),
                        specifiers: original.specifiers.clone(),
                        source: original.source.clone(),
                        attributes: original.attributes.clone(),
                        export_kind: original.export_kind,
                        comments: original.comments.dupe(),
                    }),
                }),
                Some(ExportKind::Default {
                    loc: export_loc,
                    inner: original,
                }) => statement::Statement::new(StatementInner::ExportDefaultDeclaration {
                    loc: export_loc.dupe(),
                    inner: Arc::new(statement::ExportDefaultDeclaration {
                        default: original.default.dupe(),
                        declaration:
                            statement::export_default_declaration::Declaration::Declaration(
                                result.function,
                            ),
                        comments: original.comments.dupe(),
                    }),
                }),
                None => result.function,
            };
            output.push(function);
        }
    }

    fn map_statement_list_inner(&mut self, statements: &[Stmt]) -> Vec<Stmt> {
        let mut output = Vec::with_capacity(statements.len());
        for statement in statements {
            match &**statement {
                StatementInner::ComponentDeclaration { loc, inner } => {
                    let mapped = ast_visitor::map_component_declaration_default(self, loc, inner);
                    self.map_component_into(&mut output, loc, &mapped, None);
                }
                StatementInner::FunctionDeclaration { loc, inner }
                    if inner.effect_ == function::Effect::Hook =>
                {
                    let function = self.map_hook_function(loc, inner);
                    output.push(statement::Statement::new(
                        StatementInner::FunctionDeclaration {
                            loc: loc.dupe(),
                            inner: Arc::new(function),
                        },
                    ));
                }
                StatementInner::ExportNamedDeclaration { loc, inner }
                    if matches!(
                        inner.declaration.as_deref(),
                        Some(StatementInner::FunctionDeclaration { inner, .. })
                            if inner.effect_ == function::Effect::Hook
                    ) =>
                {
                    let Some(declaration) = &inner.declaration else {
                        continue;
                    };
                    let StatementInner::FunctionDeclaration {
                        loc: function_loc,
                        inner: function,
                    } = &**declaration
                    else {
                        continue;
                    };
                    let function = self.map_hook_function(function_loc, function);
                    output.push(statement::Statement::new(
                        StatementInner::ExportNamedDeclaration {
                            loc: loc.dupe(),
                            inner: Arc::new(statement::ExportNamedDeclaration {
                                declaration: Some(statement::Statement::new(
                                    StatementInner::FunctionDeclaration {
                                        loc: function_loc.dupe(),
                                        inner: Arc::new(function),
                                    },
                                )),
                                specifiers: inner.specifiers.clone(),
                                source: inner.source.clone(),
                                attributes: inner.attributes.clone(),
                                export_kind: inner.export_kind,
                                comments: inner.comments.dupe(),
                            }),
                        },
                    ));
                }
                StatementInner::ExportDefaultDeclaration { loc, inner }
                    if matches!(
                        &inner.declaration,
                        statement::export_default_declaration::Declaration::Declaration(declaration)
                            if matches!(&**declaration, StatementInner::FunctionDeclaration { inner, .. }
                                if inner.effect_ == function::Effect::Hook)
                    ) =>
                {
                    let statement::export_default_declaration::Declaration::Declaration(
                        declaration,
                    ) = &inner.declaration
                    else {
                        continue;
                    };
                    let StatementInner::FunctionDeclaration {
                        loc: function_loc,
                        inner: function,
                    } = &**declaration
                    else {
                        continue;
                    };
                    let function = self.map_hook_function(function_loc, function);
                    output.push(statement::Statement::new(
                        StatementInner::ExportDefaultDeclaration {
                            loc: loc.dupe(),
                            inner: Arc::new(statement::ExportDefaultDeclaration {
                                default: inner.default.dupe(),
                                declaration:
                                    statement::export_default_declaration::Declaration::Declaration(
                                        statement::Statement::new(
                                            StatementInner::FunctionDeclaration {
                                                loc: function_loc.dupe(),
                                                inner: Arc::new(function),
                                            },
                                        ),
                                    ),
                                comments: inner.comments.dupe(),
                            }),
                        },
                    ));
                }
                StatementInner::ExportNamedDeclaration { loc, inner }
                    if matches!(
                        inner.declaration.as_deref(),
                        Some(StatementInner::ComponentDeclaration { .. })
                    ) =>
                {
                    let Some(declaration) = &inner.declaration else {
                        continue;
                    };
                    let StatementInner::ComponentDeclaration {
                        loc: component_loc,
                        inner: component,
                    } = &**declaration
                    else {
                        continue;
                    };
                    let mapped = ast_visitor::map_component_declaration_default(
                        self,
                        component_loc,
                        component,
                    );
                    self.map_component_into(
                        &mut output,
                        component_loc,
                        &mapped,
                        Some(ExportKind::Named { loc, inner }),
                    );
                }
                StatementInner::ExportDefaultDeclaration { loc, inner }
                    if matches!(
                        &inner.declaration,
                        statement::export_default_declaration::Declaration::Declaration(declaration)
                            if matches!(&**declaration, StatementInner::ComponentDeclaration { .. })
                    ) =>
                {
                    let statement::export_default_declaration::Declaration::Declaration(
                        declaration,
                    ) = &inner.declaration
                    else {
                        continue;
                    };
                    let StatementInner::ComponentDeclaration {
                        loc: component_loc,
                        inner: component,
                    } = &**declaration
                    else {
                        continue;
                    };
                    let mapped = ast_visitor::map_component_declaration_default(
                        self,
                        component_loc,
                        component,
                    );
                    self.map_component_into(
                        &mut output,
                        component_loc,
                        &mapped,
                        Some(ExportKind::Default { loc, inner }),
                    );
                }
                _ => output.push(self.map_statement(statement)),
            }
        }
        output
    }

    fn map_hook_function(
        &mut self,
        loc: &Loc,
        function: &function::Function<Loc, Loc>,
    ) -> function::Function<Loc, Loc> {
        let mut function = ast_visitor::map_function_default(self, loc, function);
        function.effect_ = function::Effect::Arbitrary;
        self.metadata.hook_functions.insert(loc.dupe());
        function
    }
}

enum ExportKind<'a> {
    Named {
        loc: &'a Loc,
        inner: &'a statement::ExportNamedDeclaration<Loc, Loc>,
    },
    Default {
        loc: &'a Loc,
        inner: &'a statement::ExportDefaultDeclaration<Loc, Loc>,
    },
}

impl<'ast> AstVisitor<'ast, Loc> for ComponentLowerer {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn map_statement_list(&mut self, statements: &'ast Arc<[Stmt]>) -> Arc<[Stmt]> {
        self.map_statement_list_inner(statements).into()
    }

    fn map_switch_case(
        &mut self,
        case: &'ast statement::switch::Case<Loc, Loc>,
    ) -> statement::switch::Case<Loc, Loc> {
        statement::switch::Case {
            loc: case.loc.dupe(),
            test: case.test.as_ref().map(|test| self.map_expression(test)),
            case_test_loc: case.case_test_loc.dupe(),
            consequent: self.map_statement_list_inner(&case.consequent).into(),
            comments: self.map_syntax_opt(case.comments.as_ref()),
        }
    }

    fn map_declare_function(
        &mut self,
        loc: &'ast Loc,
        declaration: &'ast statement::DeclareFunction<Loc, Loc>,
    ) -> statement::DeclareFunction<Loc, Loc> {
        let mut declaration = ast_visitor::map_declare_function_default(self, loc, declaration);
        if let types::TypeInner::Function {
            loc: type_loc,
            inner: function_type,
        } = declaration.annot.annotation.deref()
            && function_type.effect == function::Effect::Hook
        {
            let mut function_type = function_type.as_ref().clone();
            function_type.effect = function::Effect::Arbitrary;
            declaration.annot.annotation = types::Type::new(types::TypeInner::Function {
                loc: type_loc.dupe(),
                inner: Arc::new(function_type),
            });
        }
        declaration
    }

    fn map_declare_export_declaration_decl(
        &mut self,
        declaration: &'ast statement::declare_export_declaration::Declaration<Loc, Loc>,
    ) -> statement::declare_export_declaration::Declaration<Loc, Loc> {
        match declaration {
            statement::declare_export_declaration::Declaration::Component { loc, declaration } => {
                statement::declare_export_declaration::Declaration::Variable {
                    loc: loc.dupe(),
                    declaration: Arc::new(Self::declare_component_as_variable_declaration(
                        loc,
                        &declaration.id,
                    )),
                }
            }
            _ => ast_visitor::map_declare_export_declaration_decl_default(self, declaration),
        }
    }

    fn map_statement(&mut self, statement: &'ast Stmt) -> Stmt {
        match &**statement {
            StatementInner::DeclareComponent { loc, inner } => {
                self.declare_component_as_variable(loc, &inner.id)
            }
            StatementInner::ComponentDeclaration { loc, inner } if inner.body.is_none() => {
                self.declare_component_as_variable(loc, &inner.id)
            }
            StatementInner::ComponentDeclaration { loc, .. } => {
                self.syntax::<()>(
                    loc,
                    "Components must be defined at the top level of a module or within a BlockStatement.",
                );
                statement.clone()
            }
            StatementInner::FunctionDeclaration { loc, inner }
                if inner.effect_ == function::Effect::Hook =>
            {
                self.syntax::<()>(
                    loc,
                    "Hooks must be defined at the top level of a module or within a BlockStatement.",
                );
                statement.clone()
            }
            _ => ast_visitor::map_statement_default(self, statement),
        }
    }
}

pub(crate) fn lower_program(
    program: &ast::Program<Loc, Loc>,
    target: ReactRuntimeTarget,
) -> Result<(ast::Program<Loc, Loc>, ComponentMetadata), BabelLoweringError> {
    let mut lowerer = ComponentLowerer {
        target,
        error: None,
        metadata: ComponentMetadata::default(),
    };
    let program = lowerer.map_program(program);
    match lowerer.error {
        Some(error) => Err(error),
        None => Ok((program, lowerer.metadata)),
    }
}
