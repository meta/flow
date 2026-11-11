/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;
use std::sync::Arc;

use dupe::Dupe;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;

use crate::ast::Comment;
use crate::ast::Identifier;
use crate::ast::StringLiteral;
use crate::ast::Syntax;
use crate::ast::VariableKind;
use crate::ast::class;
use crate::ast::expression;
use crate::ast::expression::ExpressionInner;
use crate::ast::function;
use crate::ast::match_;
use crate::ast::pattern;
use crate::ast::statement;
use crate::ast::statement::StatementInner;
use crate::ast::types;
use crate::ast_utils;
use crate::ast_visitor::AstVisitor;
use crate::ast_visitor::TypeParamsContext;
use crate::comment_attachment;
use crate::declaration_parser;
use crate::enum_parser;
use crate::expression_parser;
use crate::loc::LOC_NONE;
use crate::loc::Loc;
use crate::main_parser;
use crate::match_pattern_parser;
use crate::object_parser;
use crate::parse_error::ParseError;
use crate::parser_common::assert_identifier_name_is_identifier;
use crate::parser_common::identifier_name;
use crate::parser_common::is_labelled_function;
use crate::parser_common::is_start_of_type_guard;
use crate::parser_common::with_loc;
use crate::parser_common::with_loc_opt;
use crate::parser_env::LexMode;
use crate::parser_env::ParserEnv;
use crate::parser_env::eat;
use crate::parser_env::expect;
use crate::parser_env::is_reserved_type;
use crate::parser_env::peek;
use crate::parser_env::try_parse;
use crate::parser_env::try_parse::Rollback;
use crate::pattern_cover;
use crate::pattern_cover::PatternCover;
use crate::pattern_parser;
use crate::token::TokenKind;
use crate::type_parser;

enum ForLhs {
    ForExpression(PatternCover),
    ForDeclaration((Loc, statement::VariableDeclaration<Loc, Loc>)),
}
enum SemicolonType {
    Explicit(Vec<Comment<Loc>>),
    Implicit(comment_attachment::TrailingAndRemoverResult),
}
enum DeclareAsyncResult {
    Component(statement::DeclareComponent<Loc, Loc>),
    Function(statement::DeclareFunction<Loc, Loc>),
}

// FunctionDeclaration is not a valid Statement, but Annex B sometimes allows it.
// However, AsyncFunctionDeclaration and GeneratorFunctionDeclaration are never
// allowed as statements. We still parse them as statements (and raise an error) to
// recover gracefully.
fn function_as_statement(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let func = declaration_parser::parse_function(env)?;
    if env.in_strict_mode() {
        env.function_as_statement_error_at(func.loc().dupe())?;
    } else {
        match func.deref() {
            StatementInner::FunctionDeclaration { loc, inner } => {
                if inner.async_ {
                    env.error_at(loc.dupe(), ParseError::AsyncFunctionAsStatement)?;
                } else if inner.generator {
                    env.error_at(loc.dupe(), ParseError::GeneratorFunctionAsStatement)?;
                }
            }
            _ => {}
        }
    }
    Ok(func)
}

fn string_literal(
    env: &mut ParserEnv,
    loc: Loc,
    value: FlowSmolStr,
    raw: FlowSmolStr,
    octal: bool,
) -> Result<(Loc, StringLiteral<Loc>), Rollback> {
    if octal {
        env.strict_error(ParseError::StrictOctalLiteral)?;
    }
    let leading = peek::comments(env);
    expect::token(
        env,
        TokenKind::TString(loc.dupe(), value.to_owned(), raw.to_owned(), octal),
    )?;
    let trailing = eat::trailing_comments(env);
    Ok((
        loc,
        StringLiteral {
            value,
            raw,
            comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        },
    ))
}

/// Semicolon insertion is handled here :(. There seem to be 2 cases where
/// semicolons are inserted. First, if we reach the EOF. Second, if the next
/// token is } or is separated by a LineTerminator.
fn semicolon(
    env: &mut ParserEnv,
    expected: Option<&str>,
    required: bool,
) -> Result<SemicolonType, Rollback> {
    match peek::token(env) {
        TokenKind::TEof | TokenKind::TRcurly => Ok(SemicolonType::Implicit(
            comment_attachment::TrailingAndRemoverResult {
                trailing: eat::trailing_comments(env),
                remover: None,
            },
        )),
        TokenKind::TSemicolon => {
            eat::token(env)?;
            let trailing = match peek::token(env) {
                TokenKind::TEof | TokenKind::TRcurly => eat::trailing_comments(env),
                _ => {
                    if peek::is_line_terminator(env) {
                        eat::comments_until_next_line(env)
                    } else {
                        Vec::new()
                    }
                }
            };
            Ok(SemicolonType::Explicit(trailing))
        }
        _ => {
            if peek::is_line_terminator(env) {
                Ok(SemicolonType::Implicit(
                    comment_attachment::trailing_and_remover_after_last_line(env),
                ))
            } else {
                if required {
                    let expected_msg = expected.unwrap_or("the token `;`");
                    env.error_unexpected(Some(expected_msg.to_string()))?;
                }
                Ok(SemicolonType::Explicit(Vec::new()))
            }
        }
    }
}

/// Consumes and returns the trailing comments after the end of a statement. Also returns
/// a remover that can remove all comments that are not trailing the previous token.
///
/// If a statement is the end of a block or file, all comments are trailing.
/// Otherwise, if a statement is followed by a new line, only comments on the current
/// line are trailing. If a statement is not followed by a new line, it does not have
/// trailing comments as they are instead leading comments for the next statement.
fn statement_end_trailing_comments(
    env: &mut ParserEnv,
) -> comment_attachment::TrailingAndRemoverResult {
    match peek::token(env) {
        TokenKind::TEof | TokenKind::TRcurly => comment_attachment::TrailingAndRemoverResult {
            trailing: eat::trailing_comments(env),
            remover: None,
        },
        _ => comment_attachment::trailing_and_remover(env),
    }
}

fn variable_declaration_end(
    env: &mut ParserEnv,
    kind: VariableKind,
    declarations: &mut [statement::variable::Declarator<Loc, Loc>],
) -> Result<Vec<Comment<Loc>>, Rollback> {
    match semicolon(env, None, true)? {
        SemicolonType::Explicit(comments) => Ok(comments),
        SemicolonType::Implicit(comment_attachment::TrailingAndRemoverResult {
            trailing: _,
            remover: None,
        }) => Ok(Vec::new()),
        SemicolonType::Implicit(comment_attachment::TrailingAndRemoverResult {
            trailing: _,
            remover: Some(mut remover),
        }) => {
            // Remove trailing comments from the last declarator
            if let Some(last) = declarations.last_mut() {
                *last = remover.map_variable_declarator(kind, last);
            }
            Ok(Vec::new())
        }
    }
}

fn empty_statement(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let loc = peek::loc(env).dupe();
    let leading = peek::comments(env);
    expect::token(env, TokenKind::TSemicolon)?;
    let trailing = statement_end_trailing_comments(env).trailing;
    Ok(statement::Statement::new(StatementInner::Empty {
        loc,
        inner: Arc::new(statement::Empty {
            comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        }),
    }))
}

fn break_statement(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let leading = peek::comments(env);
    let (loc, (label, trailing)) = with_loc(None, env, |env| {
        expect::token(env, TokenKind::TBreak)?;
        let mut label =
            if peek::token(env) == &TokenKind::TSemicolon || peek::is_implicit_semicolon(env) {
                None
            } else {
                let label = main_parser::parse_identifier(env, None)?;
                let label_name = &label.name;
                if !env.in_labels(label_name) {
                    env.error_at(
                        label.loc.dupe(),
                        ParseError::UnknownLabel(label_name.as_str().to_owned()),
                    )?;
                }
                Some(label)
            };

        let trailing = match (semicolon(env, None, true)?, label.as_mut()) {
            (SemicolonType::Explicit(trailing), _)
            | (
                SemicolonType::Implicit(comment_attachment::TrailingAndRemoverResult {
                    trailing,
                    ..
                }),
                None,
            ) => trailing,
            (SemicolonType::Implicit(result), Some(label)) => {
                if let Some(mut remover) = result.remover {
                    *label = remover.map_identifier(label);
                }
                Vec::new()
            }
        };
        Ok((label, trailing))
    })?;

    if label.is_none() && !env.in_loop() && !env.in_switch() {
        env.error_at(
            loc.dupe(),
            ParseError::IllegalBreak {
                in_match_statement: env.in_match_statement(),
            },
        )?;
    }
    let comments = ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into()));
    Ok(statement::Statement::new(StatementInner::Break {
        loc,
        inner: Arc::new(statement::Break { label, comments }),
    }))
}

fn continue_statement(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let leading = peek::comments(env);
    let (loc, (label, trailing)) = with_loc(None, env, |env| {
        expect::token(env, TokenKind::TContinue)?;
        let mut label =
            if peek::token(env) == &TokenKind::TSemicolon || peek::is_implicit_semicolon(env) {
                None
            } else {
                let label = main_parser::parse_identifier(env, None)?;
                let label_name = &label.name;
                if !env.in_labels(label_name) {
                    env.error_at(
                        label.loc.dupe(),
                        ParseError::UnknownLabel(label_name.as_str().to_owned()),
                    )?;
                }
                Some(label)
            };

        let trailing = match (semicolon(env, None, true)?, label.as_mut()) {
            (SemicolonType::Explicit(trailing), _)
            | (
                SemicolonType::Implicit(comment_attachment::TrailingAndRemoverResult {
                    trailing,
                    ..
                }),
                None,
            ) => trailing,
            (SemicolonType::Implicit(result), Some(label)) => {
                if let Some(mut remover) = result.remover {
                    *label = remover.map_identifier(label);
                }
                Vec::new()
            }
        };
        Ok((label, trailing))
    })?;

    if !env.in_loop() {
        env.error_at(loc.dupe(), ParseError::IllegalContinue)?;
    }
    let comments = ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into()));
    Ok(statement::Statement::new(StatementInner::Continue {
        loc,
        inner: Arc::new(statement::Continue { label, comments }),
    }))
}

fn debugger(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, s) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TDebugger)?;
        let pre_semicolon_trailing = if peek::token(env) == &TokenKind::TSemicolon {
            eat::trailing_comments(env)
        } else {
            Vec::new()
        };
        let trailing = match semicolon(env, None, true)? {
            SemicolonType::Explicit(trailing)
            | SemicolonType::Implicit(comment_attachment::TrailingAndRemoverResult {
                trailing,
                ..
            }) => {
                let mut all_trailing = pre_semicolon_trailing;
                all_trailing.extend(trailing);
                all_trailing
            }
        };
        Ok(statement::Debugger {
            comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        })
    })?;
    Ok(statement::Statement::new(StatementInner::Debugger {
        loc,
        inner: Arc::new(s),
    }))
}

fn do_while(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, s) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TDo)?;
        // Parse the body in loop context
        let body = env.with_in_loop(true, |env| parse_statement(env, true))?;
        // Annex B allows labelled FunctionDeclarations (see
        // sec-labelled-function-declarations), but not in IterationStatement
        // (see sec-semantics-static-semantics-early-errors).
        if !env.in_strict_mode() && is_labelled_function(&body) {
            env.function_as_statement_error_at(body.loc().dupe())?;
        }
        let pre_keyword_trailing = eat::trailing_comments(env);
        expect::token(env, TokenKind::TWhile)?;
        let pre_cond_trailing = eat::trailing_comments(env);
        expect::token(env, TokenKind::TLparen)?;
        let test = main_parser::parse_expression(env)?;
        expect::token(env, TokenKind::TRparen)?;
        let past_cond_trailing = if peek::token(env) == &TokenKind::TSemicolon {
            eat::trailing_comments(env)
        } else {
            Vec::new()
        };
        // The rules of automatic semicolon insertion in ES5 don't mention this,
        // but the semicolon after a do-while loop is optional. This is properly
        // specified in ES6
        let past_cond_trailing = match semicolon(env, None, false)? {
            SemicolonType::Explicit(trailing) => {
                let mut all_trailing = past_cond_trailing;
                all_trailing.extend(trailing);
                all_trailing
            }
            SemicolonType::Implicit(comment_attachment::TrailingAndRemoverResult {
                trailing,
                ..
            }) => trailing,
        };
        let mut trailing = pre_keyword_trailing;
        trailing.extend(pre_cond_trailing);
        trailing.extend(past_cond_trailing);
        Ok(statement::DoWhile {
            body,
            test,
            comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        })
    })?;
    Ok(statement::Statement::new(StatementInner::DoWhile {
        loc,
        inner: Arc::new(s),
    }))
}

fn for_loop(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    fn assert_can_be_forin_or_forof(
        env: &mut ParserEnv,
        err: ParseError,
        decl: &(Loc, statement::VariableDeclaration<Loc, Loc>),
    ) -> Result<(), Rollback> {
        // Only a single declarator is allowed, without an init. So
        // something like
        //
        // for (var x in y) {}
        //
        // is allowed, but we disallow
        //
        // for (var x, y in z) {}
        // for (var x = 42 in y) {}
        match &decl.1.declarations[..] {
            [decl] if decl.init.is_none() => Ok(()),
            _ => env.error_at(decl.0.dupe(), err),
        }
    }

    // Annex B allows labelled FunctionDeclarations (see sec-labelled-function-declarations),
    // but not in IterationStatement (see sec-semantics-static-semantics-early-errors).
    fn assert_not_labelled_function(
        env: &mut ParserEnv,
        body: &statement::Statement<Loc, Loc>,
    ) -> Result<(), Rollback> {
        if !env.in_strict_mode() && is_labelled_function(body) {
            env.function_as_statement_error_at(body.loc().dupe())?;
        }
        Ok(())
    }

    let (loc, mut inner) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TFor)?;
        let async_ = env.allow_await() && eat::maybe(env, TokenKind::TAwait)?;
        let leading = {
            let mut l = leading;
            l.extend(peek::comments(env));
            l
        };
        expect::token(env, TokenKind::TLparen)?;
        let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
        let init_starts_with_async = matches!(peek::token(env), TokenKind::TAsync);

        let (init, errs) = env.with_no_in(true, |env| match peek::token(env).clone() {
            TokenKind::TSemicolon => Ok((None, vec![])),
            TokenKind::TLet if !peek::let_token_is_followed_by_in(env) => {
                let (loc, (declarations, leading, errs)) =
                    with_loc(None, env, |env| declaration_parser::parse_let(env, false))?;
                Ok((
                    Some(ForLhs::ForDeclaration((
                        loc,
                        statement::VariableDeclaration {
                            kind: VariableKind::Let,
                            declarations: declarations.into(),
                            comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                        },
                    ))),
                    errs,
                ))
            }
            TokenKind::TLet => {
                let expr = main_parser::parse_expression_or_pattern(env)?;
                Ok((Some(ForLhs::ForExpression(expr)), vec![]))
            }
            TokenKind::TConst => {
                let (loc, (declarations, leading, errs)) =
                    with_loc(None, env, |env| declaration_parser::parse_const(env, false))?;
                Ok((
                    Some(ForLhs::ForDeclaration((
                        loc,
                        statement::VariableDeclaration {
                            kind: VariableKind::Const,
                            declarations: declarations.into(),
                            comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                        },
                    ))),
                    errs,
                ))
            }
            TokenKind::TVar => {
                let (loc, (declarations, leading, errs)) =
                    with_loc(None, env, |env| declaration_parser::parse_var(env, false))?;
                Ok((
                    Some(ForLhs::ForDeclaration((
                        loc,
                        statement::VariableDeclaration {
                            kind: VariableKind::Var,
                            declarations: declarations.into(),
                            comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                        },
                    ))),
                    errs,
                ))
            }
            _ => {
                let expr = main_parser::parse_expression_or_pattern(env)?;
                Ok((Some(ForLhs::ForExpression(expr)), vec![]))
            }
        })?;

        match peek::token(env) {
            TokenKind::TOf => {
                // This is a for-of loop
                let left = match init {
                    Some(ForLhs::ForDeclaration(decl)) => {
                        assert_can_be_forin_or_forof(env, ParseError::InvalidLHSInForOf, &decl)?;
                        statement::for_of::Left::LeftDeclaration(decl)
                    }
                    Some(ForLhs::ForExpression(expr)) => {
                        // #sec-for-in-and-for-of-statements-static-semantics-early-errors
                        let patt = pattern_cover::as_pattern(
                            env,
                            Some(ParseError::InvalidLHSInForOf),
                            expr,
                        )?;
                        // Check for `for (async of ...)` which is forbidden
                        if !async_ && init_starts_with_async {
                            if let pattern::Pattern::Identifier { inner, .. } = &patt {
                                if inner.name.name == "async" {
                                    // #prod-nLtPS4oB - `for (async of ...)` is forbidden because it is
                                    // ambiguous whether it's a for-of with an `async` identifier, or a
                                    // regular for loop with an async arrow function with a param named
                                    // `of`. We can backtrack, so we know it's a for-of, but the spec
                                    // still disallows it.
                                    env.error_at(
                                        inner.name.loc.dupe(),
                                        ParseError::InvalidLHSInForOf,
                                    )?;
                                }
                            }
                        }
                        statement::for_of::Left::LeftPattern(patt)
                    }
                    None => unreachable!(),
                };
                expect::token(env, TokenKind::TOf)?;
                let right = expression_parser::assignment(env)?;
                expect::token(env, TokenKind::TRparen)?;
                let body = env.with_in_loop(true, |env| parse_statement(env, true))?;
                assert_not_labelled_function(env, &body)?;
                let inner = statement::ForOf {
                    left,
                    right,
                    body,
                    await_: async_,
                    comments,
                };
                Ok(StatementInner::ForOf {
                    loc: LOC_NONE,
                    inner: Arc::new(inner),
                })
            }
            TokenKind::TIn => {
                // This is a for-in loop
                let left = match init {
                    Some(ForLhs::ForDeclaration(decl)) => {
                        assert_can_be_forin_or_forof(env, ParseError::InvalidLHSInForIn, &decl)?;
                        statement::for_in::Left::LeftDeclaration(decl)
                    }
                    Some(ForLhs::ForExpression(expr)) => {
                        // #sec-for-in-and-for-of-statements-static-semantics-early-errors
                        let patt = pattern_cover::as_pattern(
                            env,
                            Some(ParseError::InvalidLHSInForIn),
                            expr,
                        )?;
                        statement::for_in::Left::LeftPattern(patt)
                    }
                    None => unreachable!(),
                };
                if async_ {
                    // If async is true, this should have been a for-await-of loop, but we
                    // recover by trying to parse like a for-in loop.
                    expect::token(env, TokenKind::TOf)?;
                } else {
                    expect::token(env, TokenKind::TIn)?;
                }
                let right = main_parser::parse_expression(env)?;
                expect::token(env, TokenKind::TRparen)?;
                let body = env.with_in_loop(true, |env| parse_statement(env, true))?;
                assert_not_labelled_function(env, &body)?;
                let inner = statement::ForIn {
                    left,
                    right,
                    body,
                    comments,
                };
                Ok(StatementInner::ForIn {
                    loc: LOC_NONE,
                    inner: Arc::new(inner),
                })
            }
            _ => {
                // This is a regular for loop
                for err in errs {
                    env.error_at(err.0.dupe(), err.1)?;
                }
                if async_ {
                    // If async is true, this should have been a for-await-of loop, but we
                    // recover by trying to parse like a normal loop.
                    expect::token(env, TokenKind::TOf)?;
                } else {
                    expect::token(env, TokenKind::TSemicolon)?;
                }
                let init = match init {
                    Some(ForLhs::ForDeclaration(decl)) => {
                        Some(statement::for_::Init::InitDeclaration(decl))
                    }
                    Some(ForLhs::ForExpression(expr)) => {
                        Some(statement::for_::Init::InitExpression(
                            pattern_cover::as_expression(env, expr)?,
                        ))
                    }
                    None => None,
                };
                let test = match peek::token(env) {
                    TokenKind::TSemicolon => None,
                    _ => Some(main_parser::parse_expression(env)?),
                };
                expect::token(env, TokenKind::TSemicolon)?;
                let update = match peek::token(env) {
                    TokenKind::TRparen => None,
                    _ => Some(main_parser::parse_expression(env)?),
                };
                expect::token(env, TokenKind::TRparen)?;
                let body = env.with_in_loop(true, |env| parse_statement(env, true))?;
                assert_not_labelled_function(env, &body)?;
                let inner = statement::For {
                    init,
                    test,
                    update,
                    body,
                    comments,
                };
                Ok(StatementInner::For {
                    loc: LOC_NONE,
                    inner: Arc::new(inner),
                })
            }
        }
    })?;
    *inner.loc_mut() = loc;
    Ok(statement::Statement::new(inner))
}

