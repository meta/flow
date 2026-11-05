/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;

use dupe::Dupe;
use dupe::OptionDupedExt;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use serde_json::Map;
use serde_json::Number;
use serde_json::Value;
use serde_json::json;

use crate::ast;
use crate::ast_utils;
use crate::comment_attachment;
use crate::loc::Loc;
use crate::loc::Position;
use crate::offset_utils::OffsetTable;
use crate::parse_error::ParseError;

pub struct Config {
    pub include_locs: bool,
    pub include_filename: bool,
    pub offset_style: OffsetStyle,
}

#[derive(Clone, Copy)]
pub enum OffsetStyle {
    Utf8Bytes,
    JsIndices,
}

fn loc_and_range(
    offset_table: &OffsetTable,
    include_filename: bool,
    offset_style: OffsetStyle,
    location: &Loc,
) -> (Value, Value) {
    let source = if include_filename {
        match &location.source {
            Some(f) => Value::String(f.as_str().to_owned()),
            None => Value::Null,
        }
    } else {
        Value::Null
    };

    fn js_position(offset_table: &OffsetTable, p: Position) -> Position {
        match offset_table.convert_flow_position_to_js_position(p) {
            Ok(p) => p,
            Err(e) => panic!("{}", e.debug_to_string()),
        }
    }

    let js_start_pos = js_position(offset_table, location.start);
    let js_end_pos = js_position(offset_table, location.end);

    let loc = json!({
        "source": source,
        "start": {
            "line": js_start_pos.line,
            "column": js_start_pos.column
        },
        "end": {
            "line": js_end_pos.line,
            "column": js_end_pos.column
        }
    });

    fn offset(
        offset_table: &OffsetTable,
        offset_style: OffsetStyle,
        flow_position: Position,
    ) -> Value {
        match offset_table.offsets_of_flow_position(flow_position) {
            Ok((utf8, utf16)) => Value::Number(Number::from(match offset_style {
                OffsetStyle::Utf8Bytes => utf8,
                OffsetStyle::JsIndices => utf16,
            })),
            Err(e) => {
                // To debug, do
                // dbg!("{}", e.debug_to_string())
                // Value::String(e.debug_to_string())
                panic!("{}", e.debug_to_string())
            }
        }
    }

    let range = Value::Array(vec![
        offset(offset_table, offset_style, location.start),
        offset(offset_table, offset_style, location.end),
    ]);

    (loc, range)
}

fn format_internal_comments(
    comments: Option<&ast::Syntax<Loc, std::sync::Arc<[ast::Comment<Loc>]>>>,
) -> Option<ast::Syntax<Loc, ()>> {
    match comments {
        None => None,
        Some(c) => {
            if c.internal.is_empty() {
                Some(ast::Syntax {
                    leading: c.leading.clone(),
                    trailing: c.trailing.clone(),
                    internal: (),
                })
            } else {
                // Merge internal comments with trailing comments
                let mut new_trailing: Vec<ast::Comment<Loc>> = c.internal.iter().cloned().collect();
                new_trailing.extend(c.trailing.iter().cloned());
                Some(ast::Syntax {
                    leading: c.leading.clone(),
                    trailing: std::sync::Arc::from(new_trailing),
                    internal: (),
                })
            }
        }
    }
}

pub fn errors(
    offset_table: &OffsetTable,
    include_filename: bool,
    offset_style: OffsetStyle,
    errors: &[(Loc, ParseError)],
) -> Value {
    let error_values: Vec<Value> = errors
        .iter()
        .map(|(location, e)| {
            let (loc, _) = loc_and_range(offset_table, include_filename, offset_style, location);
            json!({
                "loc": loc,
                "message": format!("{e}")
            })
        })
        .collect();
    Value::Array(error_values)
}

fn array_of_list<T, F>(items: &[T], f: F) -> Value
where
    F: Fn(&T) -> Value,
{
    Value::Array(items.iter().map(f).collect())
}

fn option<T, F>(opt: &Option<T>, f: F) -> Value
where
    F: Fn(&T) -> Value,
{
    match opt {
        Some(v) => f(v),
        None => Value::Null,
    }
}

fn annotation_or_hint(
    offset_table: &OffsetTable,
    config: &Config,
    annot: &ast::types::AnnotationOrHint<Loc, Loc>,
) -> Value {
    use ast::types::AnnotationOrHint;

    match annot {
        AnnotationOrHint::Available(a) => type_annotation(offset_table, config, a),
        AnnotationOrHint::Missing(_) => Value::Null,
    }
}

fn string(s: &str) -> Value {
    Value::String(s.to_string())
}

fn bool_value(b: bool) -> Value {
    Value::Bool(b)
}

fn obj(props: Vec<(&str, Value)>) -> Value {
    let mut map = Map::new();
    for (key, value) in props {
        map.insert(key.to_string(), value);
    }
    Value::Object(map)
}

fn node(
    offset_table: &OffsetTable,
    config: &Config,
    node_type: &str,
    location: &Loc,
    comments: Option<&ast::Syntax<Loc, ()>>,
    props: Vec<(&str, Value)>,
) -> Value {
    let mut obj = Map::new();
    obj.insert("type".to_string(), Value::String(node_type.to_string()));
    if let Some(c) = comments {
        match (&c.leading[..], &c.trailing[..]) {
            ([_, ..], [_, ..]) => {
                obj.insert(
                    "leadingComments".to_string(),
                    comment_list(offset_table, config, &c.leading),
                );
                obj.insert(
                    "trailingComments".to_string(),
                    comment_list(offset_table, config, &c.trailing),
                );
            }
            ([_, ..], []) => {
                obj.insert(
                    "leadingComments".to_string(),
                    comment_list(offset_table, config, &c.leading),
                );
            }
            ([], [_, ..]) => {
                obj.insert(
                    "trailingComments".to_string(),
                    comment_list(offset_table, config, &c.trailing),
                );
            }
            ([], []) => {}
        }
    }
    if config.include_locs {
        let (loc, range) = loc_and_range(
            offset_table,
            config.include_filename,
            config.offset_style,
            location,
        );
        obj.insert("range".to_string(), range);
        obj.insert("loc".to_string(), loc);
    }
    for (key, value) in props {
        obj.insert(key.to_owned(), value);
    }
    Value::Object(obj)
}

pub fn program(
    offset_table: &OffsetTable,
    config: &Config,
    program: &ast::Program<Loc, Loc>,
) -> Value {
    let body = statement_list(offset_table, config, &program.statements);
    let comments = comment_list(offset_table, config, &program.all_comments);

    let mut props = vec![("body", body), ("comments", comments)];

    if let Some((loc, value)) = &program.interpreter {
        let directive = node(
            offset_table,
            config,
            "InterpreterDirective",
            loc,
            None,
            vec![("value", string(value))],
        );
        props.push(("interpreter", directive));
    }

    node(
        offset_table,
        config,
        "Program",
        &program.loc,
        program.comments.as_ref(),
        props,
    )
}

fn statement_list(
    offset_table: &OffsetTable,
    config: &Config,
    statements: &[ast::statement::Statement<Loc, Loc>],
) -> Value {
    array_of_list(statements, |s| statement(offset_table, config, s))
}

fn statement(
    offset_table: &OffsetTable,
    config: &Config,
    stmt: &ast::statement::Statement<Loc, Loc>,
) -> Value {
    use ast::statement::StatementInner;

    match stmt.deref() {
        StatementInner::Empty { loc, inner } => node(
            offset_table,
            config,
            "EmptyStatement",
            loc,
            inner.comments.as_ref(),
            vec![],
        ),
        StatementInner::Block { loc, inner } => block(offset_table, config, loc, inner),
        StatementInner::Expression { loc, inner } => node(
            offset_table,
            config,
            "ExpressionStatement",
            loc,
            inner.comments.as_ref(),
            vec![
                (
                    "expression",
                    expression(offset_table, config, false, &inner.expression),
                ),
                ("directive", option(&inner.directive, |d| string(d))),
            ],
        ),
        StatementInner::If { loc, inner } => {
            let alternate = match &inner.alternate {
                None => Value::Null,
                Some(ast::statement::if_::Alternate {
                    loc: _,
                    body,
                    comments,
                }) => {
                    let stmt =
                        comment_attachment::statement_add_comments(body.clone(), comments.clone());
                    statement(offset_table, config, &stmt)
                }
            };
            node(
                offset_table,
                config,
                "IfStatement",
                loc,
                inner.comments.as_ref(),
                vec![
                    ("test", expression(offset_table, config, false, &inner.test)),
                    (
                        "consequent",
                        statement(offset_table, config, &inner.consequent),
                    ),
                    ("alternate", alternate),
                ],
            )
        }
        StatementInner::Labeled { loc, inner } => node(
            offset_table,
            config,
            "LabeledStatement",
            loc,
            inner.comments.as_ref(),
            vec![
                ("label", identifier(offset_table, config, &inner.label)),
                ("body", statement(offset_table, config, &inner.body)),
            ],
        ),
        StatementInner::Break { loc, inner } => node(
            offset_table,
            config,
            "BreakStatement",
            loc,
            inner.comments.as_ref(),
            vec![(
                "label",
                option(&inner.label, |l| identifier(offset_table, config, l)),
            )],
        ),
        StatementInner::Continue { loc, inner } => node(
            offset_table,
            config,
            "ContinueStatement",
            loc,
            inner.comments.as_ref(),
            vec![(
                "label",
                option(&inner.label, |l| identifier(offset_table, config, l)),
            )],
        ),
        StatementInner::With { loc, inner } => node(
            offset_table,
            config,
            "WithStatement",
            loc,
            inner.comments.as_ref(),
            vec![
                (
                    "object",
                    expression(offset_table, config, false, &inner.object),
                ),
                ("body", statement(offset_table, config, &inner.body)),
            ],
        ),
        StatementInner::TypeAlias { loc, inner } => type_alias(offset_table, config, loc, inner),
        StatementInner::OpaqueType { loc, inner } => {
            opaque_type(offset_table, config, false, loc, inner)
        }
        StatementInner::Match { loc, inner } => node(
            offset_table,
            config,
            "MatchStatement",
            loc,
            inner.comments.as_ref(),
            vec![
                (
                    "argument",
                    expression(offset_table, config, false, &inner.arg),
                ),
                (
                    "cases",
                    array_of_list(&inner.cases, |c| {
                        match_statement_case(offset_table, config, c)
                    }),
                ),
            ],
        ),
        StatementInner::Switch { loc, inner: switch } => node(
            offset_table,
            config,
            "SwitchStatement",
            loc,
            switch.comments.as_ref(),
            vec![
                (
                    "discriminant",
                    expression(offset_table, config, false, &switch.discriminant),
                ),
                (
                    "cases",
                    array_of_list(&switch.cases, |c| switch_case(offset_table, config, c)),
                ),
            ],
        ),
        StatementInner::Return { loc, inner } => node(
            offset_table,
            config,
            "ReturnStatement",
            loc,
            inner.comments.as_ref(),
            vec![(
                "argument",
                option(&inner.argument, |e| {
                    expression(offset_table, config, false, e)
                }),
            )],
        ),
        StatementInner::Throw { loc, inner } => node(
            offset_table,
            config,
            "ThrowStatement",
            loc,
            inner.comments.as_ref(),
            vec![(
                "argument",
                expression(offset_table, config, false, &inner.argument),
            )],
        ),
        StatementInner::Try {
            loc,
            inner: try_stmt,
        } => node(
            offset_table,
            config,
            "TryStatement",
            loc,
            try_stmt.comments.as_ref(),
            vec![
                (
                    "block",
                    block(offset_table, config, &try_stmt.block.0, &try_stmt.block.1),
                ),
                (
                    "handler",
                    option(&try_stmt.handler, |h| catch_clause(offset_table, config, h)),
                ),
                (
                    "finalizer",
                    option(&try_stmt.finalizer, |f| {
                        block(offset_table, config, &f.0, &f.1)
                    }),
                ),
            ],
        ),
        StatementInner::While { loc, inner } => node(
            offset_table,
            config,
            "WhileStatement",
            loc,
            inner.comments.as_ref(),
            vec![
                ("test", expression(offset_table, config, false, &inner.test)),
                ("body", statement(offset_table, config, &inner.body)),
            ],
        ),
        StatementInner::DoWhile { loc, inner } => node(
            offset_table,
            config,
            "DoWhileStatement",
            loc,
            inner.comments.as_ref(),
            vec![
                ("body", statement(offset_table, config, &inner.body)),
                ("test", expression(offset_table, config, false, &inner.test)),
            ],
        ),
        StatementInner::For {
            loc,
            inner: for_stmt,
        } => {
            let init = match &for_stmt.init {
                Some(init) => match init {
                    ast::statement::for_::Init::InitDeclaration((loc, decl)) => {
                        variable_declaration(offset_table, config, loc, decl)
                    }
                    ast::statement::for_::Init::InitExpression(expr) => {
                        expression(offset_table, config, false, expr)
                    }
                },
                None => Value::Null,
            };

            node(
                offset_table,
                config,
                "ForStatement",
                loc,
                for_stmt.comments.as_ref(),
                vec![
                    ("init", init),
                    (
                        "test",
                        option(&for_stmt.test, |e| {
                            expression(offset_table, config, false, e)
                        }),
                    ),
                    (
                        "update",
                        option(&for_stmt.update, |e| {
                            expression(offset_table, config, false, e)
                        }),
                    ),
                    ("body", statement(offset_table, config, &for_stmt.body)),
                ],
            )
        }
        StatementInner::ForIn { loc, inner: for_in } => {
            let left = match &for_in.left {
                ast::statement::for_in::Left::LeftDeclaration((loc, decl)) => {
                    variable_declaration(offset_table, config, loc, decl)
                }
                ast::statement::for_in::Left::LeftPattern(pat) => {
                    pattern(offset_table, config, pat)
                }
            };

            node(
                offset_table,
                config,
                "ForInStatement",
                loc,
                for_in.comments.as_ref(),
                vec![
                    ("left", left),
                    (
                        "right",
                        expression(offset_table, config, false, &for_in.right),
                    ),
                    ("body", statement(offset_table, config, &for_in.body)),
                ],
            )
        }
        StatementInner::ForOf { loc, inner: for_of } => {
            let left = match &for_of.left {
                ast::statement::for_of::Left::LeftDeclaration((loc, decl)) => {
                    variable_declaration(offset_table, config, loc, decl)
                }
                ast::statement::for_of::Left::LeftPattern(pat) => {
                    pattern(offset_table, config, pat)
                }
            };
            node(
                offset_table,
                config,
                "ForOfStatement",
                loc,
                for_of.comments.as_ref(),
                vec![
                    ("left", left),
                    (
                        "right",
                        expression(offset_table, config, false, &for_of.right),
                    ),
                    ("body", statement(offset_table, config, &for_of.body)),
                    ("await", bool_value(for_of.await_)),
                ],
            )
        }
        StatementInner::EnumDeclaration { loc, inner } => {
            enum_declaration(offset_table, config, loc, inner)
        }
        StatementInner::Debugger { loc, inner } => node(
            offset_table,
            config,
            "DebuggerStatement",
            loc,
            inner.comments.as_ref(),
            vec![],
        ),
        StatementInner::ClassDeclaration { loc, inner } => {
            class_declaration(offset_table, config, loc, inner)
        }
        StatementInner::InterfaceDeclaration { loc, inner } => {
            interface_declaration(offset_table, config, loc, inner)
        }
        StatementInner::VariableDeclaration { loc, inner } => {
            variable_declaration(offset_table, config, loc, inner)
        }
        StatementInner::FunctionDeclaration { loc, inner } => {
            function_declaration(offset_table, config, loc, inner)
        }
        StatementInner::ComponentDeclaration { loc, inner } => {
            component_declaration(offset_table, config, loc, inner)
        }
        StatementInner::DeclareVariable { loc, inner } => {
            declare_variable(offset_table, config, loc, inner)
        }
        StatementInner::DeclareFunction { loc, inner } => {
            declare_function(offset_table, config, loc, inner)
        }
        StatementInner::DeclareClass { loc, inner } => {
            declare_class(offset_table, config, loc, inner)
        }
        StatementInner::DeclareComponent { loc, inner } => {
            declare_component(offset_table, config, loc, inner)
        }
        StatementInner::DeclareEnum { loc, inner } => {
            declare_enum(offset_table, config, loc, inner)
        }
        StatementInner::DeclareInterface { loc, inner } => {
            declare_interface(offset_table, config, loc, inner)
        }
        StatementInner::DeclareTypeAlias { loc, inner } => {
            declare_type_alias(offset_table, config, loc, inner)
        }
        StatementInner::DeclareOpaqueType { loc, inner } => {
            opaque_type(offset_table, config, true, loc, inner)
        }
        StatementInner::DeclareModule { loc, inner: module } => {
            let id = match &module.id {
                ast::statement::declare_module::Id::Literal((id_loc, lit)) => {
                    string_literal(offset_table, config, id_loc, lit)
                }
                ast::statement::declare_module::Id::Identifier(id) => {
                    identifier(offset_table, config, id)
                }
            };
            node(
                offset_table,
                config,
                "DeclareModule",
                loc,
                module.comments.as_ref(),
                vec![
                    ("id", id),
                    (
                        "body",
                        block(offset_table, config, &module.body.0, &module.body.1),
                    ),
                ],
            )
        }
        StatementInner::DeclareNamespace {
            loc,
            inner: namespace,
        } => {
            let (id, global) = match &namespace.id {
                ast::statement::declare_namespace::Id::Local(id) => {
                    (identifier(offset_table, config, id), false)
                }
                ast::statement::declare_namespace::Id::Global(id) => {
                    (identifier(offset_table, config, id), true)
                }
            };
            let keyword_str = match namespace.keyword {
                ast::statement::declare_namespace::Keyword::Namespace => "namespace",
                ast::statement::declare_namespace::Keyword::Module => "module",
            };
            let mut props = Vec::with_capacity(5);
            if global {
                props.push(("global", bool_value(global)));
            }
            props.push(("id", id));
            props.push((
                "body",
                block(offset_table, config, &namespace.body.0, &namespace.body.1),
            ));
            props.push(("implicitDeclare", Value::Bool(namespace.implicit_declare)));
            props.push(("keyword", string(keyword_str)));
            node(
                offset_table,
                config,
                "DeclareNamespace",
                loc,
                namespace.comments.as_ref(),
                props,
            )
        }
        StatementInner::DeclareExportDeclaration { loc, inner } => match &inner.specifiers {
            Some(ast::statement::export_named_declaration::Specifier::ExportBatchSpecifier(
                batch,
            )) if batch.specifier.is_none() => node(
                offset_table,
                config,
                "DeclareExportAllDeclaration",
                loc,
                inner.comments.as_ref(),
                vec![(
                    "source",
                    option(&inner.source, |(source_loc, lit)| {
                        string_literal(offset_table, config, source_loc, lit)
                    }),
                )],
            ),
            _ => {
                let declaration = match &inner.declaration {
                    Some(ast::statement::declare_export_declaration::Declaration::Variable {
                        loc: decl_loc,
                        declaration: var,
                    }) => declare_variable(offset_table, config, decl_loc, var),
                    Some(ast::statement::declare_export_declaration::Declaration::Function {
                        loc: decl_loc,
                        declaration: func,
                    }) => declare_function(offset_table, config, decl_loc, func),
                    Some(ast::statement::declare_export_declaration::Declaration::Class {
                        loc: decl_loc,
                        declaration: cls,
                    }) => declare_class(offset_table, config, decl_loc, cls),
                    Some(ast::statement::declare_export_declaration::Declaration::Component {
                        loc: decl_loc,
                        declaration: comp,
                    }) => declare_component(offset_table, config, decl_loc, comp),
                    Some(
                        ast::statement::declare_export_declaration::Declaration::DefaultType {
                            type_: t,
                        },
                    ) => type_(offset_table, config, t),
                    Some(ast::statement::declare_export_declaration::Declaration::NamedType {
                        loc: decl_loc,
                        declaration: decl,
                    }) => type_alias(offset_table, config, decl_loc, decl),
                    Some(
                        ast::statement::declare_export_declaration::Declaration::NamedOpaqueType {
                            loc: decl_loc,
                            declaration: opaque,
                        },
                    ) => opaque_type(offset_table, config, true, decl_loc, opaque),
                    Some(ast::statement::declare_export_declaration::Declaration::Interface {
                        loc: decl_loc,
                        declaration: interface,
                    }) => interface_declaration(offset_table, config, decl_loc, interface),
                    Some(ast::statement::declare_export_declaration::Declaration::Enum {
                        loc: decl_loc,
                        declaration: enum_decl,
                    }) => declare_enum(offset_table, config, decl_loc, enum_decl),
                    Some(ast::statement::declare_export_declaration::Declaration::Namespace {
                        loc: decl_loc,
                        declaration: namespace,
                    }) => declare_namespace(offset_table, config, decl_loc, namespace),
                    None => Value::Null,
                };

                node(
                    offset_table,
                    config,
                    "DeclareExportDeclaration",
                    loc,
                    inner.comments.as_ref(),
                    vec![
                        ("default", bool_value(inner.default.is_some())),
                        ("declaration", declaration),
                        (
                            "specifiers",
                            export_specifiers(offset_table, config, &inner.specifiers),
                        ),
                        (
                            "source",
                            option(&inner.source, |(source_loc, lit)| {
                                string_literal(offset_table, config, source_loc, lit)
                            }),
                        ),
                    ],
                )
            }
        },
        StatementInner::DeclareModuleExports { loc, inner } => node(
            offset_table,
            config,
            "DeclareModuleExports",
            loc,
            inner.comments.as_ref(),
            vec![(
                "typeAnnotation",
                type_annotation(offset_table, config, &inner.annot),
            )],
        ),
        StatementInner::ExportNamedDeclaration { loc, inner } => match &inner.specifiers {
            Some(ast::statement::export_named_declaration::Specifier::ExportBatchSpecifier(
                batch,
            )) => {
                let export_kind_str = match inner.export_kind {
                    ast::statement::ExportKind::ExportType => "type",
                    ast::statement::ExportKind::ExportValue => "value",
                };
                let mut properties = vec![
                    (
                        "source",
                        option(&inner.source, |(source_loc, lit)| {
                            string_literal(offset_table, config, source_loc, lit)
                        }),
                    ),
                    (
                        "exported",
                        option(&batch.specifier, |id| identifier(offset_table, config, id)),
                    ),
                    ("exportKind", string(export_kind_str)),
                ];
                if let Some((_, attrs)) = &inner.attributes {
                    properties.push((
                        "attributes",
                        array_of_list(attrs, |a| import_attribute(offset_table, config, a)),
                    ));
                }
                node(
                    offset_table,
                    config,
                    "ExportAllDeclaration",
                    loc,
                    inner.comments.as_ref(),
                    properties,
                )
            }
            _ => {
                let export_kind_str = match inner.export_kind {
                    ast::statement::ExportKind::ExportType => "type",
                    ast::statement::ExportKind::ExportValue => "value",
                };
                let mut properties = vec![
                    (
                        "declaration",
                        option(&inner.declaration, |stmt| {
                            statement(offset_table, config, stmt)
                        }),
                    ),
                    (
                        "specifiers",
                        export_specifiers(offset_table, config, &inner.specifiers),
                    ),
                    (
                        "source",
                        option(&inner.source, |(source_loc, lit)| {
                            string_literal(offset_table, config, source_loc, lit)
                        }),
                    ),
                    ("exportKind", string(export_kind_str)),
                ];
                if let Some((_, attrs)) = &inner.attributes {
                    properties.push((
                        "attributes",
                        array_of_list(attrs, |a| import_attribute(offset_table, config, a)),
                    ));
                }
                node(
                    offset_table,
                    config,
                    "ExportNamedDeclaration",
                    loc,
                    inner.comments.as_ref(),
                    properties,
                )
            }
        },
        StatementInner::ExportDefaultDeclaration { loc, inner } => {
            let declaration = match &inner.declaration {
                ast::statement::export_default_declaration::Declaration::Declaration(stmt) => {
                    statement(offset_table, config, stmt)
                }
                ast::statement::export_default_declaration::Declaration::Expression(expr) => {
                    expression(offset_table, config, false, expr)
                }
            };
            node(
                offset_table,
                config,
                "ExportDefaultDeclaration",
                loc,
                inner.comments.as_ref(),
                vec![
                    ("declaration", declaration),
                    ("exportKind", string("value")),
                ],
            )
        }
        StatementInner::ExportAssignment { loc, inner } => {
            let expr_value = match &inner.rhs {
                ast::statement::ExportAssignmentRhs::Expression(expr) => {
                    expression(offset_table, config, false, expr)
                }
                ast::statement::ExportAssignmentRhs::DeclareFunction(fn_loc, decl) => {
                    declare_function(offset_table, config, fn_loc, decl)
                }
            };
            node(
                offset_table,
                config,
                "ExportAssignment",
                loc,
                inner.comments.as_ref(),
                vec![("expression", expr_value)],
            )
        }
        StatementInner::NamespaceExportDeclaration { loc, inner } => node(
            offset_table,
            config,
            "NamespaceExportDeclaration",
            loc,
            inner.comments.as_ref(),
            vec![("id", identifier(offset_table, config, &inner.id))],
        ),
        StatementInner::RecordDeclaration { loc, inner } => {
            fn property_key(
                offset_table: &OffsetTable,
                config: &Config,
                key: &ast::expression::object::Key<Loc, Loc>,
            ) -> (Value, bool) {
                use ast::expression::object::Key;
                match key {
                    Key::StringLiteral((loc, lit)) => {
                        (string_literal(offset_table, config, loc, lit), false)
                    }
                    Key::NumberLiteral((loc, lit)) => {
                        (number_literal(offset_table, config, loc, lit), false)
                    }
                    Key::BigIntLiteral((loc, lit)) => {
                        (bigint_literal(offset_table, config, loc, lit), false)
                    }
                    Key::Identifier(id) => (identifier(offset_table, config, id), false),
                    Key::PrivateName(_) => panic!("Internal Error: Private name"),
                    Key::Computed(comp) => (
                        expression(offset_table, config, false, &comp.expression),
                        true,
                    ),
                }
            }

            fn record_property(
                offset_table: &OffsetTable,
                config: &Config,
                property: &ast::statement::record_declaration::Property<Loc, Loc>,
            ) -> Value {
                let (key, computed) = property_key(offset_table, config, &property.key);
                if computed {
                    panic!("Records cannot have computed keys");
                }

                node(
                    offset_table,
                    config,
                    "RecordDeclarationProperty",
                    &property.loc,
                    property.comments.as_ref(),
                    vec![
                        ("key", key),
                        (
                            "typeAnnotation",
                            type_annotation(offset_table, config, &property.annot),
                        ),
                        (
                            "defaultValue",
                            option(&property.default_value, |e| {
                                expression(offset_table, config, false, e)
                            }),
                        ),
                    ],
                )
            }

            fn record_static_property(
                offset_table: &OffsetTable,
                config: &Config,
                property: &ast::statement::record_declaration::StaticProperty<Loc, Loc>,
            ) -> Value {
                let (key, computed) = property_key(offset_table, config, &property.key);
                if computed {
                    panic!("Records cannot have computed keys");
                }

                node(
                    offset_table,
                    config,
                    "RecordDeclarationStaticProperty",
                    &property.loc,
                    property.comments.as_ref(),
                    vec![
                        ("key", key),
                        (
                            "typeAnnotation",
                            type_annotation(offset_table, config, &property.annot),
                        ),
                        (
                            "value",
                            expression(offset_table, config, false, &property.value),
                        ),
                    ],
                )
            }

            fn record_body_element(
                offset_table: &OffsetTable,
                config: &Config,
                element: &ast::statement::record_declaration::BodyElement<Loc, Loc>,
            ) -> Value {
                use ast::statement::record_declaration::BodyElement;

                match element {
                    BodyElement::Property(p) => record_property(offset_table, config, p),
                    BodyElement::StaticProperty(sp) => {
                        record_static_property(offset_table, config, sp)
                    }
                    BodyElement::Method(m) => class_method(offset_table, config, m),
                }
            }

            fn record_body(
                offset_table: &OffsetTable,
                config: &Config,
                body: &ast::statement::record_declaration::Body<Loc, Loc>,
            ) -> Value {
                node(
                    offset_table,
                    config,
                    "RecordDeclarationBody",
                    &body.loc,
                    body.comments.as_ref(),
                    vec![(
                        "elements",
                        array_of_list(&body.body, |elem| {
                            record_body_element(offset_table, config, elem)
                        }),
                    )],
                )
            }

            fn record_implements(
                offset_table: &OffsetTable,
                config: &Config,
                interface: &ast::class::implements::Interface<Loc, Loc>,
            ) -> Value {
                node(
                    offset_table,
                    config,
                    "RecordDeclarationImplements",
                    &interface.loc,
                    None,
                    vec![
                        ("id", {
                            use ast::types::generic;
                            match &interface.id {
                                generic::Identifier::Unqualified(id) => {
                                    identifier(offset_table, config, id)
                                }
                                generic::Identifier::Qualified(q) => {
                                    generic_type_qualified_identifier(offset_table, config, q)
                                }
                                generic::Identifier::ImportTypeAnnot(import_type) => {
                                    import_type_annotation(
                                        offset_table,
                                        config,
                                        &import_type.loc,
                                        import_type,
                                    )
                                }
                            }
                        }),
                        (
                            "typeArguments",
                            option(&interface.targs, |t| type_args(offset_table, config, t)),
                        ),
                    ],
                )
            }

            let record: &ast::statement::RecordDeclaration<Loc, Loc> = inner;
            let implements = match &record.implements {
                Some(impls) => array_of_list(&impls.interfaces, |i| {
                    record_implements(offset_table, config, i)
                }),
                None => Value::Array(vec![]),
            };

            node(
                offset_table,
                config,
                "RecordDeclaration",
                loc,
                record.comments.as_ref(),
                vec![
                    ("id", identifier(offset_table, config, &record.id)),
                    (
                        "typeParameters",
                        option(&record.tparams, |t| {
                            type_parameter_declaration(offset_table, config, t)
                        }),
                    ),
                    ("implements", implements),
                    ("body", record_body(offset_table, config, &record.body)),
                ],
            )
        }
        StatementInner::ImportDeclaration { loc, inner } => {
            let mut specifiers: Vec<Value> = Vec::new();
            if let Some(default) = &inner.default {
                specifiers.push(import_default_specifier(offset_table, config, default));
            }
            match &inner.specifiers {
                Some(ast::statement::import_declaration::Specifier::ImportNamedSpecifiers(
                    named_specs,
                )) => specifiers.extend(
                    named_specs
                        .iter()
                        .map(|spec| import_named_specifier(offset_table, config, spec)),
                ),
                Some(ast::statement::import_declaration::Specifier::ImportNamespaceSpecifier(
                    ns,
                )) => specifiers.push(import_namespace_specifier(offset_table, config, ns)),
                None => {}
            };

            let import_kind_str = match inner.import_kind {
                ast::statement::ImportKind::ImportType => "type",
                ast::statement::ImportKind::ImportTypeof => "typeof",
                ast::statement::ImportKind::ImportValue => "value",
            };

            let mut properties = vec![
                ("specifiers", Value::Array(specifiers)),
                (
                    "source",
                    string_literal(offset_table, config, &inner.source.0, &inner.source.1),
                ),
                ("importKind", string(import_kind_str)),
            ];

            if let Some(phase) = inner.phase {
                properties.push(("phase", string(phase.as_str())));
            }

            if let Some((_, attrs)) = &inner.attributes {
                properties.push((
                    "attributes",
                    array_of_list(attrs, |a| import_attribute(offset_table, config, a)),
                ));
            }

            node(
                offset_table,
                config,
                "ImportDeclaration",
                loc,
                inner.comments.as_ref(),
                properties,
            )
        }
        StatementInner::ImportEqualsDeclaration { loc, inner } => {
            let module_reference_json = match &inner.module_reference {
                ast::statement::import_equals_declaration::ModuleReference::ExternalModuleReference(
                    annot_loc,
                    lit,
                ) => node(
                    offset_table,
                    config,
                    "ExternalModuleReference",
                    annot_loc,
                    None,
                    vec![(
                        "expression",
                        string_literal(offset_table, config, annot_loc, lit),
                    )],
                ),
                ast::statement::import_equals_declaration::ModuleReference::Identifier(git) => {
                    use ast::types::generic;
                    match git {
                        generic::Identifier::Unqualified(id) => {
                            identifier(offset_table, config, id)
                        }
                        generic::Identifier::Qualified(q) => {
                            generic_type_qualified_identifier(offset_table, config, q)
                        }
                        generic::Identifier::ImportTypeAnnot(import_type) => {
                            import_type_annotation(
                                offset_table,
                                config,
                                &import_type.loc,
                                import_type,
                            )
                        }
                    }
                }
            };
            let import_kind_str = match inner.import_kind {
                ast::statement::ImportKind::ImportType => "type",
                ast::statement::ImportKind::ImportTypeof => "typeof",
                ast::statement::ImportKind::ImportValue => "value",
            };
            node(
                offset_table,
                config,
                "ImportEqualsDeclaration",
                loc,
                inner.comments.as_ref(),
                vec![
                    ("id", identifier(offset_table, config, &inner.id)),
                    ("moduleReference", module_reference_json),
                    ("importKind", string(import_kind_str)),
                    ("isExport", bool_value(inner.is_export)),
                ],
            )
        }
    }
}

