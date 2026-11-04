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

use crate::ast;
use crate::ast::expression::ExpressionInner;
use crate::ast_utils;
use crate::comment_utils;
use crate::estree_translator;
use crate::expression_parser;
use crate::file_key::FileKey;
use crate::loc::Loc;
use crate::object_parser;
use crate::offset_utils;
use crate::parse_error::ParseError;
use crate::parser_common::assert_identifier_name_is_identifier;
use crate::parser_common::identifier_name;
use crate::parser_common::with_loc;
use crate::parser_common::with_loc_extra;
use crate::parser_env::ParseOptions;
use crate::parser_env::ParserEnv;
use crate::parser_env::TokenSinkResult;
use crate::parser_env::eat;
use crate::parser_env::expect;
use crate::parser_env::init_env;
use crate::parser_env::peek;
use crate::parser_env::try_parse::Rollback;
use crate::pattern_cover;
use crate::pattern_cover::PatternCover;
use crate::pattern_cover::PatternCoverErrors;
use crate::statement_parser;
use crate::token::TokenKind;
use crate::type_parser;

pub(super) fn parse_identifier(
    env: &mut ParserEnv,
    restricted_error: Option<ParseError>,
) -> Result<ast::Identifier<Loc, Loc>, Rollback> {
    let id = identifier_name(env)?;
    assert_identifier_name_is_identifier(restricted_error, env, &id)?;
    Ok(id)
}

pub fn parse_expression(
    env: &mut ParserEnv,
) -> Result<ast::expression::Expression<Loc, Loc>, Rollback> {
    let start_loc = peek::loc(env).dupe();
    let expr = expression_parser::assignment(env)?;
    match peek::token(env) {
        TokenKind::TComma => expression_parser::sequence(env, start_loc, vec![expr]),
        _ => Ok(expr),
    }
}

/// Returns the string and location of the first identifier in [input] for which
/// [predicate] holds.
pub fn find_ident(predicate: impl Fn(&str) -> bool, input: &str) -> Option<(Loc, FlowSmolStr)> {
    let mut env = init_env::<()>(None, None, None, Ok(input));
    fn loop_token(
        env: &mut ParserEnv,
        predicate: &impl Fn(&str) -> bool,
        token: &TokenKind,
    ) -> Option<(Loc, FlowSmolStr)> {
        match token {
            TokenKind::TEof => None,
            _ => {
                let loc = peek::loc(env).dupe();
                match token {
                    TokenKind::TIdentifier { value, .. } if predicate(value.as_str()) => {
                        Some((loc, value.dupe()))
                    }
                    _ => {
                        eat::token(env).ok()?;
                        let next_token = peek::token(env).clone();
                        loop_token(env, predicate, &next_token)
                    }
                }
            }
        }
    }
    let token = peek::token(&mut env).clone();
    loop_token(&mut env, &predicate, &token)
}

pub(super) fn parse_expression_or_pattern(env: &mut ParserEnv) -> Result<PatternCover, Rollback> {
    let start_loc = peek::loc(env).dupe();
    let expr_or_pattern = expression_parser::assignment_cover(env)?;
    match peek::token(env) {
        TokenKind::TComma => {
            let expr = pattern_cover::as_expression(env, expr_or_pattern)?;
            let seq = expression_parser::sequence(env, start_loc, vec![expr])?;
            Ok(PatternCover::CoverExpr(seq))
        }
        _ => Ok(expr_or_pattern),
    }
}

pub(super) fn parse_identifier_with_type(
    env: &mut ParserEnv,
    allow_optional: bool,
    restricted_error: Option<ParseError>,
) -> Result<(Loc, ast::pattern::Identifier<Loc, Loc>), Rollback> {
    with_loc(None, env, |env| {
        let name = parse_identifier(env, restricted_error)?;
        let optional = match peek::token(env) {
            TokenKind::TPling => {
                let optional = if allow_optional {
                    if !env.should_parse_types() {
                        env.error(ParseError::UnexpectedTypeAnnotation)?;
                    }
                    true
                } else {
                    env.error(ParseError::UnexpectedOptional)?;
                    false
                };
                eat::token(env)?;
                optional
            }
            _ => false,
        };
        let annot = type_parser::parse_annotation_opt(env)?;
        Ok(ast::pattern::Identifier {
            name,
            annot,
            optional,
        })
    })
}