fn if_statement(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    // Either the consequent or alternate of an if statement
    fn if_branch(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
        // Normally this would just be a Statement, but Annex B allows
        // FunctionDeclarations in non-strict mode. See
        // sec-functiondeclarations-in-ifstatement-statement-clauses
        let stmt = if peek::is_function(env) {
            function_as_statement(env)?
        } else {
            parse_statement(env, true)?
        };
        // Annex B allows labelled FunctionDeclarations in non-strict mode
        // (see sec-labelled-function-declarations), but not in IfStatement
        // (see sec-if-statement-static-semantics-early-errors).
        if !env.in_strict_mode() && is_labelled_function(&stmt) {
            env.function_as_statement_error_at(stmt.loc().dupe())?;
        }
        Ok(stmt)
    }

    fn alternate(env: &mut ParserEnv) -> Result<statement::if_::Alternate<Loc, Loc>, Rollback> {
        let (loc, mut s) = with_loc(None, env, |env| {
            let leading = peek::comments(env);
            expect::token(env, TokenKind::TElse)?;
            let body = if_branch(env)?;
            Ok(statement::if_::Alternate {
                loc: LOC_NONE,
                body,
                comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
            })
        })?;
        s.loc = loc;
        Ok(s)
    }

    let (loc, s) = with_loc(None, env, |env| {
        let pre_if_leading = peek::comments(env);
        expect::token(env, TokenKind::TIf)?;
        let pre_cond_leading = peek::comments(env);
        let leading = {
            let mut l = pre_if_leading;
            l.extend(pre_cond_leading);
            l
        };
        expect::token(env, TokenKind::TLparen)?;
        let test = main_parser::parse_expression(env)?;
        expect::token(env, TokenKind::TRparen)?;
        let consequent = if_branch(env)?;
        let alternate = if peek::token(env) == &TokenKind::TElse {
            Some(alternate(env)?)
        } else {
            None
        };
        Ok(statement::If {
            test,
            consequent,
            alternate,
            comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
        })
    })?;
    Ok(statement::Statement::new(StatementInner::If {
        loc,
        inner: Arc::new(s),
    }))
}

fn return_statement(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, s) = with_loc(None, env, |env| {
        if !env.in_function() && !env.parse_options().allow_return_outside_function {
            env.error(ParseError::IllegalReturn)?;
        }
        let leading = peek::comments(env);
        let start_loc = peek::loc(env).dupe();
        expect::token(env, TokenKind::TReturn)?;
        let trailing = if peek::token(env) == &TokenKind::TSemicolon {
            eat::trailing_comments(env)
        } else {
            Vec::new()
        };
        let mut argument =
            if peek::token(env) == &TokenKind::TSemicolon || peek::is_implicit_semicolon(env) {
                None
            } else {
                Some(main_parser::parse_expression(env)?)
            };
        let return_out = Loc::between(&start_loc, peek::loc(env));
        let trailing = match (semicolon(env, None, true)?, argument.as_mut()) {
            (SemicolonType::Explicit(comments), _)
            | (
                SemicolonType::Implicit(comment_attachment::TrailingAndRemoverResult {
                    trailing: comments,
                    ..
                }),
                None,
            ) => {
                let mut all_trailing = trailing;
                all_trailing.extend(comments);
                all_trailing
            }
            (SemicolonType::Implicit(result), Some(arg)) => {
                if let Some(mut remover) = result.remover {
                    *arg = remover.map_expression(arg);
                }
                trailing
            }
        };
        Ok(statement::Return {
            argument,
            return_out,
            comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        })
    })?;
    Ok(statement::Statement::new(StatementInner::Return {
        loc,
        inner: Arc::new(s),
    }))
}

fn switch(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    fn case(
        env: &mut ParserEnv,
        seen_default: bool,
    ) -> Result<(statement::switch::Case<Loc, Loc>, bool), Rollback> {
        let (loc, (mut switch_case, seen_default)) = with_loc(None, env, |env| {
            let leading = peek::comments(env);
            let (test, trailing, case_loc_opt) = match peek::token(env) {
                TokenKind::TDefault => {
                    if seen_default {
                        env.error(ParseError::MultipleDefaultsInSwitch)?;
                    }
                    expect::token(env, TokenKind::TDefault)?;
                    (None, eat::trailing_comments(env), None)
                }
                _ => {
                    let case_loc = peek::loc(env).dupe();
                    expect::token(env, TokenKind::TCase)?;
                    (
                        Some(main_parser::parse_expression(env)?),
                        Vec::new(),
                        Some(case_loc),
                    )
                }
            };
            let seen_default = seen_default || test.is_none();
            expect::token(env, TokenKind::TColon)?;
            let line_end_trailing = statement_end_trailing_comments(env).trailing;
            let trailing = {
                let mut t = trailing;
                t.extend(line_end_trailing);
                t
            };
            let is_terminal_token = |token: &TokenKind| {
                matches!(
                    token,
                    TokenKind::TRcurly | TokenKind::TDefault | TokenKind::TCase
                )
            };
            let consequent =
                env.with_in_switch(true, |env| parse_statement_list(env, is_terminal_token))?;
            let comments = ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into()));
            let case_test_loc = match (case_loc_opt, &test) {
                (_, None) | (None, _) => None,
                (Some(case_loc), Some(expr)) => Some(Loc::between(&case_loc, expr.loc())),
            };
            let case = statement::switch::Case {
                loc: LOC_NONE,
                test,
                case_test_loc,
                consequent: consequent.into(),
                comments,
            };
            Ok((case, seen_default))
        })?;
        switch_case.loc = loc;
        Ok((switch_case, seen_default))
    }

    fn case_list(
        env: &mut ParserEnv,
        mut seen_default: bool,
    ) -> Result<Vec<statement::switch::Case<Loc, Loc>>, Rollback> {
        let mut cases = Vec::new();
        loop {
            match peek::token(env) {
                TokenKind::TEof | TokenKind::TRcurly => {
                    return Ok(cases);
                }
                _ => {
                    let (case_, seen_default_new) = case(env, seen_default)?;
                    cases.push(case_);
                    seen_default = seen_default_new;
                }
            }
        }
    }

    let (loc, s) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TSwitch)?;
        expect::token(env, TokenKind::TLparen)?;
        let discriminant = main_parser::parse_expression(env)?;
        expect::token(env, TokenKind::TRparen)?;
        expect::token(env, TokenKind::TLcurly)?;
        let cases = case_list(env, false)?;
        expect::token(env, TokenKind::TRcurly)?;
        let trailing = statement_end_trailing_comments(env).trailing;
        Ok(statement::Switch {
            discriminant: discriminant.clone(),
            cases: cases.into(),
            comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            exhaustive_out: discriminant.loc().dupe(),
        })
    })?;
    Ok(statement::Statement::new(StatementInner::Switch {
        loc,
        inner: Arc::new(s),
    }))
}

fn match_statement(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    fn case(
        env: &mut ParserEnv,
    ) -> Result<match_::Case<Loc, Loc, statement::Statement<Loc, Loc>>, Rollback> {
        let (loc, mut case_node) = with_loc(None, env, |env| {
            let leading = peek::comments(env);
            let case_match_root_loc = peek::loc(env).start_loc();
            let invalid_prefix_case = if peek::token(env) == &TokenKind::TCase {
                let loc = peek::loc(env).dupe();
                eat::token(env)?;
                Some(loc)
            } else {
                None
            };
            let pattern = match_pattern_parser::parse_match_pattern(env)?;
            let guard = if eat::maybe(env, TokenKind::TIf)? {
                expect::token(env, TokenKind::TLparen)?;
                let test = main_parser::parse_expression(env)?;
                expect::token(env, TokenKind::TRparen)?;
                Some(test)
            } else {
                None
            };
            let invalid_infix_colon = if peek::token(env) == &TokenKind::TColon {
                let loc = peek::loc(env).dupe();
                eat::token(env)?;
                Some(loc)
            } else {
                expect::token(env, TokenKind::TArrow)?;
                None
            };
            let body = env.with_in_match_statement(true, |env| parse_statement(env, false))?;
            let _ = eat::maybe(env, TokenKind::TComma)?;
            let trailing = eat::trailing_comments(env);
            let comments = ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into()));
            let invalid_syntax = match_::InvalidSyntax {
                invalid_prefix_case,
                invalid_infix_colon,
                invalid_suffix_semicolon: None,
            };
            Ok(match_::Case {
                loc: LOC_NONE,
                pattern,
                body,
                guard,
                comments,
                invalid_syntax,
                case_match_root_loc,
            })
        })?;
        case_node.loc = loc;
        Ok(case_node)
    }

    fn case_list(
        env: &mut ParserEnv,
    ) -> Result<Vec<match_::Case<Loc, Loc, statement::Statement<Loc, Loc>>>, Rollback> {
        let mut cases = Vec::new();
        loop {
            match peek::token(env) {
                TokenKind::TEof | TokenKind::TRcurly => {
                    return Ok(cases);
                }
                _ => {
                    cases.push(case(env)?);
                }
            }
        }
    }

    let (loc, s) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        let match_keyword_loc = peek::loc(env).dupe();
        expect::token(env, TokenKind::TMatch)?;
        if peek::is_line_terminator(env) {
            return Err(Rollback);
        }
        let args = crate::expression_parser::arguments(env)?;
        if peek::is_line_terminator(env) || !eat::maybe(env, TokenKind::TLcurly)? {
            return Err(Rollback);
        }
        let arg = crate::parser_common::reparse_arguments_as_match_argument(env, args)?;
        let cases = case_list(env)?;
        expect::token(env, TokenKind::TRcurly)?;
        let trailing = eat::trailing_comments(env);
        Ok(match_::Match {
            arg,
            cases: cases.into(),
            match_keyword_loc,
            comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        })
    })?;
    Ok(statement::Statement::new(StatementInner::Match {
        loc,
        inner: Arc::new(s),
    }))
}

fn throw(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, s) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        let start_loc = peek::loc(env).dupe();
        expect::token(env, TokenKind::TThrow)?;
        if peek::is_line_terminator(env) {
            env.error_at(start_loc, ParseError::NewlineAfterThrow)?;
        }
        let argument = main_parser::parse_expression(env)?;
        let (trailing, argument) = match semicolon(env, None, true)? {
            SemicolonType::Explicit(trailing) => (trailing, argument),
            SemicolonType::Implicit(result) => {
                let mut arg = argument;
                if let Some(mut remover) = result.remover {
                    arg = remover.map_expression(&arg);
                }
                (Vec::new(), arg)
            }
        };
        Ok(statement::Throw {
            argument,
            comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        })
    })?;
    Ok(statement::Statement::new(StatementInner::Throw {
        loc,
        inner: Arc::new(s),
    }))
}

fn try_statement(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, s) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TTry)?;
        let mut block = main_parser::parse_block_body(env)?;
        if peek::token(env) == &TokenKind::TCatch {
            comment_attachment::block_remove_trailing(env, &mut block.1);
        }
        let handler = match peek::token(env) {
            TokenKind::TCatch => {
                let (loc, mut catch) = with_loc(None, env, |env| {
                    let leading = peek::comments(env);
                    expect::token(env, TokenKind::TCatch)?;
                    let trailing = eat::trailing_comments(env);
                    let param = if peek::token(env) == &TokenKind::TLparen {
                        expect::token(env, TokenKind::TLparen)?;
                        let p = Some(pattern_parser::pattern(
                            env,
                            false,
                            ParseError::StrictCatchVariable,
                        )?);
                        expect::token(env, TokenKind::TRparen)?;
                        p
                    } else {
                        None
                    };
                    let mut body = main_parser::parse_block_body(env)?;
                    if !matches!(peek::token(env), TokenKind::TFinally) {
                        if let Some(mut remover) = statement_end_trailing_comments(env).remover {
                            body.1 = remover.map_block(&body.0, &body.1);
                        };
                    }
                    Ok(statement::try_::CatchClause {
                        loc: LOC_NONE,
                        param,
                        body,
                        comments: ast_utils::mk_comments_opt(
                            Some(leading.into()),
                            Some(trailing.into()),
                        ),
                    })
                })?;
                catch.loc = loc;
                Some(catch)
            }
            _ => None,
        };
        let finalizer = match peek::token(env) {
            TokenKind::TFinally => {
                expect::token(env, TokenKind::TFinally)?;
                let mut block_body = main_parser::parse_block_body(env)?;
                if let Some(mut remover) = statement_end_trailing_comments(env).remover {
                    block_body.1 = remover.map_block(&block_body.0, &block_body.1);
                };
                Some(block_body)
            }
            _ => None,
        };
        // No catch or finally? That's an error!
        if handler.is_none() && finalizer.is_none() {
            env.error_at(block.0.dupe(), ParseError::NoCatchOrFinally)?;
        }
        Ok(statement::Try {
            block,
            handler,
            finalizer,
            comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
        })
    })?;
    Ok(statement::Statement::new(StatementInner::Try {
        loc,
        inner: Arc::new(s),
    }))
}

fn var_statement(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, s) = with_loc(None, env, |env| {
        let kind = VariableKind::Var;
        let (mut declarations, leading, errs) = declaration_parser::parse_var(env, true)?;
        let trailing = variable_declaration_end(env, kind, &mut declarations)?;
        for err in errs {
            env.error_at(err.0, err.1)?;
        }
        Ok(statement::VariableDeclaration {
            kind,
            declarations: declarations.into(),
            comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        })
    })?;
    Ok(statement::Statement::new(
        StatementInner::VariableDeclaration {
            loc,
            inner: Arc::new(s),
        },
    ))
}

fn const_statement_after_const(
    env: &mut ParserEnv,
    start_loc: Loc,
    leading: Vec<Comment<Loc>>,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, s) = with_loc(Some(start_loc), env, |env| {
        let kind = VariableKind::Const;
        let (mut declarations, leading, errs) =
            declaration_parser::parse_const_after_const(env, leading, true)?;
        let trailing = variable_declaration_end(env, kind, &mut declarations)?;
        for err in errs {
            env.error_at(err.0, err.1)?;
        }
        Ok(statement::VariableDeclaration {
            kind,
            declarations: declarations.into(),
            comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        })
    })?;
    Ok(statement::Statement::new(
        StatementInner::VariableDeclaration {
            loc,
            inner: Arc::new(s),
        },
    ))
}

fn let_statement(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, s) = with_loc(None, env, |env| {
        let kind = VariableKind::Let;
        let (mut declarations, leading, errs) = declaration_parser::parse_let(env, true)?;
        let trailing = variable_declaration_end(env, kind, &mut declarations)?;
        for err in errs {
            env.error_at(err.0, err.1)?;
        }
        Ok(statement::VariableDeclaration {
            kind,
            declarations: declarations.into(),
            comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        })
    })?;
    Ok(statement::Statement::new(
        StatementInner::VariableDeclaration {
            loc,
            inner: Arc::new(s),
        },
    ))
}

fn while_statement(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, s) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TWhile)?;
        let leading = {
            let mut l = leading;
            l.extend(peek::comments(env));
            l
        };
        expect::token(env, TokenKind::TLparen)?;
        let test = main_parser::parse_expression(env)?;
        expect::token(env, TokenKind::TRparen)?;
        let body = env.with_in_loop(true, |env| parse_statement(env, true))?;
        // Annex B allows labelled FunctionDeclarations in non-strict mode
        // (see sec-labelled-function-declarations), but not in IterationStatement
        // (see sec-semantics-static-semantics-early-errors).
        if !env.in_strict_mode() && is_labelled_function(&body) {
            env.function_as_statement_error_at(body.loc().dupe())?;
        }
        Ok(statement::While {
            test,
            body,
            comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
        })
    })?;
    Ok(statement::Statement::new(StatementInner::While {
        loc,
        inner: Arc::new(s),
    }))
}

fn with_statement(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, stmt) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TWith)?;
        let leading = {
            let mut l = leading;
            l.extend(peek::comments(env));
            l
        };
        expect::token(env, TokenKind::TLparen)?;
        let object = main_parser::parse_expression(env)?;
        expect::token(env, TokenKind::TRparen)?;
        let body = parse_statement(env, true)?;
        // Annex B allows labelled FunctionDeclarations in non-strict mode
        // (see sec-labelled-function-declarations), but not in WithStatement
        // (see sec-with-statement-static-semantics-early-errors).
        if !env.in_strict_mode() && is_labelled_function(&body) {
            env.function_as_statement_error_at(body.loc().dupe())?;
        }
        Ok(statement::With {
            object,
            body,
            comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
        })
    })?;
    env.strict_error_at((loc.dupe(), ParseError::StrictModeWith))?;
    Ok(statement::Statement::new(StatementInner::With {
        loc,
        inner: Arc::new(stmt),
    }))
}

fn block(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, mut block) = main_parser::parse_block_body(env)?;
    let result = statement_end_trailing_comments(env);
    if let Some(mut remover) = result.remover {
        block = remover.map_block(&loc, &block);
    }
    Ok(statement::Statement::new(StatementInner::Block {
        loc,
        inner: Arc::new(block),
    }))
}

fn maybe_labeled(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, mut inner) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        let expression = main_parser::parse_expression(env)?;
        let token = peek::token(env);
        match (&*expression, token) {
            (ExpressionInner::Identifier { loc, inner: label }, TokenKind::TColon) => {
                let name = &label.name;
                expect::token(env, TokenKind::TColon)?;
                if env.in_labels(name) {
                    env.error_at(
                        loc.dupe(),
                        ParseError::Redeclaration("Label".to_string(), name.as_str().to_owned()),
                    )?;
                }
                let body = env.with_added_label(name.to_owned(), |env| {
                    // labelled FunctionDeclarations are allowed in non-strict mode
                    // (see #sec-labelled-function-declarations)
                    if peek::is_function(env) {
                        function_as_statement(env)
                    } else {
                        parse_statement(env, true)
                    }
                })?;
                Ok(StatementInner::Labeled {
                    loc: LOC_NONE,
                    inner: Arc::new(statement::Labeled {
                        label: label.dupe(),
                        body,
                        comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                    }),
                })
            }
            (_, _) => {
                let mut expression = expression;
                let trailing =
                    match semicolon(env, Some("the end of an expression statement (`;`)"), true)? {
                        SemicolonType::Explicit(comments) => comments,
                        SemicolonType::Implicit(result) => {
                            if let Some(mut remover) = result.remover {
                                expression = remover.map_expression(&expression);
                            }
                            Vec::new()
                        }
                    };
                Ok(StatementInner::Expression {
                    loc: LOC_NONE,
                    inner: Arc::new(statement::Expression {
                        expression,
                        directive: None,
                        comments: ast_utils::mk_comments_opt(None, Some(trailing.into())),
                    }),
                })
            }
        }
    })?;
    *inner.loc_mut() = loc;
    Ok(statement::Statement::new(inner))
}

fn expression(
    env: &mut ParserEnv,
    allow_sequence: bool,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, s) = with_loc(None, env, |env| {
        let mut expression = if allow_sequence {
            main_parser::parse_expression(env)?
        } else {
            expression_parser::assignment(env)?
        };
        let trailing = match semicolon(env, Some("the end of an expression statement (`;`)"), true)?
        {
            SemicolonType::Explicit(comments) => comments,
            SemicolonType::Implicit(result) => {
                if let Some(mut remover) = result.remover {
                    expression = remover.map_expression(&expression);
                }
                Vec::new()
            }
        };
        let directive = if env.allow_directive() {
            match &*expression {
                ExpressionInner::StringLiteral { loc: _, inner } => {
                    let raw = &inner.raw;
                    // the parser may recover from errors and generate unclosed strings, where
                    // the opening quote should be reliable but the closing one might not exist.
                    // be defensive.
                    if raw.len() > 1 && raw.chars().next() == raw.chars().last() {
                        Some(raw[1..raw.len() - 1].to_string())
                    } else {
                        None
                    }
                }
                _ => None,
            }
        } else {
            None
        };
        Ok(statement::Expression {
            expression,
            directive,
            comments: ast_utils::mk_comments_opt(None, Some(trailing.into())),
        })
    })?;
    Ok(statement::Statement::new(StatementInner::Expression {
        loc,
        inner: Arc::new(s),
    }))
}

fn type_alias_helper(
    env: &mut ParserEnv,
    leading: Vec<Comment<Loc>>,
) -> Result<statement::TypeAlias<Loc, Loc>, Rollback> {
    if !env.should_parse_types() {
        env.error(ParseError::UnexpectedTypeAlias)?;
    }
    let leading = {
        let mut l = leading;
        l.extend(peek::comments(env));
        l
    };
    expect::token(env, TokenKind::TType)?;
    type_alias_helper_after_type(env, leading)
}

fn type_alias_helper_after_type(
    env: &mut ParserEnv,
    leading: Vec<Comment<Loc>>,
) -> Result<statement::TypeAlias<Loc, Loc>, Rollback> {
    eat::push_lex_mode(env, LexMode::Type);
    let id = {
        let mut id = type_parser::type_identifier(env)?;
        if peek::token(env) == &TokenKind::TLessThan {
            crate::comment_attachment::id_remove_trailing(env, &mut id)
        }
        id
    };
    let tparams = type_parser::parse_type_params(env)?;
    expect::token(env, TokenKind::TAssign)?;
    let right = type_parser::parse_type(env)?;
    eat::pop_lex_mode(env);
    let (trailing, right) = match semicolon(env, None, true)? {
        SemicolonType::Explicit(comments) => (comments, right),
        SemicolonType::Implicit(result) => {
            let mut r = right;
            if let Some(mut remover) = result.remover {
                r = remover.map_type_(&r);
            }
            (Vec::new(), r)
        }
    };

    Ok(statement::TypeAlias {
        id,
        tparams,
        right,
        comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
    })
}

fn declare_type_alias(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, s) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TDeclare)?;
        let type_alias = type_alias_helper(env, leading)?;
        Ok(type_alias)
    })?;
    Ok(statement::Statement::new(
        StatementInner::DeclareTypeAlias {
            loc,
            inner: Arc::new(s),
        },
    ))
}

/// Type aliases squeeze into an unambiguous unused portion of the grammar: `type` is not a
/// reserved word, so `type T` is otherwise two identifiers in a row and that's never valid JS.
/// However, if there's a line separator between the two, ASI makes it valid JS, so line
/// separators are disallowed.
fn type_alias(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    if peek::current_token_is_followed_by_identifier(env)
        && !peek::current_token_is_followed_by_implicit_semicolon(env)
    {
        let (loc, type_alias) = with_loc(None, env, |env| type_alias_helper(env, Vec::new()))?;
        Ok(statement::Statement::new(StatementInner::TypeAlias {
            loc,
            inner: Arc::new(type_alias),
        }))
    } else {
        let stmt = parse_statement(env, true)?;
        Ok(stmt)
    }
}