/// Whether an expression continues an optional chain through non-null assertions.
pub fn is_optional_chain(expr: &ast::expression::Expression<Loc, Loc>) -> bool {
    use ast::expression::ExpressionInner;
    match &**expr {
        ExpressionInner::OptionalMember { .. } | ExpressionInner::OptionalCall { .. } => true,
        ExpressionInner::Unary { loc, inner }
            if inner.operator == ast::expression::UnaryOperator::Nonnull =>
        {
            loc.start == inner.argument.loc().start && is_optional_chain(&inner.argument)
        }
        _ => false,
    }
}

pub fn expression(
    offset_table: &OffsetTable,
    config: &Config,
    in_optional_chain: bool,
    expr: &ast::expression::Expression<Loc, Loc>,
) -> Value {
    use ast::expression::ExpressionInner;

    if !in_optional_chain && is_optional_chain(expr) {
        return node(
            offset_table,
            config,
            "ChainExpression",
            expr.loc(),
            None,
            vec![("expression", expression(offset_table, config, true, expr))],
        );
    }

    match &**expr {
        ExpressionInner::This { loc, inner } => node(
            offset_table,
            config,
            "ThisExpression",
            loc,
            inner.comments.as_ref(),
            vec![],
        ),
        ExpressionInner::Super { loc, inner } => node(
            offset_table,
            config,
            "Super",
            loc,
            inner.comments.as_ref(),
            vec![],
        ),
        ExpressionInner::Array { loc, inner } => {
            let formatted_comments = format_internal_comments(inner.comments.as_ref());
            node(
                offset_table,
                config,
                "ArrayExpression",
                loc,
                formatted_comments.as_ref(),
                vec![
                    (
                        "elements",
                        array_of_list(&inner.elements, |el| {
                            array_element(offset_table, config, el)
                        }),
                    ),
                    ("trailingComma", bool_value(inner.trailing_comma)),
                ],
            )
        }
        ExpressionInner::Object { loc, inner } => {
            let formatted_comments = format_internal_comments(inner.comments.as_ref());
            node(
                offset_table,
                config,
                "ObjectExpression",
                loc,
                formatted_comments.as_ref(),
                vec![(
                    "properties",
                    array_of_list(&inner.properties, |p| {
                        object_property(offset_table, config, p)
                    }),
                )],
            )
        }
        ExpressionInner::Function { loc, inner } => {
            function_expression(offset_table, config, loc, inner)
        }
        ExpressionInner::ArrowFunction { loc, inner: func } => {
            let (body, is_expression) = match &func.body {
                ast::function::Body::BodyBlock(b) => {
                    (block(offset_table, config, &b.0, &b.1), false)
                }
                ast::function::Body::BodyExpression(expr) => {
                    (expression(offset_table, config, false, expr), true)
                }
            };

            let formatted_comments = format_internal_comments(func.params.comments.as_ref());
            let comments = ast_utils::merge_comments(formatted_comments, func.comments.dupe());

            node(
                offset_table,
                config,
                "ArrowFunctionExpression",
                loc,
                comments.as_ref(),
                vec![
                    ("id", Value::Null),
                    (
                        "params",
                        function_params(offset_table, config, &func.params),
                    ),
                    ("body", body),
                    ("async", bool_value(func.async_)),
                    ("generator", bool_value(false)),
                    (
                        "predicate",
                        option(&func.predicate, |p| predicate(offset_table, config, p)),
                    ),
                    ("expression", bool_value(is_expression)),
                    (
                        "returnType",
                        function_return_type(offset_table, config, &func.return_),
                    ),
                    (
                        "typeParameters",
                        option(&func.tparams, |t| {
                            type_parameter_declaration(offset_table, config, t)
                        }),
                    ),
                ],
            )
        }
        ExpressionInner::Record { loc, inner } => {
            let properties = {
                let (props_loc, obj) = &inner.properties;
                node(
                    offset_table,
                    config,
                    "RecordExpressionProperties",
                    props_loc,
                    format_internal_comments(obj.comments.as_ref()).as_ref(),
                    vec![(
                        "properties",
                        array_of_list(&obj.properties, |p| {
                            object_property(offset_table, config, p)
                        }),
                    )],
                )
            };
            node(
                offset_table,
                config,
                "RecordExpression",
                loc,
                inner.comments.as_ref(),
                vec![
                    (
                        "recordConstructor",
                        expression(offset_table, config, false, &inner.constructor),
                    ),
                    (
                        "typeArguments",
                        option(&inner.targs, |t| call_type_args(offset_table, config, t)),
                    ),
                    ("properties", properties),
                ],
            )
        }
        ExpressionInner::Sequence { loc, inner } => node(
            offset_table,
            config,
            "SequenceExpression",
            loc,
            inner.comments.as_ref(),
            vec![(
                "expressions",
                array_of_list(&inner.expressions, |e| {
                    expression(offset_table, config, false, e)
                }),
            )],
        ),
        ExpressionInner::Unary { loc, inner: unary } => {
            use ast::expression::UnaryOperator;
            match unary.operator {
                UnaryOperator::Await => node(
                    offset_table,
                    config,
                    "AwaitExpression",
                    loc,
                    unary.comments.as_ref(),
                    vec![(
                        "argument",
                        expression(offset_table, config, false, &unary.argument),
                    )],
                ),
                UnaryOperator::Nonnull => node(
                    offset_table,
                    config,
                    "NonNullExpression",
                    loc,
                    unary.comments.as_ref(),
                    vec![(
                        "expression",
                        expression(
                            offset_table,
                            config,
                            is_optional_chain(expr),
                            &unary.argument,
                        ),
                    )],
                ),
                _ => {
                    let operator = match unary.operator {
                        UnaryOperator::Minus => "-",
                        UnaryOperator::Plus => "+",
                        UnaryOperator::Not => "!",
                        UnaryOperator::BitNot => "~",
                        UnaryOperator::Typeof => "typeof",
                        UnaryOperator::Void => "void",
                        UnaryOperator::Delete => "delete",
                        UnaryOperator::Await | UnaryOperator::Nonnull => {
                            unreachable!("matched above")
                        }
                    };
                    node(
                        offset_table,
                        config,
                        "UnaryExpression",
                        loc,
                        unary.comments.as_ref(),
                        vec![
                            ("operator", string(operator)),
                            ("prefix", bool_value(true)),
                            (
                                "argument",
                                expression(offset_table, config, false, &unary.argument),
                            ),
                        ],
                    )
                }
            }
        }
        ExpressionInner::Binary { loc, inner } => node(
            offset_table,
            config,
            "BinaryExpression",
            loc,
            inner.comments.as_ref(),
            vec![
                ("operator", string(inner.operator.as_str())),
                ("left", expression(offset_table, config, false, &inner.left)),
                (
                    "right",
                    expression(offset_table, config, false, &inner.right),
                ),
            ],
        ),
        ExpressionInner::TypeCast { loc, inner } => node(
            offset_table,
            config,
            "TypeCastExpression",
            loc,
            inner.comments.as_ref(),
            vec![
                (
                    "expression",
                    expression(offset_table, config, false, &inner.expression),
                ),
                (
                    "typeAnnotation",
                    type_annotation(offset_table, config, &inner.annot),
                ),
            ],
        ),
        ExpressionInner::AsExpression { loc, inner } => node(
            offset_table,
            config,
            "AsExpression",
            loc,
            inner.comments.as_ref(),
            vec![
                (
                    "expression",
                    expression(offset_table, config, false, &inner.expression),
                ),
                (
                    "typeAnnotation",
                    type_(offset_table, config, &inner.annot.annotation),
                ),
            ],
        ),
        ExpressionInner::TSSatisfies { loc, inner } => node(
            offset_table,
            config,
            "SatisfiesExpression",
            loc,
            inner.comments.as_ref(),
            vec![
                (
                    "expression",
                    expression(offset_table, config, false, &inner.expression),
                ),
                (
                    "typeAnnotation",
                    type_(offset_table, config, &inner.annot.annotation),
                ),
            ],
        ),
        ExpressionInner::AsConstExpression { loc, inner } => node(
            offset_table,
            config,
            "AsConstExpression",
            loc,
            inner.comments.as_ref(),
            vec![(
                "expression",
                expression(offset_table, config, false, &inner.expression),
            )],
        ),
        ExpressionInner::Assignment { loc, inner } => {
            let operator = match &inner.operator {
                None => "=",
                Some(op) => op.as_str(),
            };
            node(
                offset_table,
                config,
                "AssignmentExpression",
                loc,
                inner.comments.as_ref(),
                vec![
                    ("operator", string(operator)),
                    ("left", pattern(offset_table, config, &inner.left)),
                    (
                        "right",
                        expression(offset_table, config, false, &inner.right),
                    ),
                ],
            )
        }
        ExpressionInner::Update { loc, inner } => {
            let operator = match inner.operator {
                ast::expression::UpdateOperator::Increment => "++",
                ast::expression::UpdateOperator::Decrement => "--",
            };
            node(
                offset_table,
                config,
                "UpdateExpression",
                loc,
                inner.comments.as_ref(),
                vec![
                    ("operator", string(operator)),
                    (
                        "argument",
                        expression(offset_table, config, false, &inner.argument),
                    ),
                    ("prefix", bool_value(inner.prefix)),
                ],
            )
        }
        ExpressionInner::Logical { loc, inner } => {
            let operator = match inner.operator {
                ast::expression::LogicalOperator::Or => "||",
                ast::expression::LogicalOperator::And => "&&",
                ast::expression::LogicalOperator::NullishCoalesce => "??",
            };
            node(
                offset_table,
                config,
                "LogicalExpression",
                loc,
                inner.comments.as_ref(),
                vec![
                    ("operator", string(operator)),
                    ("left", expression(offset_table, config, false, &inner.left)),
                    (
                        "right",
                        expression(offset_table, config, false, &inner.right),
                    ),
                ],
            )
        }
        ExpressionInner::Conditional { loc, inner } => node(
            offset_table,
            config,
            "ConditionalExpression",
            loc,
            inner.comments.as_ref(),
            vec![
                ("test", expression(offset_table, config, false, &inner.test)),
                (
                    "consequent",
                    expression(offset_table, config, false, &inner.consequent),
                ),
                (
                    "alternate",
                    expression(offset_table, config, false, &inner.alternate),
                ),
            ],
        ),
        ExpressionInner::New { loc, inner } => {
            let (arguments, comments) = match &inner.arguments {
                Some(args) => (
                    arg_list(offset_table, config, false, args),
                    ast_utils::merge_comments(
                        format_internal_comments(args.comments.as_ref()),
                        inner.comments.dupe(),
                    ),
                ),
                None => (Value::Array(vec![]), inner.comments.dupe()),
            };
            node(
                offset_table,
                config,
                "NewExpression",
                loc,
                comments.as_ref(),
                vec![
                    (
                        "callee",
                        expression(offset_table, config, false, &inner.callee),
                    ),
                    (
                        "typeArguments",
                        option(&inner.targs, |t| call_type_args(offset_table, config, t)),
                    ),
                    ("arguments", arguments),
                ],
            )
        }
        ExpressionInner::Call { loc, inner: call } => {
            let comments = ast_utils::merge_comments(
                format_internal_comments(call.arguments.comments.as_ref()),
                call.comments.dupe(),
            );
            // Plain Call inside an optional chain marks a parenthesis
            // boundary: reset chain state for children so an inner optional
            // access starts a new chain. Otherwise, normal call. Either way
            // emit CallExpression with optional=false.
            let mut props = call_node_properties(offset_table, config, false, None, loc, call);
            props.push(("optional", bool_value(false)));
            node(
                offset_table,
                config,
                "CallExpression",
                loc,
                comments.as_ref(),
                props,
            )
        }
        ExpressionInner::OptionalCall {
            loc,
            inner: optional_call,
        } => {
            let call = &optional_call.call;
            let comments = ast_utils::merge_comments(
                format_internal_comments(call.arguments.comments.as_ref()),
                call.comments.dupe(),
            );

            let assertion_loc = match &optional_call.optional {
                ast::expression::OptionalCallKind::AssertNonnull { assertion_loc } => {
                    Some(assertion_loc)
                }
                _ => None,
            };
            let mut props =
                call_node_properties(offset_table, config, true, assertion_loc, loc, call);
            props.push((
                "optional",
                bool_value(matches!(
                    optional_call.optional,
                    ast::expression::OptionalCallKind::Optional
                )),
            ));
            node(
                offset_table,
                config,
                "CallExpression",
                loc,
                comments.as_ref(),
                props,
            )
        }
        ExpressionInner::Member { loc, inner } => {
            // Plain Member inside an optional chain marks a parenthesis
            // boundary; reset chain state for children so an inner optional
            // access starts a new chain. Either way emit MemberExpression
            // with optional=false.
            let mut props = member_node_properties(offset_table, config, false, None, loc, inner);
            props.push(("optional", bool_value(false)));
            node(
                offset_table,
                config,
                "MemberExpression",
                loc,
                inner.comments.as_ref(),
                props,
            )
        }
        ExpressionInner::OptionalMember { loc, inner } => {
            let member = &inner.member;

            let assertion_loc = match &inner.optional {
                ast::expression::OptionalMemberKind::AssertNonnull { assertion_loc } => {
                    Some(assertion_loc)
                }
                _ => None,
            };
            let mut props =
                member_node_properties(offset_table, config, true, assertion_loc, loc, member);
            props.push((
                "optional",
                bool_value(matches!(
                    inner.optional,
                    ast::expression::OptionalMemberKind::Optional
                )),
            ));
            node(
                offset_table,
                config,
                "MemberExpression",
                loc,
                member.comments.as_ref(),
                props,
            )
        }
        ExpressionInner::Yield { loc, inner } => node(
            offset_table,
            config,
            "YieldExpression",
            loc,
            inner.comments.as_ref(),
            vec![
                (
                    "argument",
                    option(&inner.argument, |e| {
                        expression(offset_table, config, false, e)
                    }),
                ),
                ("delegate", bool_value(inner.delegate)),
            ],
        ),
        ExpressionInner::Identifier { loc: _, inner } => identifier(offset_table, config, inner),
        ExpressionInner::StringLiteral { loc, inner } => {
            string_literal(offset_table, config, loc, inner)
        }
        ExpressionInner::BooleanLiteral { loc, inner } => {
            boolean_literal(offset_table, config, loc, inner)
        }
        ExpressionInner::NullLiteral { loc, inner } => {
            null_literal(offset_table, config, loc, inner.as_ref().as_ref())
        }
        ExpressionInner::NumberLiteral { loc, inner } => {
            number_literal(offset_table, config, loc, inner)
        }
        ExpressionInner::BigIntLiteral { loc, inner } => {
            bigint_literal(offset_table, config, loc, inner)
        }
        ExpressionInner::RegExpLiteral { loc, inner } => {
            regexp_literal(offset_table, config, loc, inner)
        }
        ExpressionInner::ModuleRefLiteral { loc, inner } => string_literal(
            offset_table,
            config,
            loc,
            &ast::StringLiteral {
                value: inner.value.dupe(),
                raw: inner.raw.dupe(),
                comments: inner.comments.dupe(),
            },
        ),
        ExpressionInner::TemplateLiteral { loc, inner } => {
            template_literal(offset_table, config, loc, inner)
        }
        ExpressionInner::TaggedTemplate { loc, inner } => {
            tagged_template(offset_table, config, loc, inner)
        }
        ExpressionInner::Class { loc, inner } => class_expression(offset_table, config, loc, inner),
        ExpressionInner::JSXElement { loc, inner } => jsx_element(offset_table, config, loc, inner),
        ExpressionInner::JSXFragment { loc, inner } => {
            jsx_fragment(offset_table, config, loc, inner)
        }
        ExpressionInner::Match { loc, inner } => node(
            offset_table,
            config,
            "MatchExpression",
            loc,
            inner.comments.as_ref(),
            vec![
                (
                    "argument",
                    expression(offset_table, config, false, &inner.arg),
                ),
                (
                    "cases",
                    array_of_list(&inner.cases, |c| {
                        match_expression_case(offset_table, config, c)
                    }),
                ),
            ],
        ),
        ExpressionInner::MetaProperty { loc, inner } => node(
            offset_table,
            config,
            "MetaProperty",
            loc,
            inner.comments.as_ref(),
            vec![
                ("meta", identifier(offset_table, config, &inner.meta)),
                (
                    "property",
                    identifier(offset_table, config, &inner.property),
                ),
            ],
        ),
        ExpressionInner::Import { loc, inner } => {
            let mut fields = vec![(
                "source",
                expression(offset_table, config, false, &inner.argument),
            )];
            if let Some(opts) = &inner.options {
                fields.push(("options", expression(offset_table, config, false, opts)));
            }
            if let Some(phase) = inner.phase {
                fields.push(("phase", string(phase.as_str())));
            }
            node(
                offset_table,
                config,
                "ImportExpression",
                loc,
                inner.comments.as_ref(),
                fields,
            )
        }
    }
}