pub(super) fn parse_block_body(
    env: &mut ParserEnv,
) -> Result<(Loc, ast::statement::Block<Loc, Loc>), Rollback> {
    let start_loc = peek::loc(env).dupe();
    let leading = peek::comments(env);
    expect::token(env, TokenKind::TLcurly)?;
    let body =
        statement_parser::parse_statement_list(env, |t: &TokenKind| t == &TokenKind::TRcurly)?;
    let end_loc = peek::loc(env).dupe();
    let internal = if body.is_empty() {
        peek::comments(env)
    } else {
        Vec::new()
    };
    expect::token(env, TokenKind::TRcurly)?;
    let trailing = eat::trailing_comments(env);
    let loc = Loc::between(&start_loc, &end_loc);
    Ok((
        loc,
        ast::statement::Block {
            body: body.into(),
            comments: ast_utils::mk_comments_with_internal_opt(
                Some(leading.into()),
                Some(trailing.into()),
                if internal.is_empty() {
                    None
                } else {
                    Some(internal.into())
                },
            ),
        },
    ))
}

pub(super) fn parse_function_block_body(
    env: &mut ParserEnv,
    expression: bool,
) -> Result<((Loc, ast::statement::Block<Loc, Loc>), bool), Rollback> {
    with_loc_extra(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TLcurly)?;
        let (body, contains_use_strict) =
            statement_parser::parse_statement_list_with_directives(env)?;
        let internal = if body.is_empty() {
            peek::comments(env)
        } else {
            Vec::new()
        };
        expect::token(env, TokenKind::TRcurly)?;
        let trailing = match (expression, peek::token(env)) {
            (true, _) | (_, TokenKind::TRcurly) | (_, TokenKind::TEof) => {
                eat::trailing_comments(env)
            }
            _ => {
                if peek::is_line_terminator(env) {
                    eat::comments_until_next_line(env)
                } else {
                    Vec::new()
                }
            }
        };
        let comments = ast_utils::mk_comments_with_internal_opt(
            Some(leading.into()),
            Some(trailing.into()),
            if internal.is_empty() {
                None
            } else {
                Some(internal.into())
            },
        );
        Ok((
            ast::statement::Block {
                body: body.into(),
                comments,
            },
            contains_use_strict,
        ))
    })
}

// Functions above serve as internal APIs for the recursive descent parser.
// Functions below power the main parser API.

// Sometimes we add the same error for multiple different reasons. This is hard
// to avoid, so instead we just filter the duplicates out. This function takes
// a list of errors and returns the list in forward order with dupes
// removed. This differs from a set because the original order is preserved.
fn filter_duplicate_errors(errors: Vec<(Loc, ParseError)>) -> Vec<(Loc, ParseError)> {
    if errors.len() <= 1 {
        return errors;
    }
    let mut seen = HashSet::new();
    let to_retain: Vec<bool> = errors.iter().map(|e| seen.insert(e)).collect();
    let mut new_errors = Vec::new();
    for (error, retain) in errors.into_iter().zip(to_retain) {
        if retain {
            new_errors.push(error)
        }
    }
    new_errors
}