fn opaque_type_helper(
    env: &mut ParserEnv,
    declare: bool,
    leading: Vec<Comment<Loc>>,
) -> Result<statement::OpaqueType<Loc, Loc>, Rollback> {
    if !env.should_parse_types() {
        env.error(ParseError::UnexpectedOpaqueTypeAlias)?;
    }
    let leading_opaque = {
        let mut l = leading;
        l.extend(peek::comments(env));
        l
    };
    expect::token(env, TokenKind::TOpaque)?;
    let leading_type = peek::comments(env);
    expect::token(env, TokenKind::TType)?;
    let leading = {
        let mut l = leading_opaque;
        l.extend(leading_type);
        l
    };
    eat::push_lex_mode(env, LexMode::Type);
    let mut id = {
        let mut id = type_parser::type_identifier(env)?;
        if peek::token(env) == &TokenKind::TLessThan {
            comment_attachment::id_remove_trailing(env, &mut id)
        }
        id
    };
    let tparams = type_parser::parse_type_params(env)?;

    // Helper function to check for super/extends
    let expect_opt_super_or_extends =
        |env: &mut ParserEnv, token: TokenKind| -> Result<bool, Rollback> {
            // super and extends are treated like identifiers in type parsing context.
            // We need to switch back to normal mode in order to correctly identify them.
            eat::push_lex_mode(env, LexMode::Normal);
            let consumed = if peek::token(env) == &token {
                expect::token(env, token)?;
                true
            } else {
                false
            };
            eat::pop_lex_mode(env);
            Ok(consumed)
        };
    let lower_bound = if expect_opt_super_or_extends(env, TokenKind::TSuper)? {
        // Do not accept conditional type in this position. Otherwise, the sequence
        // `opaque type A super B extends C = D` will fail because `B extends C` are parsed
        // together as conditional type.
        Some(type_parser::parse_union_no_wrap(env)?)
    } else {
        None
    };
    let upper_bound = if expect_opt_super_or_extends(env, TokenKind::TExtends)? {
        Some(type_parser::parse_type(env)?)
    } else {
        None
    };
    let legacy_upper_bound = if upper_bound.is_none() && lower_bound.is_none() {
        match peek::token(env) {
            TokenKind::TColon => {
                expect::token(env, TokenKind::TColon)?;
                Some(type_parser::parse_type(env)?)
            }
            _ => None,
        }
    } else {
        None
    };

    let impl_type = if declare {
        match peek::token(env) {
            TokenKind::TAssign => {
                env.error(ParseError::DeclareOpaqueTypeInitializer)?;
                eat::token(env)?;
                if peek::token(env) == &TokenKind::TSemicolon || peek::is_implicit_semicolon(env) {
                    None
                } else {
                    Some(type_parser::parse_type(env)?)
                }
            }
            _ => None,
        }
    } else {
        expect::token(env, TokenKind::TAssign)?;
        Some(type_parser::parse_type(env)?)
    };

    eat::pop_lex_mode(env);

    let (trailing, id, tparams, lower_bound, upper_bound, legacy_upper_bound, impl_type) = match (
        semicolon(env, None, true)?,
        tparams,
        lower_bound,
        upper_bound,
        legacy_upper_bound,
        impl_type,
    ) {
        // opaque type Foo = Bar;
        (SemicolonType::Explicit(comments), tp, lb, ub, lub, it) => {
            (comments, id, tp, lb, ub, lub, it)
        }
        // opaque type Foo = Bar
        (SemicolonType::Implicit(result), tp, lb, ub, lub, Some(impl_)) => {
            let mut impl_mapped = impl_;
            if let Some(mut remover) = result.remover {
                impl_mapped = remover.map_type_(&impl_mapped);
            }
            (Vec::new(), id, tp, lb, ub, lub, Some(impl_mapped))
        }
        // opaque type Foo: U
        (SemicolonType::Implicit(result), tp, lb, Some(upper_bound), lub, None) => {
            let mut ub_mapped = upper_bound;
            if let Some(mut remover) = result.remover {
                ub_mapped = remover.map_type_(&ub_mapped);
            }
            (Vec::new(), id, tp, lb, Some(ub_mapped), lub, None)
        }
        // opaque type Foo: Legacy
        (SemicolonType::Implicit(result), tp, lb, ub, Some(legacy_upper_bound), None) => {
            let mut lub_mapped = legacy_upper_bound;
            if let Some(mut remover) = result.remover {
                lub_mapped = remover.map_type_(&lub_mapped);
            }
            (Vec::new(), id, tp, lb, ub, Some(lub_mapped), None)
        }
        // opaque type Foo super Super
        (SemicolonType::Implicit(result), tp, Some(lower_bound), None, None, None) => {
            let mut lb_mapped = lower_bound;
            if let Some(mut remover) = result.remover {
                lb_mapped = remover.map_type_(&lb_mapped);
            }
            (Vec::new(), id, tp, Some(lb_mapped), None, None, None)
        }
        // opaque type Foo<T>
        (SemicolonType::Implicit(result), Some(mut tparams), None, None, None, None) => {
            if let Some(mut remover) = result.remover {
                tparams = remover.map_type_params(&TypeParamsContext::OpaqueType, &tparams);
            }
            (Vec::new(), id, Some(tparams), None, None, None, None)
        }
        // declare opaque type Foo
        (SemicolonType::Implicit(result), None, None, None, None, None) => {
            if let Some(mut remover) = result.remover {
                id = remover.map_identifier(&id);
            }
            (Vec::new(), id, None, None, None, None, None)
        }
    };

    Ok(statement::OpaqueType {
        id,
        tparams,
        impl_type,
        lower_bound,
        upper_bound,
        legacy_upper_bound,
        comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
    })
}

fn declare_opaque_type(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, s) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TDeclare)?;
        let opaque_t = opaque_type_helper(env, true, leading)?;
        Ok(opaque_t)
    })?;
    Ok(statement::Statement::new(
        StatementInner::DeclareOpaqueType {
            loc,
            inner: Arc::new(s),
        },
    ))
}

fn opaque_type(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    if peek::opaque_token_is_followed_by_type(env) {
        let (loc, opaque_t) =
            with_loc(None, env, |env| opaque_type_helper(env, false, Vec::new()))?;
        Ok(statement::Statement::new(StatementInner::OpaqueType {
            loc,
            inner: Arc::new(opaque_t),
        }))
    } else {
        parse_statement(env, true)
    }
}

fn interface_helper(
    env: &mut ParserEnv,
    leading: Vec<Comment<Loc>>,
) -> Result<statement::Interface<Loc, Loc>, Rollback> {
    if !env.should_parse_types() {
        env.error(ParseError::UnexpectedTypeInterface)?;
    }
    let leading = {
        let mut l = leading;
        l.extend(peek::comments(env));
        l
    };
    expect::token(env, TokenKind::TInterface)?;
    let id = {
        let id = crate::type_parser::type_identifier(env)?;
        if peek::token(env) == &TokenKind::TExtends {
            id
        } else {
            let mut id_mut = id;
            comment_attachment::id_remove_trailing(env, &mut id_mut);
            id_mut
        }
    };
    let tparams = {
        let mut tparams_opt = type_parser::parse_type_params(env)?;
        if peek::token(env) != &TokenKind::TExtends {
            comment_attachment::type_params_remove_trailing(env, tparams_opt.as_mut());
        }
        tparams_opt
    };
    let (extends, mut body) = type_parser::parse_interface_helper(env)?;
    let result = statement_end_trailing_comments(env);
    if let Some(mut remover) = result.remover {
        body.1 = remover.map_object_type(&body.0, &body.1);
    }

    Ok(statement::Interface {
        id,
        tparams,
        body,
        extends: extends.into(),
        comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
    })
}

fn declare_interface(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, s) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TDeclare)?;
        let iface = interface_helper(env, leading)?;
        Ok(iface)
    })?;
    Ok(statement::Statement::new(
        StatementInner::DeclareInterface {
            loc,
            inner: Arc::new(s),
        },
    ))
}

/// Disambiguate between a value named `interface`, like `var interface = 1; interface++`,
/// and an interface declaration like `interface Foo {}`.
fn interface(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    if peek::current_token_is_followed_by_identifier_name(env) {
        let (loc, iface) = with_loc(None, env, |env| interface_helper(env, Vec::new()))?;
        Ok(statement::Statement::new(
            StatementInner::InterfaceDeclaration {
                loc,
                inner: Arc::new(iface),
            },
        ))
    } else {
        expression(env, true)
    }
}

fn declare_class_helper(
    env: &mut ParserEnv,
    abstract_: bool,
    leading: Vec<Comment<Loc>>,
) -> Result<statement::DeclareClass<Loc, Loc>, Rollback> {
    fn mixins(
        env: &mut ParserEnv,
    ) -> Result<Vec<(Loc, crate::ast::types::Generic<Loc, Loc>)>, Rollback> {
        let mut acc = Vec::new();
        loop {
            let super_generic = type_parser::parse_generic(env)?;
            acc.push(super_generic);
            match peek::token(env) {
                TokenKind::TComma => {
                    expect::token(env, TokenKind::TComma)?;
                }
                _ => return Ok(acc),
            }
        }
    }

    fn parse_extends(
        env: &mut ParserEnv,
    ) -> Result<statement::DeclareClassExtends<Loc, Loc>, Rollback> {
        let generic = type_parser::parse_generic(env)?;
        match peek::token(env) {
            TokenKind::TLparen => {
                expect::token(env, TokenKind::TLparen)?;
                let arg = with_loc(None, env, parse_extends)?;
                expect::token(env, TokenKind::TRparen)?;
                Ok(statement::DeclareClassExtends::ExtendsCall {
                    callee: generic,
                    arg: Box::new(arg),
                })
            }
            _ => Ok(statement::DeclareClassExtends::ExtendsIdent(generic.1)),
        }
    }

    // This is identical to `interface`, except that mixins are allowed
    env.with_strict(true, |env| {
        let leading = {
            let mut l = leading;
            l.extend(peek::comments(env));
            l
        };
        expect::token(env, TokenKind::TClass)?;
        let id = {
            let mut id = main_parser::parse_identifier(env, None)?;
            match peek::token(env) {
                TokenKind::TLessThan | TokenKind::TLcurly => {
                    comment_attachment::id_remove_trailing(env, &mut id);
                    id
                }
                _ => id,
            }
        };
        let tparams = {
            let mut tparams_opt = type_parser::parse_type_params(env)?;
            if peek::token(env) == &TokenKind::TLcurly {
                comment_attachment::type_params_remove_trailing(env, tparams_opt.as_mut());
            }
            tparams_opt
        };
        let extends = if eat::maybe(env, TokenKind::TExtends)? {
            Some(with_loc(None, env, parse_extends)?)
        } else {
            None
        };
        let mixins = match peek::token(env) {
            TokenKind::TIdentifier { raw, .. } if raw == "mixins" => {
                eat::token(env)?;
                let mut mixins = mixins(env)?;
                if peek::token(env) == &TokenKind::TLcurly {
                    comment_attachment::generic_type_list_remove_trailing(env, &mut mixins);
                }
                mixins
            }
            _ => Vec::new(),
        };
        let implements = match peek::token(env) {
            TokenKind::TImplements => {
                let mut implements = object_parser::class_implements(env, false)?;
                if peek::token(env) == &TokenKind::TLcurly {
                    comment_attachment::class_implements_remove_trailing(env, &mut implements);
                }
                Some(implements)
            }
            _ => None,
        };
        let mut body = type_parser::parse_object_type(env, true)?;
        let result = statement_end_trailing_comments(env);
        if let Some(mut remover) = result.remover {
            body.1 = remover.map_object_type(&body.0, &body.1);
        }
        let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
        Ok(statement::DeclareClass {
            id,
            tparams,
            body,
            extends,
            mixins: mixins.into(),
            implements,
            abstract_,
            comments,
        })
    })
}

fn declare_class_statement(
    env: &mut ParserEnv,
    abstract_: bool,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, s) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TDeclare)?;
        if abstract_ {
            expect::identifier(env, "abstract")?;
        }
        declare_class_helper(env, abstract_, leading)
    })?;
    Ok(statement::Statement::new(StatementInner::DeclareClass {
        loc,
        inner: Arc::new(s),
    }))
}

fn declare_component(
    env: &mut ParserEnv,
    leading: Vec<Comment<Loc>>,
) -> Result<statement::DeclareComponent<Loc, Loc>, Rollback> {
    let leading = {
        let mut l = leading;
        l.extend(peek::comments(env));
        l
    };
    expect::identifier(env, "component")?;
    // Components should have at least the same strictness as functions
    let mut id = main_parser::parse_identifier(env, Some(ParseError::StrictFunctionName))?;
    comment_attachment::id_remove_trailing(env, &mut id);
    let mut tparams = type_parser::parse_type_params(env)?;
    comment_attachment::type_params_remove_trailing(env, tparams.as_mut());
    let params = declaration_parser::parse_component_params(env)?;
    let mut renders = if peek::is_renders_ident(env) {
        let mut renders = type_parser::parse_renders_annotation_opt(env)?;
        comment_attachment::component_renders_annotation_remove_trailing(env, &mut renders);
        renders
    } else {
        type_parser::parse_renders_annotation_opt(env)?
    };

    let trailing = match semicolon(env, None, true)? {
        SemicolonType::Explicit(comments) => comments,
        SemicolonType::Implicit(result) => {
            if let Some(mut remover) = result.remover {
                renders = remover.map_component_renders_annotation(&renders);
            }
            Vec::new()
        }
    };

    Ok(statement::DeclareComponent {
        id,
        params,
        renders,
        tparams,
        comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
    })
}

fn declare_component_statement(
    env: &mut ParserEnv,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, component) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TDeclare)?;
        declare_component(env, leading)
    })?;
    Ok(statement::Statement::new(
        StatementInner::DeclareComponent {
            loc,
            inner: Arc::new(component),
        },
    ))
}

fn declare_async_statement(
    env: &mut ParserEnv,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, inner) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TDeclare)?;
        // Now T_ASYNC is at position 0 and we can peek at position 1
        let is_component =
            env.parse_options().components && peek::async_token_is_followed_by_component(env);
        let is_hook = env.parse_options().components && peek::async_token_is_followed_by_hook(env);
        if is_component {
            env.error(ParseError::DeclareAsyncComponent)?;
            let leading_async = peek::comments(env);
            eat::token(env)?;
            let mut leading = leading;
            leading.extend(leading_async);
            let component = declare_component(env, leading)?;
            Ok(DeclareAsyncResult::Component(component))
        } else if is_hook {
            env.error(ParseError::DeclareAsyncHook)?;
            expect::token(env, TokenKind::TAsync)?;
            let fn_decl = declare_function(env, false, leading)?;
            Ok(DeclareAsyncResult::Function(fn_decl))
        } else {
            env.error(ParseError::DeclareAsync)?;
            expect::token(env, TokenKind::TAsync)?;
            let fn_decl = declare_function(env, true, leading)?;
            Ok(DeclareAsyncResult::Function(fn_decl))
        }
    })?;
    match inner {
        DeclareAsyncResult::Component(component) => Ok(statement::Statement::new(
            StatementInner::DeclareComponent {
                loc,
                inner: Arc::new(component),
            },
        )),
        DeclareAsyncResult::Function(fn_decl) => {
            Ok(statement::Statement::new(StatementInner::DeclareFunction {
                loc,
                inner: Arc::new(fn_decl),
            }))
        }
    }
}

fn declare_enum(
    env: &mut ParserEnv,
    const_: bool,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, enum_decl) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TDeclare)?;
        if const_ {
            expect::token(env, TokenKind::TConst)?;
        }
        enum_parser::declaration(leading, const_, env)
    })?;
    Ok(statement::Statement::new(StatementInner::DeclareEnum {
        loc,
        inner: Arc::new(enum_decl),
    }))
}

/// Handles `declare const` - could be `declare const enum` or `declare const var`
fn declare_const_or_enum(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    enum DeclarationKind {
        Enum(statement::EnumDeclaration<Loc, Loc>),
        Variable(statement::DeclareVariable<Loc, Loc>),
    }

    let (loc, kind) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TDeclare)?;
        let parse_enums = env.parse_options().enums;
        if parse_enums && peek::current_token_is_followed_by_enum(env) {
            expect::token(env, TokenKind::TConst)?;
            let enum_decl = enum_parser::declaration(leading, true, env)?;
            Ok(DeclarationKind::Enum(enum_decl))
        } else {
            let var_decl = declare_var(env, VariableKind::Const, leading)?;
            Ok(DeclarationKind::Variable(var_decl))
        }
    })?;

    Ok(match kind {
        DeclarationKind::Enum(enum_decl) => {
            statement::Statement::new(StatementInner::DeclareEnum {
                loc,
                inner: Arc::new(enum_decl),
            })
        }
        DeclarationKind::Variable(var_decl) => {
            statement::Statement::new(StatementInner::DeclareVariable {
                loc,
                inner: Arc::new(var_decl),
            })
        }
    })
}

fn declare_function(
    env: &mut ParserEnv,
    _async_: bool,
    leading: Vec<Comment<Loc>>,
) -> Result<statement::DeclareFunction<Loc, Loc>, Rollback> {
    let leading = {
        let mut l = leading;
        l.extend(peek::comments(env));
        l
    };
    let effect = match peek::token(env) {
        TokenKind::TFunction => {
            eat::token(env)?;
            function::Effect::Arbitrary
        }
        TokenKind::TIdentifier { raw, .. } if raw == "hook" => {
            eat::token(env)?;
            function::Effect::Hook
        }
        t => {
            let kind = t.clone();
            expect::error(env, &kind)?;
            function::Effect::Arbitrary
        }
    };
    let mut id = main_parser::parse_identifier(env, None)?;
    comment_attachment::id_remove_trailing(env, &mut id);
    let mut annot = {
        let (loc, func) = with_loc(None, env, |env| {
            let mut tparams = type_parser::parse_type_params(env)?;
            comment_attachment::type_params_remove_trailing(env, tparams.as_mut());
            let params = type_parser::parse_function_param_list(env)?;
            expect::token(env, TokenKind::TColon)?;
            eat::push_lex_mode(env, LexMode::Type);
            let return_ = if is_start_of_type_guard(env) && effect != function::Effect::Hook {
                let guard = type_parser::parse_type_guard(env)?;
                types::function::ReturnAnnotation::TypeGuard(guard)
            } else {
                let return_type = type_parser::parse_type(env)?;
                let has_predicate = peek::token(env) == &TokenKind::TChecks;
                if has_predicate && effect != function::Effect::Hook {
                    let mut rt = return_type;
                    comment_attachment::type_remove_trailing(env, &mut rt);
                    types::function::ReturnAnnotation::Available(types::Annotation {
                        loc: rt.loc().dupe(),
                        annotation: rt,
                    })
                } else {
                    types::function::ReturnAnnotation::Available(types::Annotation {
                        loc: return_type.loc().dupe(),
                        annotation: return_type,
                    })
                }
            };
            eat::pop_lex_mode(env);
            Ok(types::Function {
                params,
                return_,
                tparams,
                comments: None,
                effect,
            })
        })?;
        types::Type::new(types::TypeInner::Function {
            loc,
            inner: Arc::new(func),
        })
    };
    let mut predicate = type_parser::parse_predicate_opt(env)?;
    let trailing = match (semicolon(env, None, true)?, predicate.as_mut()) {
        (SemicolonType::Explicit(comments), _) => comments,
        (SemicolonType::Implicit(result), None) => {
            if let Some(mut remover) = result.remover {
                annot = remover.map_type_(&annot);
            }
            Vec::new()
        }
        (SemicolonType::Implicit(result), Some(pred)) => {
            if let Some(mut remover) = result.remover {
                *pred = remover.map_predicate(pred);
            }
            Vec::new()
        }
    };
    Ok(statement::DeclareFunction {
        id: Some(id),
        annot: types::Annotation {
            loc: annot.loc().dupe(),
            annotation: annot,
        },
        predicate,
        comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        implicit_declare: false,
    })
}

fn declare_function_statement(
    env: &mut ParserEnv,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, fn_decl) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TDeclare)?;
        let async_ = match peek::token(env) {
            TokenKind::TAsync => {
                env.error(ParseError::DeclareAsync)?;
                expect::token(env, TokenKind::TAsync)?;
                true
            }
            _ => false,
        };
        declare_function(env, async_, leading)
    })?;
    Ok(statement::Statement::new(StatementInner::DeclareFunction {
        loc,
        inner: Arc::new(fn_decl),
    }))
}

fn declare_var(
    env: &mut ParserEnv,
    kind: VariableKind,
    leading: Vec<Comment<Loc>>,
) -> Result<statement::DeclareVariable<Loc, Loc>, Rollback> {
    let leading = {
        let mut l = leading;
        l.extend(peek::comments(env));
        l
    };
    match kind {
        VariableKind::Var => expect::token(env, TokenKind::TVar)?,
        VariableKind::Let => expect::token(env, TokenKind::TLet)?,
        VariableKind::Const => expect::token(env, TokenKind::TConst)?,
    };
    let (mut declarations, errs) = declaration_parser::variable_declaration_list(env, false)?;
    for err in errs {
        env.error_at(err.0, err.1)?;
    }
    let trailing = variable_declaration_end(env, kind, &mut declarations)?;
    Ok(statement::DeclareVariable {
        declarations: declarations.into(),
        kind,
        comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
    })
}

fn declare_var_statement(
    env: &mut ParserEnv,
    kind: VariableKind,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, var_decl) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TDeclare)?;
        declare_var(env, kind, leading)
    })?;
    Ok(statement::Statement::new(StatementInner::DeclareVariable {
        loc,
        inner: Arc::new(var_decl),
    }))
}