fn match_expression_case(
    offset_table: &OffsetTable,
    config: &Config,
    case: &ast::match_::Case<Loc, Loc, ast::expression::Expression<Loc, Loc>>,
) -> Value {
    match_case(
        offset_table,
        config,
        "MatchExpressionCase",
        case,
        |offset_table, config, expr| expression(offset_table, config, false, expr),
    )
}

fn match_statement_case(
    offset_table: &OffsetTable,
    config: &Config,
    case: &ast::match_::Case<Loc, Loc, ast::statement::Statement<Loc, Loc>>,
) -> Value {
    match_case(offset_table, config, "MatchStatementCase", case, statement)
}

fn match_case<B, F>(
    offset_table: &OffsetTable,
    config: &Config,
    kind: &str,
    case: &ast::match_::Case<Loc, Loc, B>,
    on_case_body: F,
) -> Value
where
    F: Fn(&OffsetTable, &Config, &B) -> Value,
{
    node(
        offset_table,
        config,
        kind,
        &case.loc,
        case.comments.as_ref(),
        vec![
            (
                "pattern",
                match_pattern(offset_table, config, &case.pattern),
            ),
            ("body", on_case_body(offset_table, config, &case.body)),
            (
                "guard",
                option(&case.guard, |e| expression(offset_table, config, false, e)),
            ),
        ],
    )
}

fn match_pattern(
    offset_table: &OffsetTable,
    config: &Config,
    pattern: &ast::match_pattern::MatchPattern<Loc, Loc>,
) -> Value {
    use ast::match_pattern::MatchPattern;

    let literal = |node_type: &str, loc: &Loc, literal_value: Value| {
        node(
            offset_table,
            config,
            node_type,
            loc,
            None,
            vec![("literal", literal_value)],
        )
    };

    match pattern {
        MatchPattern::WildcardPattern { loc, inner } => node(
            offset_table,
            config,
            "MatchWildcardPattern",
            loc,
            inner.comments.as_ref(),
            vec![],
        ),
        MatchPattern::StringPattern { loc, inner } => literal(
            "MatchLiteralPattern",
            loc,
            string_literal(offset_table, config, loc, inner),
        ),
        MatchPattern::BooleanPattern { loc, inner } => literal(
            "MatchLiteralPattern",
            loc,
            boolean_literal(offset_table, config, loc, inner),
        ),
        MatchPattern::NullPattern { loc, inner } => literal(
            "MatchLiteralPattern",
            loc,
            null_literal(offset_table, config, loc, (**inner).as_ref()),
        ),
        MatchPattern::NumberPattern { loc, inner } => literal(
            "MatchLiteralPattern",
            loc,
            number_literal(offset_table, config, loc, inner),
        ),
        MatchPattern::BigIntPattern { loc, inner } => literal(
            "MatchLiteralPattern",
            loc,
            bigint_literal(offset_table, config, loc, inner),
        ),
        MatchPattern::UnaryPattern { loc, inner } => {
            let operator = match inner.operator {
                ast::match_pattern::unary_pattern::Operator::Minus => "-",
                ast::match_pattern::unary_pattern::Operator::Plus => "+",
            };
            let argument = match &inner.argument {
                (arg_loc, ast::match_pattern::unary_pattern::Argument::NumberLiteral(lit)) => {
                    number_literal(offset_table, config, arg_loc, lit)
                }
                (arg_loc, ast::match_pattern::unary_pattern::Argument::BigIntLiteral(lit)) => {
                    bigint_literal(offset_table, config, arg_loc, lit)
                }
            };
            node(
                offset_table,
                config,
                "MatchUnaryPattern",
                loc,
                inner.comments.as_ref(),
                vec![("operator", string(operator)), ("argument", argument)],
            )
        }
        MatchPattern::BindingPattern { loc, inner } => {
            match_binding_pattern(offset_table, config, loc, inner)
        }
        MatchPattern::IdentifierPattern { loc, inner } => {
            match_identifier_pattern(offset_table, config, loc, inner)
        }
        MatchPattern::MemberPattern { loc, inner } => {
            match_member_pattern(offset_table, config, loc, inner)
        }
        MatchPattern::ObjectPattern { loc, inner } => match_object_pattern(
            offset_table,
            config,
            "MatchObjectPattern",
            loc,
            inner.as_ref(),
        ),
        MatchPattern::ArrayPattern { loc, inner } => {
            match_array_pattern(offset_table, config, loc, inner.as_ref())
        }
        MatchPattern::InstancePattern { loc, inner } => {
            let constructor = match &inner.constructor {
                ast::match_pattern::InstancePatternConstructor::IdentifierConstructor(id) => {
                    match_identifier_pattern(offset_table, config, &id.loc, id)
                }
                ast::match_pattern::InstancePatternConstructor::MemberConstructor(member) => {
                    match_member_pattern(offset_table, config, &member.loc, member)
                }
            };
            node(
                offset_table,
                config,
                "MatchInstancePattern",
                loc,
                inner.comments.as_ref(),
                vec![
                    ("targetConstructor", constructor),
                    (
                        "properties",
                        match_object_pattern(
                            offset_table,
                            config,
                            "MatchInstanceObjectPattern",
                            &inner.properties.0,
                            &inner.properties.1,
                        ),
                    ),
                ],
            )
        }
        MatchPattern::OrPattern { loc, inner } => node(
            offset_table,
            config,
            "MatchOrPattern",
            loc,
            inner.comments.as_ref(),
            vec![(
                "patterns",
                array_of_list(&inner.patterns, |p| match_pattern(offset_table, config, p)),
            )],
        ),
        MatchPattern::AsPattern { loc, inner } => {
            let target = match &inner.target {
                ast::match_pattern::as_pattern::Target::Binding { loc, pattern } => {
                    match_binding_pattern(offset_table, config, loc, pattern)
                }
                ast::match_pattern::as_pattern::Target::Identifier(id) => {
                    identifier(offset_table, config, id)
                }
            };
            node(
                offset_table,
                config,
                "MatchAsPattern",
                loc,
                inner.comments.as_ref(),
                vec![
                    (
                        "pattern",
                        match_pattern(offset_table, config, &inner.pattern),
                    ),
                    ("target", target),
                ],
            )
        }
    }
}

fn match_identifier_pattern(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    id: &ast::Identifier<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "MatchIdentifierPattern",
        loc,
        None,
        vec![("id", identifier(offset_table, config, id))],
    )
}

fn match_member_pattern(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    member: &ast::match_pattern::MemberPattern<Loc, Loc>,
) -> Value {
    let base = match &member.base {
        ast::match_pattern::member_pattern::Base::BaseIdentifier(id) => {
            match_identifier_pattern(offset_table, config, &id.loc, id)
        }
        ast::match_pattern::member_pattern::Base::BaseMember(member) => {
            match_member_pattern(offset_table, config, &member.loc, member.as_ref())
        }
    };

    let property = match &member.property {
        ast::match_pattern::member_pattern::Property::PropertyString { loc, literal } => {
            string_literal(offset_table, config, loc, literal)
        }
        ast::match_pattern::member_pattern::Property::PropertyNumber { loc, literal } => {
            number_literal(offset_table, config, loc, literal)
        }
        ast::match_pattern::member_pattern::Property::PropertyBigInt { loc, literal } => {
            bigint_literal(offset_table, config, loc, literal)
        }
        ast::match_pattern::member_pattern::Property::PropertyIdentifier(id) => {
            identifier(offset_table, config, id)
        }
    };

    node(
        offset_table,
        config,
        "MatchMemberPattern",
        loc,
        member.comments.as_ref(),
        vec![("base", base), ("property", property)],
    )
}

fn match_binding_pattern(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    binding: &ast::match_pattern::BindingPattern<Loc, Loc>,
) -> Value {
    let kind = binding.kind.as_str();
    node(
        offset_table,
        config,
        "MatchBindingPattern",
        loc,
        binding.comments.as_ref(),
        vec![
            ("id", identifier(offset_table, config, &binding.id)),
            ("kind", string(kind)),
        ],
    )
}

fn match_array_pattern(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    arr: &ast::match_pattern::ArrayPattern<Loc, Loc>,
) -> Value {
    let formatted_comments = format_internal_comments(arr.comments.as_ref());
    node(
        offset_table,
        config,
        "MatchArrayPattern",
        loc,
        formatted_comments.as_ref(),
        vec![
            (
                "elements",
                array_of_list(&arr.elements, |elem| {
                    match_pattern(offset_table, config, &elem.pattern)
                }),
            ),
            (
                "rest",
                option(&arr.rest, |r| match_rest_pattern(offset_table, config, r)),
            ),
        ],
    )
}

fn match_object_pattern(
    offset_table: &OffsetTable,
    config: &Config,
    kind: &str,
    loc: &Loc,
    obj: &ast::match_pattern::ObjectPattern<Loc, Loc>,
) -> Value {
    let formatted_comments = format_internal_comments(obj.comments.as_ref());

    let property = |prop: &ast::match_pattern::object_pattern::Property<Loc, Loc>| match prop {
        ast::match_pattern::object_pattern::Property::Valid { loc, property } => {
            let key = match &property.key {
                ast::match_pattern::object_pattern::Key::StringLiteral((loc, lit)) => {
                    string_literal(offset_table, config, loc, lit)
                }
                ast::match_pattern::object_pattern::Key::NumberLiteral((loc, lit)) => {
                    number_literal(offset_table, config, loc, lit)
                }
                ast::match_pattern::object_pattern::Key::BigIntLiteral((loc, lit)) => {
                    bigint_literal(offset_table, config, loc, lit)
                }
                ast::match_pattern::object_pattern::Key::Identifier(id) => {
                    identifier(offset_table, config, id)
                }
            };

            node(
                offset_table,
                config,
                "MatchObjectPatternProperty",
                loc,
                property.comments.as_ref(),
                vec![
                    ("key", key),
                    (
                        "pattern",
                        match_pattern(offset_table, config, &property.pattern),
                    ),
                    ("shorthand", bool_value(property.shorthand)),
                ],
            )
        }
        ast::match_pattern::object_pattern::Property::InvalidShorthand {
            loc,
            identifier: id,
        } => node(
            offset_table,
            config,
            "MatchObjectPatternProperty",
            loc,
            None,
            vec![
                ("key", identifier(offset_table, config, id)),
                (
                    "pattern",
                    match_identifier_pattern(offset_table, config, &id.loc, id),
                ),
                ("shorthand", bool_value(true)),
            ],
        ),
    };

    node(
        offset_table,
        config,
        kind,
        loc,
        formatted_comments.as_ref(),
        vec![
            ("properties", array_of_list(&obj.properties, property)),
            (
                "rest",
                option(&obj.rest, |r| match_rest_pattern(offset_table, config, r)),
            ),
        ],
    )
}

fn match_rest_pattern(
    offset_table: &OffsetTable,
    config: &Config,
    rest: &ast::match_pattern::RestPattern<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "MatchRestPattern",
        &rest.loc,
        rest.comments.as_ref(),
        vec![(
            "argument",
            option(&rest.argument, |(loc, binding)| {
                match_binding_pattern(offset_table, config, loc, binding)
            }),
        )],
    )
}

fn function_declaration(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    func: &ast::function::Function<Loc, Loc>,
) -> Value {
    use ast::function::Effect;

    let body = match &func.body {
        ast::function::Body::BodyBlock(b) => block(offset_table, config, &b.0, &b.1),
        ast::function::Body::BodyExpression(_) => {
            panic!("Unexpected FunctionDeclaration with BodyExpression")
        }
    };

    let formatted_comments = format_internal_comments(func.params.comments.as_ref());
    let comments = ast_utils::merge_comments(formatted_comments, func.comments.dupe());

    let (node_name, nonhook_attrs) = if func.effect_ == Effect::Hook {
        ("HookDeclaration", vec![("async", bool_value(func.async_))])
    } else {
        (
            "FunctionDeclaration",
            vec![
                ("async", bool_value(func.async_)),
                ("generator", bool_value(func.generator)),
                (
                    "predicate",
                    option(&func.predicate, |p| predicate(offset_table, config, p)),
                ),
                ("expression", bool_value(false)),
            ],
        )
    };

    let mut props = vec![
        // estree hasn't come around to the idea that function decls can have
        // optional ids, but acorn, babel, espree and esprima all have, so let's
        // do it too. see https://github.com/estree/estree/issues/98
        (
            "id",
            option(&func.id, |id| identifier(offset_table, config, id)),
        ),
        (
            "params",
            function_params(offset_table, config, &func.params),
        ),
        ("body", body),
        (
            "returnType",
            function_return_type(offset_table, config, &func.return_),
        ),
        (
            "typeParameters",
            option(&func.tparams, |t| {
                type_parameter_declaration(offset_table, config, t)
            }),
        ),
    ];

    props.extend(nonhook_attrs);

    node(
        offset_table,
        config,
        node_name,
        loc,
        comments.as_ref(),
        props,
    )
}