fn check_for_duplicate_exports(env: &mut ParserEnv, stmts: &[ast::statement::Statement<Loc, Loc>]) {
    use ast::statement::ExportKind;
    use ast::statement::StatementInner;

    fn record_export(
        env: &mut ParserEnv,
        seen: &mut (HashSet<String>, HashSet<String>),
        kind: ExportKind,
        loc: &Loc,
        export_name: &str,
    ) {
        if export_name.is_empty() {
            // empty identifiers signify an error, don't export it
            return;
        }
        let relevant_seen = match kind {
            ExportKind::ExportType => &seen.0,
            ExportKind::ExportValue => &seen.1,
        };
        if relevant_seen.contains(export_name) {
            env.error_at(
                loc.dupe(),
                ParseError::DuplicateExport(export_name.to_owned()),
            )
            .ok();
        } else {
            match kind {
                ExportKind::ExportType => seen.0.insert(export_name.to_owned()),
                ExportKind::ExportValue => seen.1.insert(export_name.to_owned()),
            };
        }
    }

    fn extract_pattern_binding_names(
        pattern: &ast::pattern::Pattern<Loc, Loc>,
    ) -> Vec<ast::Identifier<Loc, Loc>> {
        fn fold(
            acc: &mut Vec<ast::Identifier<Loc, Loc>>,
            pattern: &ast::pattern::Pattern<Loc, Loc>,
        ) {
            use ast::pattern::Pattern;
            match pattern {
                Pattern::Object { inner, .. } => {
                    for prop in inner.properties.iter() {
                        use ast::pattern::object::Property;
                        match prop {
                            Property::NormalProperty(prop) => {
                                fold(acc, &prop.pattern);
                            }
                            Property::RestElement(rest) => {
                                fold(acc, &rest.argument);
                            }
                        }
                    }
                }
                Pattern::Array { inner, .. } => {
                    for elem in inner.elements.iter() {
                        use ast::pattern::array::Element;
                        match elem {
                            Element::NormalElement(elem) => {
                                fold(acc, &elem.argument);
                            }
                            Element::RestElement(rest) => {
                                fold(acc, &rest.argument);
                            }
                            Element::Hole(..) => {}
                        }
                    }
                }
                Pattern::Identifier { inner, .. } => {
                    acc.push(inner.name.dupe());
                }
                Pattern::Expression { .. } => {
                    unreachable!("Parser error: No such thing as an expression pattern!")
                }
            }
        }

        let mut result = Vec::new();
        fold(&mut result, pattern);
        result
    }

    let mut seen = (HashSet::new(), HashSet::new());
    let mut seen_interfaces = HashSet::new();

    for stmt in stmts {
        match stmt.deref() {
            StatementInner::ExportDefaultDeclaration { inner, .. } => {
                let export_kind = match &inner.declaration {
                    ast::statement::export_default_declaration::Declaration::Declaration(decl)
                        if matches!(decl.deref(), StatementInner::InterfaceDeclaration { .. }) =>
                    {
                        ExportKind::ExportType
                    }
                    _ => ExportKind::ExportValue,
                };
                record_export(env, &mut seen, export_kind, &inner.default, "default");
            }
            StatementInner::ExportNamedDeclaration { inner, .. } => {
                if inner.declaration.is_none() {
                    if let Some(
                        ast::statement::export_named_declaration::Specifier::ExportSpecifiers(
                            specs,
                        ),
                    ) = &inner.specifiers
                    {
                        let statement_export_kind = inner.export_kind;
                        for spec in specs {
                            let kind = ast_utils::effective_export_kind(
                                statement_export_kind,
                                spec.export_kind,
                            );
                            let exported = spec.exported.as_ref().unwrap_or(&spec.local);
                            record_export(env, &mut seen, kind, &exported.loc, &exported.name);
                        }
                    }
                }

                if let Some(decl) = &inner.declaration {
                    let export_kind = inner.export_kind;
                    match decl.deref() {
                        StatementInner::TypeAlias { loc, inner } => {
                            record_export(env, &mut seen, export_kind, loc, &inner.id.name);
                        }
                        StatementInner::OpaqueType { loc, inner } => {
                            record_export(env, &mut seen, export_kind, loc, &inner.id.name);
                        }
                        StatementInner::InterfaceDeclaration { loc, inner } => {
                            if !env.is_ts() || seen_interfaces.insert(inner.id.name.dupe()) {
                                record_export(env, &mut seen, export_kind, loc, &inner.id.name);
                            }
                        }
                        StatementInner::ClassDeclaration { loc, inner } => {
                            if let Some(ref id) = inner.id {
                                record_export(env, &mut seen, export_kind, loc, &id.name);
                            }
                        }
                        StatementInner::FunctionDeclaration { loc, inner } => {
                            if let Some(ref id) = inner.id {
                                record_export(env, &mut seen, export_kind, loc, &id.name);
                            }
                        }
                        StatementInner::EnumDeclaration { loc, inner } => {
                            record_export(env, &mut seen, export_kind, loc, &inner.id.name);
                        }
                        StatementInner::ComponentDeclaration { loc, inner } => {
                            record_export(env, &mut seen, export_kind, loc, &inner.id.name);
                        }
                        StatementInner::RecordDeclaration { loc, inner } => {
                            record_export(env, &mut seen, export_kind, loc, &inner.id.name);
                        }
                        StatementInner::VariableDeclaration {
                            inner: var_inner, ..
                        } => {
                            for declarator in var_inner.declarations.iter() {
                                let names = extract_pattern_binding_names(&declarator.id);
                                for name in names {
                                    record_export(
                                        env,
                                        &mut seen,
                                        export_kind,
                                        &name.loc,
                                        &name.name,
                                    );
                                }
                            }
                        }
                        StatementInner::Block { .. }
                        | StatementInner::Break { .. }
                        | StatementInner::Continue { .. }
                        | StatementInner::Debugger { .. }
                        | StatementInner::DeclareClass { .. }
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
                        | StatementInner::DoWhile { .. }
                        | StatementInner::Empty { .. }
                        | StatementInner::ExportDefaultDeclaration { .. }
                        | StatementInner::ExportNamedDeclaration { .. }
                        | StatementInner::ExportAssignment { .. }
                        | StatementInner::NamespaceExportDeclaration { .. }
                        | StatementInner::Expression { .. }
                        | StatementInner::For { .. }
                        | StatementInner::ForIn { .. }
                        | StatementInner::ForOf { .. }
                        | StatementInner::If { .. }
                        | StatementInner::ImportDeclaration { .. }
                        | StatementInner::ImportEqualsDeclaration { .. }
                        | StatementInner::Labeled { .. }
                        | StatementInner::Match { .. }
                        | StatementInner::Return { .. }
                        | StatementInner::Switch { .. }
                        | StatementInner::Throw { .. }
                        | StatementInner::Try { .. }
                        | StatementInner::While { .. }
                        | StatementInner::With { .. } => {
                            //  these don't export names -- some are invalid, but the AST allows them
                        }
                    }
                }
            }
            _ => {
                // Not an export statement
            }
        }
    }
}