fn declare_module_or_namespace_body(
    env: &mut ParserEnv,
) -> Result<(Loc, statement::Block<Loc, Loc>), Rollback> {
    with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TLcurly)?;
        env.with_ambient_context(true, |env| {
            let body = parse_module_body(env, |token: &TokenKind| token == &TokenKind::TRcurly)?;
            let internal = if body.is_empty() {
                peek::comments(env)
            } else {
                Vec::new()
            };
            expect::token(env, TokenKind::TRcurly)?;
            let result = statement_end_trailing_comments(env);
            let trailing = result.trailing;
            let comments = ast_utils::mk_comments_with_internal_opt(
                Some(leading.into()),
                Some(trailing.into()),
                if internal.is_empty() {
                    None
                } else {
                    Some(internal.into())
                },
            );
            Ok(statement::Block {
                body: body.into(),
                comments,
            })
        })
    })
}

fn declare_module(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    fn declare_module_helper(
        env: &mut ParserEnv,
        leading: Vec<Comment<Loc>>,
    ) -> Result<statement::DeclareModule<Loc, Loc>, Rollback> {
        let id = match peek::token(env) {
            TokenKind::TString(loc, value, raw, octal) => {
                let loc = loc.dupe();
                let value = value.dupe();
                let raw = raw.dupe();
                let octal = *octal;
                let (lit_loc, mut lit) = string_literal(env, loc, value, raw, octal)?;
                comment_attachment::string_literal_remove_trailing(env, &mut lit);
                statement::declare_module::Id::Literal((lit_loc, lit))
            }
            _ => {
                let mut id = main_parser::parse_identifier(env, None)?;
                comment_attachment::id_remove_trailing(env, &mut id);
                statement::declare_module::Id::Identifier(id)
            }
        };
        if peek::token(env) == &TokenKind::TSemicolon
            || (peek::token(env) != &TokenKind::TLcurly && peek::is_implicit_semicolon(env))
        {
            // Shorthand ambient module: `declare module 'name'` — desugar to empty body.
            let trailing = match semicolon(env, None, false)? {
                SemicolonType::Explicit(trailing) => trailing,
                SemicolonType::Implicit(result) => result.trailing,
            };
            let comments = ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into()));
            let body_loc = match env.last_loc() {
                Some(loc) => loc.end_loc(),
                None => peek::loc(env).start_loc(),
            };
            let body = (
                body_loc,
                statement::Block {
                    body: Vec::new().into(),
                    comments: None,
                },
            );
            Ok(statement::DeclareModule { id, body, comments })
        } else {
            let body = declare_module_or_namespace_body(env)?;
            let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
            Ok(statement::DeclareModule { id, body, comments })
        }
    }
    let start_loc = peek::loc(env).dupe();
    let leading = peek::comments(env);
    expect::token(env, TokenKind::TDeclare)?;
    let leading = {
        let mut l = leading;
        l.extend(peek::comments(env));
        l
    };
    expect::identifier(env, "module")?;
    if peek::token(env) == &TokenKind::TPeriod {
        let module_exports = declare_module_exports(env, start_loc, leading)?;
        Ok(module_exports)
    } else {
        let is_d_ts = env.is_d_ts();
        match peek::token(env) {
            TokenKind::TString { .. } => {
                let (loc, module) = with_loc(Some(start_loc), env, |env| {
                    declare_module_helper(env, leading)
                })?;
                Ok(statement::Statement::new(StatementInner::DeclareModule {
                    loc,
                    inner: Arc::new(module),
                }))
            }
            _ if is_d_ts => {
                // identifier-named: `declare module Foo {}` → treat as namespace synonym in .d.ts
                let (loc, namespace) = with_loc(Some(start_loc), env, |env| {
                    let mut id = main_parser::parse_identifier(env, None)?;
                    comment_attachment::id_remove_trailing(env, &mut id);
                    let id = statement::declare_namespace::Id::Local(id);
                    let body = declare_module_or_namespace_body(env)?;
                    let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
                    Ok(statement::DeclareNamespace {
                        id,
                        body,
                        comments,
                        implicit_declare: false,
                        keyword: statement::declare_namespace::Keyword::Module,
                    })
                })?;
                Ok(statement::Statement::new(
                    StatementInner::DeclareNamespace {
                        loc,
                        inner: Arc::new(namespace),
                    },
                ))
            }
            _ => {
                let (loc, module) = with_loc(Some(start_loc), env, |env| {
                    declare_module_helper(env, leading)
                })?;
                Ok(statement::Statement::new(StatementInner::DeclareModule {
                    loc,
                    inner: Arc::new(module),
                }))
            }
        }
    }
}

fn declare_namespace(
    env: &mut ParserEnv,
    global: bool,
    implicit_declare: bool,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    if implicit_declare && !env.should_parse_types() {
        env.error(ParseError::UnexpectedTypeDeclaration)?;
    }
    fn declare_namespace_helper(
        env: &mut ParserEnv,
        leading: Vec<Comment<Loc>>,
        global: bool,
        implicit_declare: bool,
        keyword: statement::declare_namespace::Keyword,
    ) -> Result<statement::DeclareNamespace<Loc, Loc>, Rollback> {
        let mut id = main_parser::parse_identifier(env, None)?;
        comment_attachment::id_remove_trailing(env, &mut id);
        let id = if global {
            statement::declare_namespace::Id::Global(id)
        } else {
            statement::declare_namespace::Id::Local(id)
        };
        let body = declare_module_or_namespace_body(env)?;
        let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
        Ok(statement::DeclareNamespace {
            id,
            body,
            implicit_declare,
            comments,
            keyword,
        })
    }

    let start_loc = peek::loc(env).dupe();
    let leading = peek::comments(env);
    let is_d_ts = env.is_d_ts();
    if !implicit_declare {
        expect::token(env, TokenKind::TDeclare)?;
        let leading = {
            let mut l = leading;
            l.extend(peek::comments(env));
            l
        };
        if !global {
            let keyword = match peek::token(env) {
                TokenKind::TIdentifier { raw, .. } if raw == "module" && is_d_ts => {
                    expect::identifier(env, "module")?;
                    statement::declare_namespace::Keyword::Module
                }
                _ => {
                    expect::identifier(env, "namespace")?;
                    statement::declare_namespace::Keyword::Namespace
                }
            };
            let (loc, namespace) = with_loc(Some(start_loc), env, |env| {
                declare_namespace_helper(env, leading, global, implicit_declare, keyword)
            })?;
            Ok(statement::Statement::new(
                StatementInner::DeclareNamespace {
                    loc,
                    inner: Arc::new(namespace),
                },
            ))
        } else {
            let (loc, namespace) = with_loc(Some(start_loc), env, |env| {
                declare_namespace_helper(
                    env,
                    leading,
                    global,
                    implicit_declare,
                    statement::declare_namespace::Keyword::Namespace,
                )
            })?;
            Ok(statement::Statement::new(
                StatementInner::DeclareNamespace {
                    loc,
                    inner: Arc::new(namespace),
                },
            ))
        }
    } else {
        // implicit declare: namespace/module keyword already peeked, just consume it
        if !global {
            let keyword = match peek::token(env) {
                TokenKind::TIdentifier { raw, .. } if raw == "module" && is_d_ts => {
                    expect::identifier(env, "module")?;
                    statement::declare_namespace::Keyword::Module
                }
                _ => {
                    expect::identifier(env, "namespace")?;
                    statement::declare_namespace::Keyword::Namespace
                }
            };
            let leading = {
                let mut l = leading;
                l.extend(peek::comments(env));
                l
            };
            let (loc, namespace) = with_loc(Some(start_loc), env, |env| {
                declare_namespace_helper(env, leading, false, implicit_declare, keyword)
            })?;
            Ok(statement::Statement::new(
                StatementInner::DeclareNamespace {
                    loc,
                    inner: Arc::new(namespace),
                },
            ))
        } else {
            let leading = {
                let mut l = leading;
                l.extend(peek::comments(env));
                l
            };
            let (loc, namespace) = with_loc(Some(start_loc), env, |env| {
                declare_namespace_helper(
                    env,
                    leading,
                    global,
                    implicit_declare,
                    statement::declare_namespace::Keyword::Namespace,
                )
            })?;
            Ok(statement::Statement::new(
                StatementInner::DeclareNamespace {
                    loc,
                    inner: Arc::new(namespace),
                },
            ))
        }
    }
}

fn declare_module_exports(
    env: &mut ParserEnv,
    start_loc: Loc,
    leading: Vec<Comment<Loc>>,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, mut inner) = with_loc(Some(start_loc), env, |env| {
        let leading_period = peek::comments(env);
        expect::token(env, TokenKind::TPeriod)?;
        let leading_exports = peek::comments(env);
        expect::identifier(env, "exports")?;
        let leading_annot = peek::comments(env);
        let leading = {
            let mut l = leading;
            l.extend(leading_period);
            l.extend(leading_exports);
            l.extend(leading_annot);
            l
        };
        let mut annot = type_parser::parse_annotation(env)?;
        let trailing = match semicolon(env, None, true)? {
            SemicolonType::Explicit(trailing) => trailing,
            SemicolonType::Implicit(result) => {
                if let Some(mut remover) = result.remover {
                    annot = remover.map_type_annotation(&annot);
                }
                Vec::new()
            }
        };
        let comments = ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into()));
        Ok(StatementInner::DeclareModuleExports {
            loc: LOC_NONE,
            inner: Arc::new(statement::DeclareModuleExports { annot, comments }),
        })
    })?;
    *inner.loc_mut() = loc.dupe();
    Ok(statement::Statement::new(inner))
}

fn declare(
    env: &mut ParserEnv,
    in_module_or_namespace: bool,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    if !env.should_parse_types() {
        env.error(ParseError::UnexpectedTypeDeclaration)?;
    }

    let parse_components = env.parse_options().components;
    let parse_enums = env.parse_options().enums;
    let current_token_is_import = peek::token(env) == &TokenKind::TImport;
    // Eventually, just emit a wrapper AST node
    match peek::declaration_after_declare(env) {
        TokenKind::TClass => declare_class_statement(env, false),
        TokenKind::TIdentifier { raw, .. } if raw == "abstract" => {
            declare_class_statement(env, true)
        }
        TokenKind::TConst => declare_const_or_enum(env),
        TokenKind::TEnum if parse_enums => declare_enum(env, false),
        TokenKind::TInterface => declare_interface(env),
        TokenKind::TType => {
            if peek::token(env) == &TokenKind::TImport && in_module_or_namespace {
                import_declaration(env)
            } else {
                declare_type_alias(env)
            }
        }
        TokenKind::TOpaque => declare_opaque_type(env),
        TokenKind::TTypeof if current_token_is_import => import_declaration(env),
        TokenKind::TFunction => declare_function_statement(env),
        TokenKind::TAsync => declare_async_statement(env),
        TokenKind::TIdentifier { raw, .. } if raw == "hook" && parse_components => {
            declare_function_statement(env)
        }
        TokenKind::TVar => declare_var_statement(env, VariableKind::Var),
        TokenKind::TLet => declare_var_statement(env, VariableKind::Let),
        TokenKind::TExport if in_module_or_namespace => declare_export_declaration(env),
        TokenKind::TIdentifier { raw, .. } if raw == "module" => declare_module(env),
        TokenKind::TIdentifier { raw, .. } if raw == "global" => {
            declare_namespace(env, true, false)
        }
        TokenKind::TIdentifier { raw, .. } if raw == "namespace" => {
            declare_namespace(env, false, false)
        }
        TokenKind::TIdentifier { raw, .. } if raw == "component" && parse_components => {
            declare_component_statement(env)
        }
        _ if in_module_or_namespace => {
            if current_token_is_import {
                import_declaration(env)
            } else {
                // Oh boy, found some bad stuff in a declare module. Let's just
                // pretend it's a declare var (arbitrary choice)
                declare_var_statement(env, VariableKind::Var)
            }
        }
        _ => parse_statement(env, true),
    }
}

fn export_source(env: &mut ParserEnv) -> Result<(Loc, StringLiteral<Loc>), Rollback> {
    expect::identifier(env, "from")?;
    match peek::token(env) {
        TokenKind::TString(loc, value, raw, octal) => {
            let loc = loc.dupe();
            let value = value.dupe();
            let raw = raw.dupe();
            let octal = *octal;
            string_literal(env, loc, value, raw, octal)
        }
        _ => {
            // Just make up a string for the error case
            let loc = peek::loc(env).dupe();
            env.error_unexpected(Some("a string".to_owned()))?;
            Ok((
                loc,
                StringLiteral {
                    value: FlowSmolStr::new_inline(""),
                    raw: FlowSmolStr::new_inline(""),
                    comments: None,
                },
            ))
        }
    }
}

fn export_source_and_semicolon(
    env: &mut ParserEnv,
) -> Result<((Loc, StringLiteral<Loc>), Vec<Comment<Loc>>), Rollback> {
    let (source_loc, mut source) = export_source(env)?;
    match semicolon(env, None, true)? {
        SemicolonType::Explicit(trailing) => Ok(((source_loc, source), trailing)),
        SemicolonType::Implicit(result) => {
            if let Some(mut remover) = result.remover {
                source = remover.map_string_literal(&source);
            }
            Ok(((source_loc, source), Vec::new()))
        }
    }
}

fn export_specifiers(
    env: &mut ParserEnv,
) -> Result<Vec<statement::export_named_declaration::ExportSpecifier<Loc, Loc>>, Rollback> {
    let mut specifiers = Vec::new();
    let mut preceding_comma = true;
    loop {
        match peek::token(env) {
            TokenKind::TEof | TokenKind::TRcurly => return Ok(specifiers),
            _ => {
                if !preceding_comma {
                    env.error(ParseError::ExportSpecifierMissingComma)?;
                }
                let (loc, mut specifier) = with_loc(None, env, |env| {
                    if peek::token(env) == &TokenKind::TType {
                        // consume `type`, but we don't know yet whether this is `type foo` or
                        // `type as foo` (a value named "type").
                        let type_keyword_or_local = identifier_name(env)?;
                        match peek::token(env) {
                            // `type` (a value named "type")
                            TokenKind::TEof | TokenKind::TRcurly | TokenKind::TComma => {
                                let local = type_keyword_or_local;
                                Ok(statement::export_named_declaration::ExportSpecifier {
                                    loc: LOC_NONE,
                                    local,
                                    exported: None,
                                    export_kind: statement::ExportKind::ExportValue,
                                    from_remote: false,
                                    imported_name_def_loc: None,
                                })
                            }
                            // `type as ...` - ambiguous
                            TokenKind::TIdentifier { raw, .. } if raw == "as" => {
                                let local_or_as = identifier_name(env)?;
                                match peek::token(env) {
                                    // `type as` - type export of "as"
                                    TokenKind::TEof | TokenKind::TRcurly | TokenKind::TComma => {
                                        Ok(statement::export_named_declaration::ExportSpecifier {
                                            loc: LOC_NONE,
                                            local: local_or_as,
                                            exported: None,
                                            export_kind: statement::ExportKind::ExportType,
                                            from_remote: false,
                                            imported_name_def_loc: None,
                                        })
                                    }
                                    // `type as as foo` - type export of "as" renamed to "foo"
                                    TokenKind::TIdentifier { raw, .. } if raw == "as" => {
                                        eat::token(env)?;
                                        let exported = Some(identifier_name(env)?);
                                        Ok(statement::export_named_declaration::ExportSpecifier {
                                            loc: LOC_NONE,
                                            local: local_or_as,
                                            exported,
                                            export_kind: statement::ExportKind::ExportType,
                                            from_remote: false,
                                            imported_name_def_loc: None,
                                        })
                                    }
                                    _ => {
                                        // `type as foo` - value named "type" exported as "foo"
                                        let local = type_keyword_or_local;
                                        let exported = Some(identifier_name(env)?);
                                        Ok(statement::export_named_declaration::ExportSpecifier {
                                            loc: LOC_NONE,
                                            local,
                                            exported,
                                            export_kind: statement::ExportKind::ExportValue,
                                            from_remote: false,
                                            imported_name_def_loc: None,
                                        })
                                    }
                                }
                            }
                            // `type X` or `type X as Y` - type export of X
                            _ => {
                                let local = identifier_name(env)?;
                                let exported = match peek::token(env) {
                                    TokenKind::TIdentifier { raw, .. } if raw == "as" => {
                                        eat::token(env)?;
                                        Some(identifier_name(env)?)
                                    }
                                    _ => None,
                                };
                                Ok(statement::export_named_declaration::ExportSpecifier {
                                    loc: LOC_NONE,
                                    local,
                                    exported,
                                    export_kind: statement::ExportKind::ExportType,
                                    from_remote: false,
                                    imported_name_def_loc: None,
                                })
                            }
                        }
                    } else {
                        let local = identifier_name(env)?;
                        let exported = match peek::token(env) {
                            TokenKind::TIdentifier { raw, .. } if raw == "as" => {
                                eat::token(env)?;
                                Some(identifier_name(env)?)
                            }
                            _ => None,
                        };
                        Ok(statement::export_named_declaration::ExportSpecifier {
                            loc: LOC_NONE,
                            local,
                            exported,
                            export_kind: statement::ExportKind::ExportValue,
                            from_remote: false,
                            imported_name_def_loc: None,
                        })
                    }
                })?;
                specifier.loc = loc;
                if specifier.export_kind == statement::ExportKind::ExportType
                    && !env.should_parse_types()
                {
                    env.error_at(specifier.loc.dupe(), ParseError::UnexpectedTypeExport)?;
                }
                specifiers.push(specifier);
                preceding_comma = eat::maybe(env, TokenKind::TComma)?;
            }
        }
    }
}

fn assert_export_specifier_identifiers(
    env: &mut ParserEnv,
    specifiers: &[statement::export_named_declaration::ExportSpecifier<Loc, Loc>],
) -> Result<(), Rollback> {
    for specifier in specifiers {
        match specifier.export_kind {
            statement::ExportKind::ExportValue => {
                assert_identifier_name_is_identifier(
                    Some(ParseError::StrictVarName),
                    env,
                    &specifier.local,
                )?;
            }
            statement::ExportKind::ExportType => {}
        }
    }
    Ok(())
}

// Parse the module_reference part of an ImportEqualsDeclaration:
// either `require("module")` or a qualified name like `A.B.C`
fn import_equals_module_reference(
    env: &mut ParserEnv,
) -> Result<statement::import_equals_declaration::ModuleReference<Loc, Loc>, Rollback> {
    let is_require_call = matches!(peek::token(env), TokenKind::TIdentifier { raw, .. } if raw == "require")
        && peek::token_after_current_is_lparen(env);
    match peek::token(env) {
        TokenKind::TIdentifier { raw, .. } if raw == "require" && is_require_call => {
            // require("module")
            eat::token(env)?;
            // consume 'require'
            expect::token(env, TokenKind::TLparen)?;
            let source = match peek::token(env) {
                TokenKind::TString(loc, value, raw, octal) => {
                    let loc = loc.dupe();
                    let value = value.clone();
                    let raw = raw.clone();
                    let octal = *octal;
                    string_literal(env, loc, value, raw, octal)?
                }
                _ => {
                    env.error_unexpected(Some("a string".to_owned()))?;
                    let loc = peek::loc_skip_lookahead(env).dupe();
                    (
                        loc,
                        StringLiteral {
                            value: FlowSmolStr::new_inline(""),
                            raw: FlowSmolStr::new_inline(""),
                            comments: None,
                        },
                    )
                }
            };
            expect::token(env, TokenKind::TRparen)?;
            Ok(
                statement::import_equals_declaration::ModuleReference::ExternalModuleReference(
                    source.0, source.1,
                ),
            )
        }
        _ => {
            // Qualified name like A.B.C
            let (_loc, generic) = type_parser::generic(env)?;
            Ok(statement::import_equals_declaration::ModuleReference::Identifier(generic.id))
        }
    }
}

// Parse an ImportEqualsDeclaration:
// `import [type] Foo = require("module")` or `import [type] Foo = A.B.C`
fn import_equals_declaration(
    env: &mut ParserEnv,
    import_kind: statement::ImportKind,
    is_export: bool,
    leading: Vec<Comment<Loc>>,
) -> Result<Arc<statement::ImportEqualsDeclaration<Loc, Loc>>, Rollback> {
    let id = main_parser::parse_identifier(env, None)?;
    import_equals_declaration_with_id(env, import_kind, is_export, leading, id)
}

fn import_equals_declaration_with_id(
    env: &mut ParserEnv,
    import_kind: statement::ImportKind,
    is_export: bool,
    leading: Vec<Comment<Loc>>,
    id: Identifier<Loc, Loc>,
) -> Result<Arc<statement::ImportEqualsDeclaration<Loc, Loc>>, Rollback> {
    if import_kind != statement::ImportKind::ImportValue && !env.should_parse_types() {
        env.error(ParseError::UnexpectedTypeImport)?;
    }
    expect::token(env, TokenKind::TAssign)?;
    let module_reference = import_equals_module_reference(env)?;
    let trailing = match semicolon(env, None, true)? {
        SemicolonType::Explicit(trailing) => trailing,
        SemicolonType::Implicit(result) => result.trailing,
    };
    Ok(Arc::new(statement::ImportEqualsDeclaration {
        id,
        module_reference,
        import_kind,
        is_export,
        comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
    }))
}