fn function_expression(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    func: &ast::function::Function<Loc, Loc>,
) -> Value {
    let body = match &func.body {
        ast::function::Body::BodyBlock(b) => block(offset_table, config, &b.0, &b.1),
        ast::function::Body::BodyExpression(_) => {
            panic!("Unexpected FunctionExpression with BodyExpression")
        }
    };

    let formatted_comments = format_internal_comments(func.params.comments.as_ref());
    let comments = ast_utils::merge_comments(formatted_comments, func.comments.dupe());

    node(
        offset_table,
        config,
        "FunctionExpression",
        loc,
        comments.as_ref(),
        vec![
            (
                "id",
                option(&func.id, |id| identifier(offset_table, config, id)),
            ),
            (
                "params",
                function_params(offset_table, config, &func.params),
            ),
            ("body", body),
            ("async", bool_value(func.async_)),
            ("generator", bool_value(func.generator)),
            (
                "predicate",
                option(&func.predicate, |p| predicate(offset_table, config, p)),
            ),
            ("expression", bool_value(false)),
            (
                "returnType",
                function_return_type(offset_table, config, &func.return_),
            ),
            (
                "typeParameters",
                option(&func.tparams, |t| {
                    type_parameter_declaration(offset_table, config, t)
                }),
            ),
        ],
    )
}

fn identifier(
    offset_table: &OffsetTable,
    config: &Config,
    id: &ast::Identifier<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "Identifier",
        &id.loc,
        id.comments.as_ref(),
        vec![
            ("name", string(&id.name)),
            ("typeAnnotation", Value::Null),
            ("optional", bool_value(false)),
        ],
    )
}

fn private_identifier(
    offset_table: &OffsetTable,
    config: &Config,
    name: &ast::PrivateName<Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "PrivateIdentifier",
        &name.loc,
        name.comments.as_ref(),
        vec![
            ("name", string(&name.name)),
            ("typeAnnotation", Value::Null),
            ("optional", bool_value(false)),
        ],
    )
}

fn pattern_identifier(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    pattern_id: &ast::pattern::Identifier<Loc, Loc>,
) -> Value {
    use ast::types::AnnotationOrHint;

    let type_annot = match &pattern_id.annot {
        AnnotationOrHint::Available(annot) => type_annotation(offset_table, config, annot),
        AnnotationOrHint::Missing(_) => Value::Null,
    };

    node(
        offset_table,
        config,
        "Identifier",
        loc,
        pattern_id.name.comments.as_ref(),
        vec![
            ("name", string(&pattern_id.name.name)),
            ("typeAnnotation", type_annot),
            ("optional", bool_value(pattern_id.optional)),
        ],
    )
}

fn arg_list(
    offset_table: &OffsetTable,
    config: &Config,
    in_optional_chain: bool,
    arg_list: &ast::expression::ArgList<Loc, Loc>,
) -> Value {
    // ESTree does not have a unique node for argument lists, so there's nowhere to
    // include the loc.
    array_of_list(&arg_list.arguments, |arg| {
        use ast::expression::ExpressionOrSpread;
        match arg {
            ExpressionOrSpread::Expression(expr) => {
                expression(offset_table, config, in_optional_chain, expr)
            }
            ExpressionOrSpread::Spread(spread) => {
                spread_element(offset_table, config, in_optional_chain, spread)
            }
        }
    })
}

fn switch_case(
    offset_table: &OffsetTable,
    config: &Config,
    case: &ast::statement::switch::Case<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "SwitchCase",
        &case.loc,
        case.comments.as_ref(),
        vec![
            (
                "test",
                option(&case.test, |e| expression(offset_table, config, false, e)),
            ),
            (
                "consequent",
                statement_list(offset_table, config, &case.consequent),
            ),
        ],
    )
}

fn catch_clause(
    offset_table: &OffsetTable,
    config: &Config,
    catch: &ast::statement::try_::CatchClause<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "CatchClause",
        &catch.loc,
        catch.comments.as_ref(),
        vec![
            (
                "param",
                option(&catch.param, |p| pattern(offset_table, config, p)),
            ),
            (
                "body",
                block(offset_table, config, &catch.body.0, &catch.body.1),
            ),
        ],
    )
}

fn block(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    block: &ast::statement::Block<Loc, Loc>,
) -> Value {
    let formatted_comments = format_internal_comments(block.comments.as_ref());
    node(
        offset_table,
        config,
        "BlockStatement",
        loc,
        formatted_comments.as_ref(),
        vec![("body", statement_list(offset_table, config, &block.body))],
    )
}

fn declare_variable(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    declare: &ast::statement::DeclareVariable<Loc, Loc>,
) -> Value {
    let kind_str = declare.kind.as_str();
    node(
        offset_table,
        config,
        "DeclareVariable",
        loc,
        declare.comments.as_ref(),
        vec![
            (
                "declarations",
                array_of_list(&declare.declarations, |d| {
                    variable_declarator(offset_table, config, d)
                }),
            ),
            ("kind", string(kind_str)),
        ],
    )
}

fn declare_function(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    declare: &ast::statement::DeclareFunction<Loc, Loc>,
) -> Value {
    let id_loc = match &declare.id {
        Some(id) => Loc::between(&id.loc, &declare.annot.loc),
        None => declare.annot.loc.dupe(),
    };

    // Check if this is a hook based on the annotation
    let (node_name, predicate_props) = match declare.annot.annotation.deref() {
        ast::types::TypeInner::Function { inner, .. }
            if inner.effect == ast::function::Effect::Hook =>
        {
            ("DeclareHook", vec![])
        }
        _ => (
            "DeclareFunction",
            vec![(
                "predicate",
                option(&declare.predicate, |p| predicate(offset_table, config, p)),
            )],
        ),
    };

    let annot_field = if declare.id.is_none() {
        vec![(
            "typeAnnotation",
            type_annotation(offset_table, config, &declare.annot),
        )]
    } else {
        vec![]
    };

    let mut props = vec![
        (
            "id",
            match &declare.id {
                Some(id) => {
                    let pattern_id = ast::pattern::Identifier {
                        name: id.dupe(),
                        annot: ast::types::AnnotationOrHint::Available(declare.annot.clone()),
                        optional: false,
                    };
                    pattern_identifier(offset_table, config, &id_loc, &pattern_id)
                }
                None => Value::Null,
            },
        ),
        ("implicitDeclare", Value::Bool(declare.implicit_declare)),
    ];
    props.extend(annot_field);
    props.extend(predicate_props);

    node(
        offset_table,
        config,
        node_name,
        loc,
        declare.comments.as_ref(),
        props,
    )
}

fn declare_class(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    declare: &ast::statement::DeclareClass<Loc, Loc>,
) -> Value {
    // TODO: extends shouldn't return an array
    fn declare_class_extends_to_estree(
        offset_table: &OffsetTable,
        config: &Config,
        loc: &Loc,
        ext: &ast::statement::DeclareClassExtends<Loc, Loc>,
    ) -> Value {
        match ext {
            ast::statement::DeclareClassExtends::ExtendsIdent(generic) => {
                interface_extends(offset_table, config, loc, generic)
            }
            ast::statement::DeclareClassExtends::ExtendsCall { callee, arg } => {
                let fields = vec![
                    (
                        "callee",
                        generic_type(offset_table, config, &callee.0, &callee.1),
                    ),
                    (
                        "argument",
                        declare_class_extends_to_estree(offset_table, config, &arg.0, &arg.1),
                    ),
                ];
                node(
                    offset_table,
                    config,
                    "DeclareClassExtendsCall",
                    loc,
                    None,
                    fields,
                )
            }
        }
    }
    let extends = match &declare.extends {
        Some((loc, ext)) => Value::Array(vec![declare_class_extends_to_estree(
            offset_table,
            config,
            loc,
            ext,
        )]),
        None => Value::Array(vec![]),
    };

    let implements = match &declare.implements {
        Some(impls) => array_of_list(&impls.interfaces, |i| {
            class_implements(offset_table, config, i)
        }),
        None => Value::Array(vec![]),
    };

    let mut fields = vec![
        ("id", identifier(offset_table, config, &declare.id)),
        (
            "typeParameters",
            option(&declare.tparams, |t| {
                type_parameter_declaration(offset_table, config, t)
            }),
        ),
        (
            "body",
            object_type(
                offset_table,
                config,
                &declare.body.0,
                &declare.body.1,
                false,
            ),
        ),
        ("extends", extends),
        ("implements", implements),
        (
            "mixins",
            array_of_list(&declare.mixins, |(loc, i)| {
                interface_extends(offset_table, config, loc, i)
            }),
        ),
    ];
    if declare.abstract_ {
        fields.push(("abstract", bool_value(declare.abstract_)));
    }

    node(
        offset_table,
        config,
        "DeclareClass",
        loc,
        declare.comments.as_ref(),
        fields,
    )
}

fn declare_component(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    component: &ast::statement::DeclareComponent<Loc, Loc>,
) -> Value {
    let formatted_comments = format_internal_comments(component.params.comments.as_ref());
    let comments = ast_utils::merge_comments(formatted_comments, component.comments.dupe());

    node(
        offset_table,
        config,
        "DeclareComponent",
        loc,
        comments.as_ref(),
        vec![
            ("id", identifier(offset_table, config, &component.id)),
            (
                "params",
                component_params(offset_table, config, &component.params),
            ),
            (
                "rendersType",
                renders_annotation(offset_table, config, &component.renders),
            ),
            (
                "typeParameters",
                option(&component.tparams, |t| {
                    type_parameter_declaration(offset_table, config, t)
                }),
            ),
        ],
    )
}

fn component_type(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    component: &ast::types::Component<Loc, Loc>,
) -> Value {
    let formatted_comments = format_internal_comments(component.params.comments.as_ref());
    let comments = ast_utils::merge_comments(formatted_comments, component.comments.dupe());

    node(
        offset_table,
        config,
        "ComponentTypeAnnotation",
        loc,
        comments.as_ref(),
        vec![
            (
                "params",
                component_type_params(offset_table, config, &component.params.params),
            ),
            (
                "rest",
                option(&component.params.rest, |r| {
                    component_type_rest_param(offset_table, config, r)
                }),
            ),
            (
                "rendersType",
                renders_annotation(offset_table, config, &component.renders),
            ),
            (
                "typeParameters",
                option(&component.tparams, |t| {
                    type_parameter_declaration(offset_table, config, t)
                }),
            ),
        ],
    )
}

fn component_type_params(
    offset_table: &OffsetTable,
    config: &Config,
    params: &[ast::types::component_params::Param<Loc, Loc>],
) -> Value {
    let params_json: Vec<Value> = params
        .iter()
        .map(|param| {
            component_type_param(
                offset_table,
                config,
                &param.loc,
                Some(&param.name),
                &param.annot.annotation,
                param.optional,
                None,
            )
        })
        .collect();
    Value::Array(params_json)
}

fn component_type_rest_param(
    offset_table: &OffsetTable,
    config: &Config,
    rest: &ast::types::component_params::RestParam<Loc, Loc>,
) -> Value {
    let name = rest
        .argument
        .as_ref()
        .map(|id| ast::statement::component_params::ParamName::Identifier(id.dupe()));

    component_type_param(
        offset_table,
        config,
        &rest.loc,
        name.as_ref(),
        &rest.annot,
        rest.optional,
        rest.comments.as_ref(),
    )
}

fn component_type_param(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    name: Option<&ast::statement::component_params::ParamName<Loc, Loc>>,
    annot: &ast::types::Type<Loc, Loc>,
    optional: bool,
    comments: Option<&ast::Syntax<Loc, ()>>,
) -> Value {
    use ast::statement::component_params::ParamName;

    let name_value = match name {
        Some(ParamName::Identifier(id)) => {
            option(&Some(id), |i| identifier(offset_table, config, i))
        }
        Some(ParamName::StringLiteral((loc, lit))) => option(&Some((loc, lit)), |(l, s)| {
            string_literal(offset_table, config, l, s)
        }),
        None => Value::Null,
    };

    node(
        offset_table,
        config,
        "ComponentTypeParameter",
        loc,
        comments,
        vec![
            ("name", name_value),
            ("typeAnnotation", type_(offset_table, config, annot)),
            ("optional", bool_value(optional)),
        ],
    )
}

fn declare_enum(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    enum_decl: &ast::statement::EnumDeclaration<Loc, Loc>,
) -> Value {
    let mut props = Vec::with_capacity(3);
    if enum_decl.const_ {
        props.push(("const", bool_value(true)));
    }
    props.push(("id", identifier(offset_table, config, &enum_decl.id)));
    props.push(("body", enum_body(offset_table, config, &enum_decl.body)));
    node(
        offset_table,
        config,
        "DeclareEnum",
        loc,
        enum_decl.comments.as_ref(),
        props,
    )
}

fn declare_namespace(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    namespace: &ast::statement::DeclareNamespace<Loc, Loc>,
) -> Value {
    let (id, global) = match &namespace.id {
        ast::statement::declare_namespace::Id::Local(id) => {
            (identifier(offset_table, config, id), false)
        }
        ast::statement::declare_namespace::Id::Global(id) => {
            (identifier(offset_table, config, id), true)
        }
    };
    let keyword_str = match namespace.keyword {
        ast::statement::declare_namespace::Keyword::Namespace => "namespace",
        ast::statement::declare_namespace::Keyword::Module => "module",
    };
    let mut props = Vec::with_capacity(5);
    if global {
        props.push(("global", bool_value(global)));
    }
    props.push(("id", id));
    props.push((
        "body",
        block(offset_table, config, &namespace.body.0, &namespace.body.1),
    ));
    props.push(("implicitDeclare", bool_value(namespace.implicit_declare)));
    props.push(("keyword", string(keyword_str)));
    node(
        offset_table,
        config,
        "DeclareNamespace",
        loc,
        namespace.comments.as_ref(),
        props,
    )
}
fn declare_interface(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    interface: &ast::statement::Interface<Loc, Loc>,
) -> Value {
    let extends: Vec<Value> = interface
        .extends
        .iter()
        .map(|(loc, ext)| interface_extends(offset_table, config, loc, ext))
        .collect();

    node(
        offset_table,
        config,
        "DeclareInterface",
        loc,
        interface.comments.as_ref(),
        vec![
            ("id", identifier(offset_table, config, &interface.id)),
            (
                "typeParameters",
                option(&interface.tparams, |t| {
                    type_parameter_declaration(offset_table, config, t)
                }),
            ),
            (
                "body",
                object_type(
                    offset_table,
                    config,
                    &interface.body.0,
                    &interface.body.1,
                    false,
                ),
            ),
            ("extends", Value::Array(extends)),
        ],
    )
}

fn export_specifiers(
    offset_table: &OffsetTable,
    config: &Config,
    specifiers: &Option<ast::statement::export_named_declaration::Specifier<Loc, Loc>>,
) -> Value {
    use ast::statement::export_named_declaration::Specifier;

    match specifiers {
        Some(Specifier::ExportSpecifiers(specs)) => {
            array_of_list(specs, |spec| export_specifier(offset_table, config, spec))
        }
        Some(Specifier::ExportBatchSpecifier(batch)) => match &batch.specifier {
            Some(name) => Value::Array(vec![node(
                offset_table,
                config,
                "ExportNamespaceSpecifier",
                &batch.loc,
                None,
                vec![("exported", identifier(offset_table, config, name))],
            )]),
            // this should've been handled by callers, since this represents an
            // ExportAllDeclaration, not a specifier.
            None => Value::Array(vec![]),
        },
        None => Value::Array(vec![]),
    }
}

fn declare_type_alias(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    type_alias: &ast::statement::TypeAlias<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "DeclareTypeAlias",
        loc,
        type_alias.comments.as_ref(),
        vec![
            ("id", identifier(offset_table, config, &type_alias.id)),
            (
                "typeParameters",
                option(&type_alias.tparams, |t| {
                    type_parameter_declaration(offset_table, config, t)
                }),
            ),
            ("right", type_(offset_table, config, &type_alias.right)),
        ],
    )
}

fn type_alias(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    type_alias: &ast::statement::TypeAlias<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "TypeAlias",
        loc,
        type_alias.comments.as_ref(),
        vec![
            ("id", identifier(offset_table, config, &type_alias.id)),
            (
                "typeParameters",
                option(&type_alias.tparams, |t| {
                    type_parameter_declaration(offset_table, config, t)
                }),
            ),
            ("right", type_(offset_table, config, &type_alias.right)),
        ],
    )
}

fn opaque_type(
    offset_table: &OffsetTable,
    config: &Config,
    declare: bool,
    loc: &Loc,
    opaque_type: &ast::statement::OpaqueType<Loc, Loc>,
) -> Value {
    let name = if declare {
        "DeclareOpaqueType"
    } else {
        "OpaqueType"
    };

    node(
        offset_table,
        config,
        name,
        loc,
        opaque_type.comments.as_ref(),
        vec![
            ("id", identifier(offset_table, config, &opaque_type.id)),
            (
                "typeParameters",
                option(&opaque_type.tparams, |t| {
                    type_parameter_declaration(offset_table, config, t)
                }),
            ),
            (
                "impltype",
                option(&opaque_type.impl_type, |t| type_(offset_table, config, t)),
            ),
            (
                "lowerBound",
                option(&opaque_type.lower_bound, |t| type_(offset_table, config, t)),
            ),
            (
                "upperBound",
                option(&opaque_type.upper_bound, |t| type_(offset_table, config, t)),
            ),
            (
                "supertype",
                option(&opaque_type.legacy_upper_bound, |t| {
                    type_(offset_table, config, t)
                }),
            ),
        ],
    )
}

fn class_declaration(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    class: &ast::class::Class<Loc, Loc>,
) -> Value {
    class_helper(offset_table, config, "ClassDeclaration", loc, class)
}

fn class_expression(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    class: &ast::class::Class<Loc, Loc>,
) -> Value {
    class_helper(offset_table, config, "ClassExpression", loc, class)
}

fn class_helper(
    offset_table: &OffsetTable,
    config: &Config,
    node_type: &str,
    loc: &Loc,
    class: &ast::class::Class<Loc, Loc>,
) -> Value {
    let (super_class, super_type_parameters, comments) = match &class.extends {
        Some(extends) => {
            let merged = ast_utils::merge_comments(extends.comments.dupe(), class.comments.dupe());
            (
                Some(expression(offset_table, config, false, &extends.expr)),
                option(&extends.targs, |t| type_args(offset_table, config, t)),
                merged,
            )
        }
        None => (None, Value::Null, class.comments.dupe()),
    };

    let (implements, comments) = match &class.implements {
        Some(impls) => {
            let merged = ast_utils::merge_comments(impls.comments.dupe(), comments.dupe());
            (
                array_of_list(&impls.interfaces, |i| {
                    class_implements(offset_table, config, i)
                }),
                merged,
            )
        }
        None => (Value::Array(vec![]), comments),
    };

    let mut fields = vec![
        // estree hasn't come around to the idea that class decls can have
        // optional ids, but acorn, babel, espree and esprima all have, so let's
        // do it too. see https://github.com/estree/estree/issues/98
        (
            "id",
            option(&class.id, |id| identifier(offset_table, config, id)),
        ),
        ("body", class_body(offset_table, config, &class.body)),
        (
            "typeParameters",
            option(&class.tparams, |t| {
                type_parameter_declaration(offset_table, config, t)
            }),
        ),
        ("superClass", option(&super_class, |sc| sc.clone())),
        ("superTypeArguments", super_type_parameters),
        ("implements", implements),
        (
            "decorators",
            array_of_list(&class.class_decorators, |d| {
                class_decorator(offset_table, config, d)
            }),
        ),
    ];
    if class.abstract_ {
        fields.push(("abstract", bool_value(class.abstract_)));
    }

    node(
        offset_table,
        config,
        node_type,
        loc,
        comments.as_ref(),
        fields,
    )
}

fn class_decorator(
    offset_table: &OffsetTable,
    config: &Config,
    decorator: &ast::class::Decorator<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "Decorator",
        &decorator.loc,
        decorator.comments.as_ref(),
        vec![(
            "expression",
            expression(offset_table, config, false, &decorator.expression),
        )],
    )
}

fn class_implements(
    offset_table: &OffsetTable,
    config: &Config,
    interface: &ast::class::implements::Interface<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "ClassImplements",
        &interface.loc,
        None,
        vec![
            ("id", {
                use ast::types::generic;
                match &interface.id {
                    generic::Identifier::Unqualified(id) => identifier(offset_table, config, id),
                    generic::Identifier::Qualified(q) => {
                        generic_type_qualified_identifier(offset_table, config, q)
                    }
                    generic::Identifier::ImportTypeAnnot(import_type) => {
                        import_type_annotation(offset_table, config, &import_type.loc, import_type)
                    }
                }
            }),
            (
                "typeArguments",
                option(&interface.targs, |t| type_args(offset_table, config, t)),
            ),
        ],
    )
}

fn class_body(
    offset_table: &OffsetTable,
    config: &Config,
    body: &ast::class::Body<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "ClassBody",
        &body.loc,
        body.comments.as_ref(),
        vec![(
            "body",
            array_of_list(&body.body, |elem| class_element(offset_table, config, elem)),
        )],
    )
}