fn parse_program_inner(env: &mut ParserEnv) -> Result<ast::Program<Loc, Loc>, Rollback> {
    // Set ambient context for .flow and .d.ts files, or if ambient parse option is set
    let should_use_ambient = env.parse_options().ambient
        || match env.source() {
            Some(file_key) => file_key.check_suffix(".flow") || env.is_d_ts(),
            None => false,
        };
    env.with_ambient_context(should_use_ambient, |env| {
        // Handle shebang/interpreter directive
        let interpreter = if let TokenKind::TInterpreter(loc, value) = peek::token(env) {
            let loc = loc.dupe();
            let value = value.clone();
            eat::token(env)?;
            Some((loc, value))
        } else {
            None
        };

        let leading = eat::program_comments(env);
        let stmts = statement_parser::parse_module_body_with_directives(env)?;
        let end_loc = peek::loc(env).dupe();
        expect::token(env, TokenKind::TEof)?;

        check_for_duplicate_exports(env, &stmts);

        let loc = Loc::between(&Loc::cursor(env.source(), 1, 0), &end_loc);

        let all_comments = env.take_comments();

        Ok(ast::Program {
            loc,
            statements: stmts.into(),
            interpreter,
            comments: ast_utils::mk_comments_opt(
                if leading.is_empty() {
                    None
                } else {
                    Some(leading.into())
                },
                None,
            ),
            all_comments: all_comments.into(),
        })
    })
}

pub fn do_parse<T, F: FnOnce(&mut ParserEnv) -> T>(
    mut env: ParserEnv,
    fail: bool,
    parser: F,
) -> Result<(T, Vec<(Loc, ParseError)>), Vec<(Loc, ParseError)>> {
    let ast = parser(&mut env);
    let error_list = filter_duplicate_errors(env.errors());
    if fail && !error_list.is_empty() {
        Err(error_list)
    } else {
        Ok((ast, error_list))
    }
}

/// An Err variant of content signals bad unicode.
fn parse_program<'a>(
    fail: bool,
    token_sink: Option<&'a mut dyn FnMut(TokenSinkResult)>,
    parse_options: Option<ParseOptions>,
    file_name: Option<FileKey>,
    content: Result<&'a str, ()>,
) -> (ast::Program<Loc, Loc>, Vec<(Loc, ParseError)>) {
    let env = init_env(token_sink, parse_options, file_name, content);
    do_parse(env, fail, |env| parse_program_inner(env).ok().unwrap()).unwrap()
}

pub fn parse_program_without_file<'a>(
    panic_if_failed: bool,
    token_sink: Option<&'a mut dyn FnMut(TokenSinkResult)>,
    parse_options: Option<ParseOptions>,
    content: Result<&'a str, ()>,
) -> (ast::Program<Loc, Loc>, Vec<(Loc, ParseError)>) {
    parse_program(panic_if_failed, token_sink, parse_options, None, content)
}

pub fn parse_program_file<'a, E>(
    panic_if_failed: bool,
    token_sink: Option<&'a mut dyn FnMut(TokenSinkResult)>,
    parse_options: Option<ParseOptions>,
    file_name: FileKey,
    content: Result<&'a str, ()>,
) -> (ast::Program<Loc, Loc>, Vec<(Loc, ParseError)>) {
    parse_program(
        panic_if_failed,
        token_sink,
        parse_options,
        Some(file_name),
        content,
    )
}

/// Parse an ES module in strict mode with top-level await enabled.
pub fn parse_module_file(
    parse_options: ParseOptions,
    file_name: FileKey,
    content: &str,
) -> Result<ast::Program<Loc, Loc>, Vec<(Loc, ParseError)>> {
    let env = init_env::<()>(None, Some(parse_options), Some(file_name), Ok(content));
    do_parse(env, true, parse_module_program_inner)
        .and_then(|(program, errors)| program.map_err(|_| errors))
}

/// Parse an ES module, retaining the partial AST, diagnostics, and optional tokens.
pub fn parse_module_file_with_errors<'a>(
    token_sink: Option<&'a mut dyn FnMut(TokenSinkResult)>,
    parse_options: ParseOptions,
    file_name: FileKey,
    content: &'a str,
) -> (ast::Program<Loc, Loc>, Vec<(Loc, ParseError)>) {
    let env = init_env::<()>(
        token_sink,
        Some(parse_options),
        Some(file_name),
        Ok(content),
    );
    do_parse(env, false, parse_module_program_inner)
        .and_then(|(program, errors)| match program {
            Ok(program) => Ok((program, errors)),
            Err(_) => Err(errors),
        })
        .expect("module parsing cannot roll back")
}