fn export_declaration_statement(
    stmt: statement::Statement<Loc, Loc>,
    comments: Option<Syntax<Loc, ()>>,
    ambient: bool,
) -> StatementInner<Loc, Loc> {
    use statement::declare_export_declaration::Declaration;

    let declaration = match stmt.deref() {
        StatementInner::VariableDeclaration { loc, inner }
            if ambient && ast_utils::is_ambient_variable_declaration(inner) =>
        {
            Some(Declaration::Variable {
                loc: loc.dupe(),
                declaration: Arc::new(statement::DeclareVariable {
                    kind: inner.kind,
                    declarations: inner.declarations.dupe(),
                    comments: inner.comments.clone(),
                }),
            })
        }
        StatementInner::DeclareVariable { loc, inner } => Some(Declaration::Variable {
            loc: loc.dupe(),
            declaration: inner.dupe(),
        }),
        StatementInner::DeclareClass { loc, inner } => Some(Declaration::Class {
            loc: loc.dupe(),
            declaration: inner.dupe(),
        }),
        StatementInner::DeclareFunction { loc, inner } => Some(Declaration::Function {
            loc: loc.dupe(),
            declaration: inner.dupe(),
        }),
        StatementInner::ComponentDeclaration { loc, inner } if inner.body.is_none() => {
            Some(Declaration::Component {
                loc: loc.dupe(),
                declaration: Arc::new(statement::DeclareComponent {
                    id: inner.id.dupe(),
                    tparams: inner.tparams.clone(),
                    params: inner.params.clone(),
                    renders: inner.renders.clone(),
                    comments: inner.comments.clone(),
                }),
            })
        }
        _ => None,
    };
    match declaration {
        Some(declaration) => StatementInner::DeclareExportDeclaration {
            loc: LOC_NONE,
            inner: Arc::new(statement::DeclareExportDeclaration {
                default: None,
                declaration: Some(declaration),
                specifiers: None,
                source: None,
                comments,
            }),
        },
        None => StatementInner::ExportNamedDeclaration {
            loc: LOC_NONE,
            inner: Arc::new(statement::ExportNamedDeclaration {
                declaration: Some(stmt),
                specifiers: None,
                source: None,
                attributes: None,
                export_kind: statement::ExportKind::ExportValue,
                comments,
            }),
        },
    }
}

fn export_declaration(
    env: &mut ParserEnv,
    decorators: Vec<class::Decorator<Loc, Loc>>,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    fn parse(
        env: &mut ParserEnv,
        decorators: Vec<class::Decorator<Loc, Loc>>,
    ) -> Result<statement::Statement<Loc, Loc>, Rollback> {
        let leading = peek::comments(env);
        let start_loc = peek::loc(env).dupe();
        expect::token(env, TokenKind::TExport)?;
        let parse_enum = env.parse_options().enums;
        let in_ambient_context = env.in_ambient_context();
        let is_d_ts = env.is_d_ts();
        let is_import_equals = peek::token(env) == &TokenKind::TImport
            && peek::current_token_is_followed_by_identifier(env);
        let is_export_as_namespace = matches!(peek::token(env), TokenKind::TIdentifier { raw, .. } if raw == "as")
            && peek::identifier_token_is_followed_by_namespace(env);
        match peek::token(env) {
            // export = expr;
            TokenKind::TAssign => with_loc(Some(start_loc), env, |env| {
                eat::token(env)?;
                if env.in_ambient_context() && (peek::is_function(env) || peek::is_hook(env)) {
                    let fn_stmt = declaration_parser::parse_function(env)?;
                    match fn_stmt.0.as_ref() {
                        StatementInner::DeclareFunction { loc: fn_loc, inner: decl_func } => {
                            Ok(StatementInner::ExportAssignment {
                                loc: LOC_NONE,
                                inner: Arc::new(statement::ExportAssignment {
                                    rhs: statement::ExportAssignmentRhs::DeclareFunction(
                                        fn_loc.dupe(),
                                        decl_func.as_ref().clone(),
                                    ),
                                    comments: ast_utils::mk_comments_opt(
                                        Some(leading.into()),
                                        None,
                                    ),
                                }),
                            })
                        }
                        StatementInner::FunctionDeclaration { loc: fn_loc, inner: func } => {
                            let trailing = match semicolon(env, None, true)? {
                                SemicolonType::Explicit(trailing) => trailing,
                                SemicolonType::Implicit(result) => result.trailing,
                            };
                            Ok(StatementInner::ExportAssignment {
                                loc: LOC_NONE,
                                inner: Arc::new(statement::ExportAssignment {
                                    rhs: statement::ExportAssignmentRhs::Expression(
                                        expression::Expression::new(
                                            expression::ExpressionInner::Function {
                                                loc: fn_loc.dupe(),
                                                inner: func.dupe(),
                                            },
                                        ),
                                    ),
                                    comments: ast_utils::mk_comments_opt(
                                        Some(leading.into()),
                                        Some(trailing.into()),
                                    ),
                                }),
                            })
                        }
                        _ => panic!("Declaration._function must return DeclareFunction or FunctionDeclaration"),
                    }
                } else {
                    let expression = expression_parser::assignment(env)?;
                    let trailing = match semicolon(env, None, true)? {
                        SemicolonType::Explicit(trailing) => trailing,
                        SemicolonType::Implicit(result) => result.trailing,
                    };
                    Ok(StatementInner::ExportAssignment {
                        loc: LOC_NONE,
                        inner: Arc::new(statement::ExportAssignment {
                            rhs: statement::ExportAssignmentRhs::Expression(expression),
                            comments: ast_utils::mk_comments_opt(
                                Some(leading.into()),
                                Some(trailing.into()),
                            ),
                        }),
                    })
                }
            })
            .map(|(loc, mut inner)| {
                *inner.loc_mut() = loc;
                statement::Statement::new(inner)
            }),
            // export as namespace Foo;
            TokenKind::TIdentifier { raw, .. } if raw == "as" && is_export_as_namespace => {
                with_loc(Some(start_loc), env, |env| {
                    if !env.should_parse_types() {
                        env.error(ParseError::UnexpectedTypeExport)?;
                    }
                    expect::identifier(env, "as")?;
                    expect::identifier(env, "namespace")?;
                    let id = main_parser::parse_identifier(env, None)?;
                    let trailing = match semicolon(env, None, true)? {
                        SemicolonType::Explicit(trailing) => trailing,
                        SemicolonType::Implicit(result) => result.trailing,
                    };
                    Ok(StatementInner::NamespaceExportDeclaration {
                        loc: LOC_NONE,
                        inner: Arc::new(statement::NamespaceExportDeclaration {
                            id,
                            comments: ast_utils::mk_comments_opt(
                                Some(leading.into()),
                                Some(trailing.into()),
                            ),
                        }),
                    })
                })
                .map(|(loc, mut inner)| {
                    *inner.loc_mut() = loc;
                    statement::Statement::new(inner)
                })
            }
            // export import [type] Foo = ...;
            TokenKind::TImport if is_import_equals => {
                with_loc(Some(start_loc), env, |env| {
                    let leading = {
                        let mut l = leading;
                        l.extend(peek::comments(env));
                        l
                    };
                    eat::token(env)?;
                    match peek::token(env) {
                        TokenKind::TType => {
                            let type_id = main_parser::parse_identifier(env, None)?;
                            let import_kind = match peek::token(env) {
                                // `export import type, ...` or `export import type from ...` or
                                // `export import type = ...` means 'type' is the binding name, not a keyword
                                TokenKind::TComma | TokenKind::TAssign => {
                                    statement::ImportKind::ImportValue
                                }
                                TokenKind::TIdentifier { raw, .. } if raw == "from" => {
                                    statement::ImportKind::ImportValue
                                }
                                _ => statement::ImportKind::ImportType,
                            };
                            if import_kind == statement::ImportKind::ImportValue {
                                import_equals_declaration_with_id(
                                    env,
                                    import_kind,
                                    true,
                                    leading,
                                    type_id,
                                )
                            } else {
                                import_equals_declaration(env, import_kind, true, leading)
                            }
                        }
                        _ => import_equals_declaration(
                            env,
                            statement::ImportKind::ImportValue,
                            true,
                            leading,
                        ),
                    }
                })
                .map(|(loc, inner)| {
                    statement::Statement::new(StatementInner::ImportEqualsDeclaration {
                        loc,
                        inner,
                    })
                })
            }
            TokenKind::TDefault => {
                // export default ...
                with_loc(Some(start_loc), env, |env| {
                    let leading = {
                        let mut l = leading;
                        l.extend(peek::comments(env));
                        l
                    };
                    let (default, _) = with_loc(None, env, |env| {
                        expect::token(env, TokenKind::TDefault)?;
                        Ok(())
                    })?;
                    // export default [async] function [foo] (...) { ... }
                    if peek::is_function(env) || peek::is_hook(env) {
                        let func = env.with_in_export_default(true, |env| {
                            declaration_parser::parse_function(env)
                        })?;
                        match &*func {
                            StatementInner::DeclareFunction { loc: fn_loc, inner: decl_func } => {
                                Ok(StatementInner::DeclareExportDeclaration {
                                    loc: LOC_NONE,
                                    inner: Arc::new(statement::DeclareExportDeclaration {
                                        default: Some(default),
                                        declaration: Some(
                                            statement::declare_export_declaration::Declaration::Function {
                                                loc: fn_loc.dupe(),
                                                declaration: decl_func.dupe(),
                                            },
                                        ),
                                        specifiers: None,
                                        source: None,
                                        comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                                    }),
                                })
                            }
                            _ => {
                                Ok(StatementInner::ExportDefaultDeclaration {
                                    loc: LOC_NONE,
                                    inner: Arc::new(statement::ExportDefaultDeclaration {
                                        default,
                                        declaration: statement::export_default_declaration::Declaration::Declaration(
                                            func,
                                        ),
                                        comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                                    }),
                                })
                            }
                        }
                        // export default component foo { ... }
                    } else if peek::is_component(env) {
                        let component = env.with_in_export_default(true, |env| {
                            declaration_parser::parse_component(env)
                        })?;
                        match &*component {
                            StatementInner::ComponentDeclaration {
                                loc: comp_loc,
                                inner,
                            } if inner.body.is_none() => {
                                let statement::ComponentDeclaration {
                                    body: _,
                                    id,
                                    tparams,
                                    params,
                                    renders,
                                    comments,
                                    sig_loc: _,
                                    async_: _,
                                } = (**inner).clone();
                                let decl_comp = statement::DeclareComponent {
                                    id,
                                    tparams,
                                    params,
                                    renders,
                                    comments,
                                };
                                Ok(StatementInner::DeclareExportDeclaration {
                                    loc: LOC_NONE,
                                    inner: Arc::new(statement::DeclareExportDeclaration {
                                        default: Some(default),
                                        declaration: Some(
                                            statement::declare_export_declaration::Declaration::Component {
                                                loc: comp_loc.dupe(),
                                                declaration: Arc::new(decl_comp),
                                            },
                                        ),
                                        specifiers: None,
                                        source: None,
                                        comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                                    }),
                                })
                            }
                            _ => {
                                Ok(StatementInner::ExportDefaultDeclaration {
                                    loc: LOC_NONE,
                                    inner: Arc::new(statement::ExportDefaultDeclaration {
                                        default,
                                        declaration: statement::export_default_declaration::Declaration::Declaration(
                                            component,
                                        ),
                                        comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                                    }),
                                })
                            }
                        }
                    } else {
                        let (declaration, trailing) = env.with_in_export_default(true, |env| {
                            if peek::is_class(env) {
                                // export default class foo { ... }
                                let class = object_parser::class_declaration(env, decorators)?;
                                Ok((
                                    statement::export_default_declaration::Declaration::Declaration(
                                        class,
                                    ),
                                    Vec::new(),
                                ))
                            } else if peek::token(env) == &TokenKind::TEnum {
                                // export default enum foo { ... }
                                let enum_decl =
                                    declaration_parser::parse_enum_declaration(env, None, None, false)?;
                                Ok((
                                    statement::export_default_declaration::Declaration::Declaration(
                                        enum_decl,
                                    ),
                                    Vec::new(),
                                ))
                            } else if peek::is_record(env) {
                                // export default record R { ... }
                                let record = object_parser::record_declaration(env)?;
                                Ok((
                                    statement::export_default_declaration::Declaration::Declaration(
                                        record,
                                    ),
                                    Vec::new(),
                                ))
                            } else if peek::token(env) == &TokenKind::TInterface {
                                let (loc, iface) =
                                    with_loc(None, env, |env| interface_helper(env, Vec::new()))?;
                                let interface = statement::Statement::new(
                                    StatementInner::InterfaceDeclaration {
                                        loc,
                                        inner: Arc::new(iface),
                                    },
                                );
                                Ok((
                                    statement::export_default_declaration::Declaration::Declaration(
                                        interface,
                                    ),
                                    Vec::new(),
                                ))
                            } else {
                                // export default [assignment expression];
                                let mut expr = expression_parser::assignment(env)?;
                                let trailing = match semicolon(env, None, true)? {
                                    SemicolonType::Explicit(trailing) => trailing,
                                    SemicolonType::Implicit(result) => {
                                        if let Some(mut remover) = result.remover {
                                            expr = remover.map_expression(&expr);
                                        }
                                        Vec::new()
                                    }
                                };
                                Ok((
                                    statement::export_default_declaration::Declaration::Expression(
                                        expr,
                                    ),
                                    trailing,
                                ))
                            }
                        })?;

                        Ok(StatementInner::ExportDefaultDeclaration {
                            loc: LOC_NONE,
                            inner: Arc::new(statement::ExportDefaultDeclaration {
                                default,
                                declaration,
                                comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
                            }),
                        })
                    }
                })
                .map(|(loc, mut inner)| {
                    *inner.loc_mut() = loc;
                    statement::Statement::new(inner)
                })
            }
            TokenKind::TOpaque => {
                // export opaque type ...
                with_loc(Some(start_loc), env, |env| {
                    let (loc, opaque_t) =
                        with_loc(None, env, |env| opaque_type_helper(env, false, Vec::new()))?;
                    let opaque_t = statement::Statement::new(StatementInner::OpaqueType {
                        loc,
                        inner: Arc::new(opaque_t),
                    });
                    Ok(StatementInner::ExportNamedDeclaration {
                        loc: LOC_NONE,
                        inner: Arc::new(statement::ExportNamedDeclaration {
                            declaration: Some(opaque_t),
                            specifiers: None,
                            source: None,
                            attributes: None,
                            export_kind: statement::ExportKind::ExportType,
                            comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                        }),
                    })
                })
                .map(|(loc, mut inner)| {
                    *inner.loc_mut() = loc;
                    statement::Statement::new(inner)
                })
            }
            TokenKind::TInterface => {
                // export interface I { ... }
                with_loc(Some(start_loc), env, |env| {
                    if !env.should_parse_types() {
                        env.error(ParseError::UnexpectedTypeExport)?;
                    }
                    let (loc, iface) =
                        with_loc(None, env, |env| interface_helper(env, Vec::new()))?;
                    let interface =
                        statement::Statement::new(StatementInner::InterfaceDeclaration {
                            loc,
                            inner: Arc::new(iface),
                        });
                    Ok(StatementInner::ExportNamedDeclaration {
                        loc: LOC_NONE,
                        inner: Arc::new(statement::ExportNamedDeclaration {
                            declaration: Some(interface),
                            specifiers: None,
                            source: None,
                            attributes: None,
                            export_kind: statement::ExportKind::ExportType,
                            comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                        }),
                    })
                })
                .map(|(loc, mut inner)| {
                    *inner.loc_mut() = loc;
                    statement::Statement::new(inner)
                })
            }
            TokenKind::TLet | TokenKind::TConst | TokenKind::TVar => {
                with_loc(Some(start_loc), env, |env| {
                    let stmt = parse_statement_list_item(env, decorators.clone())?;
                    Ok(export_declaration_statement(
                        stmt,
                        ast_utils::mk_comments_opt(Some(leading.into()), None),
                        in_ambient_context,
                    ))
                })
                .map(|(loc, mut inner)| {
                    *inner.loc_mut() = loc;
                    statement::Statement::new(inner)
                })
            }
            TokenKind::TEnum if parse_enum => with_loc(Some(start_loc), env, |env| {
                let stmt = parse_statement_list_item(env, decorators.clone())?;
                Ok(StatementInner::ExportNamedDeclaration {
                    loc: LOC_NONE,
                    inner: Arc::new(statement::ExportNamedDeclaration {
                        declaration: Some(stmt),
                        specifiers: None,
                        source: None,
                        attributes: None,
                        export_kind: statement::ExportKind::ExportValue,
                        comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                    }),
                })
            })
            .map(|(loc, mut inner)| {
                *inner.loc_mut() = loc;
                statement::Statement::new(inner)
            }),
            TokenKind::TDeclare => {
                // export declare ...
                let (loc, mut inner) = with_loc(Some(start_loc), env, |env| {
                    if !env.should_parse_types() {
                        env.error(ParseError::UnexpectedTypeDeclaration)?;
                    }
                    let leading = {
                        let mut l = leading;
                        l.extend(peek::comments(env));
                        l
                    };
                    expect::token(env, TokenKind::TDeclare)?;
                    declare_export_declaration_body(
                        env,
                        leading,
                        false,
                        "a declaration after 'export declare'",
                    )
                })?;
                *inner.loc_mut() = loc;
                Ok(statement::Statement::new(inner))
            }
            TokenKind::TIdentifier { raw, .. }
                if (raw == "namespace" || (raw == "module" && is_d_ts)) && in_ambient_context =>
            {
                // export namespace/module X { ... } in ambient context - implicit declare
                let (loc, mut s) = with_loc(Some(start_loc), env, |env| {
                    if !env.should_parse_types() {
                        env.error(ParseError::UnexpectedTypeExport)?;
                    }
                    let (loc, declaration) = with_loc(None, env, |env| {
                        let keyword = match peek::token(env) {
                            TokenKind::TIdentifier { raw, .. } if raw == "module" => {
                                expect::identifier(env, "module")?;
                                statement::declare_namespace::Keyword::Module
                            }
                            _ => {
                                expect::identifier(env, "namespace")?;
                                statement::declare_namespace::Keyword::Namespace
                            }
                        };
                        let mut id = main_parser::parse_identifier(env, None)?;
                        comment_attachment::id_remove_trailing(env, &mut id);
                        let id = statement::declare_namespace::Id::Local(id);
                        let body = declare_module_or_namespace_body(env)?;
                        let comments = ast_utils::mk_comments_opt::<Loc>(None, None);
                        Ok(statement::DeclareNamespace {
                            id,
                            body,
                            comments,
                            implicit_declare: true,
                            keyword,
                        })
                    })?;
                    let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
                    Ok(StatementInner::DeclareExportDeclaration {
                        loc: LOC_NONE,
                        inner: Arc::new(statement::DeclareExportDeclaration {
                            default: None,
                            declaration: Some(
                                statement::declare_export_declaration::Declaration::Namespace {
                                    loc,
                                    declaration: Box::new(declaration),
                                },
                            ),
                            specifiers: None,
                            source: None,
                            comments,
                        }),
                    })
                })?;
                *s.loc_mut() = loc;
                Ok(statement::Statement::new(s))
            }
            TokenKind::TMult => with_loc(Some(start_loc), env, |env| {
                let loc = peek::loc(env).dupe();
                expect::token(env, TokenKind::TMult)?;
                let local_name = match peek::token(env) {
                    TokenKind::TIdentifier { raw, .. } if raw == "as" => {
                        eat::token(env)?;
                        Some(crate::parser_common::identifier_name(env)?)
                    }
                    _ => None,
                };
                let specifiers = Some(
                    statement::export_named_declaration::Specifier::ExportBatchSpecifier(
                        statement::export_named_declaration::ExportBatchSpecifier {
                            loc,
                            specifier: local_name,
                        },
                    ),
                );
                let mut source = export_source(env)?;
                let attributes = import_attributes(env)?;
                let trailing = semicolon_and_trailing(env, &mut source)?;
                Ok(StatementInner::ExportNamedDeclaration {
                    loc: LOC_NONE,
                    inner: Arc::new(statement::ExportNamedDeclaration {
                        declaration: None,
                        specifiers,
                        source: Some(source),
                        attributes,
                        export_kind: statement::ExportKind::ExportValue,
                        comments: ast_utils::mk_comments_opt(
                            Some(leading.into()),
                            Some(trailing.into()),
                        ),
                    }),
                })
            })
            .map(|(loc, mut inner)| {
                *inner.loc_mut() = loc;
                statement::Statement::new(inner)
            }),
            t => {
                if t == &TokenKind::TType {
                    // export type ...
                    with_loc(Some(start_loc), env, |env| {
                        if !env.should_parse_types() {
                            env.error(ParseError::UnexpectedTypeExport)?;
                        }
                        let type_loc = peek::loc(env).dupe();
                        let type_alias_leading = peek::comments(env);
                        expect::token(env, TokenKind::TType)?;
                        match peek::token(env) {
                            TokenKind::TLcurly => {
                                expect::token(env, TokenKind::TLcurly)?;
                                let mut specifiers = export_specifiers(env)?;
                                expect::token(env, TokenKind::TRcurly)?;
                                let (source, attributes, trailing, specifiers) = match peek::token(env) {
                                    TokenKind::TIdentifier { raw, .. } if raw == "from" => {
                                        let mut source = export_source(env)?;
                                        let attributes = import_attributes(env)?;
                                        let trailing = semicolon_and_trailing(env, &mut source)?;
                                        specifiers.iter_mut().for_each(|s| {
                                            s.from_remote = true;
                                        });
                                        (Some(source), attributes, trailing, specifiers)
                                    }
                                    _ => {
                                        assert_export_specifier_identifiers(env, &specifiers)?;
                                        let trailing = match semicolon(env, None, true)? {
                                            SemicolonType::Explicit(trailing) => trailing,
                                            SemicolonType::Implicit(result) => result.trailing,
                                        };
                                        (None, None, trailing, specifiers)
                                    }
                                };
                                Ok(StatementInner::ExportNamedDeclaration {
                                    loc: LOC_NONE,
                                    inner: Arc::new(statement::ExportNamedDeclaration {
                                        declaration: None,
                                        specifiers: Some(statement::export_named_declaration::Specifier::ExportSpecifiers(specifiers)),
                                        source,
                                        attributes,
                                        export_kind: statement::ExportKind::ExportType,
                                        comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
                                    }),
                                })
                            }
                            TokenKind::TMult => {
                                let specifier_loc = peek::loc(env).dupe();
                                expect::token(env, TokenKind::TMult)?;
                                let mut source = export_source(env)?;
                                let attributes = import_attributes(env)?;
                                let trailing = semicolon_and_trailing(env, &mut source)?;
                                Ok(StatementInner::ExportNamedDeclaration {
                                    loc: LOC_NONE,
                                    inner: Arc::new(statement::ExportNamedDeclaration {
                                        declaration: None,
                                        specifiers: Some(statement::export_named_declaration::Specifier::ExportBatchSpecifier(
                                            statement::export_named_declaration::ExportBatchSpecifier{ loc: specifier_loc, specifier: None } )),
                                        source: Some(source),
                                        attributes,
                                        export_kind: statement::ExportKind::ExportType,
                                        comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
                                    }),
                                })
                            }
                            TokenKind::TEnum => {
                                env.error_at(type_loc, ParseError::EnumInvalidExport)?;
                                Ok(StatementInner::ExportNamedDeclaration {
                                    loc: LOC_NONE,
                                    inner: Arc::new(statement::ExportNamedDeclaration {
                                        declaration: None,
                                        specifiers: None,
                                        source: None,
                                        attributes: None,
                                        export_kind: statement::ExportKind::ExportType,
                                        comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                                    }),
                                })
                            }
                            _ => {
                                let (loc, type_alias) = with_loc(Some(type_loc), env, |env| {
                                    type_alias_helper_after_type(env, type_alias_leading)
                                })?;
                                let type_alias = statement::Statement::new(StatementInner::TypeAlias {
                                    loc,
                                    inner: Arc::new(type_alias),
                                });
                                Ok(StatementInner::ExportNamedDeclaration {
                                    loc: LOC_NONE,
                                    inner: Arc::new(statement::ExportNamedDeclaration {
                                        declaration: Some(type_alias),
                                        specifiers: None,
                                        source: None,
                                        attributes: None,
                                        export_kind: statement::ExportKind::ExportType,
                                        comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                                    }),
                                })
                            }
                        }
                    }).map(|(loc, mut inner)| {
                        *inner.loc_mut() = loc;
                        statement::Statement::new(inner)
                    })
                } else if peek::is_class(env) || peek::is_function(env) || peek::is_hook(env) || peek::is_component(env) {
                    with_loc(Some(start_loc), env, |env| {
                        let stmt = parse_statement_list_item(env, decorators)?;
                        Ok(export_declaration_statement(
                            stmt,
                            ast_utils::mk_comments_opt(Some(leading.into()), None),
                            in_ambient_context,
                        ))
                    })
                    .map(|(loc, mut inner)| {
                        *inner.loc_mut() = loc;
                        statement::Statement::new(inner)
                    })
                } else if peek::is_record(env) {
                    with_loc(Some(start_loc), env, |env| {
                        let stmt = parse_statement_list_item(env, decorators.clone())?;
                        Ok(StatementInner::ExportNamedDeclaration {
                            loc: LOC_NONE,
                            inner: Arc::new(statement::ExportNamedDeclaration {
                                declaration: Some(stmt),
                                specifiers: None,
                                source: None,
                                attributes: None,
                                export_kind: statement::ExportKind::ExportValue,
                                comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                            }),
                        })
                    })
                    .map(|(loc, mut inner)| {
                        *inner.loc_mut() = loc;
                        statement::Statement::new(inner)
                    })
                } else {
                    let export_kind = if eat::maybe(env, TokenKind::TType)? {
                        statement::ExportKind::ExportType
                    } else {
                        statement::ExportKind::ExportValue
                    };

                    if eat::maybe(env, TokenKind::TLcurly)? {
                        with_loc(Some(start_loc), env, |env| {
                            let mut specifiers = export_specifiers(env)?;
                            expect::token(env, TokenKind::TRcurly)?;
                            let (source, attributes, trailing, specifiers) = match peek::token(env) {
                                TokenKind::TIdentifier { raw, .. } if raw == "from" => {
                                    let mut source = export_source(env)?;
                                    let attributes = import_attributes(env)?;
                                    let trailing = semicolon_and_trailing(env, &mut source)?;
                                    specifiers.iter_mut().for_each(|s| {
                                        s.from_remote=true;
                                    });
                                    (Some(source), attributes, trailing, specifiers)
                                }
                                _ => {
                                    assert_export_specifier_identifiers(env, &specifiers)?;
                                    let trailing = match semicolon(env, None, true)? {
                                        SemicolonType::Explicit(trailing) => trailing,
                                        SemicolonType::Implicit(result) => result.trailing,
                                    };
                                    (None, None, trailing, specifiers)
                                }
                            };
                            Ok(StatementInner::ExportNamedDeclaration {
                                loc: LOC_NONE,
                                inner: Arc::new(statement::ExportNamedDeclaration {
                                    declaration: None,
                                    specifiers: Some(statement::export_named_declaration::Specifier::ExportSpecifiers(specifiers)),
                                    source,
                                    attributes,
                                    export_kind,
                                    comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
                                }),
                            })
                        }).map(|(loc, mut inner)| {
                            *inner.loc_mut() = loc;
                            statement::Statement::new(inner)
                        })
                    } else {
                        // error. recover by ignoring the `export`
                        env.error_unexpected(Some(
                            "a declaration, statement or export specifiers".to_owned(),
                        ))?;
                        parse_statement_list_item(env, decorators)
                    }
                }
            }
        }
    }

    env.with_strict(true, |env| {
        env.with_in_export(true, |env| parse(env, decorators))
    })
}