fn class_element(
    offset_table: &OffsetTable,
    config: &Config,
    element: &ast::class::BodyElement<Loc, Loc>,
) -> Value {
    use ast::class::BodyElement;

    match element {
        BodyElement::Method(m) => class_method(offset_table, config, m),
        BodyElement::Property(p) => class_property(offset_table, config, p),
        BodyElement::PrivateField(pf) => class_private_field(offset_table, config, pf),
        BodyElement::StaticBlock(sb) => node(
            offset_table,
            config,
            "StaticBlock",
            &sb.loc,
            format_internal_comments(sb.comments.as_ref()).as_ref(),
            vec![("body", statement_list(offset_table, config, &sb.body))],
        ),
        BodyElement::DeclareMethod(dm) => class_declare_method(offset_table, config, dm),
        BodyElement::AbstractMethod(am) => class_abstract_method(offset_table, config, am),
        BodyElement::AbstractProperty(ap) => class_abstract_property(offset_table, config, ap),
        BodyElement::IndexSignature(i) => object_type_indexer(offset_table, config, i),
    }
}

fn class_method(
    offset_table: &OffsetTable,
    config: &Config,
    method: &ast::class::Method<Loc, Loc>,
) -> Value {
    use ast::class::MethodKind;
    use ast::expression::object::Key;

    let (key, computed, comments) = match &method.key {
        Key::StringLiteral((loc, lit)) => (
            string_literal(offset_table, config, loc, lit),
            false,
            method.comments.dupe(),
        ),
        Key::NumberLiteral((loc, lit)) => (
            number_literal(offset_table, config, loc, lit),
            false,
            method.comments.dupe(),
        ),
        Key::BigIntLiteral((loc, lit)) => (
            bigint_literal(offset_table, config, loc, lit),
            false,
            method.comments.dupe(),
        ),
        Key::Identifier(id) => (
            identifier(offset_table, config, id),
            false,
            method.comments.dupe(),
        ),
        Key::PrivateName(name) => (
            private_identifier(offset_table, config, name),
            false,
            method.comments.dupe(),
        ),
        Key::Computed(computed_key) => {
            let merged =
                ast_utils::merge_comments(computed_key.comments.dupe(), method.comments.dupe());
            (
                expression(offset_table, config, false, &computed_key.expression),
                true,
                merged,
            )
        }
    };

    let kind = match method.kind {
        MethodKind::Constructor => "constructor",
        MethodKind::Method => "method",
        MethodKind::Get => "get",
        MethodKind::Set => "set",
    };

    let mut props = vec![
        ("key", key),
        (
            "value",
            function_expression(offset_table, config, &method.value.0, &method.value.1),
        ),
        ("kind", string(kind)),
        ("static", bool_value(method.static_)),
        ("computed", bool_value(computed)),
        (
            "decorators",
            array_of_list(&method.decorators, |d| {
                class_decorator(offset_table, config, d)
            }),
        ),
    ];
    if method.override_ {
        props.push(("override", bool_value(true)));
    }
    props.push((
        "tsAccessibility",
        option(&method.ts_accessibility, ts_accessibility),
    ));
    node(
        offset_table,
        config,
        "MethodDefinition",
        &method.loc,
        comments.as_ref(),
        props,
    )
}

fn class_declare_method(
    offset_table: &OffsetTable,
    config: &Config,
    decl_meth: &ast::class::DeclareMethod<Loc, Loc>,
) -> Value {
    use ast::expression::object::Key;

    let (key, computed, comments) = match &decl_meth.key {
        Key::StringLiteral((loc, lit)) => (
            string_literal(offset_table, config, loc, lit),
            false,
            decl_meth.comments.dupe(),
        ),
        Key::NumberLiteral((loc, lit)) => (
            number_literal(offset_table, config, loc, lit),
            false,
            decl_meth.comments.dupe(),
        ),
        Key::BigIntLiteral((loc, lit)) => (
            bigint_literal(offset_table, config, loc, lit),
            false,
            decl_meth.comments.dupe(),
        ),
        Key::Identifier(id) => (
            identifier(offset_table, config, id),
            false,
            decl_meth.comments.dupe(),
        ),
        Key::PrivateName(name) => (
            private_identifier(offset_table, config, name),
            false,
            decl_meth.comments.dupe(),
        ),
        Key::Computed(computed_key) => {
            let merged =
                ast_utils::merge_comments(computed_key.comments.dupe(), decl_meth.comments.dupe());
            (
                expression(offset_table, config, false, &computed_key.expression),
                true,
                merged,
            )
        }
    };

    let mut props = vec![
        ("key", key),
        (
            "value",
            type_annotation(offset_table, config, &decl_meth.annot),
        ),
        ("static", Value::Bool(decl_meth.static_)),
        ("optional", Value::Bool(decl_meth.optional)),
        ("computed", Value::Bool(computed)),
    ];
    match decl_meth.kind {
        ast::class::MethodKind::Get => props.push(("kind", Value::String("get".into()))),
        ast::class::MethodKind::Set => props.push(("kind", Value::String("set".into()))),
        ast::class::MethodKind::Method | ast::class::MethodKind::Constructor => {}
    }
    if decl_meth.override_ {
        props.push(("override", bool_value(true)));
    }
    if let Some(tsa) = &decl_meth.ts_accessibility {
        props.push(("tsAccessibility", ts_accessibility(tsa)));
    }
    node(
        offset_table,
        config,
        "DeclareMethodDefinition",
        &decl_meth.loc,
        comments.as_ref(),
        props,
    )
}

fn class_abstract_method(
    offset_table: &OffsetTable,
    config: &Config,
    abs_meth: &ast::class::AbstractMethod<Loc, Loc>,
) -> Value {
    let (key, computed, comments) = property_key(
        offset_table,
        config,
        &abs_meth.key,
        abs_meth.comments.as_ref(),
    );
    let mut fields = vec![
        ("key", key),
        (
            "value",
            function_type(offset_table, config, &abs_meth.annot.0, &abs_meth.annot.1),
        ),
        ("computed", bool_value(computed)),
    ];
    if abs_meth.override_ {
        fields.push(("override", bool_value(true)));
    }
    if let Some(tsa) = &abs_meth.ts_accessibility {
        fields.push(("tsAccessibility", ts_accessibility(tsa)));
    }
    node(
        offset_table,
        config,
        "AbstractMethodDefinition",
        &abs_meth.loc,
        comments.as_ref(),
        fields,
    )
}

fn class_abstract_property(
    offset_table: &OffsetTable,
    config: &Config,
    abs_prop: &ast::class::AbstractProperty<Loc, Loc>,
) -> Value {
    let (key, computed, comments) = property_key(
        offset_table,
        config,
        &abs_prop.key,
        abs_prop.comments.as_ref(),
    );
    let mut fields = vec![
        ("key", key),
        ("value", Value::Null),
        (
            "typeAnnotation",
            annotation_or_hint(offset_table, config, &abs_prop.annot),
        ),
        ("computed", bool_value(computed)),
        (
            "variance",
            option(&abs_prop.variance, |v| variance(offset_table, config, v)),
        ),
    ];
    if abs_prop.override_ {
        fields.push(("override", bool_value(true)));
    }
    if let Some(tsa) = &abs_prop.ts_accessibility {
        fields.push(("tsAccessibility", ts_accessibility(tsa)));
    }
    node(
        offset_table,
        config,
        "AbstractPropertyDefinition",
        &abs_prop.loc,
        comments.as_ref(),
        fields,
    )
}

fn class_private_field(
    offset_table: &OffsetTable,
    config: &Config,
    field: &ast::class::PrivateField<Loc, Loc>,
) -> Value {
    use ast::class::property::Value as PropertyValue;

    let (value, declare) = match &field.value {
        PropertyValue::Declared => (None, true),
        PropertyValue::Uninitialized => (None, false),
        PropertyValue::Initialized(expr) => (Some(expr), false),
    };

    let mut props = vec![
        ("key", private_identifier(offset_table, config, &field.key)),
        (
            "value",
            option(&value, |v| expression(offset_table, config, false, v)),
        ),
        (
            "typeAnnotation",
            annotation_or_hint(offset_table, config, &field.annot),
        ),
        ("computed", bool_value(false)),
        ("static", bool_value(field.static_)),
        ("optional", bool_value(field.optional)),
        (
            "variance",
            option(&field.variance, |v| variance(offset_table, config, v)),
        ),
        (
            "tsAccessibility",
            option(&field.ts_accessibility, ts_accessibility),
        ),
    ];

    if field.override_ {
        props.push(("override", bool_value(true)));
    }

    if !field.decorators.is_empty() {
        props.push((
            "decorators",
            array_of_list(&field.decorators, |d| {
                class_decorator(offset_table, config, d)
            }),
        ));
    }

    if declare {
        props.push(("declare", bool_value(declare)));
    }

    node(
        offset_table,
        config,
        "PropertyDefinition",
        &field.loc,
        field.comments.as_ref(),
        props,
    )
}

fn property_key<'a>(
    offset_table: &OffsetTable,
    config: &Config,
    key: &ast::expression::object::Key<Loc, Loc>,
    comments: Option<&'a ast::Syntax<Loc, ()>>,
) -> (Value, bool, Option<ast::Syntax<Loc, ()>>) {
    use ast::expression::object::Key;

    match key {
        Key::StringLiteral((loc, lit)) => (
            string_literal(offset_table, config, loc, lit),
            false,
            comments.duped(),
        ),
        Key::NumberLiteral((loc, lit)) => (
            number_literal(offset_table, config, loc, lit),
            false,
            comments.duped(),
        ),
        Key::BigIntLiteral((loc, lit)) => (
            bigint_literal(offset_table, config, loc, lit),
            false,
            comments.duped(),
        ),
        Key::Identifier(id) => (
            identifier(offset_table, config, id),
            false,
            comments.duped(),
        ),
        Key::PrivateName(_) => {
            panic!("Internal Error: Private name")
        }
        Key::Computed(computed_key) => {
            let merged = ast_utils::merge_comments(computed_key.comments.dupe(), comments.duped());
            (
                expression(offset_table, config, false, &computed_key.expression),
                true,
                merged,
            )
        }
    }
}

fn class_property(
    offset_table: &OffsetTable,
    config: &Config,
    property: &ast::class::Property<Loc, Loc>,
) -> Value {
    class_property_helper(
        offset_table,
        config,
        "PropertyDefinition",
        &property.loc,
        property,
    )
}

fn class_property_helper(
    offset_table: &OffsetTable,
    config: &Config,
    node_type: &str,
    loc: &Loc,
    property: &ast::class::Property<Loc, Loc>,
) -> Value {
    use ast::class::property::Value as PropertyValue;

    let (key, computed, comments) = property_key(
        offset_table,
        config,
        &property.key,
        property.comments.as_ref(),
    );

    let (value, declare) = match &property.value {
        PropertyValue::Declared => (None, true),
        PropertyValue::Uninitialized => (None, false),
        PropertyValue::Initialized(expr) => (Some(expr), false),
    };

    let mut props = vec![
        ("key", key),
        (
            "value",
            option(&value, |v| expression(offset_table, config, false, v)),
        ),
        (
            "typeAnnotation",
            annotation_or_hint(offset_table, config, &property.annot),
        ),
        ("computed", bool_value(computed)),
        ("static", bool_value(property.static_)),
        ("optional", bool_value(property.optional)),
        (
            "variance",
            option(&property.variance, |v| variance(offset_table, config, v)),
        ),
        (
            "tsAccessibility",
            option(&property.ts_accessibility, ts_accessibility),
        ),
    ];

    if property.override_ {
        props.push(("override", bool_value(true)));
    }

    if !property.decorators.is_empty() {
        props.push((
            "decorators",
            array_of_list(&property.decorators, |d| {
                class_decorator(offset_table, config, d)
            }),
        ));
    }

    if declare {
        props.push(("declare", bool_value(declare)));
    }

    node(
        offset_table,
        config,
        node_type,
        loc,
        comments.as_ref(),
        props,
    )
}

fn component_declaration(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    component: &ast::statement::ComponentDeclaration<Loc, Loc>,
) -> Value {
    let comments = ast_utils::merge_comments(
        format_internal_comments(component.params.comments.as_ref()),
        component.comments.dupe(),
    );

    let (node_type, implicit_declare) = match &component.body {
        None => ("DeclareComponent", true),
        Some(_) => ("ComponentDeclaration", false),
    };

    node(
        offset_table,
        config,
        node_type,
        loc,
        comments.as_ref(),
        vec![
            (
                "body",
                option(&component.body, |(body_loc, body_block)| {
                    block(offset_table, config, body_loc, body_block)
                }),
            ),
            ("id", identifier(offset_table, config, &component.id)),
            ("implicitDeclare", Value::Bool(implicit_declare)),
            (
                "params",
                component_params(offset_table, config, &component.params),
            ),
            (
                "rendersType",
                renders_annotation(offset_table, config, &component.renders),
            ),
            (
                "typeParameters",
                option(&component.tparams, |t| {
                    type_parameter_declaration(offset_table, config, t)
                }),
            ),
            ("async", bool_value(component.async_)),
        ],
    )
}

fn component_params(
    offset_table: &OffsetTable,
    config: &Config,
    params: &ast::statement::component_params::Params<Loc, Loc>,
) -> Value {
    let mut param_values: Vec<Value> = params
        .params
        .iter()
        .map(|p| component_param(offset_table, config, p))
        .collect();

    if let Some(ref rest) = params.rest {
        let rest_value = node(
            offset_table,
            config,
            "RestElement",
            &rest.loc,
            rest.comments.as_ref(),
            vec![("argument", pattern(offset_table, config, &rest.argument))],
        );
        param_values.push(rest_value);
    }

    Value::Array(param_values)
}

fn component_param(
    offset_table: &OffsetTable,
    config: &Config,
    param: &ast::statement::component_params::Param<Loc, Loc>,
) -> Value {
    use ast::statement::component_params::ParamName;

    let name = match &param.name {
        ParamName::Identifier(id) => identifier(offset_table, config, id),
        ParamName::StringLiteral((loc, lit)) => string_literal(offset_table, config, loc, lit),
    };

    let local = match &param.default {
        Some(default) => node(
            offset_table,
            config,
            "AssignmentPattern",
            &param.loc,
            None,
            vec![
                ("left", pattern(offset_table, config, &param.local)),
                ("right", expression(offset_table, config, false, default)),
            ],
        ),
        None => pattern(offset_table, config, &param.local),
    };

    node(
        offset_table,
        config,
        "ComponentParameter",
        &param.loc,
        None,
        vec![
            ("name", name),
            ("local", local),
            ("shorthand", bool_value(param.shorthand)),
        ],
    )
}

fn enum_body(
    offset_table: &OffsetTable,
    config: &Config,
    body: &ast::statement::enum_declaration::Body<Loc>,
) -> Value {
    use ast::statement::enum_declaration::Member;

    let ast::statement::enum_declaration::Body {
        loc,
        members,
        explicit_type,
        has_unknown_members,
        comments,
    } = body;

    let enum_member_name = |id: &ast::statement::enum_declaration::MemberName<Loc>| -> Value {
        match id {
            ast::statement::enum_declaration::MemberName::Identifier(ident) => {
                identifier(offset_table, config, ident)
            }
            ast::statement::enum_declaration::MemberName::StringLiteral(loc, sl) => {
                string_literal(offset_table, config, loc, sl)
            }
        }
    };
    let enum_member = |member: &Member<Loc>| -> Value {
        match member {
            Member::BooleanMember(member) => node(
                offset_table,
                config,
                "EnumBooleanMember",
                &member.loc,
                None,
                vec![
                    ("id", enum_member_name(&member.id)),
                    (
                        "init",
                        boolean_literal(offset_table, config, &member.init.0, &member.init.1),
                    ),
                ],
            ),
            Member::NumberMember(member) => node(
                offset_table,
                config,
                "EnumNumberMember",
                &member.loc,
                None,
                vec![
                    ("id", enum_member_name(&member.id)),
                    (
                        "init",
                        number_literal(offset_table, config, &member.init.0, &member.init.1),
                    ),
                ],
            ),
            Member::StringMember(member) => node(
                offset_table,
                config,
                "EnumStringMember",
                &member.loc,
                None,
                vec![
                    ("id", enum_member_name(&member.id)),
                    (
                        "init",
                        string_literal(offset_table, config, &member.init.0, &member.init.1),
                    ),
                ],
            ),
            Member::BigIntMember(member) => node(
                offset_table,
                config,
                "EnumBigIntMember",
                &member.loc,
                None,
                vec![
                    ("id", enum_member_name(&member.id)),
                    (
                        "init",
                        bigint_literal(offset_table, config, &member.init.0, &member.init.1),
                    ),
                ],
            ),
            Member::DefaultedMember(member) => node(
                offset_table,
                config,
                "EnumDefaultedMember",
                &member.loc,
                None,
                vec![("id", enum_member_name(&member.id))],
            ),
        }
    };

    let members_value: Vec<Value> = members.iter().map(enum_member).collect();

    let explicit_type_str = match explicit_type {
        Some((_, t)) => Value::String(t.as_str().to_owned()),
        None => Value::Null,
    };

    node(
        offset_table,
        config,
        "EnumBody",
        loc,
        format_internal_comments(comments.as_ref()).as_ref(),
        vec![
            ("members", Value::Array(members_value)),
            ("explicitType", explicit_type_str),
            (
                "hasUnknownMembers",
                bool_value(has_unknown_members.is_some()),
            ),
        ],
    )
}

fn enum_declaration(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    enum_decl: &ast::statement::EnumDeclaration<Loc, Loc>,
) -> Value {
    let mut props = Vec::with_capacity(3);
    if enum_decl.const_ {
        props.push(("const", bool_value(true)));
    }
    props.push(("id", identifier(offset_table, config, &enum_decl.id)));
    props.push(("body", enum_body(offset_table, config, &enum_decl.body)));
    node(
        offset_table,
        config,
        "EnumDeclaration",
        loc,
        enum_decl.comments.as_ref(),
        props,
    )
}

fn interface_declaration(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    interface: &ast::statement::Interface<Loc, Loc>,
) -> Value {
    let extends: Vec<Value> = interface
        .extends
        .iter()
        .map(|(loc, ext)| interface_extends(offset_table, config, loc, ext))
        .collect();

    node(
        offset_table,
        config,
        "InterfaceDeclaration",
        loc,
        interface.comments.as_ref(),
        vec![
            ("id", identifier(offset_table, config, &interface.id)),
            (
                "typeParameters",
                option(&interface.tparams, |t| {
                    type_parameter_declaration(offset_table, config, t)
                }),
            ),
            (
                "body",
                object_type(
                    offset_table,
                    config,
                    &interface.body.0,
                    &interface.body.1,
                    false,
                ),
            ),
            ("extends", Value::Array(extends)),
        ],
    )
}

fn interface_extends(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    extends: &ast::types::Generic<Loc, Loc>,
) -> Value {
    use ast::types::generic;

    let id = match &extends.id {
        generic::Identifier::Unqualified(id) => identifier(offset_table, config, id),
        generic::Identifier::Qualified(q) => {
            generic_type_qualified_identifier(offset_table, config, q)
        }
        generic::Identifier::ImportTypeAnnot(import_type) => {
            // import("module") type syntax in extends position
            import_type_annotation(offset_table, config, &import_type.loc, import_type)
        }
    };

    node(
        offset_table,
        config,
        "InterfaceExtends",
        loc,
        extends.comments.as_ref(),
        vec![
            ("id", id),
            (
                "typeArguments",
                option(&extends.targs, |t| type_args(offset_table, config, t)),
            ),
        ],
    )
}

fn pattern(
    offset_table: &OffsetTable,
    config: &Config,
    pat: &ast::pattern::Pattern<Loc, Loc>,
) -> Value {
    use ast::types::AnnotationOrHint;

    match pat {
        ast::pattern::Pattern::Object { loc, inner } => {
            let formatted_comments = format_internal_comments(inner.comments.as_ref());
            let type_annot = match &inner.annot {
                AnnotationOrHint::Available(annot) => type_annotation(offset_table, config, annot),
                AnnotationOrHint::Missing(_) => Value::Null,
            };
            node(
                offset_table,
                config,
                "ObjectPattern",
                loc,
                formatted_comments.as_ref(),
                vec![
                    (
                        "properties",
                        array_of_list(&inner.properties, |p| {
                            object_pattern_property(offset_table, config, p)
                        }),
                    ),
                    ("typeAnnotation", type_annot),
                    ("optional", bool_value(inner.optional)),
                ],
            )
        }
        ast::pattern::Pattern::Array { loc, inner } => {
            let formatted_comments = format_internal_comments(inner.comments.as_ref());
            let type_annot = match &inner.annot {
                AnnotationOrHint::Available(annot) => type_annotation(offset_table, config, annot),
                AnnotationOrHint::Missing(_) => Value::Null,
            };
            node(
                offset_table,
                config,
                "ArrayPattern",
                loc,
                formatted_comments.as_ref(),
                vec![
                    (
                        "elements",
                        array_of_list(&inner.elements, |e| {
                            array_pattern_element(offset_table, config, e)
                        }),
                    ),
                    ("typeAnnotation", type_annot),
                    ("optional", bool_value(inner.optional)),
                ],
            )
        }
        ast::pattern::Pattern::Identifier { loc, inner } => {
            pattern_identifier(offset_table, config, loc, inner)
        }
        ast::pattern::Pattern::Expression { loc: _, inner } => {
            expression(offset_table, config, false, inner)
        }
    }
}