fn parse_module_program_inner(env: &mut ParserEnv) -> Result<ast::Program<Loc, Loc>, Rollback> {
    env.with_strict(true, |env| env.with_allow_await(true, parse_program_inner))
}

pub fn parse_annotation<E>(
    parse_options: Option<ParseOptions>,
    file_name: Option<FileKey>,
    content: &str,
) -> (ast::types::Annotation<Loc, Loc>, Vec<(Loc, ParseError)>) {
    let env = init_env::<()>(None, parse_options, file_name, Ok(content));
    do_parse(env, false, |env| {
        type_parser::parse_annotation(env).ok().unwrap()
    })
    .unwrap()
}

pub fn parse_package_json_file<'a>(
    panic_if_failed: bool,
    token_sink: Option<&'a mut dyn FnMut(TokenSinkResult)>,
    parse_options: Option<ParseOptions>,
    file_name: Option<FileKey>,
    content: Result<&'a str, ()>,
) -> (
    (Loc, ast::expression::Object<Loc, Loc>),
    Vec<(Loc, ParseError)>,
) {
    fn parse(env: &mut ParserEnv) -> Result<(Loc, ast::expression::Object<Loc, Loc>), Rollback> {
        let (loc, obj, PatternCoverErrors { if_expr, .. }) = object_parser::initializer(env)?;
        for (loc, error) in if_expr {
            env.error_at(loc, error)?;
        }
        Ok((loc, obj))
    }
    let env = init_env(token_sink, parse_options, file_name, content);
    do_parse(env, panic_if_failed, |env| parse(env).ok().unwrap()).unwrap()
}

// even if fail=false, still raises an error on a totally invalid token, since
// there's no legitimate fallback.
pub fn parse_json_file<'a>(
    panic_if_failed: bool,
    token_sink: Option<&'a mut dyn FnMut(TokenSinkResult)>,
    parse_options: Option<ParseOptions>,
    file_name: Option<FileKey>,
    content: Result<&'a str, ()>,
) -> (
    ast::expression::Expression<Loc, Loc>,
    Vec<(Loc, ParseError)>,
) {
    fn null_fallback(
        env: &mut ParserEnv,
    ) -> Result<ast::expression::Expression<Loc, Loc>, Rollback> {
        let (loc, _) = with_loc(None, env, |_| Ok(()))?;
        Ok(ast::expression::Expression::new(
            ExpressionInner::NullLiteral {
                loc,
                inner: Arc::new(None),
            },
        ))
    }

    fn parse(env: &mut ParserEnv) -> Result<ast::expression::Expression<Loc, Loc>, Rollback> {
        match peek::token(env) {
            TokenKind::TLbracket
            | TokenKind::TLcurly
            | TokenKind::TString(_, _, _, _)
            | TokenKind::TNumber { .. }
            | TokenKind::TTrue
            | TokenKind::TFalse
            | TokenKind::TNull => parse_expression(env),
            TokenKind::TMinus => match peek::unary_minus_is_followed_by_number(env) {
                Some(_) => parse_expression(env),
                None => {
                    env.error_unexpected(Some("a number".to_owned()))?;
                    null_fallback(env)
                }
            },
            _ => {
                let _ = env.error_unexpected(Some("a valid JSON value".to_owned()));
                null_fallback(env)
            }
        }
    }

    let env = init_env(token_sink, parse_options, file_name, content);
    do_parse(env, panic_if_failed, |env| parse(env).ok().unwrap()).unwrap()
}

pub fn parse_jsx_pragma_expression<E>(
    file_name: Option<FileKey>,
    content: &str,
) -> Result<
    (
        ast::expression::Expression<Loc, Loc>,
        Vec<(Loc, ParseError)>,
    ),
    Vec<(Loc, ParseError)>,
> {
    fn left_hand_side(
        env: &mut ParserEnv,
    ) -> Result<ast::expression::Expression<Loc, Loc>, Rollback> {
        let ast = env.with_no_new(true, |env| expression_parser::left_hand_side(env))?;
        expect::token(env, TokenKind::TEof)?;
        Ok(ast)
    }

    let env = init_env::<()>(None, None, file_name, Ok(content));
    do_parse(env, true, |env| left_hand_side(env).ok().unwrap())
}