type DeclareExportDefaultResult = (
    Option<statement::declare_export_declaration::Declaration<Loc, Loc>>,
    Vec<Comment<Loc>>,
);

// `declare export default [type];`
fn declare_export_default_type(
    env: &mut ParserEnv,
) -> Result<DeclareExportDefaultResult, Rollback> {
    let mut type_ = type_parser::parse_type(env)?;
    let trailing = match semicolon(env, None, true)? {
        SemicolonType::Explicit(trailing) => trailing,
        SemicolonType::Implicit(result) => {
            if let Some(mut remover) = result.remover {
                type_ = remover.map_type_(&type_);
            }
            Vec::new()
        }
    };
    Ok((
        Some(
            statement::declare_export_declaration::Declaration::DefaultType {
                type_: Arc::new(type_),
            },
        ),
        trailing,
    ))
}

// Helper function for parsing the body of a declare export declaration.
// This is shared between `declare export ...` and `export declare ...` syntaxes.
// The [allow_specifiers] parameter controls whether batch exports (`* from`)
// and named specifiers (`{ ... }`) are allowed - these are only valid for
// `declare export`, not `export declare`.
// The [err_expected] parameter specifies the error message to use when an
// unexpected token is encountered.
fn declare_export_declaration_body(
    env: &mut ParserEnv,
    leading: Vec<Comment<Loc>>,
    allow_specifiers: bool,
    err_expected: &str,
) -> Result<StatementInner<Loc, Loc>, Rollback> {
    let parse_components = env.parse_options().components;
    let parse_enums = env.parse_options().enums;
    let is_d_ts = env.is_d_ts();
    match peek::token(env) {
        TokenKind::TDefault => {
            // declare export default ...
            let leading = {
                let mut l = leading;
                l.extend(peek::comments(env));
                l
            };
            let (default, _) = with_loc(None, env, |env| {
                expect::token(env, TokenKind::TDefault)?;
                Ok(())
            })?;

            let (declaration, trailing) = env.with_in_export_default(true, |env| {
                let parse_components = env.parse_options().components;
                match peek::token(env) {
                    TokenKind::TFunction => {
                        // declare export default function foo (...): ...
                        let (loc, declaration) =
                            with_loc(None, env, |env| declare_function(env, false, Vec::new()))?;
                        Ok((
                            Some(
                                statement::declare_export_declaration::Declaration::Function {
                                    loc,
                                    declaration: Arc::new(declaration),
                                },
                            ),
                            Vec::new(),
                        ))
                    }
                    TokenKind::TClass => {
                        // declare export default class foo { ... }
                        let (loc, declaration) = with_loc(None, env, |env| {
                            declare_class_helper(env, false, Vec::new())
                        })?;
                        Ok((
                            Some(statement::declare_export_declaration::Declaration::Class {
                                loc,
                                declaration: Arc::new(declaration),
                            }),
                            Vec::new(),
                        ))
                    }
                    TokenKind::TIdentifier { raw, .. } if raw == "abstract" => {
                        // declare export default abstract class foo { ... }
                        eat::token(env)?;
                        let (loc, declaration) =
                            with_loc(None, env, |env| declare_class_helper(env, true, Vec::new()))?;
                        Ok((
                            Some(statement::declare_export_declaration::Declaration::Class {
                                loc,
                                declaration: Arc::new(declaration),
                            }),
                            Vec::new(),
                        ))
                    }
                    TokenKind::TIdentifier { raw, .. }
                        if raw == "component" && parse_components =>
                    {
                        if peek::token_after_current_starts_type_call(env) {
                            // declare export default component(...);
                            // A nameless `component` is a type annotation.
                            declare_export_default_type(env)
                        } else {
                            // declare export default component Foo() { ... }
                            let (loc, declaration) =
                                with_loc(None, env, |env| declare_component(env, Vec::new()))?;
                            Ok((
                                Some(
                                    statement::declare_export_declaration::Declaration::Component {
                                        loc,
                                        declaration: Arc::new(declaration),
                                    },
                                ),
                                Vec::new(),
                            ))
                        }
                    }
                    TokenKind::TAsync if parse_components => {
                        // Check if it's `declare export default async component`
                        if peek::async_token_is_followed_by_component(env) {
                            // declare export default async component Foo();
                            env.error(ParseError::DeclareAsyncComponent)?;
                            eat::token(env)?; // consume 'async'
                            let (loc, declaration) = with_loc(None, env, |env| {
                                let comp = declare_component(env, Vec::new())?;
                                Ok(comp)
                            })?;
                            Ok((
                                Some(
                                    statement::declare_export_declaration::Declaration::Component {
                                        loc,
                                        declaration: Arc::new(declaration),
                                    },
                                ),
                                Vec::new(),
                            ))
                        } else if peek::async_token_is_followed_by_hook(env) {
                            // declare export default async hook foo (...): ...
                            env.error(ParseError::DeclareAsyncHook)?;
                            eat::token(env)?; // consume 'async'
                            let (loc, declaration) = with_loc(None, env, |env| {
                                declare_function(env, false, Vec::new())
                            })?;
                            Ok((
                                Some(
                                    statement::declare_export_declaration::Declaration::Function {
                                        loc,
                                        declaration: Arc::new(declaration),
                                    },
                                ),
                                Vec::new(),
                            ))
                        } else {
                            // declare export default async function ...
                            let (loc, declaration) = with_loc(None, env, |env| {
                                declare_function(env, false, Vec::new())
                            })?;
                            Ok((
                                Some(
                                    statement::declare_export_declaration::Declaration::Function {
                                        loc,
                                        declaration: Arc::new(declaration),
                                    },
                                ),
                                Vec::new(),
                            ))
                        }
                    }
                    TokenKind::TIdentifier { raw, .. } if raw == "hook" && parse_components => {
                        if peek::token_after_current_starts_type_call(env) {
                            // declare export default hook (...): ...;
                            // A nameless `hook` is a type annotation.
                            declare_export_default_type(env)
                        } else {
                            // declare export default hook foo (...): ...
                            let (loc, declaration) = with_loc(None, env, |env| {
                                declare_function(env, false, Vec::new())
                            })?;
                            Ok((
                                Some(
                                    statement::declare_export_declaration::Declaration::Function {
                                        loc,
                                        declaration: Arc::new(declaration),
                                    },
                                ),
                                Vec::new(),
                            ))
                        }
                    }
                    _ => declare_export_default_type(env),
                }
            })?;

            let comments = ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into()));
            Ok(StatementInner::DeclareExportDeclaration {
                loc: LOC_NONE,
                inner: Arc::new(statement::DeclareExportDeclaration {
                    default: Some(default),
                    declaration,
                    specifiers: None,
                    source: None,
                    comments,
                }),
            })
        }
        TokenKind::TLet
        | TokenKind::TConst
        | TokenKind::TVar
        | TokenKind::TClass
        | TokenKind::TFunction => {
            let declaration = match peek::token(env) {
                TokenKind::TFunction => {
                    // declare export function foo (...): ...
                    let (loc, declaration) =
                        with_loc(None, env, |env| declare_function(env, false, Vec::new()))?;
                    Some(
                        statement::declare_export_declaration::Declaration::Function {
                            loc,
                            declaration: Arc::new(declaration),
                        },
                    )
                }
                TokenKind::TClass => {
                    // declare export class foo { ... }
                    let (loc, declaration) = with_loc(None, env, |env| {
                        declare_class_helper(env, false, Vec::new())
                    })?;
                    Some(statement::declare_export_declaration::Declaration::Class {
                        loc,
                        declaration: Arc::new(declaration),
                    })
                }
                TokenKind::TVar => {
                    // declare export var foo: ...
                    let (loc, declaration) = with_loc(None, env, |env| {
                        declare_var(env, VariableKind::Var, Vec::new())
                    })?;
                    Some(
                        statement::declare_export_declaration::Declaration::Variable {
                            loc,
                            declaration: Arc::new(declaration),
                        },
                    )
                }
                TokenKind::TLet => {
                    // declare export let foo: ...
                    let (loc, declaration) = with_loc(None, env, |env| {
                        declare_var(env, VariableKind::Let, Vec::new())
                    })?;
                    Some(
                        statement::declare_export_declaration::Declaration::Variable {
                            loc,
                            declaration: Arc::new(declaration),
                        },
                    )
                }
                TokenKind::TConst => {
                    // declare export const foo: ... or declare export const enum ...
                    if parse_enums && peek::current_token_is_followed_by_enum(env) {
                        expect::token(env, TokenKind::TConst)?;
                        let (loc, declaration) = with_loc(None, env, |env| {
                            enum_parser::declaration(Vec::new(), true, env)
                        })?;
                        Some(statement::declare_export_declaration::Declaration::Enum {
                            loc,
                            declaration: Arc::new(declaration),
                        })
                    } else {
                        let (loc, declaration) = with_loc(None, env, |env| {
                            declare_var(env, VariableKind::Const, Vec::new())
                        })?;
                        Some(
                            statement::declare_export_declaration::Declaration::Variable {
                                loc,
                                declaration: Arc::new(declaration),
                            },
                        )
                    }
                }
                _ => None,
            };
            let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
            Ok(StatementInner::DeclareExportDeclaration {
                loc: LOC_NONE,
                inner: Arc::new(statement::DeclareExportDeclaration {
                    default: None,
                    declaration,
                    specifiers: None,
                    source: None,
                    comments,
                }),
            })
        }
        TokenKind::TIdentifier { raw, .. } if raw == "hook" && parse_components => {
            // declare export hook foo (...): ...
            let declaration = {
                let (loc, declaration) =
                    with_loc(None, env, |env| declare_function(env, false, Vec::new()))?;
                Some(
                    statement::declare_export_declaration::Declaration::Function {
                        loc,
                        declaration: Arc::new(declaration),
                    },
                )
            };
            let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
            Ok(StatementInner::DeclareExportDeclaration {
                loc: LOC_NONE,
                inner: Arc::new(statement::DeclareExportDeclaration {
                    default: None,
                    declaration,
                    specifiers: None,
                    source: None,
                    comments,
                }),
            })
        }
        TokenKind::TIdentifier { raw, .. } if raw == "component" && parse_components => {
            // declare export component Foo() { ... }
            let declaration = {
                let (loc, declaration) =
                    with_loc(None, env, |env| declare_component(env, Vec::new()))?;
                Some(
                    statement::declare_export_declaration::Declaration::Component {
                        loc,
                        declaration: Arc::new(declaration),
                    },
                )
            };
            let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
            Ok(StatementInner::DeclareExportDeclaration {
                loc: LOC_NONE,
                inner: Arc::new(statement::DeclareExportDeclaration {
                    default: None,
                    declaration,
                    specifiers: None,
                    source: None,
                    comments,
                }),
            })
        }
        TokenKind::TAsync if parse_components => {
            // Check if it's `declare export async component` vs `declare export async hook` vs `declare export async function`
            if peek::async_token_is_followed_by_component(env) {
                // declare export async component Foo();
                env.error(ParseError::DeclareAsyncComponent)?;
                eat::token(env)?; // consume 'async'
                let declaration = {
                    let (loc, declaration) = with_loc(None, env, |env| {
                        let comp = declare_component(env, Vec::new())?;
                        Ok(comp)
                    })?;
                    Some(
                        statement::declare_export_declaration::Declaration::Component {
                            loc,
                            declaration: Arc::new(declaration),
                        },
                    )
                };
                let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
                Ok(StatementInner::DeclareExportDeclaration {
                    loc: LOC_NONE,
                    inner: Arc::new(statement::DeclareExportDeclaration {
                        default: None,
                        declaration,
                        specifiers: None,
                        source: None,
                        comments,
                    }),
                })
            } else if peek::async_token_is_followed_by_hook(env) {
                // declare export async hook foo (...): ...
                env.error(ParseError::DeclareAsyncHook)?;
                eat::token(env)?; // consume 'async'
                let declaration = {
                    let (loc, declaration) =
                        with_loc(None, env, |env| declare_function(env, false, Vec::new()))?;
                    Some(
                        statement::declare_export_declaration::Declaration::Function {
                            loc,
                            declaration: Arc::new(declaration),
                        },
                    )
                };
                let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
                Ok(StatementInner::DeclareExportDeclaration {
                    loc: LOC_NONE,
                    inner: Arc::new(statement::DeclareExportDeclaration {
                        default: None,
                        declaration,
                        specifiers: None,
                        source: None,
                        comments,
                    }),
                })
            } else {
                // declare export async function ...
                let declaration = {
                    let (loc, declaration) =
                        with_loc(None, env, |env| declare_function(env, false, Vec::new()))?;
                    Some(
                        statement::declare_export_declaration::Declaration::Function {
                            loc,
                            declaration: Arc::new(declaration),
                        },
                    )
                };
                let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
                Ok(StatementInner::DeclareExportDeclaration {
                    loc: LOC_NONE,
                    inner: Arc::new(statement::DeclareExportDeclaration {
                        default: None,
                        declaration,
                        specifiers: None,
                        source: None,
                        comments,
                    }),
                })
            }
        }
        TokenKind::TIdentifier { raw, .. } if raw == "abstract" => {
            // declare export abstract class foo { ... }
            let declaration = {
                eat::token(env)?;
                let (loc, declaration) =
                    with_loc(None, env, |env| declare_class_helper(env, true, Vec::new()))?;
                Some(statement::declare_export_declaration::Declaration::Class {
                    loc,
                    declaration: Arc::new(declaration),
                })
            };
            let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
            Ok(StatementInner::DeclareExportDeclaration {
                loc: LOC_NONE,
                inner: Arc::new(statement::DeclareExportDeclaration {
                    default: None,
                    declaration,
                    specifiers: None,
                    source: None,
                    comments,
                }),
            })
        }
        TokenKind::TMult if allow_specifiers => {
            // declare export * from 'foo'
            let loc = peek::loc(env).dupe();
            expect::token(env, TokenKind::TMult)?;
            let local_name = match peek::token(env) {
                TokenKind::TIdentifier { raw, .. } if raw == "as" => {
                    eat::token(env)?;
                    Some(main_parser::parse_identifier(env, None)?)
                }
                _ => None,
            };
            let specifiers = Some(
                statement::export_named_declaration::Specifier::ExportBatchSpecifier(
                    statement::export_named_declaration::ExportBatchSpecifier {
                        loc,
                        specifier: local_name,
                    },
                ),
            );
            let (source, trailing) = export_source_and_semicolon(env)?;
            let comments = ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into()));
            Ok(StatementInner::DeclareExportDeclaration {
                loc: LOC_NONE,
                inner: Arc::new(statement::DeclareExportDeclaration {
                    default: None,
                    declaration: None,
                    specifiers,
                    source: Some(source),
                    comments,
                }),
            })
        }
        TokenKind::TType => {
            // declare export type = ...
            let (loc, declaration) = with_loc(None, env, |env| type_alias_helper(env, Vec::new()))?;
            let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
            Ok(StatementInner::DeclareExportDeclaration {
                loc: LOC_NONE,
                inner: Arc::new(statement::DeclareExportDeclaration {
                    default: None,
                    declaration: Some(
                        statement::declare_export_declaration::Declaration::NamedType {
                            loc,
                            declaration: Arc::new(declaration),
                        },
                    ),
                    specifiers: None,
                    source: None,
                    comments,
                }),
            })
        }
        TokenKind::TOpaque => {
            // declare export opaque type = ...
            let (loc, declaration) =
                with_loc(None, env, |env| opaque_type_helper(env, true, Vec::new()))?;
            let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
            Ok(StatementInner::DeclareExportDeclaration {
                loc: LOC_NONE,
                inner: Arc::new(statement::DeclareExportDeclaration {
                    default: None,
                    declaration: Some(
                        statement::declare_export_declaration::Declaration::NamedOpaqueType {
                            loc,
                            declaration: Arc::new(declaration),
                        },
                    ),
                    specifiers: None,
                    source: None,
                    comments,
                }),
            })
        }
        TokenKind::TInterface => {
            // declare export interface ...
            let (loc, declaration) = with_loc(None, env, |env| interface_helper(env, Vec::new()))?;
            let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
            Ok(StatementInner::DeclareExportDeclaration {
                loc: LOC_NONE,
                inner: Arc::new(statement::DeclareExportDeclaration {
                    default: None,
                    declaration: Some(
                        statement::declare_export_declaration::Declaration::Interface {
                            loc,
                            declaration: Arc::new(declaration),
                        },
                    ),
                    specifiers: None,
                    source: None,
                    comments,
                }),
            })
        }
        TokenKind::TEnum if parse_enums => {
            // declare export enum ...
            let (loc, declaration) = with_loc(None, env, |env| {
                enum_parser::declaration(Vec::new(), false, env)
            })?;
            let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
            Ok(StatementInner::DeclareExportDeclaration {
                loc: LOC_NONE,
                inner: Arc::new(statement::DeclareExportDeclaration {
                    default: None,
                    declaration: Some(statement::declare_export_declaration::Declaration::Enum {
                        loc,
                        declaration: Arc::new(declaration),
                    }),
                    specifiers: None,
                    source: None,
                    comments,
                }),
            })
        }
        TokenKind::TIdentifier { raw, .. }
            if raw == "namespace" || (raw == "module" && is_d_ts) =>
        {
            // declare export namespace/module X { ... }
            let (loc, declaration) = with_loc(None, env, |env| {
                let keyword = match peek::token(env) {
                    TokenKind::TIdentifier { raw, .. } if raw == "module" => {
                        expect::identifier(env, "module")?;
                        statement::declare_namespace::Keyword::Module
                    }
                    _ => {
                        expect::identifier(env, "namespace")?;
                        statement::declare_namespace::Keyword::Namespace
                    }
                };
                let mut id = main_parser::parse_identifier(env, None)?;
                comment_attachment::id_remove_trailing(env, &mut id);
                let id = statement::declare_namespace::Id::Local(id);
                let body = declare_module_or_namespace_body(env)?;
                let comments = ast_utils::mk_comments_opt::<Loc>(None, None);
                Ok(statement::DeclareNamespace {
                    id,
                    body,
                    comments,
                    implicit_declare: false,
                    keyword,
                })
            })?;
            let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
            Ok(StatementInner::DeclareExportDeclaration {
                loc: LOC_NONE,
                inner: Arc::new(statement::DeclareExportDeclaration {
                    default: None,
                    declaration: Some(
                        statement::declare_export_declaration::Declaration::Namespace {
                            loc,
                            declaration: Box::new(declaration),
                        },
                    ),
                    specifiers: None,
                    source: None,
                    comments,
                }),
            })
        }
        _ if allow_specifiers => {
            // declare export { ... } [from ...]
            expect::token(env, TokenKind::TLcurly)?;
            let specifiers = export_specifiers(env)?;
            expect::token(env, TokenKind::TRcurly)?;
            let (source, trailing) = match peek::token(env) {
                TokenKind::TIdentifier { raw, .. } if raw == "from" => {
                    let (source, trailing) = export_source_and_semicolon(env)?;
                    (Some(source), trailing)
                }
                _ => {
                    assert_export_specifier_identifiers(env, &specifiers)?;
                    let trailing = match semicolon(env, None, true)? {
                        SemicolonType::Explicit(trailing) => trailing,
                        SemicolonType::Implicit(result) => result.trailing,
                    };
                    (None, trailing)
                }
            };
            let comments = ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into()));
            Ok(StatementInner::DeclareExportDeclaration {
                loc: LOC_NONE,
                inner: Arc::new(statement::DeclareExportDeclaration {
                    default: None,
                    declaration: None,
                    specifiers: Some(
                        statement::export_named_declaration::Specifier::ExportSpecifiers(
                            specifiers,
                        ),
                    ),
                    source,
                    comments,
                }),
            })
        }
        _ => {
            env.error_unexpected(Some(err_expected.to_owned()))?;
            Ok(StatementInner::DeclareExportDeclaration {
                loc: LOC_NONE,
                inner: Arc::new(statement::DeclareExportDeclaration {
                    default: None,
                    declaration: None,
                    specifiers: None,
                    source: None,
                    comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                }),
            })
        }
    }
}