fn function_param(
    offset_table: &OffsetTable,
    config: &Config,
    param: &ast::function::Param<Loc, Loc>,
) -> Value {
    match param {
        ast::function::Param::RegularParam {
            loc,
            argument,
            default,
        } => match default {
            Some(default) => node(
                offset_table,
                config,
                "AssignmentPattern",
                loc,
                None,
                vec![
                    ("left", pattern(offset_table, config, argument)),
                    ("right", expression(offset_table, config, false, default)),
                ],
            ),
            None => pattern(offset_table, config, argument),
        },
        ast::function::Param::ParamProperty { loc, property } => {
            class_property_helper(offset_table, config, "ParameterProperty", loc, property)
        }
    }
}

fn this_param(
    offset_table: &OffsetTable,
    config: &Config,
    this: &ast::function::ThisParam<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "Identifier",
        &this.loc,
        this.comments.as_ref(),
        vec![
            ("name", string("this")),
            (
                "typeAnnotation",
                type_annotation(offset_table, config, &this.annot),
            ),
        ],
    )
}

fn function_params(
    offset_table: &OffsetTable,
    config: &Config,
    params: &ast::function::Params<Loc, Loc>,
) -> Value {
    let mut param_list: Vec<Value> = Vec::new();
    if let Some(ref this) = params.this_ {
        param_list.push(this_param(offset_table, config, this));
    }
    for param in params.params.iter() {
        param_list.push(function_param(offset_table, config, param));
    }
    if let Some(ref rest) = params.rest {
        let rest_element = node(
            offset_table,
            config,
            "RestElement",
            &rest.loc,
            rest.comments.as_ref(),
            vec![("argument", pattern(offset_table, config, &rest.argument))],
        );
        param_list.push(rest_element);
    }
    Value::Array(param_list)
}

fn rest_element(
    offset_table: &OffsetTable,
    config: &Config,
    rest: &ast::pattern::RestElement<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "RestElement",
        &rest.loc,
        rest.comments.as_ref(),
        vec![("argument", pattern(offset_table, config, &rest.argument))],
    )
}

fn array_pattern_element(
    offset_table: &OffsetTable,
    config: &Config,
    element: &ast::pattern::array::Element<Loc, Loc>,
) -> Value {
    match element {
        ast::pattern::array::Element::Hole(_) => Value::Null,
        ast::pattern::array::Element::NormalElement(inner) => match &inner.default {
            Some(default) => node(
                offset_table,
                config,
                "AssignmentPattern",
                &inner.loc,
                None,
                vec![
                    ("left", pattern(offset_table, config, &inner.argument)),
                    ("right", expression(offset_table, config, false, default)),
                ],
            ),
            None => pattern(offset_table, config, &inner.argument),
        },
        ast::pattern::array::Element::RestElement(inner) => {
            rest_element(offset_table, config, inner)
        }
    }
}

fn function_return_type(
    offset_table: &OffsetTable,
    config: &Config,
    return_: &ast::function::ReturnAnnot<Loc, Loc>,
) -> Value {
    use ast::function::ReturnAnnot;

    match return_ {
        ReturnAnnot::Missing(_) => Value::Null,
        ReturnAnnot::Available(annot) => type_annotation(offset_table, config, annot),
        ReturnAnnot::TypeGuard(guard) => type_guard_annotation(offset_table, config, guard),
    }
}

fn object_property(
    offset_table: &OffsetTable,
    config: &Config,
    prop: &ast::expression::object::Property<Loc, Loc>,
) -> Value {
    use ast::expression::object::NormalProperty;
    use ast::expression::object::Property;

    match prop {
        Property::NormalProperty(normal_prop) => {
            let (key, value, kind, method, shorthand, comments) = match normal_prop {
                NormalProperty::Init {
                    loc: _,
                    key,
                    value,
                    shorthand,
                } => (
                    key,
                    expression(offset_table, config, false, value),
                    "init",
                    false,
                    *shorthand,
                    None,
                ),
                NormalProperty::Method { loc: _, key, value } => (
                    key,
                    function_expression(offset_table, config, &value.0, &value.1),
                    "init",
                    true,
                    false,
                    None,
                ),
                NormalProperty::Get {
                    loc: _,
                    key,
                    value,
                    comments,
                } => (
                    key,
                    function_expression(offset_table, config, &value.0, &value.1),
                    "get",
                    false,
                    false,
                    comments.as_ref(),
                ),
                NormalProperty::Set {
                    loc: _,
                    key,
                    value,
                    comments,
                } => (
                    key,
                    function_expression(offset_table, config, &value.0, &value.1),
                    "set",
                    false,
                    false,
                    comments.as_ref(),
                ),
            };

            let (key_value, computed, final_comments) =
                property_key(offset_table, config, key, comments);

            node(
                offset_table,
                config,
                "Property",
                normal_prop.loc(),
                final_comments.as_ref(),
                vec![
                    ("key", key_value),
                    ("value", value),
                    ("kind", string(kind)),
                    ("method", bool_value(method)),
                    ("shorthand", bool_value(shorthand)),
                    ("computed", bool_value(computed)),
                ],
            )
        }
        Property::SpreadProperty(spread) => node(
            offset_table,
            config,
            "SpreadElement",
            &spread.loc,
            spread.comments.as_ref(),
            vec![(
                "argument",
                expression(offset_table, config, false, &spread.argument),
            )],
        ),
    }
}

fn object_pattern_property(
    offset_table: &OffsetTable,
    config: &Config,
    prop: &ast::pattern::object::Property<Loc, Loc>,
) -> Value {
    use ast::pattern::object::Property;

    match prop {
        Property::NormalProperty(normal) => {
            let (key, computed, comments) = match &normal.key {
                ast::pattern::object::Key::StringLiteral((loc, lit)) => {
                    (string_literal(offset_table, config, loc, lit), false, None)
                }
                ast::pattern::object::Key::NumberLiteral((loc, lit)) => {
                    (number_literal(offset_table, config, loc, lit), false, None)
                }
                ast::pattern::object::Key::BigIntLiteral((loc, lit)) => {
                    (bigint_literal(offset_table, config, loc, lit), false, None)
                }
                ast::pattern::object::Key::Identifier(id) => {
                    (identifier(offset_table, config, id), false, None)
                }
                ast::pattern::object::Key::Computed(computed_key) => (
                    expression(offset_table, config, false, &computed_key.expression),
                    true,
                    computed_key.comments.dupe(),
                ),
            };

            let value = match &normal.default {
                Some(default) => {
                    let loc = Loc::between(normal.pattern.loc(), default.loc());
                    node(
                        offset_table,
                        config,
                        "AssignmentPattern",
                        &loc,
                        None,
                        vec![
                            ("left", pattern(offset_table, config, &normal.pattern)),
                            ("right", expression(offset_table, config, false, default)),
                        ],
                    )
                }
                None => pattern(offset_table, config, &normal.pattern),
            };

            node(
                offset_table,
                config,
                "Property",
                &normal.loc,
                comments.as_ref(),
                vec![
                    ("key", key),
                    ("value", value),
                    ("kind", string("init")),
                    ("method", bool_value(false)),
                    ("shorthand", bool_value(normal.shorthand)),
                    ("computed", bool_value(computed)),
                ],
            )
        }
        Property::RestElement(rest) => rest_element(offset_table, config, rest),
    }
}

fn spread_element(
    offset_table: &OffsetTable,
    config: &Config,
    in_optional_chain: bool,
    spread: &ast::expression::SpreadElement<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "SpreadElement",
        &spread.loc,
        spread.comments.as_ref(),
        vec![(
            "argument",
            expression(offset_table, config, in_optional_chain, &spread.argument),
        )],
    )
}

fn array_element(
    offset_table: &OffsetTable,
    config: &Config,
    element: &ast::expression::ArrayElement<Loc, Loc>,
) -> Value {
    match element {
        ast::expression::ArrayElement::Hole(_) => Value::Null,
        ast::expression::ArrayElement::Expression(expr) => {
            expression(offset_table, config, false, expr)
        }
        ast::expression::ArrayElement::Spread(spread) => {
            spread_element(offset_table, config, false, spread)
        }
    }
}

fn f64_to_number(v: f64) -> Number {
    if v.trunc() == v {
        match Number::from_i128(v as i128) {
            Some(n) => n,
            None => Number::from_f64(v).unwrap(),
        }
    } else {
        Number::from_f64(v).unwrap()
    }
}

fn number_literal(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    lit: &ast::NumberLiteral<Loc>,
) -> Value {
    let value = Value::Number(f64_to_number(lit.value));
    node(
        offset_table,
        config,
        "Literal",
        loc,
        lit.comments.as_ref(),
        vec![("value", value), ("raw", string(&lit.raw))],
    )
}

fn bigint_literal(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    lit: &ast::BigIntLiteral<Loc>,
) -> Value {
    // https://github.com/estree/estree/blob/master/es2020.md#bigintliteral
    // `bigint` property is the string representation of the `BigInt` value.
    // It must contain only decimal digits and not include numeric separators `_` or the suffix `n`.
    let bigint = match lit.value {
        Some(value) => value.to_string(),
        None => lit.raw[..lit.raw.len() - 1].replace('_', ""),
    };
    node(
        offset_table,
        config,
        "Literal",
        loc,
        lit.comments.as_ref(),
        vec![
            ("value", Value::Null),
            ("bigint", string(&bigint)),
            ("raw", string(&lit.raw)),
        ],
    )
}

fn string_literal(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    lit: &ast::StringLiteral<Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "Literal",
        loc,
        lit.comments.as_ref(),
        vec![("value", string(&lit.value)), ("raw", string(&lit.raw))],
    )
}

fn boolean_literal(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    lit: &ast::BooleanLiteral<Loc>,
) -> Value {
    let raw = if lit.value { "true" } else { "false" };
    node(
        offset_table,
        config,
        "Literal",
        loc,
        lit.comments.as_ref(),
        vec![("value", bool_value(lit.value)), ("raw", string(raw))],
    )
}

fn regexp_literal(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    lit: &ast::RegExpLiteral<Loc>,
) -> Value {
    let regex = obj(vec![
        ("pattern", string(&lit.pattern)),
        ("flags", string(&lit.flags)),
    ]);
    node(
        offset_table,
        config,
        "Literal",
        loc,
        lit.comments.as_ref(),
        vec![
            ("value", Value::Null),
            ("raw", string(&lit.raw)),
            ("regex", regex),
        ],
    )
}

fn null_literal(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    comments: Option<&ast::Syntax<Loc, ()>>,
) -> Value {
    node(
        offset_table,
        config,
        "Literal",
        loc,
        comments,
        vec![("value", Value::Null), ("raw", string("null"))],
    )
}

fn template_literal(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    lit: &ast::expression::TemplateLiteral<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "TemplateLiteral",
        loc,
        lit.comments.as_ref(),
        vec![
            (
                "quasis",
                array_of_list(&lit.quasis, |q| template_element(offset_table, config, q)),
            ),
            (
                "expressions",
                array_of_list(&lit.expressions, |e| {
                    expression(offset_table, config, false, e)
                }),
            ),
        ],
    )
}

fn template_element(
    offset_table: &OffsetTable,
    config: &Config,
    element: &ast::expression::template_literal::Element<Loc>,
) -> Value {
    let value = obj(vec![
        ("raw", string(&element.value.raw)),
        ("cooked", string(&element.value.cooked)),
    ]);
    node(
        offset_table,
        config,
        "TemplateElement",
        &element.loc,
        None,
        vec![("value", value), ("tail", bool_value(element.tail))],
    )
}

fn tagged_template(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    tagged: &ast::expression::TaggedTemplate<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "TaggedTemplateExpression",
        loc,
        tagged.comments.as_ref(),
        vec![
            ("tag", expression(offset_table, config, false, &tagged.tag)),
            (
                "typeArguments",
                option(&tagged.targs, |t| call_type_args(offset_table, config, t)),
            ),
            (
                "quasi",
                template_literal(offset_table, config, &tagged.quasi.0, &tagged.quasi.1),
            ),
        ],
    )
}

fn variable_declaration(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    decl: &ast::statement::VariableDeclaration<Loc, Loc>,
) -> Value {
    let kind = decl.kind.as_str();
    node(
        offset_table,
        config,
        "VariableDeclaration",
        loc,
        decl.comments.as_ref(),
        vec![
            ("kind", string(kind)),
            (
                "declarations",
                array_of_list(&decl.declarations, |d| {
                    variable_declarator(offset_table, config, d)
                }),
            ),
        ],
    )
}

fn variable_declarator(
    offset_table: &OffsetTable,
    config: &Config,
    declarator: &ast::statement::variable::Declarator<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "VariableDeclarator",
        &declarator.loc,
        None,
        vec![
            ("id", pattern(offset_table, config, &declarator.id)),
            (
                "init",
                option(&declarator.init, |e| {
                    expression(offset_table, config, false, e)
                }),
            ),
        ],
    )
}

fn variance(offset_table: &OffsetTable, config: &Config, var: &ast::Variance<Loc>) -> Value {
    use ast::VarianceKind;
    let kind_str = match var.kind {
        VarianceKind::Plus => "plus",
        VarianceKind::Minus => "minus",
        VarianceKind::Readonly => "readonly",
        VarianceKind::Writeonly => "writeonly",
        VarianceKind::In => "in",
        VarianceKind::Out => "out",
        VarianceKind::InOut => "in out",
    };
    node(
        offset_table,
        config,
        "Variance",
        &var.loc,
        var.comments.as_ref(),
        vec![("kind", string(kind_str))],
    )
}

fn ts_accessibility(access: &ast::class::ts_accessibility::TSAccessibility<Loc>) -> Value {
    use ast::class::ts_accessibility::Kind;
    let kind_str = match access.kind {
        Kind::Public => "public",
        Kind::Protected => "protected",
        Kind::Private => "private",
    };
    string(kind_str)
}

fn type_(offset_table: &OffsetTable, config: &Config, ty: &ast::types::Type<Loc, Loc>) -> Value {
    use ast::types::TypeInner;

    match ty.deref() {
        TypeInner::Any { loc, comments } => node(
            offset_table,
            config,
            "AnyTypeAnnotation",
            loc,
            comments.as_ref(),
            vec![],
        ),
        TypeInner::Mixed { loc, comments } => node(
            offset_table,
            config,
            "MixedTypeAnnotation",
            loc,
            comments.as_ref(),
            vec![],
        ),
        TypeInner::Empty { loc, comments } => node(
            offset_table,
            config,
            "EmptyTypeAnnotation",
            loc,
            comments.as_ref(),
            vec![],
        ),
        TypeInner::Void { loc, comments } => node(
            offset_table,
            config,
            "VoidTypeAnnotation",
            loc,
            comments.as_ref(),
            vec![],
        ),
        TypeInner::Null { loc, comments } => node(
            offset_table,
            config,
            "NullLiteralTypeAnnotation",
            loc,
            comments.as_ref(),
            vec![],
        ),
        TypeInner::Symbol { loc, comments } => node(
            offset_table,
            config,
            "SymbolTypeAnnotation",
            loc,
            comments.as_ref(),
            vec![],
        ),
        TypeInner::Number { loc, comments } => node(
            offset_table,
            config,
            "NumberTypeAnnotation",
            loc,
            comments.as_ref(),
            vec![],
        ),
        TypeInner::BigInt { loc, comments } => node(
            offset_table,
            config,
            "BigIntTypeAnnotation",
            loc,
            comments.as_ref(),
            vec![],
        ),
        TypeInner::String { loc, comments } => node(
            offset_table,
            config,
            "StringTypeAnnotation",
            loc,
            comments.as_ref(),
            vec![],
        ),
        TypeInner::Boolean { loc, comments, .. } => node(
            offset_table,
            config,
            "BooleanTypeAnnotation",
            loc,
            comments.as_ref(),
            vec![],
        ),
        TypeInner::Nullable { loc, inner } => node(
            offset_table,
            config,
            "NullableTypeAnnotation",
            loc,
            inner.comments.as_ref(),
            vec![(
                "typeAnnotation",
                type_(offset_table, config, &inner.argument),
            )],
        ),
        TypeInner::Function { loc, inner } => function_type(offset_table, config, loc, inner),
        TypeInner::ConstructorType {
            loc,
            abstract_,
            inner,
        } => constructor_type(offset_table, config, loc, *abstract_, inner),
        TypeInner::Component { loc, inner } => component_type(offset_table, config, loc, inner),
        TypeInner::Object { loc, inner } => object_type(offset_table, config, loc, inner, true),
        TypeInner::Interface { loc, inner } => interface_type(offset_table, config, loc, inner),
        TypeInner::Array { loc, inner } => node(
            offset_table,
            config,
            "ArrayTypeAnnotation",
            loc,
            inner.comments.as_ref(),
            vec![("elementType", type_(offset_table, config, &inner.argument))],
        ),
        TypeInner::Conditional { loc, inner } => conditional_type(offset_table, config, loc, inner),
        TypeInner::Infer { loc, inner } => infer_type(offset_table, config, loc, inner),
        TypeInner::Generic { loc, inner } => generic_type(offset_table, config, loc, inner),
        TypeInner::IndexedAccess { loc, inner } => node(
            offset_table,
            config,
            "IndexedAccessType",
            loc,
            inner.comments.as_ref(),
            vec![
                ("objectType", type_(offset_table, config, &inner.object)),
                ("indexType", type_(offset_table, config, &inner.index)),
            ],
        ),
        TypeInner::OptionalIndexedAccess { loc, inner } => node(
            offset_table,
            config,
            "OptionalIndexedAccessType",
            loc,
            inner.indexed_access.comments.as_ref(),
            vec![
                (
                    "objectType",
                    type_(offset_table, config, &inner.indexed_access.object),
                ),
                (
                    "indexType",
                    type_(offset_table, config, &inner.indexed_access.index),
                ),
                ("optional", bool_value(inner.optional)),
            ],
        ),
        TypeInner::Union { loc, inner } => {
            let mut types = vec![inner.types.0.clone()];
            types.push(inner.types.1.clone());
            types.extend(inner.types.2.clone());
            node(
                offset_table,
                config,
                "UnionTypeAnnotation",
                loc,
                inner.comments.as_ref(),
                vec![(
                    "types",
                    array_of_list(&types, |t| type_(offset_table, config, t)),
                )],
            )
        }
        TypeInner::Intersection { loc, inner } => {
            let mut types = vec![inner.types.0.clone()];
            types.push(inner.types.1.clone());
            types.extend(inner.types.2.clone());
            node(
                offset_table,
                config,
                "IntersectionTypeAnnotation",
                loc,
                inner.comments.as_ref(),
                vec![(
                    "types",
                    array_of_list(&types, |t| type_(offset_table, config, t)),
                )],
            )
        }
        TypeInner::Typeof { loc, inner } => typeof_type(offset_table, config, loc, inner),
        TypeInner::Keyof { loc, inner } => node(
            offset_table,
            config,
            "KeyofTypeAnnotation",
            loc,
            inner.comments.as_ref(),
            vec![("argument", type_(offset_table, config, &inner.argument))],
        ),
        TypeInner::Renders { loc, inner } => render_type(offset_table, config, loc, inner),
        TypeInner::ReadOnly { loc, inner } => node(
            offset_table,
            config,
            "TypeOperator",
            loc,
            inner.comments.as_ref(),
            vec![
                ("operator", string("readonly")),
                (
                    "typeAnnotation",
                    type_(offset_table, config, &inner.argument),
                ),
            ],
        ),
        TypeInner::Tuple { loc, inner } => tuple_type(offset_table, config, loc, inner),
        TypeInner::StringLiteral { loc, literal } => node(
            offset_table,
            config,
            "StringLiteralTypeAnnotation",
            loc,
            literal.comments.as_ref(),
            vec![
                ("value", string(&literal.value)),
                ("raw", string(&literal.raw)),
            ],
        ),
        TypeInner::NumberLiteral { loc, literal } => node(
            offset_table,
            config,
            "NumberLiteralTypeAnnotation",
            loc,
            literal.comments.as_ref(),
            vec![
                ("value", Value::Number(f64_to_number(literal.value))),
                ("raw", string(&literal.raw)),
            ],
        ),
        TypeInner::BigIntLiteral { loc, literal } => node(
            offset_table,
            config,
            "BigIntLiteralTypeAnnotation",
            loc,
            literal.comments.as_ref(),
            vec![("value", Value::Null), ("raw", string(&literal.raw))],
        ),
        TypeInner::BooleanLiteral { loc, literal } => {
            let raw = if literal.value { "true" } else { "false" };
            node(
                offset_table,
                config,
                "BooleanLiteralTypeAnnotation",
                loc,
                literal.comments.as_ref(),
                vec![("value", bool_value(literal.value)), ("raw", string(raw))],
            )
        }
        TypeInner::TemplateLiteral { loc, inner } => {
            template_literal_type(offset_table, config, loc, inner)
        }
        TypeInner::Exists { loc, comments } => node(
            offset_table,
            config,
            "ExistsTypeAnnotation",
            loc,
            comments.as_ref(),
            vec![],
        ),
        TypeInner::Unknown { loc, comments } => node(
            offset_table,
            config,
            "UnknownTypeAnnotation",
            loc,
            comments.as_ref(),
            vec![],
        ),
        TypeInner::Never { loc, comments } => node(
            offset_table,
            config,
            "NeverTypeAnnotation",
            loc,
            comments.as_ref(),
            vec![],
        ),
        TypeInner::Undefined { loc, comments } => node(
            offset_table,
            config,
            "UndefinedTypeAnnotation",
            loc,
            comments.as_ref(),
            vec![],
        ),
        TypeInner::UniqueSymbol {
            loc,
            symbol_loc,
            comments,
        } => node(
            offset_table,
            config,
            "TypeOperator",
            loc,
            comments.as_ref(),
            vec![
                ("operator", Value::String("unique".into())),
                (
                    "typeAnnotation",
                    node(
                        offset_table,
                        config,
                        "SymbolTypeAnnotation",
                        symbol_loc,
                        None,
                        vec![],
                    ),
                ),
            ],
        ),
    }
}