pub fn parse_to_json(
    parse_options: Option<ParseOptions>,
    intern_comments: bool,
    content: Result<&str, ()>,
) -> serde_json::Value {
    let offset_table = offset_utils::OffsetTable::make(content.as_ref().unwrap_or(&""));
    let (mut ast, errors) = parse_program_without_file(false, None, parse_options, content);
    if !intern_comments {
        comment_utils::strip_inlined_comments(&mut ast);
    }
    let mut result = estree_translator::program(
        &offset_table,
        &estree_translator::Config {
            include_locs: true,
            include_filename: true,
            offset_style: estree_translator::OffsetStyle::JsIndices,
        },
        &ast,
    );
    match &mut result {
        serde_json::Value::Object(obj) => {
            if !errors.is_empty() {
                obj.insert(
                    "errors".to_owned(),
                    estree_translator::errors(
                        &offset_table,
                        true,
                        estree_translator::OffsetStyle::JsIndices,
                        &errors,
                    ),
                );
            }
        }
        _ => unreachable!(),
    }
    result
}

#[cfg(test)]
mod tests {
    use dupe::Dupe;

    use super::*;
    use crate::ast::statement::StatementInner;
    use crate::file_key::FileKeyInner;

    #[test]
    fn program_location_spans_source() {
        // Programs include whitespace, comments, and interpreter directives, even without statements.
        for (source, end_line, end_column) in [
            (" a ", 1, 3),
            ("", 1, 0),
            (" \t\r\n ", 2, 1),
            (" /* comment */ \n", 2, 0),
            ("#!/usr/bin/env node\n a \n", 3, 0),
            ("\n/* leading */\na;\n/* trailing */\n ", 5, 1),
            ("\u{feff}a \u{2028}\u{2029} ", 3, 1),
            (" '\u{1f6a6}' ", 1, 8),
        ] {
            for filename in [None, Some("test.js")] {
                let file =
                    filename.map(|name| FileKey::new(FileKeyInner::SourceFile(name.to_owned())));
                let (program, errors) = parse_program(false, None, None, file.dupe(), Ok(source));
                assert!(errors.is_empty(), "{source:?}: {errors:?}"); // OK
                assert_eq!(program.loc, Loc::mk(file, 1, 0, end_line, end_column)); // OK
            }
        }

        // Statement locations remain token-based within the full-source program.
        let (program, errors) = parse_program_without_file(false, None, None, Ok(" a "));
        assert!(errors.is_empty()); // OK
        assert_eq!(program.statements[0].loc(), &Loc::mk(None, 1, 1, 1, 2)); // OK

        // Module parsing shares the same program bounds.
        let file = FileKey::new(FileKeyInner::SourceFile("test.mjs".to_owned()));
        let program =
            parse_module_file(ParseOptions::default(), file.dupe(), " a \n").expect("valid module");
        assert_eq!(program.loc, Loc::mk(Some(file), 1, 0, 2, 0)); // OK

        // Recoverable errors do not truncate the program at the last statement.
        let (program, errors) = parse_program_without_file(false, None, None, Ok(" const = ; \n"));
        assert!(!errors.is_empty()); // OK
        assert_eq!(program.loc, Loc::mk(None, 1, 0, 2, 0)); // OK
    }

    #[test]
    fn type_syntax_requires_types() {
        // Type-only class modifiers and signatures require types, even without annotations.
        let sources = [
            "abstract class C {}",
            "const C = abstract class {};",
            "abstract class C { abstract m(); }",
            "abstract class C { abstract x; }",
            "class C extends B { override m() {} }",
            "class C extends B { override x = 1; }",
            "class C { public x = 1; protected m() {} private n() {} }",
            "class C { constructor(public x) {} }",
            "class C { constructor(readonly x) {} }",
            "class C { constructor(writeonly x) {} }",
            "class C { m?(); }",
            "class C { m?(): void; }",
            "class C { m(): void; m() {} }",
            "class C { readonly x = 1; }",
            "class C { writeonly x = 1; }",
            "class C { +x = 1; }",
            "class C { -x = 1; }",
            "class C { x?; }",
            "class C { declare x; }",
            "class C extends B<number> {}",
            // Type-only imports and exports require types in every supported spelling.
            "export as namespace X;",
            "export {type T};",
            "export {type as};",
            "export {type as as T};",
            "import {type T} from 'foo';",
            "import {typeof X} from 'foo';",
            "import {type as} from 'foo';",
            "import {type as as T} from 'foo';",
            "export import type X = A.B;",
            // Existing annotation, expression, and declaration gates stay effective.
            "const x: number = 1;",
            "const x = value as number;",
            "const x = value as const;",
            "const x = value satisfies number;",
            "const x = value!;",
            "const x = value?.prop!;",
            "export default interface I {}",
            "class C<T> {}",
            "class C implements I {}",
            "const x = <C<number> />;",
            "type T = number;",
            "interface I {}",
            "declare class C {}",
            "export type {T};",
            "export type * from 'foo';",
            "import type X = A.B;",
        ];
        for source in sources {
            for types in [false, true] {
                let options = ParseOptions {
                    types,
                    ambiguous_types: types,
                    ..crate::PERMISSIVE_PARSE_OPTIONS
                };
                let comment = if types { "OK" } else { "ERROR" };
                let content = format!("{source} // {comment}");
                let (_, errors) =
                    parse_program_without_file(false, None, Some(options), Ok(&content));
                assert_eq!(errors.is_empty(), types, "{source}: {errors:?}"); // OK
            }

            // TypeScript filenames keep enabling all type syntax despite disabled options.
            let options = ParseOptions {
                types: false,
                ambiguous_types: false,
                ..crate::PERMISSIVE_PARSE_OPTIONS
            };
            let file = FileKey::new(FileKeyInner::SourceFile("test.ts".to_owned()));
            let content = format!("{source} // OK");
            let (_, errors) =
                parse_program_file::<()>(false, None, Some(options), file, Ok(&content));
            assert!(errors.is_empty(), "{source}: {errors:?}"); // OK
        }
    }