// declare export ...
fn declare_export_declaration(
    env: &mut ParserEnv,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, mut s) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TDeclare)?;
        env.with_strict(true, |env| {
            env.with_in_export(true, |env| {
                 let leading = {
                    let mut l = leading;
                    l.extend(peek::comments(env));
                    l
                };
                expect::token(env, TokenKind::TExport)?;
                   declare_export_declaration_body(
                    env,
                    leading,
                    true,
                    "a declaration, export specifier, or export batch specifier after 'declare export'",
                )
            })
        })
    })?;
    *s.loc_mut() = loc;
    Ok(statement::Statement::new(s))
}

fn import_attributes(
    env: &mut ParserEnv,
) -> Result<
    Option<(
        Loc,
        Vec<statement::import_declaration::ImportAttribute<Loc, Loc>>,
    )>,
    Rollback,
> {
    match peek::token(env) {
        TokenKind::TWith => with_loc_opt(None, env, |env| {
            eat::token(env)?; // consume 'with'
            expect::token(env, TokenKind::TLcurly)?;

            let mut attributes = Vec::new();
            loop {
                match peek::token(env) {
                    TokenKind::TRcurly => break,
                    TokenKind::TEof => {
                        env.error_unexpected(Some("'}'".to_owned()))?;
                        break;
                    }
                    _ => {
                        if !attributes.is_empty() {
                            if !eat::maybe(env, TokenKind::TComma)? {
                                env.error(ParseError::ImportAttributeMissingComma)?;
                            }
                        }
                        // Allow trailing comma
                        if matches!(peek::token(env), TokenKind::TRcurly) {
                            break;
                        }
                        attributes.push({
                            let (loc, (key, value)) = with_loc(None, env, parse_attribute)?;
                            statement::import_declaration::ImportAttribute { loc, key, value }
                        });
                    }
                }
            }

            expect::token(env, TokenKind::TRcurly)?;
            Ok(Some(attributes))
        }),
        _ => Ok(None),
    }
}

fn parse_attribute(
    env: &mut ParserEnv,
) -> Result<
    (
        statement::import_declaration::ImportAttributeKey<Loc, Loc>,
        (Loc, StringLiteral<Loc>),
    ),
    Rollback,
> {
    let key = match peek::token(env) {
        TokenKind::TString(loc, value, raw, octal) => {
            let loc = loc.dupe();
            let value = value.to_owned();
            let raw = raw.to_owned();
            let octal = *octal;
            let (loc, lit) = string_literal(env, loc, value, raw, octal)?;
            statement::import_declaration::ImportAttributeKey::StringLiteral(loc, lit)
        }
        _ => {
            let id = identifier_name(env)?;
            statement::import_declaration::ImportAttributeKey::Identifier(id)
        }
    };

    expect::token(env, TokenKind::TColon)?;

    let value = match peek::token(env) {
        TokenKind::TString(loc, value, raw, octal) => {
            let loc = loc.dupe();
            let value = value.to_owned();
            let raw = raw.to_owned();
            let octal = *octal;
            string_literal(env, loc, value, raw, octal)?
        }
        _ => {
            env.error_unexpected(Some("string literal".to_owned()))?;
            let loc = peek::loc_skip_lookahead(env);
            (
                loc,
                StringLiteral {
                    value: FlowSmolStr::new_inline(""),
                    raw: FlowSmolStr::new_inline(""),
                    comments: None,
                },
            )
        }
    };

    Ok((key, value))
}

fn semicolon_and_trailing(
    env: &mut ParserEnv,
    source: &mut (Loc, StringLiteral<Loc>),
) -> Result<Vec<Comment<Loc>>, Rollback> {
    match semicolon(env, None, true)? {
        SemicolonType::Explicit(trailing) => Ok(trailing),
        SemicolonType::Implicit(result) => {
            if let Some(mut remover) = result.remover {
                source.1 = remover.map_string_literal(&source.1);
            }
            Ok(Vec::new())
        }
    }
}

fn import_declaration(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    fn missing_source(env: &mut ParserEnv) -> (Loc, StringLiteral<Loc>) {
        let loc = peek::loc_skip_lookahead(env);
        (
            loc,
            StringLiteral {
                value: FlowSmolStr::new_inline(""),
                raw: FlowSmolStr::new_inline(""),
                comments: None,
            },
        )
    }

    fn source(env: &mut ParserEnv) -> Result<(Loc, StringLiteral<Loc>), Rollback> {
        match peek::token(env) {
            TokenKind::TIdentifier { raw, .. } if raw == "from" => {
                eat::token(env)?;
                match peek::token(env) {
                    TokenKind::TString(loc, value, raw, octal) => {
                        let loc = loc.dupe();
                        let value = value.to_owned();
                        let raw = raw.to_owned();
                        let octal = *octal;
                        string_literal(env, loc, value, raw, octal)
                    }
                    _ => {
                        env.error_unexpected(Some("a string".to_owned()))?;
                        Ok(missing_source(env))
                    }
                }
            }
            _ => {
                env.error_unexpected(Some("the keyword `from`".to_owned()))?;
                Ok(missing_source(env))
            }
        }
    }

    fn is_type_import(token: &TokenKind) -> bool {
        matches!(token, TokenKind::TType | TokenKind::TTypeof)
    }

    // `x` or `x as y` in a specifier
    fn with_maybe_as(
        env: &mut ParserEnv,
        for_type: bool,
        error_if_type: Option<ParseError>,
    ) -> Result<(Identifier<Loc, Loc>, Option<Identifier<Loc, Loc>>), Rollback> {
        fn check_identifier(
            env: &mut ParserEnv,
            for_type: bool,
            id: &Identifier<Loc, Loc>,
        ) -> Result<(), Rollback> {
            if for_type {
                if is_reserved_type(&id.name) {
                    env.error_at(id.loc.dupe(), ParseError::UnexpectedReservedType)?;
                }
            } else {
                assert_identifier_name_is_identifier(None, env, id)?;
            }
            Ok(())
        }

        let current_is_type_import = is_type_import(peek::token(env));
        let remote = identifier_name(env)?;
        match peek::token(env) {
            TokenKind::TIdentifier { raw, .. } if raw == "as" => {
                eat::token(env)?;
                let local = if for_type {
                    type_parser::type_identifier(env)?
                } else {
                    main_parser::parse_identifier(env, None)?
                };
                Ok((remote, Some(local)))
            }
            TokenKind::TEof | TokenKind::TComma | TokenKind::TRcurly => {
                check_identifier(env, for_type, &remote)?;
                Ok((remote, None))
            }
            _ => {
                if current_is_type_import {
                    if let Some(err) = error_if_type {
                        env.error_at(remote.loc.dupe(), err)?;
                        return Ok((type_parser::type_identifier(env)?, None));
                    }
                }
                check_identifier(env, for_type, &remote)?;
                Ok((remote, None))
            }
        }
    }

    /*
          ImportSpecifier[Type]:
            [~Type] ImportedBinding
            [~Type] IdentifierName ImportedTypeBinding
            [~Type] IdentifierName IdentifierName ImportedBinding
            [~Type] IdentifierName IdentifierName IdentifierName ImportedTypeBinding
            [+Type] ImportedTypeBinding
            [+Type] IdentifierName IdentifierName ImportedTypeBinding

          Static Semantics:

          `IdentifierName ImportedTypeBinding`:
          - It is a Syntax Error if IdentifierName's StringValue is not "type" or "typeof"

          `IdentifierName IdentifierName ImportedBinding`:
          - It is a Syntax Error if the second IdentifierName's StringValue is not "as"

          `IdentifierName IdentifierName IdentifierName  ImportedTypeBinding`:
          - It is a Syntax Error if the first IdentifierName's StringValue is not "type"
            or "typeof", and the third IdentifierName's StringValue is not "as"
    */
    fn specifier(
        env: &mut ParserEnv,
    ) -> Result<statement::import_declaration::NamedSpecifier<Loc, Loc>, Rollback> {
        let kind = match peek::token(env) {
            TokenKind::TType => Some(statement::ImportKind::ImportType),
            TokenKind::TTypeof => Some(statement::ImportKind::ImportTypeof),
            _ => None,
        };

        let specifier = if is_type_import(peek::token(env)) {
            // consume `type`, but we don't know yet whether this is `type foo` or
            // `type as foo`.
            let type_keyword_or_remote = identifier_name(env)?;
            // If [kind] survives in a given branch, [type_keyword_or_remote]
            // was the per-specifier keyword and its loc is the keyword loc.
            let kind_loc = Some(type_keyword_or_remote.loc.clone());
            match peek::token(env) {
                TokenKind::TEof | TokenKind::TRcurly | TokenKind::TComma => {
                    // `type` (a value)
                    let remote = type_keyword_or_remote;
                    // `type` becomes a value
                    assert_identifier_name_is_identifier(None, env, &remote)?;
                    Ok(statement::import_declaration::NamedSpecifier {
                        remote,
                        local: None,
                        remote_name_def_loc: None,
                        kind: None,
                        kind_loc: None,
                    })
                }
                // `type as foo` (value named `type`) or `type as,` (type named `as`)
                TokenKind::TIdentifier { raw, .. } if raw == "as" => {
                    let remote_or_as = type_parser::type_identifier(env)?;
                    match peek::token(env) {
                        TokenKind::TEof | TokenKind::TRcurly | TokenKind::TComma => {
                            // `type as`
                            Ok(statement::import_declaration::NamedSpecifier {
                                remote: remote_or_as,
                                remote_name_def_loc: None,
                                local: None,
                                kind,
                                kind_loc,
                            })
                        }
                        TokenKind::TIdentifier { raw: as_raw, .. } if as_raw == "as" => {
                            // `type as as foo`
                            // first "as"
                            eat::token(env)?;
                            // second `as`
                            let local = Some(type_parser::type_identifier(env)?);
                            // foo
                            Ok(statement::import_declaration::NamedSpecifier {
                                remote: remote_or_as,
                                remote_name_def_loc: None,
                                local,
                                kind,
                                kind_loc,
                            })
                        }
                        _ => {
                            // `type as foo`
                            let remote = type_keyword_or_remote;
                            // `type` becomes a value
                            assert_identifier_name_is_identifier(None, env, &remote)?;
                            let local = Some(main_parser::parse_identifier(env, None)?);
                            Ok(statement::import_declaration::NamedSpecifier {
                                remote,
                                remote_name_def_loc: None,
                                local,
                                kind: None,
                                kind_loc: None,
                            })
                        }
                    }
                }
                _ => {
                    // `type x`, or `type x as y`
                    let (remote, local) = with_maybe_as(env, true, None)?;
                    Ok(statement::import_declaration::NamedSpecifier {
                        remote,
                        remote_name_def_loc: None,
                        local,
                        kind,
                        kind_loc,
                    })
                }
            }
        } else {
            // standard `x` or `x as y`
            let (remote, local) = with_maybe_as(env, false, None)?;
            Ok(statement::import_declaration::NamedSpecifier {
                remote,
                remote_name_def_loc: None,
                local,
                kind: None,
                kind_loc: None,
            })
        }?;
        if specifier.kind.is_some() && !env.should_parse_types() {
            env.error(ParseError::UnexpectedTypeImport)?;
        }
        Ok(specifier)
    }

    // specifier in an `import type { ... }`
    fn type_specifier(
        env: &mut ParserEnv,
    ) -> Result<statement::import_declaration::NamedSpecifier<Loc, Loc>, Rollback> {
        let (remote, local) = with_maybe_as(
            env,
            true,
            Some(ParseError::ImportTypeShorthandOnlyInPureImport),
        )?;
        Ok(statement::import_declaration::NamedSpecifier {
            remote,
            remote_name_def_loc: None,
            local,
            kind: None,
            kind_loc: None,
        })
    }

    // specifier in an `import typeof { ... }`
    fn typeof_specifier(
        env: &mut ParserEnv,
    ) -> Result<statement::import_declaration::NamedSpecifier<Loc, Loc>, Rollback> {
        let (remote, local) = with_maybe_as(
            env,
            true,
            Some(ParseError::ImportTypeShorthandOnlyInPureImport),
        )?;
        Ok(statement::import_declaration::NamedSpecifier {
            remote,
            remote_name_def_loc: None,
            local,
            kind: None,
            kind_loc: None,
        })
    }

    fn specifier_list(
        env: &mut ParserEnv,
        statement_kind: statement::ImportKind,
    ) -> Result<Vec<statement::import_declaration::NamedSpecifier<Loc, Loc>>, Rollback> {
        let mut acc = Vec::new();
        let mut preceding_comma = true;
        loop {
            match peek::token(env) {
                TokenKind::TEof | TokenKind::TRcurly => {
                    return Ok(acc);
                }
                _ => {
                    if !preceding_comma {
                        env.error(ParseError::ImportSpecifierMissingComma)?;
                    }
                    let spec = match statement_kind {
                        statement::ImportKind::ImportType => type_specifier(env)?,
                        statement::ImportKind::ImportTypeof => typeof_specifier(env)?,
                        statement::ImportKind::ImportValue => specifier(env)?,
                    };
                    preceding_comma = eat::maybe(env, TokenKind::TComma)?;
                    acc.push(spec);
                }
            }
        }
    }

    fn named_or_namespace_specifier(
        env: &mut ParserEnv,
        import_kind: statement::ImportKind,
    ) -> Result<Option<statement::import_declaration::Specifier<Loc, Loc>>, Rollback> {
        match peek::token(env) {
            TokenKind::TMult => {
                let id = with_loc_opt(None, env, |env| {
                    // consume *
                    eat::token(env)?;
                    match peek::token(env) {
                        TokenKind::TIdentifier { raw, .. } if raw == "as" => {
                            // consume "as"
                            eat::token(env)?;
                            match import_kind {
                                statement::ImportKind::ImportType
                                | statement::ImportKind::ImportTypeof => {
                                    Ok(Some(type_parser::type_identifier(env)?))
                                }
                                statement::ImportKind::ImportValue => {
                                    Ok(Some(main_parser::parse_identifier(env, None)?))
                                }
                            }
                        }
                        _ => {
                            env.error_unexpected(Some("the keyword `as`".to_owned()))?;
                            Ok(None)
                        }
                    }
                })?;
                Ok(id.map(statement::import_declaration::Specifier::ImportNamespaceSpecifier))
            }
            _ => {
                expect::token(env, TokenKind::TLcurly)?;
                let specifiers = specifier_list(env, import_kind)?;
                expect::token(env, TokenKind::TRcurly)?;
                Ok(Some(
                    statement::import_declaration::Specifier::ImportNamedSpecifiers(specifiers),
                ))
            }
        }
    }

    /// Rejects the specifier forms a phase modifier does not accept. The defer
    /// phase takes only a `NameSpaceImport`, so a named specifier list reaching
    /// here with the phase is an error. `source`, when it lands, accepts exactly
    /// the default-binding form and so will want the inverse: rejecting both
    /// specifier shapes here and accepting the default in
    /// [check_phase_default].
    fn check_phase_specifiers(
        env: &mut ParserEnv,
        phase: Option<statement::ImportPhase>,
        specifiers: &Option<statement::import_declaration::Specifier<Loc, Loc>>,
        specifier_loc: Loc,
    ) -> Result<(), Rollback> {
        match (phase, specifiers) {
            (
                Some(statement::ImportPhase::Defer),
                Some(statement::import_declaration::Specifier::ImportNamedSpecifiers(_)),
            ) => env.error_at(specifier_loc, ParseError::ImportDeferPhaseRequiresNamespace),
            _ => Ok(()),
        }
    }

    /// The default-binding counterpart of [check_phase_specifiers]: no phase
    /// this parser implements accepts one.
    fn check_phase_default(
        env: &mut ParserEnv,
        phase: Option<statement::ImportPhase>,
        default_loc: Loc,
    ) -> Result<(), Rollback> {
        match phase {
            None => Ok(()),
            Some(statement::ImportPhase::Defer) => {
                env.error_at(default_loc, ParseError::ImportDeferPhaseRequiresNamespace)
            }
        }
    }

    fn with_specifiers(
        env: &mut ParserEnv,
        import_kind: statement::ImportKind,
        phase: Option<statement::ImportPhase>,
        leading: Vec<Comment<Loc>>,
    ) -> Result<StatementInner<Loc, Loc>, Rollback> {
        let specifier_loc = peek::loc(env).dupe();
        let specifiers = named_or_namespace_specifier(env, import_kind)?;
        check_phase_specifiers(env, phase, &specifiers, specifier_loc)?;
        let mut source = source(env)?;
        let attributes = import_attributes(env)?;
        let trailing = semicolon_and_trailing(env, &mut source)?;
        Ok(StatementInner::ImportDeclaration {
            loc: LOC_NONE,
            inner: Arc::new(statement::ImportDeclaration {
                import_kind,
                phase,
                source,
                specifiers,
                default: None,
                attributes,
                comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            }),
        })
    }

    fn with_default(
        env: &mut ParserEnv,
        import_kind: statement::ImportKind,
        phase: Option<statement::ImportPhase>,
        leading: Vec<Comment<Loc>>,
    ) -> Result<StatementInner<Loc, Loc>, Rollback> {
        let default_specifier = match import_kind {
            statement::ImportKind::ImportType | statement::ImportKind::ImportTypeof => {
                statement::import_declaration::DefaultIdentifier {
                    identifier: type_parser::type_identifier(env)?,
                    remote_default_name_def_loc: None,
                }
            }
            statement::ImportKind::ImportValue => {
                statement::import_declaration::DefaultIdentifier {
                    identifier: main_parser::parse_identifier(env, None)?,
                    remote_default_name_def_loc: None,
                }
            }
        };
        with_default_identifier(env, import_kind, phase, leading, default_specifier)
    }

    fn with_default_identifier(
        env: &mut ParserEnv,
        import_kind: statement::ImportKind,
        phase: Option<statement::ImportPhase>,
        leading: Vec<Comment<Loc>>,
        default_specifier: statement::import_declaration::DefaultIdentifier<Loc, Loc>,
    ) -> Result<StatementInner<Loc, Loc>, Rollback> {
        // `import defer x from "m"` — the phase takes a namespace, so the
        // default binding is the error. It is reported once, here: with
        // `import defer x, { y } from "m"` the named specifiers are not
        // separately at fault.
        check_phase_default(env, phase, default_specifier.identifier.loc.dupe())?;
        let additional_specifiers = match peek::token(env) {
            TokenKind::TComma => {
                // `import Foo, ...`
                expect::token(env, TokenKind::TComma)?;
                named_or_namespace_specifier(env, import_kind)?
            }
            _ => None,
        };

        let mut source = source(env)?;
        let attributes = import_attributes(env)?;
        let trailing = semicolon_and_trailing(env, &mut source)?;
        Ok(StatementInner::ImportDeclaration {
            loc: LOC_NONE,
            inner: Arc::new(statement::ImportDeclaration {
                import_kind,
                phase,
                source,
                specifiers: additional_specifiers,
                default: Some(default_specifier),
                attributes,
                comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            }),
        })
    }

    let (loc, mut s) = with_loc(None, env, |env| {
        env.with_strict(true, |env| {
            let leading = peek::comments(env);
            expect::token(env, TokenKind::TImport)?;

            let should_parse_types = env.should_parse_types();
            let current_token = peek::token(env).clone();

            match current_token {
                TokenKind::TMult => {
                    // `import * as ns from "ModuleName";`
                    with_specifiers(env, statement::ImportKind::ImportValue, None, leading)
                }
                TokenKind::TLcurly => {
                    // `import { ... } from "ModuleName";`
                    with_specifiers(env, statement::ImportKind::ImportValue, None, leading)
                }
                // `import "ModuleName";`
                TokenKind::TString(loc, value, raw, octal) => {
                    let mut source = string_literal(env, loc, value, raw, octal)?;
                    let attributes = import_attributes(env)?;
                    let trailing = semicolon_and_trailing(env, &mut source)?;
                    Ok(StatementInner::ImportDeclaration {
                        loc: LOC_NONE,
                        inner: Arc::new(statement::ImportDeclaration {
                            import_kind: statement::ImportKind::ImportValue,
                            phase: None,
                            source,
                            specifiers: None,
                            default: None,
                            attributes,
                            comments: ast_utils::mk_comments_opt(
                                Some(leading.into()),
                                Some(trailing.into()),
                            ),
                        }),
                    })
                }
                // `import type [...] from "ModuleName";`
                // note that if [...] is missing, we're importing a value named `type`!
                TokenKind::TType if should_parse_types => {
                    let type_id = main_parser::parse_identifier(env, None)?;
                    let type_default = statement::import_declaration::DefaultIdentifier {
                        identifier: type_id,
                        remote_default_name_def_loc: None,
                    };
                    match peek::token(env) {
                        // `import type, { other, names } from "ModuleName";`
                        TokenKind::TComma => with_default_identifier(
                            env,
                            statement::ImportKind::ImportValue,
                            None,
                            leading,
                            type_default,
                        ),
                        // `import type from "ModuleName";`
                        TokenKind::TIdentifier { raw, .. } if raw == "from" => {
                            // Importing the exported value named "type". This is not a type-import.
                            with_default_identifier(
                                env,
                                statement::ImportKind::ImportValue,
                                None,
                                leading,
                                type_default,
                            )
                        }
                        // `import type = ...`: 'type' is the binding name, not a keyword
                        TokenKind::TAssign => import_equals_declaration_with_id(
                            env,
                            statement::ImportKind::ImportValue,
                            false,
                            leading,
                            type_default.identifier,
                        )
                        .map(|inner| {
                            StatementInner::ImportEqualsDeclaration {
                                loc: LOC_NONE,
                                inner,
                            }
                        }),
                        TokenKind::TMult => {
                            with_specifiers(env, statement::ImportKind::ImportType, None, leading)
                        }
                        TokenKind::TLcurly => {
                            with_specifiers(env, statement::ImportKind::ImportType, None, leading)
                        }
                        _ => {
                            // Check for import type Foo = ...
                            let id = type_parser::type_identifier(env)?;
                            let default_specifier =
                                statement::import_declaration::DefaultIdentifier {
                                    identifier: id,
                                    remote_default_name_def_loc: None,
                                };
                            if peek::token(env) == &TokenKind::TAssign {
                                import_equals_declaration_with_id(
                                    env,
                                    statement::ImportKind::ImportType,
                                    false,
                                    leading,
                                    default_specifier.identifier,
                                )
                                .map(|inner| {
                                    StatementInner::ImportEqualsDeclaration {
                                        loc: LOC_NONE,
                                        inner,
                                    }
                                })
                            } else {
                                with_default_identifier(
                                    env,
                                    statement::ImportKind::ImportType,
                                    None,
                                    leading,
                                    default_specifier,
                                )
                            }
                        }
                    }
                }
                // `import typeof ... from "ModuleName";`
                TokenKind::TTypeof if should_parse_types => {
                    expect::token(env, TokenKind::TTypeof)?;
                    match peek::token(env) {
                        TokenKind::TMult | TokenKind::TLcurly => {
                            with_specifiers(env, statement::ImportKind::ImportTypeof, None, leading)
                        }
                        _ => with_default(env, statement::ImportKind::ImportTypeof, None, leading),
                    }
                }
                // `import defer * as ns from "ModuleName";` — deferred
                // evaluation.
                //
                // `defer` is contextual, so it is only the phase modifier when
                // the import's binding follows it. `import defer from "m"`,
                // `import defer, { x } from "m"` and `import defer =
                // require("m")` all still bind a local named `defer`.
                TokenKind::TIdentifier { ref raw, .. } if raw == "defer" => {
                    let defer_id = main_parser::parse_identifier(env, None)?;
                    let defer_default = statement::import_declaration::DefaultIdentifier {
                        identifier: defer_id,
                        remote_default_name_def_loc: None,
                    };
                    let after_defer = peek::token(env).clone();
                    match after_defer {
                        // `import defer, { other } from "ModuleName";`
                        TokenKind::TComma => with_default_identifier(
                            env,
                            statement::ImportKind::ImportValue,
                            None,
                            leading,
                            defer_default,
                        ),
                        // `import defer = require("ModuleName");`
                        TokenKind::TAssign => import_equals_declaration_with_id(
                            env,
                            statement::ImportKind::ImportValue,
                            false,
                            leading,
                            defer_default.identifier,
                        )
                        .map(|inner| {
                            StatementInner::ImportEqualsDeclaration {
                                loc: LOC_NONE,
                                inner,
                            }
                        }),
                        // `import defer from ...`: the `from` is this import's
                        // clause keyword when a module specifier follows it,
                        // and a (rejected) default binding under the phase
                        // otherwise (`import defer from from "ModuleName";`).
                        TokenKind::TIdentifier { ref raw, .. }
                            if raw == "from" && peek::token_after_current_is_string(env) =>
                        {
                            with_default_identifier(
                                env,
                                statement::ImportKind::ImportValue,
                                None,
                                leading,
                                defer_default,
                            )
                        }
                        // Phase modifier. The binding forms it does not accept
                        // are still parsed, so that `import defer x from "m"`
                        // reports the phase error rather than a cascade of
                        // token errors.
                        _ => {
                            let phase = Some(statement::ImportPhase::Defer);
                            match peek::token(env) {
                                TokenKind::TMult | TokenKind::TLcurly => with_specifiers(
                                    env,
                                    statement::ImportKind::ImportValue,
                                    phase,
                                    leading,
                                ),
                                _ => with_default(
                                    env,
                                    statement::ImportKind::ImportValue,
                                    phase,
                                    leading,
                                ),
                            }
                        }
                    }
                }
                // `import <phase> x from "ModuleName";` for a phase this
                // parser does not implement, `source` being the one that
                // exists today. An identifier followed by another identifier
                // can only be a phase modifier — `from` excepted, since
                // `import x from "m"` is the unphased import — so no phase is
                // named here and any future one taking the default-binding
                // form is covered; the defer phase is claimed by the arm above
                // and never reaches this, and `type`/`typeof` are keyword
                // tokens with arms of their own. Reporting and then parsing the
                // default import anyway keeps it to one error rather than a
                // cascade, and leaves an AST that differs from the intended one
                // only in the phase.
                TokenKind::TIdentifier { ref raw, .. }
                    if peek::token_after_current_is_identifier_other_than_from(env) =>
                {
                    let phase_loc = peek::loc(env).dupe();
                    let phase = raw.to_string();
                    eat::token(env)?;
                    env.error_at(phase_loc, ParseError::ImportPhaseUnsupported(phase))?;
                    with_default(env, statement::ImportKind::ImportValue, None, leading)
                }
                // import Foo from "ModuleName"; or import Foo = ...
                // Check for import equals: import Foo = ...
                _ => {
                    let id = main_parser::parse_identifier(env, None)?;
                    let default_specifier = statement::import_declaration::DefaultIdentifier {
                        identifier: id,
                        remote_default_name_def_loc: None,
                    };
                    if peek::token(env) == &TokenKind::TAssign {
                        import_equals_declaration_with_id(
                            env,
                            statement::ImportKind::ImportValue,
                            false,
                            leading,
                            default_specifier.identifier,
                        )
                        .map(|inner| {
                            StatementInner::ImportEqualsDeclaration {
                                loc: LOC_NONE,
                                inner,
                            }
                        })
                    } else {
                        with_default_identifier(
                            env,
                            statement::ImportKind::ImportValue,
                            None,
                            leading,
                            default_specifier,
                        )
                    }
                }
            }
        })
    })?;
    *s.loc_mut() = loc;
    Ok(statement::Statement::new(s))
}