fn template_literal_type(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    lit: &ast::types::TypeTemplateLiteral<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "TemplateLiteralTypeAnnotation",
        loc,
        lit.comments.as_ref(),
        vec![
            (
                "quasis",
                array_of_list(&lit.quasis, |e| {
                    template_element_type(offset_table, config, e)
                }),
            ),
            (
                "types",
                array_of_list(&lit.types, |t| type_(offset_table, config, t)),
            ),
        ],
    )
}

fn template_element_type(
    offset_table: &OffsetTable,
    config: &Config,
    element: &ast::types::type_template_literal::Element<Loc>,
) -> Value {
    let value = obj(vec![
        ("raw", string(&element.value.raw)),
        ("cooked", string(&element.value.cooked)),
    ]);
    node(
        offset_table,
        config,
        "TemplateElement",
        &element.loc,
        None,
        vec![("value", value), ("tail", bool_value(element.tail))],
    )
}

fn function_return_annotation(
    offset_table: &OffsetTable,
    config: &Config,
    return_: &ast::types::function::ReturnAnnotation<Loc, Loc>,
) -> Value {
    use ast::types::function::ReturnAnnotation;

    match return_ {
        ReturnAnnotation::Missing(_) => Value::Null,
        ReturnAnnotation::Available(t) => type_(offset_table, config, &t.annotation),
        ReturnAnnotation::TypeGuard(g) => function_type_guard(offset_table, config, g),
    }
}

fn function_type_guard(
    offset_table: &OffsetTable,
    config: &Config,
    guard: &ast::types::TypeGuard<Loc, Loc>,
) -> Value {
    use ast::types::TypeGuardKind;

    let kind = match &guard.kind {
        TypeGuardKind::Default => Value::Null,
        TypeGuardKind::Implies => string("implies"),
        TypeGuardKind::Asserts => string("asserts"),
    };

    node(
        offset_table,
        config,
        "TypePredicate",
        &guard.loc,
        format_internal_comments(guard.comments.as_ref()).as_ref(),
        vec![
            (
                "parameterName",
                identifier(offset_table, config, &guard.guard.0),
            ),
            (
                "typeAnnotation",
                option(&guard.guard.1, |t| type_(offset_table, config, t)),
            ),
            ("kind", kind),
        ],
    )
}

fn function_type(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    func: &ast::types::Function<Loc, Loc>,
) -> Value {
    let formatted_comments = format_internal_comments(func.params.comments.as_ref());
    let comments = ast_utils::merge_comments(formatted_comments, func.comments.dupe());

    let is_hook = func.effect == ast::function::Effect::Hook;
    let name = if is_hook {
        "HookTypeAnnotation"
    } else {
        "FunctionTypeAnnotation"
    };

    let mut fields = vec![(
        "params",
        array_of_list(&func.params.params, |p| {
            function_type_param(offset_table, config, &p.loc, p, None)
        }),
    )];

    if !is_hook {
        fields.push((
            "this",
            option(&func.params.this, |t| {
                function_type_this_constraint(offset_table, config, t)
            }),
        ));
    }

    fields.extend([
        (
            "returnType",
            function_return_annotation(offset_table, config, &func.return_),
        ),
        (
            "rest",
            option(&func.params.rest, |r| {
                function_type_rest(offset_table, config, r)
            }),
        ),
        (
            "typeParameters",
            option(&func.tparams, |t| {
                type_parameter_declaration(offset_table, config, t)
            }),
        ),
    ]);

    node(offset_table, config, name, loc, comments.as_ref(), fields)
}

fn constructor_type(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    abstract_: bool,
    func: &ast::types::Function<Loc, Loc>,
) -> Value {
    let formatted_comments = format_internal_comments(func.params.comments.as_ref());
    let comments = ast_utils::merge_comments(formatted_comments, func.comments.dupe());

    let fields = vec![
        ("abstract", bool_value(abstract_)),
        (
            "params",
            array_of_list(&func.params.params, |p| {
                function_type_param(offset_table, config, &p.loc, p, None)
            }),
        ),
        (
            "returnType",
            function_return_annotation(offset_table, config, &func.return_),
        ),
        (
            "rest",
            option(&func.params.rest, |r| {
                function_type_rest(offset_table, config, r)
            }),
        ),
        (
            "typeParameters",
            option(&func.tparams, |t| {
                type_parameter_declaration(offset_table, config, t)
            }),
        ),
    ];

    node(
        offset_table,
        config,
        "ConstructorTypeAnnotation",
        loc,
        comments.as_ref(),
        fields,
    )
}

fn function_type_param(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    param: &ast::types::function::Param<Loc, Loc>,
    comments: Option<&ast::Syntax<Loc, ()>>,
) -> Value {
    use ast::types::function::ParamKind;
    match &param.param {
        ParamKind::Anonymous(annot) => node(
            offset_table,
            config,
            "FunctionTypeParam",
            loc,
            comments,
            vec![
                ("name", Value::Null),
                ("typeAnnotation", type_(offset_table, config, annot)),
                ("optional", bool_value(false)),
            ],
        ),
        ParamKind::Labeled {
            name,
            annot,
            optional,
        } => node(
            offset_table,
            config,
            "FunctionTypeParam",
            loc,
            comments,
            vec![
                ("name", identifier(offset_table, config, name)),
                ("typeAnnotation", type_(offset_table, config, annot)),
                ("optional", bool_value(*optional)),
            ],
        ),
        ParamKind::Destructuring(patt) => pattern(offset_table, config, patt),
    }
}

fn function_type_rest(
    offset_table: &OffsetTable,
    config: &Config,
    rest: &ast::types::function::RestParam<Loc, Loc>,
) -> Value {
    /* TODO: add a node for the rest param itself, including the `...`,
      like we do with RestElement on normal functions. This should be
      coordinated with Babel, ast-types, etc. so keeping the status quo for
      now. The new node would be a "FunctionTypeRestParam" with an
      "argument" field.
    */
    function_type_param(
        offset_table,
        config,
        &rest.argument.loc,
        &rest.argument,
        rest.comments.as_ref(),
    )
}

fn function_type_this_constraint(
    offset_table: &OffsetTable,
    config: &Config,
    this: &ast::types::function::ThisParam<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "FunctionTypeParam",
        &this.loc,
        this.comments.as_ref(),
        vec![
            ("name", Value::Null),
            (
                "typeAnnotation",
                type_(offset_table, config, &this.annot.annotation),
            ),
            ("optional", bool_value(false)),
        ],
    )
}

fn object_type(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    object: &ast::types::Object<Loc, Loc>,
    include_inexact: bool,
) -> Value {
    use ast::types::object::Property;

    let mut props: Vec<Value> = Vec::new();
    let mut ixs: Vec<Value> = Vec::new();
    let mut calls: Vec<Value> = Vec::new();
    let mut slots: Vec<Value> = Vec::new();

    for property in object.properties.iter() {
        match property {
            Property::NormalProperty(p) => {
                props.push(object_type_property(offset_table, config, p));
            }
            Property::SpreadProperty(p) => {
                props.push(object_type_spread_property(offset_table, config, p));
            }
            Property::Indexer(i) => {
                ixs.push(object_type_indexer(offset_table, config, i));
            }
            Property::CallProperty(c) => {
                calls.push(object_type_call_property(offset_table, config, c));
            }
            Property::InternalSlot(s) => {
                slots.push(object_type_internal_slot(offset_table, config, s));
            }
            Property::MappedType(m) => {
                props.push(object_type_mapped_type(offset_table, config, m));
            }
            Property::PrivateField(pf) => {
                props.push(object_type_private_field(offset_table, config, pf));
            }
        }
    }

    let mut fields = Vec::with_capacity(6);
    if include_inexact {
        fields.push(("inexact", bool_value(object.inexact)));
    }
    fields.push(("exact", bool_value(object.exact)));
    fields.push(("properties", Value::Array(props)));
    fields.push(("indexers", Value::Array(ixs)));
    fields.push(("callProperties", Value::Array(calls)));
    fields.push(("internalSlots", Value::Array(slots)));

    node(
        offset_table,
        config,
        "ObjectTypeAnnotation",
        loc,
        format_internal_comments(object.comments.as_ref()).as_ref(),
        fields,
    )
}

fn object_type_property(
    offset_table: &OffsetTable,
    config: &Config,
    prop: &ast::types::object::NormalProperty<Loc, Loc>,
) -> Value {
    use ast::types::object::PropertyValue;

    let (key, computed, comments) =
        property_key(offset_table, config, &prop.key, prop.comments.as_ref());

    let (value, kind) = match &prop.value {
        PropertyValue::Init(Some(t)) => (type_(offset_table, config, t), "init"),
        PropertyValue::Init(None) => (Value::Null, "init"),
        PropertyValue::Get(loc, f) => (function_type(offset_table, config, loc, f), "get"),
        PropertyValue::Set(loc, f) => (function_type(offset_table, config, loc, f), "set"),
    };

    let mut fields = vec![
        ("key", key),
        ("value", value),
        ("method", bool_value(prop.method)),
        ("optional", bool_value(prop.optional)),
        ("static", bool_value(prop.static_)),
        ("proto", bool_value(prop.proto)),
        ("abstract", bool_value(prop.abstract_)),
        (
            "variance",
            option(&prop.variance, |v| variance(offset_table, config, v)),
        ),
        ("kind", string(kind)),
        (
            "init",
            option(&prop.init, |e| expression(offset_table, config, false, e)),
        ),
    ];
    if computed {
        fields.push(("computed", bool_value(computed)));
    }
    if prop.override_ {
        fields.push(("override", bool_value(true)));
    }
    if let Some(tsa) = &prop.ts_accessibility {
        fields.push(("tsAccessibility", ts_accessibility(tsa)));
    }

    node(
        offset_table,
        config,
        "ObjectTypeProperty",
        &prop.loc,
        comments.as_ref(),
        fields,
    )
}

fn object_type_spread_property(
    offset_table: &OffsetTable,
    config: &Config,
    prop: &ast::types::object::SpreadProperty<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "ObjectTypeSpreadProperty",
        &prop.loc,
        prop.comments.as_ref(),
        vec![("argument", type_(offset_table, config, &prop.argument))],
    )
}

fn object_type_indexer(
    offset_table: &OffsetTable,
    config: &Config,
    indexer: &ast::types::object::Indexer<Loc, Loc>,
) -> Value {
    let mut props = vec![
        (
            "id",
            option(&indexer.id, |i| identifier(offset_table, config, i)),
        ),
        ("key", type_(offset_table, config, &indexer.key)),
        ("value", type_(offset_table, config, &indexer.value)),
        ("static", bool_value(indexer.static_)),
        (
            "variance",
            option(&indexer.variance, |v| variance(offset_table, config, v)),
        ),
    ];
    if indexer.optional {
        props.push(("optional", bool_value(indexer.optional)));
    }
    node(
        offset_table,
        config,
        "ObjectTypeIndexer",
        &indexer.loc,
        indexer.comments.as_ref(),
        props,
    )
}

fn object_type_call_property(
    offset_table: &OffsetTable,
    config: &Config,
    call: &ast::types::object::CallProperty<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "ObjectTypeCallProperty",
        &call.loc,
        call.comments.as_ref(),
        vec![
            (
                "value",
                function_type(offset_table, config, &call.value.0, &call.value.1),
            ),
            ("static", bool_value(call.static_)),
        ],
    )
}

fn object_type_mapped_type(
    offset_table: &OffsetTable,
    config: &Config,
    mapped: &ast::types::object::MappedType<Loc, Loc>,
) -> Value {
    use ast::types::object::MappedTypeOptionalFlag;
    use ast::types::object::MappedTypeVarianceOp;

    let optional_flag = match mapped.optional {
        MappedTypeOptionalFlag::PlusOptional => string("PlusOptional"),
        MappedTypeOptionalFlag::MinusOptional => string("MinusOptional"),
        MappedTypeOptionalFlag::Optional => string("Optional"),
        MappedTypeOptionalFlag::NoOptionalFlag => Value::Null,
    };
    let variance_op = match mapped.variance_op {
        Some(MappedTypeVarianceOp::Add) => string("+"),
        Some(MappedTypeVarianceOp::Remove) => string("-"),
        None => Value::Null,
    };

    node(
        offset_table,
        config,
        "ObjectTypeMappedTypeProperty",
        &mapped.loc,
        mapped.comments.as_ref(),
        vec![
            (
                "keyTparam",
                type_param(offset_table, config, &mapped.key_tparam),
            ),
            ("propType", type_(offset_table, config, &mapped.prop_type)),
            (
                "sourceType",
                type_(offset_table, config, &mapped.source_type),
            ),
            (
                "nameType",
                option(&mapped.name_type, |t| type_(offset_table, config, t)),
            ),
            (
                "variance",
                option(&mapped.variance, |v| variance(offset_table, config, v)),
            ),
            ("varianceOp", variance_op),
            ("optional", optional_flag),
        ],
    )
}

fn object_type_private_field(
    offset_table: &OffsetTable,
    config: &Config,
    pf: &ast::types::object::PrivateField<Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "ObjectTypePrivateField",
        &pf.loc,
        pf.comments.as_ref(),
        vec![("key", private_identifier(offset_table, config, &pf.key))],
    )
}

fn object_type_internal_slot(
    offset_table: &OffsetTable,
    config: &Config,
    slot: &ast::types::object::InternalSlot<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "ObjectTypeInternalSlot",
        &slot.loc,
        slot.comments.as_ref(),
        vec![
            ("id", identifier(offset_table, config, &slot.id)),
            ("optional", bool_value(slot.optional)),
            ("static", bool_value(slot.static_)),
            ("method", bool_value(slot.method)),
            ("value", type_(offset_table, config, &slot.value)),
        ],
    )
}

fn interface_type(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    interface: &ast::types::Interface<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "InterfaceTypeAnnotation",
        loc,
        interface.comments.as_ref(),
        vec![
            (
                "extends",
                array_of_list(&interface.extends, |ext| {
                    interface_extends(offset_table, config, &ext.0, &ext.1)
                }),
            ),
            (
                "body",
                object_type(
                    offset_table,
                    config,
                    &interface.body.0,
                    &interface.body.1,
                    false,
                ),
            ),
        ],
    )
}

fn conditional_type(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    conditional: &ast::types::Conditional<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "ConditionalTypeAnnotation",
        loc,
        conditional.comments.as_ref(),
        vec![
            (
                "checkType",
                type_(offset_table, config, &conditional.check_type),
            ),
            (
                "extendsType",
                type_(offset_table, config, &conditional.extends_type),
            ),
            (
                "trueType",
                type_(offset_table, config, &conditional.true_type),
            ),
            (
                "falseType",
                type_(offset_table, config, &conditional.false_type),
            ),
        ],
    )
}

fn infer_type(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    infer: &ast::types::Infer<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "InferTypeAnnotation",
        loc,
        infer.comments.as_ref(),
        vec![(
            "typeParameter",
            type_param(offset_table, config, &infer.tparam),
        )],
    )
}

fn generic_type_qualified_identifier(
    offset_table: &OffsetTable,
    config: &Config,
    q: &ast::types::generic::Qualified<Loc, Loc>,
) -> Value {
    use ast::types::generic;

    let qualification = match &q.qualification {
        generic::Identifier::Unqualified(id) => identifier(offset_table, config, id),
        generic::Identifier::Qualified(nested_q) => {
            generic_type_qualified_identifier(offset_table, config, nested_q)
        }
        generic::Identifier::ImportTypeAnnot(import_type) => {
            // import("module") type syntax in qualified position
            import_type_annotation(offset_table, config, &import_type.loc, import_type)
        }
    };

    node(
        offset_table,
        config,
        "QualifiedTypeIdentifier",
        &q.loc,
        None,
        vec![
            ("qualification", qualification),
            ("id", identifier(offset_table, config, &q.id)),
        ],
    )
}

fn generic_type(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    generic: &ast::types::Generic<Loc, Loc>,
) -> Value {
    use ast::types::generic;

    // Mirror upstream Hermes' mapGenericTypeAnnotation: collapse the
    // no-targs `this` identifier case to a ThisTypeAnnotation leaf node.
    // The parser produces a `Generic` with an `Unqualified` `this` id and
    // `targs: None` for both `type T = this` and `(this) => void` /
    // `m(): this`.
    if let (None, generic::Identifier::Unqualified(id)) = (&generic.targs, &generic.id)
        && id.name == "this"
    {
        return node(
            offset_table,
            config,
            "ThisTypeAnnotation",
            loc,
            generic.comments.as_ref(),
            vec![],
        );
    }

    let id = match &generic.id {
        generic::Identifier::Unqualified(id) => identifier(offset_table, config, id),
        generic::Identifier::Qualified(q) => {
            generic_type_qualified_identifier(offset_table, config, q)
        }
        generic::Identifier::ImportTypeAnnot(import_type) => {
            // import("module") type syntax
            import_type_annotation(offset_table, config, &import_type.loc, import_type)
        }
    };

    node(
        offset_table,
        config,
        "GenericTypeAnnotation",
        loc,
        generic.comments.as_ref(),
        vec![
            ("id", id),
            (
                "typeArguments",
                option(&generic.targs, |t| type_args(offset_table, config, t)),
            ),
        ],
    )
}

fn import_type_annotation(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    import_type: &ast::types::generic::ImportType<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "ImportType",
        loc,
        import_type.comments.as_ref(),
        vec![(
            "source",
            string_literal(
                offset_table,
                config,
                &import_type.source.0,
                &import_type.source.1,
            ),
        )],
    )
}

fn typeof_type(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    typeof_: &ast::types::Typeof<Loc, Loc>,
) -> Value {
    let mut fields = vec![(
        "argument",
        typeof_expr(offset_table, config, &typeof_.argument),
    )];
    if let Some(ref targs) = typeof_.targs {
        fields.push(("typeArguments", type_args(offset_table, config, targs)));
    }
    node(
        offset_table,
        config,
        "TypeofTypeAnnotation",
        loc,
        typeof_.comments.as_ref(),
        fields,
    )
}

fn typeof_expr(
    offset_table: &OffsetTable,
    config: &Config,
    target: &ast::types::typeof_::Target<Loc, Loc>,
) -> Value {
    use ast::types::typeof_::Target;
    match target {
        Target::Unqualified(id) => identifier(offset_table, config, id),
        Target::Qualified(q) => typeof_qualifier(offset_table, config, q),
        Target::Import(it) => import_type_annotation(offset_table, config, &it.loc, it),
    }
}

fn typeof_qualifier(
    offset_table: &OffsetTable,
    config: &Config,
    q: &ast::types::typeof_::Qualified<Loc, Loc>,
) -> Value {
    let qualification = typeof_expr(offset_table, config, &q.qualification);
    node(
        offset_table,
        config,
        "QualifiedTypeofIdentifier",
        &q.loc,
        None,
        vec![
            ("qualification", qualification),
            ("id", identifier(offset_table, config, &q.id)),
        ],
    )
}

fn renders_annotation(
    offset_table: &OffsetTable,
    config: &Config,
    renders: &ast::types::ComponentRendersAnnotation<Loc, Loc>,
) -> Value {
    match renders {
        ast::types::ComponentRendersAnnotation::AvailableRenders(loc, r) => {
            render_type(offset_table, config, loc, r)
        }
        ast::types::ComponentRendersAnnotation::MissingRenders(_) => Value::Null,
    }
}

fn render_type(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    renders: &ast::types::Renders<Loc, Loc>,
) -> Value {
    use ast::types::RendersVariant;
    let operator = match renders.variant {
        RendersVariant::Normal => "renders",
        RendersVariant::Maybe => "renders?",
        RendersVariant::Star => "renders*",
    };
    flow_type_operator(
        offset_table,
        config,
        loc,
        renders.comments.as_ref(),
        operator,
        &renders.argument,
    )
}

fn flow_type_operator(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    comments: Option<&ast::Syntax<Loc, ()>>,
    operator: &str,
    operand: &ast::types::Type<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "TypeOperator",
        loc,
        comments,
        vec![
            ("operator", string(operator)),
            ("typeAnnotation", type_(offset_table, config, operand)),
        ],
    )
}