    #[test]
    fn type_keywords_can_still_be_value_names() {
        // Contextual type keywords remain valid member and import/export names in JavaScript.
        for source in [
            "class C { abstract = 1; override = 2; public = 3; private = 4; protected = 5; readonly = 6; writeonly = 7; declare = 8; } // OK",
            "class C { abstract() {} override() {} public() {} private() {} protected() {} readonly() {} writeonly() {} declare() {} } // OK",
            "import {type, type as T} from 'foo'; // OK",
            "export {type, type as T}; // OK",
            "const x = {namespace: 1, global: 2}; // OK",
            // Value import aliases and export assignments do not require types.
            "import X = require('foo'); // OK",
            "import X = A.B; // OK",
            "import type = require('foo'); // OK",
            "export import X = A.B; // OK",
            "export import type = A.B; // OK",
            "export = X; // OK",
            // Enums remain controlled by their own parser option.
            "enum E { A } // OK",
            "const enum E { A } // OK",
        ] {
            let (expected, errors) = parse_program_without_file(false, None, None, Ok(source));
            assert!(errors.is_empty(), "{source}: {errors:?}"); // OK
            let options = ParseOptions {
                types: false,
                ambiguous_types: false,
                ..crate::PERMISSIVE_PARSE_OPTIONS
            };
            let (program, errors) =
                parse_program_without_file(false, None, Some(options), Ok(source));
            assert!(errors.is_empty(), "{source}: {errors:?}"); // OK
            assert_eq!(program, expected, "{source}"); // OK
        }
    }

    #[test]
    fn type_keywords_followed_by_line_terminators_are_class_fields() {
        // Line terminators, including those in comments, separate JavaScript keyword fields.
        for keyword in [
            "abstract",
            "override",
            "public",
            "private",
            "protected",
            "readonly",
            "writeonly",
            "declare",
        ] {
            for separator in ["\n", "\r", "\u{2028}", "\u{2029}", "/*\n*/", "// field\n"] {
                let content = format!("class C {{ {keyword}{separator}x; }} // OK");
                let options = ParseOptions {
                    types: false,
                    ambiguous_types: false,
                    ..crate::PERMISSIVE_PARSE_OPTIONS
                };
                let (program, errors) =
                    parse_program_without_file(false, None, Some(options), Ok(&content));
                assert!(errors.is_empty(), "{content}: {errors:?}"); // OK
                assert!(matches!(
                    &*program.statements[0],
                    StatementInner::ClassDeclaration { inner, .. } if inner.body.body.len() == 2
                )); // OK
            }
        }

        // Type-enabled parsing continues to accept modifiers across line terminators.
        let (program, errors) =
            parse_program_without_file(false, None, None, Ok("class C { public\nx; } // OK"));
        assert!(errors.is_empty(), "{errors:?}"); // OK
        assert!(matches!(
            &*program.statements[0],
            StatementInner::ClassDeclaration { inner, .. } if inner.body.body.len() == 1
        )); // OK
    }