fn parse_statement_list_item(
    env: &mut ParserEnv,
    decorators: Vec<class::Decorator<Loc, Loc>>,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    if !peek::is_class(env) {
        env.error_on_decorators(&decorators)?;
    }

    let parse_enums = env.parse_options().enums;
    let in_ambient_context = env.in_ambient_context();
    let is_d_ts = env.is_d_ts();
    let current_token = peek::token(env).clone();
    match &current_token {
        // Remember kids, these look like statements but they're not
        // statements... (see section 13)
        TokenKind::TLet => let_statement(env),
        TokenKind::TConst => {
            let start_loc = peek::loc(env).dupe();
            let leading = peek::comments(env);
            eat::token(env)?;
            if parse_enums && peek::token(env) == &TokenKind::TEnum {
                declaration_parser::parse_enum_declaration(env, Some(start_loc), None, true)
            } else {
                const_statement_after_const(env, start_loc, leading)
            }
        }
        TokenKind::TInterface => interface(env),
        TokenKind::TDeclare => declare(env, false),
        TokenKind::TType => type_alias(env),
        TokenKind::TOpaque => opaque_type(env),
        TokenKind::TEnum if parse_enums => {
            declaration_parser::parse_enum_declaration(env, None, None, false)
        }
        TokenKind::TIdentifier { raw, .. } if raw == "namespace" && in_ambient_context => {
            declare_namespace(env, false, true)
        }
        TokenKind::TIdentifier { raw, .. }
            if raw == "module"
                && in_ambient_context
                && is_d_ts
                && peek::token_after_current_is_identifier_token(env) =>
        {
            declare_namespace(env, false, true)
        }
        TokenKind::TIdentifier { raw, .. }
            if raw == "global"
                && in_ambient_context
                && peek::token_after_current_is_lcurly(env) =>
        {
            declare_namespace(env, true, true)
        }
        _ => {
            if peek::is_function(env) || peek::is_hook(env) {
                declaration_parser::parse_function(env)
            } else if peek::is_class(env) {
                object_parser::class_declaration(env, decorators)
            } else if peek::is_record(env) {
                object_parser::record_declaration(env)
            } else if peek::is_component(env) {
                declaration_parser::parse_component(env)
            } else {
                parse_statement(env, true)
            }
        }
    }
}

fn parse_statement(
    env: &mut ParserEnv,
    allow_sequence: bool,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let parse_pattern_matching = env.parse_options().pattern_matching;
    match peek::token(env) {
        TokenKind::TEof => {
            env.error_unexpected(Some("the start of a statement".to_string()))?;
            let loc = peek::loc(env).dupe();
            Ok(statement::Statement::new(StatementInner::Empty {
                loc,
                inner: Arc::new(statement::Empty { comments: None }),
            }))
        }
        TokenKind::TSemicolon => empty_statement(env),
        TokenKind::TLcurly => block(env),
        TokenKind::TVar => var_statement(env),
        TokenKind::TBreak => break_statement(env),
        TokenKind::TContinue => continue_statement(env),
        TokenKind::TDebugger => debugger(env),
        TokenKind::TDo => do_while(env),
        TokenKind::TFor => for_loop(env),
        TokenKind::TIf => if_statement(env),
        TokenKind::TReturn => return_statement(env),
        TokenKind::TSwitch => switch(env),
        TokenKind::TMatch if parse_pattern_matching => {
            if peek::match_token_is_followed_by_lparen_on_same_line(env) {
                return match try_parse::to_parse(env, match_statement) {
                    try_parse::ParseResult::ParsedSuccessfully(m) => Ok(m),
                    try_parse::ParseResult::FailedToParse => expression(env, allow_sequence),
                };
            }
            if peek::is_identifier(env) {
                maybe_labeled(env)
            } else {
                expression(env, allow_sequence)
            }
        }
        TokenKind::TThrow => throw(env),
        TokenKind::TTry => try_statement(env),
        TokenKind::TWhile => while_statement(env),
        TokenKind::TWith => with_statement(env),
        // If we see an else then it's definitely an error, but we can probably
        // assume that this is a malformed if statement that is missing the if
        TokenKind::TElse => if_statement(env),
        // There are a bunch of tokens that aren't the start of any valid
        // statement. We list them here in order to skip over them, rather than
        // getting stuck
        TokenKind::TColon
        | TokenKind::TRparen
        | TokenKind::TRcurly
        | TokenKind::TRbracket
        | TokenKind::TComma
        | TokenKind::TPeriod
        | TokenKind::TPlingPeriod
        | TokenKind::TArrow
        | TokenKind::TIn
        | TokenKind::TInstanceof
        | TokenKind::TCatch
        | TokenKind::TFinally
        | TokenKind::TCase
        | TokenKind::TDefault
        | TokenKind::TExtends
        | TokenKind::TStatic
        | TokenKind::TExport
        | TokenKind::TEllipsis => {
            env.error_unexpected(Some("the start of a statement".to_string()))?;
            eat::token(env)?;
            parse_statement(env, allow_sequence)
        }
        t => {
            // The rest of these patterns handle ExpressionStatement and its negative
            // lookaheads, which prevent ambiguities.
            // See https://tc39.github.io/ecma262/#sec-expression-statement
            if t == &TokenKind::TLet {
                if let Some(next_loc) = peek::loc_after_current_if_lbracket(env) {
                    // `let [foo]` is ambiguous: either a let binding pattern, or a
                    // member expression, so it is banned.
                    let loc = {
                        let curr_loc = peek::loc(env).dupe();
                        Loc::between(&curr_loc, &next_loc)
                    };
                    env.error_at(loc, ParseError::AmbiguousLetBracket)?;
                    // recover as a member expression
                    expression(env, allow_sequence)
                } else if peek::is_function(env) || peek::is_hook(env) {
                    let func = declaration_parser::parse_function(env)?;
                    env.function_as_statement_error_at(func.loc().dupe())?;
                    Ok(func)
                } else if peek::is_identifier(env) {
                    maybe_labeled(env)
                } else if peek::is_class(env) {
                    env.error_unexpected(None)?;
                    eat::token(env)?;
                    expression(env, allow_sequence)
                } else {
                    expression(env, allow_sequence)
                }
            } else if peek::is_function(env) || peek::is_hook(env) {
                let func = declaration_parser::parse_function(env)?;
                env.function_as_statement_error_at(func.loc().dupe())?;
                Ok(func)
            } else if peek::is_identifier(env) {
                maybe_labeled(env)
            } else if peek::is_class(env) {
                env.error_unexpected(None)?;
                eat::token(env)?;
                expression(env, allow_sequence)
            } else {
                expression(env, allow_sequence)
            }
        }
    }
}

pub(super) fn parse_statement_list(
    env: &mut ParserEnv,
    is_terminal_token: impl Fn(&TokenKind) -> bool,
) -> Result<Vec<statement::Statement<Loc, Loc>>, Rollback> {
    let mut statements = Vec::new();
    loop {
        match peek::token(env) {
            TokenKind::TEof => return Ok(statements),
            t => {
                if is_terminal_token(t) {
                    return Ok(statements);
                }
                statements.push(parse_statement_list_item(env, Vec::new())?);
            }
        }
    }
}

/// (15.2)
fn parse_module_item(env: &mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let decorators = object_parser::parse_decorator_list(env)?;
    match peek::token(env) {
        TokenKind::TExport => export_declaration(env, decorators),
        TokenKind::TImport => {
            env.error_on_decorators(&decorators)?;
            if peek::current_token_is_followed_by_import_expression_continuation(env) {
                // import(...) or import.meta
                expression(env, true)
            } else {
                import_declaration(env)
            }
        }
        TokenKind::TDeclare => {
            if peek::declare_token_is_followed_by_export(env) {
                env.error_on_decorators(&decorators)?;
                declare_export_declaration(env)
            } else {
                parse_statement_list_item(env, decorators)
            }
        }
        _ => parse_statement_list_item(env, decorators),
    }
}

fn parse_module_body(
    env: &mut ParserEnv,
    is_terminal_token: impl Fn(&TokenKind) -> bool,
) -> Result<Vec<statement::Statement<Loc, Loc>>, Rollback> {
    let mut statements = Vec::new();
    loop {
        match peek::token(env) {
            TokenKind::TEof => return Ok(statements),
            t => {
                if is_terminal_token(t) {
                    return Ok(statements);
                }
                statements.push(parse_module_item(env)?);
            }
        }
    }
}

fn parse_directives<F>(
    env: &mut ParserEnv,
    is_terminal_token: &dyn Fn(&TokenKind) -> bool,
    mut item_fn: F,
) -> Result<(Vec<statement::Statement<Loc, Loc>>, bool), Rollback>
where
    F: FnMut(&mut ParserEnv) -> Result<statement::Statement<Loc, Loc>, Rollback>,
{
    env.with_allow_directive(true, |env| {
        let mut statements = Vec::new();
        let mut octal_locs = Vec::new();
        let mut contains_use_strict = false;
        loop {
            match peek::token(env) {
                TokenKind::TEof => break,
                token if is_terminal_token(token) => break,
                TokenKind::TString(str_token_loc, _, _, octal) => {
                    let str_token_loc_for_error = if *octal {
                        Some(str_token_loc.dupe())
                    } else {
                        None
                    };
                    let possible_directive =
                        env.with_strict(contains_use_strict, |env| item_fn(env))?;
                    let strict_directive = match possible_directive.deref() {
                        StatementInner::Expression { loc, inner } if inner.directive.is_some() => {
                            let raw = inner.directive.as_ref().unwrap();
                            if let Some(loc) = str_token_loc_for_error {
                                octal_locs.push(loc);
                            }
                            // 14.1.1 says that it has to be "use strict" without any
                            // escapes, so "use\x20strict" is disallowed.
                            Some((loc.dupe(), raw == "use strict"))
                        }
                        _ => None,
                    };
                    statements.push(possible_directive);

                    match strict_directive {
                        Some((loc, strict)) => {
                            contains_use_strict = contains_use_strict || strict;
                            if strict && !env.has_simple_parameters() {
                                env.error_at(loc.dupe(), ParseError::StrictParamNotSimple)?;
                            }
                        }
                        None => {
                            break;
                        }
                    }
                }
                _ => break,
            }
        }
        env.with_strict(env.in_strict_mode() || contains_use_strict, |env| {
            for loc in octal_locs {
                env.strict_error_at((loc, ParseError::StrictOctalLiteral))?;
            }
            Ok(())
        })?;
        Ok((statements, contains_use_strict))
    })
}

pub fn parse_module_body_with_directives(
    env: &mut ParserEnv,
) -> Result<Vec<statement::Statement<Loc, Loc>>, Rollback> {
    fn is_terminal_token(_token: &TokenKind) -> bool {
        false
    }
    let (directives, contains_use_strict) =
        parse_directives(env, &is_terminal_token, parse_module_item)?;
    let statements = env.with_strict(env.in_strict_mode() || contains_use_strict, |env| {
        parse_module_body(env, is_terminal_token)
    })?;
    // Prepend the directives
    Ok([directives, statements].concat())
}

pub(super) fn parse_statement_list_with_directives(
    env: &mut ParserEnv,
) -> Result<(Vec<statement::Statement<Loc, Loc>>, bool), Rollback> {
    fn is_terminal_token(token: &TokenKind) -> bool {
        token == &TokenKind::TRcurly
    }

    let (directives, contains_use_strict) = parse_directives(env, &is_terminal_token, |env| {
        parse_statement_list_item(env, Vec::new())
    })?;
    let statements = env.with_strict(env.in_strict_mode() || contains_use_strict, |env| {
        parse_statement_list(env, is_terminal_token)
    })?;
    // Prepend the directives
    let statements = [directives, statements].concat();
    Ok((statements, contains_use_strict))
}