fn tuple_type(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    tuple: &ast::types::Tuple<Loc, Loc>,
) -> Value {
    use ast::types::tuple::Element;
    let element_types = tuple
        .elements
        .iter()
        .map(|el| match el {
            Element::UnlabeledElement {
                loc,
                annot: ty,
                optional,
            } => {
                if *optional {
                    node(
                        offset_table,
                        config,
                        "TupleTypeElement",
                        loc,
                        None,
                        vec![
                            ("elementType", type_(offset_table, config, ty)),
                            ("optional", bool_value(true)),
                        ],
                    )
                } else {
                    type_(offset_table, config, ty)
                }
            }
            Element::LabeledElement { loc, element } => {
                tuple_labeled_element(offset_table, config, loc, element)
            }
            Element::SpreadElement { loc, element } => {
                tuple_spread_element(offset_table, config, loc, element)
            }
        })
        .collect();
    node(
        offset_table,
        config,
        "TupleTypeAnnotation",
        loc,
        tuple.comments.as_ref(),
        vec![
            ("elementTypes", Value::Array(element_types)),
            ("inexact", bool_value(tuple.inexact)),
        ],
    )
}

fn tuple_labeled_element(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    element: &ast::types::tuple::LabeledElement<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "TupleTypeLabeledElement",
        loc,
        None,
        vec![
            ("label", identifier(offset_table, config, &element.name)),
            ("elementType", type_(offset_table, config, &element.annot)),
            (
                "variance",
                option(&element.variance, |v| variance(offset_table, config, v)),
            ),
            ("optional", bool_value(element.optional)),
        ],
    )
}

fn tuple_spread_element(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    element: &ast::types::tuple::SpreadElement<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "TupleTypeSpreadElement",
        loc,
        None,
        vec![
            (
                "label",
                option(&element.name, |n| identifier(offset_table, config, n)),
            ),
            (
                "typeAnnotation",
                type_(offset_table, config, &element.annot),
            ),
        ],
    )
}

fn type_annotation(
    offset_table: &OffsetTable,
    config: &Config,
    annot: &ast::types::Annotation<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "TypeAnnotation",
        &annot.loc,
        None,
        vec![(
            "typeAnnotation",
            type_(offset_table, config, &annot.annotation),
        )],
    )
}

fn type_guard_annotation(
    offset_table: &OffsetTable,
    config: &Config,
    annot: &ast::types::TypeGuardAnnotation<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "TypeAnnotation",
        &annot.loc,
        None,
        vec![(
            "typeAnnotation",
            function_type_guard(offset_table, config, &annot.guard),
        )],
    )
}

fn type_parameter_declaration(
    offset_table: &OffsetTable,
    config: &Config,
    type_params: &ast::types::TypeParams<Loc, Loc>,
) -> Value {
    let formatted_comments = format_internal_comments(type_params.comments.as_ref());
    node(
        offset_table,
        config,
        "TypeParameterDeclaration",
        &type_params.loc,
        formatted_comments.as_ref(),
        vec![(
            "params",
            array_of_list(&type_params.params, |p| type_param(offset_table, config, p)),
        )],
    )
}

fn type_param(
    offset_table: &OffsetTable,
    config: &Config,
    param: &ast::types::TypeParam<Loc, Loc>,
) -> Value {
    use ast::types::AnnotationOrHint;
    use ast::types::type_param::BoundKind;

    // Hermes' deserializeTypeParameter reads `bound` as a plain type node,
    // NOT a TypeAnnotation-wrapped node. Emit the inner annotation directly
    // rather than going through `type_annotation` (which writes a
    // TypeAnnotation header). When the bound is missing, write null.
    let bound = match &param.bound {
        AnnotationOrHint::Available(annot) => type_(offset_table, config, &annot.annotation),
        AnnotationOrHint::Missing(_) => Value::Null,
    };

    let mut props = vec![
        // we track the location of the name, but don't expose it here for
        // backwards-compatibility. TODO: change this?
        ("name", string(&param.name.name)),
        ("bound", bound),
        ("const", bool_value(param.const_.is_some())),
        (
            "variance",
            option(&param.variance, |v| variance(offset_table, config, v)),
        ),
        (
            "default",
            option(&param.default, |t| type_(offset_table, config, t)),
        ),
    ];

    match param.bound_kind {
        BoundKind::Colon => {}
        BoundKind::Extends => {
            props.push(("usesExtendsBound", bool_value(true)));
        }
    }

    node(
        offset_table,
        config,
        "TypeParameter",
        &param.loc,
        param.name.comments.as_ref(),
        props,
    )
}

fn type_args(
    offset_table: &OffsetTable,
    config: &Config,
    targs: &ast::types::TypeArgs<Loc, Loc>,
) -> Value {
    let formatted_comments = format_internal_comments(targs.comments.as_ref());
    node(
        offset_table,
        config,
        "TypeParameterInstantiation",
        &targs.loc,
        formatted_comments.as_ref(),
        vec![(
            "params",
            array_of_list(&targs.arguments, |t| type_(offset_table, config, t)),
        )],
    )
}

fn call_type_args(
    offset_table: &OffsetTable,
    config: &Config,
    targs: &ast::expression::CallTypeArgs<Loc, Loc>,
) -> Value {
    let formatted_comments = format_internal_comments(targs.comments.as_ref());
    node(
        offset_table,
        config,
        "TypeParameterInstantiation",
        &targs.loc,
        formatted_comments.as_ref(),
        vec![(
            "params",
            array_of_list(&targs.arguments, |arg| {
                call_type_arg(offset_table, config, arg)
            }),
        )],
    )
}

fn call_type_arg(
    offset_table: &OffsetTable,
    config: &Config,
    arg: &ast::expression::CallTypeArg<Loc, Loc>,
) -> Value {
    use ast::expression::CallTypeArg;

    match arg {
        CallTypeArg::Explicit(ty) => type_(offset_table, config, ty),
        CallTypeArg::Implicit(implicit) => {
            // Create a Generic with identifier "_"
            let underscore_id = ast::Identifier::new(ast::IdentifierInner {
                loc: implicit.loc.dupe(),
                name: FlowSmolStr::new_inline("_"),
                comments: None,
            });
            let generic = ast::types::Generic {
                id: ast::types::generic::Identifier::Unqualified(underscore_id),
                targs: None,
                comments: implicit.comments.dupe(),
            };
            generic_type(offset_table, config, &implicit.loc, &generic)
        }
    }
}

fn jsx_element(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    element: &ast::jsx::Element<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "JSXElement",
        loc,
        element.comments.as_ref(),
        vec![
            (
                "openingElement",
                jsx_opening(offset_table, config, &element.opening_element),
            ),
            (
                "closingElement",
                option(&element.closing_element, |c| {
                    jsx_closing(offset_table, config, c)
                }),
            ),
            (
                "children",
                array_of_list(&element.children.1, |child| {
                    jsx_child(offset_table, config, child)
                }),
            ),
        ],
    )
}

fn jsx_fragment(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    fragment: &ast::jsx::Fragment<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "JSXFragment",
        loc,
        fragment.frag_comments.as_ref(),
        vec![
            (
                "openingFragment",
                jsx_opening_fragment(offset_table, config, &fragment.frag_opening_element),
            ),
            (
                "children",
                array_of_list(&fragment.frag_children.1, |child| {
                    jsx_child(offset_table, config, child)
                }),
            ),
            (
                "closingFragment",
                jsx_closing_fragment(offset_table, config, &fragment.frag_closing_element),
            ),
        ],
    )
}

fn jsx_opening(
    offset_table: &OffsetTable,
    config: &Config,
    opening: &ast::jsx::Opening<Loc, Loc>,
) -> Value {
    let mut fields = vec![
        ("name", jsx_name(offset_table, config, &opening.name)),
        (
            "attributes",
            array_of_list(&opening.attributes, |attr| {
                jsx_opening_attribute(offset_table, config, attr)
            }),
        ),
        ("selfClosing", bool_value(opening.self_closing)),
    ];

    if let Some(ref targs) = opening.targs {
        fields.push(("typeArguments", call_type_args(offset_table, config, targs)));
    }

    node(
        offset_table,
        config,
        "JSXOpeningElement",
        &opening.loc,
        None,
        fields,
    )
}

fn jsx_opening_fragment(offset_table: &OffsetTable, config: &Config, loc: &Loc) -> Value {
    node(
        offset_table,
        config,
        "JSXOpeningFragment",
        loc,
        None,
        vec![],
    )
}

fn jsx_opening_attribute(
    offset_table: &OffsetTable,
    config: &Config,
    attr: &ast::jsx::OpeningAttribute<Loc, Loc>,
) -> Value {
    use ast::jsx::OpeningAttribute;

    match attr {
        OpeningAttribute::Attribute(attribute) => jsx_attribute(offset_table, config, attribute),
        OpeningAttribute::SpreadAttribute(spread) => {
            jsx_spread_attribute(offset_table, config, spread)
        }
    }
}

fn jsx_closing(
    offset_table: &OffsetTable,
    config: &Config,
    closing: &ast::jsx::Closing<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "JSXClosingElement",
        &closing.loc,
        None,
        vec![("name", jsx_name(offset_table, config, &closing.name))],
    )
}

fn jsx_closing_fragment(offset_table: &OffsetTable, config: &Config, loc: &Loc) -> Value {
    node(
        offset_table,
        config,
        "JSXClosingFragment",
        loc,
        None,
        vec![],
    )
}

fn jsx_child(
    offset_table: &OffsetTable,
    config: &Config,
    child: &ast::jsx::Child<Loc, Loc>,
) -> Value {
    use ast::jsx::Child;

    match child {
        Child::Element { loc, inner } => jsx_element(offset_table, config, loc, inner),
        Child::Fragment { loc, inner } => jsx_fragment(offset_table, config, loc, inner),
        Child::ExpressionContainer { loc, inner } => {
            jsx_expression_container(offset_table, config, loc, inner)
        }
        Child::SpreadChild { loc, inner } => jsx_spread_child(offset_table, config, loc, inner),
        Child::Text { loc, inner } => jsx_text(offset_table, config, loc, inner),
    }
}

fn jsx_name(offset_table: &OffsetTable, config: &Config, name: &ast::jsx::Name<Loc, Loc>) -> Value {
    use ast::jsx::Name;

    match name {
        Name::Identifier(id) => jsx_identifier(offset_table, config, id),
        Name::NamespacedName(ns) => jsx_namespaced_name(offset_table, config, ns),
        Name::MemberExpression(member) => jsx_member_expression(offset_table, config, member),
    }
}

fn jsx_attribute(
    offset_table: &OffsetTable,
    config: &Config,
    attribute: &ast::jsx::Attribute<Loc, Loc>,
) -> Value {
    use ast::jsx::attribute::Name;

    let name = match &attribute.name {
        Name::Identifier(id) => jsx_identifier(offset_table, config, id),
        Name::NamespacedName(ns) => jsx_namespaced_name(offset_table, config, ns),
    };

    node(
        offset_table,
        config,
        "JSXAttribute",
        &attribute.loc,
        None,
        vec![
            ("name", name),
            (
                "value",
                option(&attribute.value, |v| {
                    jsx_attribute_value(offset_table, config, v)
                }),
            ),
        ],
    )
}

fn jsx_attribute_value(
    offset_table: &OffsetTable,
    config: &Config,
    value: &ast::jsx::attribute::Value<Loc, Loc>,
) -> Value {
    use ast::jsx::attribute::Value;

    match value {
        Value::StringLiteral((loc, lit)) => string_literal(offset_table, config, loc, lit),
        Value::ExpressionContainer((loc, expr)) => {
            jsx_expression_container(offset_table, config, loc, expr)
        }
    }
}

fn jsx_spread_attribute(
    offset_table: &OffsetTable,
    config: &Config,
    spread: &ast::jsx::SpreadAttribute<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "JSXSpreadAttribute",
        &spread.loc,
        spread.comments.as_ref(),
        vec![(
            "argument",
            expression(offset_table, config, false, &spread.argument),
        )],
    )
}

fn jsx_expression_container(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    container: &ast::jsx::ExpressionContainer<Loc, Loc>,
) -> Value {
    use ast::jsx::expression_container::Expression;

    let expression = match &container.expression {
        Expression::Expression(expr) => expression(offset_table, config, false, expr),
        Expression::EmptyExpression => {
            let empty_loc = Loc {
                source: loc.source.clone(),
                start: Position {
                    line: loc.start.line,
                    column: loc.start.column + 1,
                },
                end: Position {
                    line: loc.end.line,
                    column: loc.end.column - 1,
                },
            };
            node(
                offset_table,
                config,
                "JSXEmptyExpression",
                &empty_loc,
                None,
                vec![],
            )
        }
    };

    node(
        offset_table,
        config,
        "JSXExpressionContainer",
        loc,
        format_internal_comments(container.comments.as_ref()).as_ref(),
        vec![("expression", expression)],
    )
}

fn jsx_spread_child(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    spread: &ast::jsx::SpreadChild<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "JSXSpreadChild",
        loc,
        spread.comments.as_ref(),
        vec![(
            "expression",
            expression(offset_table, config, false, &spread.expression),
        )],
    )
}

fn jsx_text(
    offset_table: &OffsetTable,
    config: &Config,
    loc: &Loc,
    text: &ast::jsx::Text,
) -> Value {
    node(
        offset_table,
        config,
        "JSXText",
        loc,
        None,
        vec![("value", string(&text.value)), ("raw", string(&text.raw))],
    )
}

fn jsx_member_expression(
    offset_table: &OffsetTable,
    config: &Config,
    member_expr: &ast::jsx::MemberExpression<Loc, Loc>,
) -> Value {
    use ast::jsx::member_expression::Object;

    let object = match &member_expr.object {
        Object::Identifier(id) => jsx_identifier(offset_table, config, id),
        Object::MemberExpression(member) => jsx_member_expression(offset_table, config, member),
    };

    node(
        offset_table,
        config,
        "JSXMemberExpression",
        &member_expr.loc,
        None,
        vec![
            ("object", object),
            (
                "property",
                jsx_identifier(offset_table, config, &member_expr.property),
            ),
        ],
    )
}

fn jsx_namespaced_name(
    offset_table: &OffsetTable,
    config: &Config,
    namespaced_name: &ast::jsx::NamespacedName<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "JSXNamespacedName",
        &namespaced_name.loc,
        None,
        vec![
            (
                "namespace",
                jsx_identifier(offset_table, config, &namespaced_name.namespace),
            ),
            (
                "name",
                jsx_identifier(offset_table, config, &namespaced_name.name),
            ),
        ],
    )
}

fn jsx_identifier(
    offset_table: &OffsetTable,
    config: &Config,
    identifier: &ast::jsx::Identifier<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "JSXIdentifier",
        &identifier.loc,
        identifier.comments.as_ref(),
        vec![("name", string(&identifier.name))],
    )
}

fn export_specifier(
    offset_table: &OffsetTable,
    config: &Config,
    spec: &ast::statement::export_named_declaration::ExportSpecifier<Loc, Loc>,
) -> Value {
    let exported = match &spec.exported {
        Some(exported) => identifier(offset_table, config, exported),
        None => identifier(offset_table, config, &spec.local),
    };
    let export_kind_str = match spec.export_kind {
        ast::statement::ExportKind::ExportType => "type",
        ast::statement::ExportKind::ExportValue => "value",
    };
    node(
        offset_table,
        config,
        "ExportSpecifier",
        &spec.loc,
        None,
        vec![
            ("exported", exported),
            ("local", identifier(offset_table, config, &spec.local)),
            ("exportKind", string(export_kind_str)),
        ],
    )
}

fn import_default_specifier(
    offset_table: &OffsetTable,
    config: &Config,
    default: &ast::statement::import_declaration::DefaultIdentifier<Loc, Loc>,
) -> Value {
    node(
        offset_table,
        config,
        "ImportDefaultSpecifier",
        &default.identifier.loc,
        None,
        vec![(
            "local",
            identifier(offset_table, config, &default.identifier),
        )],
    )
}

fn import_namespace_specifier(
    offset_table: &OffsetTable,
    config: &Config,
    ns: &(Loc, ast::Identifier<Loc, Loc>),
) -> Value {
    node(
        offset_table,
        config,
        "ImportNamespaceSpecifier",
        &ns.0,
        None,
        vec![("local", identifier(offset_table, config, &ns.1))],
    )
}

fn import_named_specifier(
    offset_table: &OffsetTable,
    config: &Config,
    specifier: &ast::statement::import_declaration::NamedSpecifier<Loc, Loc>,
) -> Value {
    use ast::statement::ImportKind;

    let start_loc = match &specifier.kind_loc {
        Some(kl) => kl,
        None => &specifier.remote.loc,
    };
    let span_loc = match &specifier.local {
        Some(local_id) => Loc::between(start_loc, &local_id.loc),
        None => Loc::between(start_loc, &specifier.remote.loc),
    };

    let local_id = match &specifier.local {
        Some(id) => id,
        None => &specifier.remote,
    };

    let import_kind = match &specifier.kind {
        Some(ImportKind::ImportType) => string("type"),
        Some(ImportKind::ImportTypeof) => string("typeof"),
        Some(ImportKind::ImportValue) | None => Value::Null,
    };

    node(
        offset_table,
        config,
        "ImportSpecifier",
        &span_loc,
        None,
        vec![
            (
                "imported",
                identifier(offset_table, config, &specifier.remote),
            ),
            ("local", identifier(offset_table, config, local_id)),
            ("importKind", import_kind),
        ],
    )
}

fn import_attribute(
    offset_table: &OffsetTable,
    config: &Config,
    attr: &ast::statement::import_declaration::ImportAttribute<Loc, Loc>,
) -> Value {
    use ast::statement::import_declaration::ImportAttributeKey;

    let key_json = match &attr.key {
        ImportAttributeKey::Identifier(id) => identifier(offset_table, config, id),
        ImportAttributeKey::StringLiteral(loc, lit) => {
            string_literal(offset_table, config, loc, lit)
        }
    };
    let value_json = string_literal(offset_table, config, &attr.value.0, &attr.value.1);

    node(
        offset_table,
        config,
        "ImportAttribute",
        &attr.loc,
        None,
        vec![("key", key_json), ("value", value_json)],
    )
}

fn comment_list(
    offset_table: &OffsetTable,
    config: &Config,
    comments: &[ast::Comment<Loc>],
) -> Value {
    Value::Array(
        comments
            .iter()
            .map(|c| comment(offset_table, config, c))
            .collect(),
    )
}

fn comment(offset_table: &OffsetTable, config: &Config, comment: &ast::Comment<Loc>) -> Value {
    use ast::CommentKind;
    let comment_type = match comment.kind {
        CommentKind::Block => "Block",
        CommentKind::Line => "Line",
    };
    node(
        offset_table,
        config,
        comment_type,
        &comment.loc,
        None,
        vec![("value", Value::String(comment.text.deref().to_owned()))],
    )
}

fn predicate(
    offset_table: &OffsetTable,
    config: &Config,
    pred: &ast::types::Predicate<Loc, Loc>,
) -> Value {
    use ast::types::PredicateKind;

    let (node_type, props) = match &pred.kind {
        PredicateKind::Declared(expr) => (
            "DeclaredPredicate",
            vec![("value", expression(offset_table, config, false, expr))],
        ),
        PredicateKind::Inferred => ("InferredPredicate", vec![]),
    };

    node(
        offset_table,
        config,
        node_type,
        &pred.loc,
        pred.comments.as_ref(),
        props,
    )
}

fn call_node_properties(
    offset_table: &OffsetTable,
    config: &Config,
    in_optional_chain: bool,
    assertion_loc: Option<&Loc>,
    loc: &Loc,
    call: &ast::expression::Call<Loc, Loc>,
) -> Vec<(&'static str, Value)> {
    let callee = expression(
        offset_table,
        config,
        in_optional_chain && loc.start == call.callee.loc().start,
        &call.callee,
    );
    let callee = match assertion_loc {
        None => callee,
        Some(assertion_loc) => node(
            offset_table,
            config,
            "NonNullExpression",
            assertion_loc,
            None,
            vec![("expression", callee)],
        ),
    };

    vec![
        ("callee", callee),
        (
            "typeArguments",
            option(&call.targs, |t| call_type_args(offset_table, config, t)),
        ),
        (
            "arguments",
            arg_list(offset_table, config, false, &call.arguments),
        ),
    ]
}

fn member_node_properties(
    offset_table: &OffsetTable,
    config: &Config,
    in_optional_chain: bool,
    assertion_loc: Option<&Loc>,
    loc: &Loc,
    member: &ast::expression::Member<Loc, Loc>,
) -> Vec<(&'static str, Value)> {
    let (property, computed) = match &member.property {
        ast::expression::member::Property::PropertyIdentifier(id) => {
            (identifier(offset_table, config, id), false)
        }
        ast::expression::member::Property::PropertyPrivateName(name) => {
            (private_identifier(offset_table, config, name), false)
        }
        ast::expression::member::Property::PropertyExpression(expr) => {
            (expression(offset_table, config, false, expr), true)
        }
    };

    let object = expression(
        offset_table,
        config,
        in_optional_chain && loc.start == member.object.loc().start,
        &member.object,
    );
    let object = match assertion_loc {
        None => object,
        Some(assertion_loc) => node(
            offset_table,
            config,
            "NonNullExpression",
            assertion_loc,
            None,
            vec![("expression", object)],
        ),
    };

    vec![
        ("object", object),
        ("property", property),
        ("computed", bool_value(computed)),
    ]
}