    #[test]
    fn type_syntax_diagnostics_describe_the_construct() {
        // Operators, modifiers, and type arguments have distinct untyped-mode diagnostics.
        for (source, expected) in [
            (
                "value as number; // ERROR",
                ParseError::UnexpectedTypeOperator,
            ),
            (
                "value satisfies number; // ERROR",
                ParseError::UnexpectedTypeOperator,
            ),
            ("value!; // ERROR", ParseError::UnexpectedTypeOperator),
            ("value?.prop!; // ERROR", ParseError::UnexpectedTypeOperator),
            (
                "value?.prop!.next; // ERROR",
                ParseError::UnexpectedTypeOperator,
            ),
            (
                "value?.method!(); // ERROR",
                ParseError::UnexpectedTypeOperator,
            ),
            (
                "abstract class C {} // ERROR",
                ParseError::UnexpectedTypeModifier,
            ),
            (
                "class C { public x; } // ERROR",
                ParseError::UnexpectedTypeModifier,
            ),
            (
                "class C { constructor(public x) {} } // ERROR",
                ParseError::UnexpectedTypeModifier,
            ),
            (
                "class C { readonly x; } // ERROR",
                ParseError::UnexpectedTypeModifier,
            ),
            (
                "class C { m?(); } // ERROR",
                ParseError::UnexpectedTypeModifier,
            ),
            (
                "class C extends B<number> {} // ERROR",
                ParseError::UnexpectedTypeArguments,
            ),
        ] {
            let options = ParseOptions {
                types: false,
                ambiguous_types: false,
                ..crate::PERMISSIVE_PARSE_OPTIONS
            };
            let (_, errors) = parse_program_without_file(false, None, Some(options), Ok(source));
            assert!(
                errors.iter().any(|(_, error)| *error == expected),
                "{source}: {errors:?}"
            ); // OK
        }
    }

    #[test]
    fn type_syntax_respects_javascript_line_terminators() {
        // A line terminator keeps identifier calls and prefix negation as separate JS statements.
        for source in [
            "value\nas(value); // OK",
            "value\nsatisfies(value); // OK",
            "value\n!other; // OK",
            "value?.prop\n!other; // OK",
            // Abstract can end a JavaScript statement before a class declaration.
            "abstract\nclass C {} // OK",
            "abstract/*\n*/class C {} // OK",
            "export default abstract\nclass C {} // OK",
        ] {
            let options = ParseOptions {
                types: false,
                ambiguous_types: false,
                ..crate::PERMISSIVE_PARSE_OPTIONS
            };
            let (program, errors) =
                parse_program_without_file(false, None, Some(options), Ok(source));
            assert!(errors.is_empty(), "{source}: {errors:?}"); // OK
            assert_eq!(program.statements.len(), 2); // OK
        }
    }

    #[test]
    fn typescript_filenames_enable_types() {
        // TypeScript filenames enable annotations and ambiguous call/new type arguments.
        let source =
            "const value: number = 1; // OK\nf<number>(value); // OK\nnew C<number>(value); // OK";
        for filename in [
            "test.ts",
            "test.tsx",
            "test.cts",
            "test.mts",
            "test.d.ts",
            "test.d.cts",
            "test.d.mts",
        ] {
            let file = FileKey::new(FileKeyInner::SourceFile(filename.to_owned()));
            let (expected, errors) =
                parse_program_file::<()>(false, None, None, file.dupe(), Ok(source));
            assert!(errors.is_empty(), "{filename}: {errors:?}"); // OK
            for types in [false, true] {
                let options = ParseOptions {
                    types,
                    ambiguous_types: false,
                    ..Default::default()
                };
                let (program, errors) =
                    parse_program_file::<()>(false, None, Some(options), file.dupe(), Ok(source));
                assert!(errors.is_empty(), "{filename}, types={types}: {errors:?}"); // OK
                assert_eq!(program, expected, "{filename}, types={types}"); // OK
            }
        }
    }

    #[test]
    fn non_typescript_filenames_respect_type_options() {
        // JavaScript and unnamed sources still reject annotations when types are disabled.
        for filename in [None, Some("test.js"), Some("test.ts.js")] {
            let file = filename.map(|name| FileKey::new(FileKeyInner::SourceFile(name.to_owned())));
            let options = ParseOptions {
                types: false,
                ambiguous_types: false,
                ..Default::default()
            };
            let (_, errors) = parse_program(
                false,
                None,
                Some(options),
                file.dupe(),
                Ok("const value: number = 1; // ERROR"),
            );
            assert!(
                errors
                    .iter()
                    .any(|(_, error)| *error == ParseError::UnexpectedTypeAnnotation)
            ); // OK

            // Disabling ambiguous types preserves JavaScript comparison parsing.
            let options = ParseOptions {
                ambiguous_types: false,
                ..Default::default()
            };
            let (program, errors) = parse_program(
                false,
                None,
                Some(options),
                file,
                Ok("f<number>(value); // OK"),
            );
            assert!(errors.is_empty(), "{filename:?}: {errors:?}"); // OK
            assert!(matches!(
                &*program.statements[0],
                StatementInner::Expression { inner, .. }
                    if matches!(&*inner.expression, ExpressionInner::Binary { .. })
            )); // OK
        }
    }
}
