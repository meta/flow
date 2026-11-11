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

use crate::ast::expression::ExpressionInner;
use crate::ast::*;
use crate::ast_utils;
use crate::ast_visitor::AstVisitor;
use crate::comment_attachment;
use crate::declaration_parser;
use crate::expression_parser;
use crate::loc::LOC_NONE;
use crate::loc::Loc;
use crate::main_parser;
use crate::parse_error::DefiniteAssignmentTarget;
use crate::parse_error::ParseError;
use crate::parser_common::identifier_name;
use crate::parser_common::is_simple_parameter_list;
use crate::parser_common::private_identifier;
use crate::parser_common::with_loc;
use crate::parser_env::AllowedSuper;
use crate::parser_env::ParserEnv;
use crate::parser_env::eat;
use crate::parser_env::expect;
use crate::parser_env::is_reserved;
use crate::parser_env::is_strict_reserved;
use crate::parser_env::peek;
use crate::parser_env::try_parse::Rollback;
use crate::pattern_cover::PatternCover;
use crate::pattern_cover::PatternCoverErrors;
use crate::pattern_parser;
use crate::statement_parser;
use crate::token::TokenKind;
use crate::type_parser;

pub(super) fn parse_decorator_list(
    env: &mut ParserEnv,
) -> Result<Vec<class::Decorator<Loc, Loc>>, Rollback> {
    if !env.parse_options().esproposal_decorators {
        return Ok(Vec::new());
    }

    fn decorator(env: &mut ParserEnv) -> Result<class::Decorator<Loc, Loc>, Rollback> {
        let (loc, mut d) = with_loc(None, env, |env| {
            let leading = peek::comments(env);
            eat::token(env)?;
            let mut expression = expression_parser::decorator(env)?;
            comment_attachment::expression_remove_trailing(env, &mut expression);
            Ok(class::Decorator {
                loc: LOC_NONE,
                expression,
                comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
            })
        })?;
        d.loc = loc;
        Ok(d)
    }

    let mut decorators = Vec::new();

    while peek::token(env) == &TokenKind::TAt {
        decorators.push(decorator(env)?);
    }
    Ok(decorators)
}

pub(super) fn key(
    env: &mut ParserEnv,
    class_body: bool,
) -> Result<(Loc, expression::object::Key<Loc, Loc>), Rollback> {
    let leading = peek::comments(env);

    match peek::token(env) {
        TokenKind::TString(_, value, raw, octal) => {
            let value = value.dupe();
            let raw = raw.dupe();
            let octal = *octal;
            let loc = peek::loc(env).dupe();

            if octal {
                env.strict_error(ParseError::StrictOctalLiteral)?;
            }

            eat::token(env)?;
            let trailing = eat::trailing_comments(env);

            Ok((
                loc.dupe(),
                expression::object::Key::StringLiteral((
                    loc,
                    StringLiteral {
                        value,
                        raw,
                        comments: ast_utils::mk_comments_opt(
                            Some(leading.into()),
                            Some(trailing.into()),
                        ),
                    },
                )),
            ))
        }
        TokenKind::TNumber { kind, raw } => {
            let kind = *kind;
            let raw = raw.dupe();
            let loc = peek::loc(env).dupe();
            let value = expression_parser::number(env, kind, &raw)?;
            let trailing = eat::trailing_comments(env);

            Ok((
                loc.dupe(),
                expression::object::Key::NumberLiteral((
                    loc,
                    NumberLiteral {
                        value,
                        raw,
                        comments: ast_utils::mk_comments_opt(
                            Some(leading.into()),
                            Some(trailing.into()),
                        ),
                    },
                )),
            ))
        }
        TokenKind::TBigint { kind, raw } => {
            let kind = *kind;
            let raw = raw.dupe();
            let loc = peek::loc(env).dupe();
            let value = expression_parser::bigint(env, kind, &raw)?;
            let trailing = eat::trailing_comments(env);

            Ok((
                loc.dupe(),
                expression::object::Key::BigIntLiteral((
                    loc,
                    BigIntLiteral {
                        value,
                        raw,
                        comments: ast_utils::mk_comments_opt(
                            Some(leading.into()),
                            Some(trailing.into()),
                        ),
                    },
                )),
            ))
        }
        TokenKind::TLbracket => {
            let (loc, mut key) = with_loc(None, env, |env| {
                let leading = peek::comments(env);
                expect::token(env, TokenKind::TLbracket)?;
                let expr = env.with_no_in(false, expression_parser::assignment)?;
                expect::token(env, TokenKind::TRbracket)?;
                let trailing = eat::trailing_comments(env);

                Ok(ComputedKey {
                    loc: LOC_NONE,
                    expression: expr,
                    comments: ast_utils::mk_comments_opt(
                        Some(leading.into()),
                        Some(trailing.into()),
                    ),
                })
            })?;
            key.loc = loc.dupe();
            Ok((loc, expression::object::Key::Computed(key)))
        }
        TokenKind::TPound if class_body => {
            let private_name = private_identifier(env)?;
            let loc = private_name.loc.dupe();
            env.add_declared_private(private_name.name.as_str().to_owned());
            Ok((loc, expression::object::Key::PrivateName(private_name)))
        }
        TokenKind::TPound => {
            let (loc, id) = with_loc(None, env, |env| {
                eat::token(env)?;
                Ok(expression::object::Key::Identifier(identifier_name(env)?))
            })?;
            env.error_at(loc.dupe(), ParseError::PrivateNotInClass)?;
            Ok((loc, id))
        }
        _ => {
            let id = identifier_name(env)?;
            let loc = id.loc.dupe();
            Ok((loc, expression::object::Key::Identifier(id)))
        }
    }
}

fn getter_or_setter(
    env: &mut ParserEnv,
    in_class_body: bool,
    is_getter: bool,
) -> Result<
    (
        expression::object::Key<Loc, Loc>,
        (Loc, function::Function<Loc, Loc>),
    ),
    Rollback,
> {
    // This is a getter or setter, it cannot be async
    let async_ = false;
    let (generator, leading) = declaration_parser::parse_generator(env)?;
    let (key_loc, mut obj_key) = key(env, in_class_body)?;
    comment_attachment::object_key_remove_trailing(env, &mut obj_key);
    let value = with_loc(None, env, |env| {
        // #sec-function-definitions-static-semantics-early-errors
        env.with_allow_super(AllowedSuper::SuperPropOrCall, |env| {
            let (sig_loc, (tparams, params, return_annot)) = with_loc(None, env, |env| {
                // It's not clear how type params on getters & setters would make sense
                // in Flow's type system. Since this is a Flow syntax extension, we might
                // as well disallow it until we need it
                let tparams = None;
                let mut params = declaration_parser::parse_function_params(env, false, false)?;
                if peek::token(env) != &TokenKind::TColon {
                    comment_attachment::function_params_remove_trailing(env, &mut params);
                }

                match (is_getter, &params) {
                    (true, function::Params { this_: Some(_), .. }) => {
                        env.error_at(key_loc.dupe(), ParseError::GetterMayNotHaveThisParam)?;
                    }
                    (false, function::Params { this_: Some(_), .. }) => {
                        env.error_at(key_loc.dupe(), ParseError::SetterMayNotHaveThisParam)?;
                    }
                    (
                        true,
                        function::Params {
                            loc: _,
                            params,
                            rest: None,
                            this_: None,
                            comments: _,
                        },
                    ) if params.is_empty() => {}
                    (false, function::Params { rest: Some(_), .. }) => {
                        env.error_at(key_loc.dupe(), ParseError::SetterArity)?;
                    }
                    (
                        false,
                        function::Params {
                            loc: _,
                            params,
                            rest: None,
                            this_: None,
                            comments: _,
                        },
                    ) if params.len() == 1 => {}
                    (true, _) => {
                        env.error_at(key_loc.dupe(), ParseError::GetterArity)?;
                    }
                    (false, _) => {
                        // rest params don't make sense on a setter
                        env.error_at(key_loc.dupe(), ParseError::SetterArity)?;
                    }
                }

                let mut return_annot = type_parser::parse_function_return_annotation_opt(env)?;
                comment_attachment::return_annotation_remove_trailing(env, &mut return_annot);

                Ok((tparams, params, return_annot))
            })?;

            let simple_params = is_simple_parameter_list(&params);
            let (body, contains_use_strict) = declaration_parser::parse_function_body(
                env,
                async_,
                generator,
                false,
                simple_params,
            )?;

            declaration_parser::strict_function_post_check(
                env,
                contains_use_strict,
                None,
                &params,
            )?;

            Ok(function::Function {
                id: None,
                params,
                body,
                generator,
                async_,
                effect_: function::Effect::Arbitrary,
                // Getters/setters are not predicates
                predicate: None,
                return_: return_annot,
                tparams,
                sig_loc,
                comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
            })
        })
    })?;
    Ok((obj_key, value))
}

// #prod-MethodDefinition
fn parse_method(
    env: &mut ParserEnv,
    async_: bool,
    generator: bool,
    leading: Vec<Comment<Loc>>,
) -> Result<(Loc, function::Function<Loc, Loc>), Rollback> {
    with_loc(None, env, |env| {
        // #sec-function-definitions-static-semantics-early-errors
        env.with_allow_super(AllowedSuper::SuperProp, |env| {
            let (sig_loc, (tparams, params, return_annot)) = with_loc(None, env, |env| {
                let mut tparams = type_parser::parse_type_params(env)?;
                comment_attachment::type_params_remove_trailing(env, tparams.as_mut());
                let mut params = declaration_parser::parse_function_params(env, async_, generator)?;
                if peek::token(env) != &TokenKind::TColon {
                    comment_attachment::function_params_remove_trailing(env, &mut params);
                }
                let mut return_annot = type_parser::parse_function_return_annotation_opt(env)?;
                comment_attachment::return_annotation_remove_trailing(env, &mut return_annot);
                Ok((tparams, params, return_annot))
            })?;

            let simple_params = is_simple_parameter_list(&params);
            let (body, contains_use_strict) = declaration_parser::parse_function_body(
                env,
                async_,
                generator,
                false, // expression = false
                simple_params,
            )?;
            declaration_parser::strict_function_post_check(
                env,
                contains_use_strict,
                None,
                &params,
            )?;

            Ok(function::Function {
                id: None,
                params,
                body,
                generator,
                async_,
                effect_: function::Effect::Arbitrary,
                // TODO: add support for object method predicates
                predicate: None,
                return_: return_annot,
                tparams,
                sig_loc,
                comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
            })
        })
    })
}

pub(super) fn initializer(
    env: &mut ParserEnv,
) -> Result<(Loc, expression::Object<Loc, Loc>, PatternCoverErrors), Rollback> {
    fn parse_assignment_cover(
        env: &mut ParserEnv,
    ) -> Result<(expression::Expression<Loc, Loc>, PatternCoverErrors), Rollback> {
        match expression_parser::assignment_cover(env)? {
            PatternCover::CoverExpr(expr) => Ok((expr, PatternCoverErrors::empty())),
            PatternCover::CoverPatt(expr, errs) => Ok((expr, errs)),
        }
    }

    fn get(
        env: &mut ParserEnv,
        start_loc: Loc,
        leading: Vec<Comment<Loc>>,
    ) -> Result<expression::object::Property<Loc, Loc>, Rollback> {
        let (loc, (key, value)) = with_loc(Some(start_loc), env, |env| {
            getter_or_setter(env, false, true)
        })?;

        Ok(expression::object::Property::NormalProperty(
            expression::object::NormalProperty::Get {
                loc,
                key,
                value,
                comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
            },
        ))
    }

    fn set(
        env: &mut ParserEnv,
        start_loc: Loc,
        leading: Vec<Comment<Loc>>,
    ) -> Result<expression::object::Property<Loc, Loc>, Rollback> {
        let (loc, (key, value)) = with_loc(Some(start_loc), env, |env| {
            getter_or_setter(env, false, false)
        })?;

        Ok(expression::object::Property::NormalProperty(
            expression::object::NormalProperty::Set {
                loc,
                key,
                value,
                comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
            },
        ))
    }

    // #prod-IdentifierReference
    fn parse_shorthand(
        env: &mut ParserEnv,
        key: &expression::object::Key<Loc, Loc>,
    ) -> Result<expression::Expression<Loc, Loc>, Rollback> {
        match key {
            expression::object::Key::StringLiteral((loc, lit)) => {
                env.error_at(loc.dupe(), ParseError::LiteralShorthandProperty)?;
                Ok(expression::Expression::new(
                    ExpressionInner::StringLiteral {
                        loc: loc.dupe(),
                        inner: Arc::new(lit.clone()),
                    },
                ))
            }
            expression::object::Key::NumberLiteral((loc, lit)) => {
                env.error_at(loc.dupe(), ParseError::LiteralShorthandProperty)?;
                Ok(expression::Expression::new(
                    ExpressionInner::NumberLiteral {
                        loc: loc.dupe(),
                        inner: Arc::new(lit.clone()),
                    },
                ))
            }
            expression::object::Key::BigIntLiteral((loc, lit)) => {
                env.error_at(loc.dupe(), ParseError::LiteralShorthandProperty)?;
                Ok(expression::Expression::new(
                    ExpressionInner::BigIntLiteral {
                        loc: loc.dupe(),
                        inner: Arc::new(lit.clone()),
                    },
                ))
            }
            expression::object::Key::Identifier(id) => {
                let loc = id.loc.dupe();
                let name = &id.name;

                // Check for reserved words
                if is_reserved(name) {
                    env.error_at(loc.dupe(), ParseError::UnexpectedReserved)?;
                } else if is_strict_reserved(name) {
                    env.strict_error_at((loc.dupe(), ParseError::StrictReservedWord))?;
                }

                Ok(expression::Expression::new(ExpressionInner::Identifier {
                    loc: id.loc.dupe(),
                    inner: id.dupe(),
                }))
            }
            expression::object::Key::PrivateName(_) => {
                panic!("Internal Error: private name found in object props")
            }
            expression::object::Key::Computed(key) => {
                let loc = key.expression.loc().dupe();
                env.error_at(loc.dupe(), ParseError::ComputedShorthandProperty)?;
                Ok(key.expression.dupe())
            }
        }
    }

    // PropertyName `:` AssignmentExpression
    fn parse_value(
        env: &mut ParserEnv,
    ) -> Result<(expression::Expression<Loc, Loc>, PatternCoverErrors), Rollback> {
        expect::token(env, TokenKind::TColon)?;
        parse_assignment_cover(env)
    }

    // #prod-CoverInitializedName
    fn parse_assignment_pattern(
        env: &mut ParserEnv,
        key: &expression::object::Key<Loc, Loc>,
    ) -> Result<(expression::Expression<Loc, Loc>, PatternCoverErrors), Rollback> {
        match key {
            expression::object::Key::Identifier(id) => {
                let assignment_loc = peek::loc(env).dupe();
                let (loc, (left, right, comments)) = with_loc(Some(id.loc.dupe()), env, |env| {
                    let leading = peek::comments(env);
                    expect::token(env, TokenKind::TAssign)?;
                    let trailing = eat::trailing_comments(env);
                    let left_expr = expression::Expression::new(ExpressionInner::Identifier {
                        loc: id.loc.dupe(),
                        inner: id.dupe(),
                    });
                    let left = pattern_parser::from_expr(env, left_expr)?;
                    let right = expression_parser::assignment(env)?;
                    let comments =
                        ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into()));
                    Ok((left, right, comments))
                })?;
                let expr = expression::Expression::new(ExpressionInner::Assignment {
                    loc,
                    inner: Arc::new(expression::Assignment {
                        operator: None,
                        left,
                        right,
                        comments,
                    }),
                });

                let mut errs = PatternCoverErrors {
                    if_expr: vec![(
                        assignment_loc.dupe(),
                        ParseError::Unexpected(TokenKind::quote_token_value("=")),
                    )],
                    if_patt: Vec::new(),
                };
                errs.if_expr.push((
                    assignment_loc,
                    ParseError::Unexpected("assignment pattern".to_string()),
                ));

                Ok((expr, errs))
            }
            expression::object::Key::StringLiteral(_)
            | expression::object::Key::NumberLiteral(_)
            | expression::object::Key::BigIntLiteral(_)
            | expression::object::Key::PrivateName(_)
            | expression::object::Key::Computed(_) => parse_value(env),
        }
    }

    fn parse_init(
        env: &mut ParserEnv,
        mut obj_key: expression::object::Key<Loc, Loc>,
        async_: bool,
        generator: bool,
        leading: Vec<Comment<Loc>>,
    ) -> Result<
        (
            expression::object::NormalProperty<Loc, Loc>,
            PatternCoverErrors,
        ),
        Rollback,
    > {
        if async_ || generator {
            comment_attachment::object_key_remove_trailing(env, &mut obj_key);
            // the `async` and `*` modifiers are only valid on methods
            let value = parse_method(env, async_, generator, leading.clone())?;
            return Ok((
                expression::object::NormalProperty::Method {
                    loc: LOC_NONE,
                    key: obj_key,
                    value,
                },
                PatternCoverErrors::empty(),
            ));
        }

        match peek::token(env) {
            TokenKind::TRcurly | TokenKind::TComma => {
                let value = parse_shorthand(env, &obj_key)?;
                Ok((
                    expression::object::NormalProperty::Init {
                        loc: LOC_NONE,
                        key: obj_key,
                        value,
                        shorthand: true,
                    },
                    PatternCoverErrors::empty(),
                ))
            }
            TokenKind::TLessThan | TokenKind::TLparen => {
                comment_attachment::object_key_remove_trailing(env, &mut obj_key);
                let value = parse_method(env, async_, generator, leading)?;
                Ok((
                    expression::object::NormalProperty::Method {
                        loc: LOC_NONE,
                        key: obj_key,
                        value,
                    },
                    PatternCoverErrors::empty(),
                ))
            }
            TokenKind::TAssign => {
                let (value, errs) = parse_assignment_pattern(env, &obj_key)?;
                Ok((
                    expression::object::NormalProperty::Init {
                        loc: LOC_NONE,
                        key: obj_key,
                        value,
                        shorthand: true,
                    },
                    errs,
                ))
            }
            TokenKind::TColon => {
                expect::token(env, TokenKind::TColon)?;
                let (value, errs) = parse_assignment_cover(env)?;
                Ok((
                    expression::object::NormalProperty::Init {
                        loc: LOC_NONE,
                        key: obj_key,
                        value,
                        shorthand: false,
                    },
                    errs,
                ))
            }
            _ => {
                // error. we recover by treating it as a shorthand property so as to not
                // consume any more tokens and make the error worse. we don't error here
                // because we'll expect a comma before the next token.
                let value = parse_shorthand(env, &obj_key)?;
                Ok((
                    expression::object::NormalProperty::Init {
                        loc: LOC_NONE,
                        key: obj_key,
                        value,
                        shorthand: true,
                    },
                    PatternCoverErrors::empty(),
                ))
            }
        }
    }

    fn init(
        env: &mut ParserEnv,
        start_loc: Loc,
        obj_key: expression::object::Key<Loc, Loc>,
        async_: bool,
        generator: bool,
        leading: Vec<Comment<Loc>>,
    ) -> Result<(expression::object::Property<Loc, Loc>, PatternCoverErrors), Rollback> {
        let (loc, (mut prop, errs)) = with_loc(Some(start_loc), env, |env| {
            parse_init(env, obj_key, async_, generator, leading)
        })?;
        *prop.loc_mut() = loc;
        Ok((expression::object::Property::NormalProperty(prop), errs))
    }

    fn property(
        env: &mut ParserEnv,
    ) -> Result<(expression::object::Property<Loc, Loc>, PatternCoverErrors), Rollback> {
        fn identifier_key_from_consumed_token(
            loc: Loc,
            name: FlowSmolStr,
            leading: Vec<Comment<Loc>>,
            trailing: Vec<Comment<Loc>>,
        ) -> expression::object::Key<Loc, Loc> {
            expression::object::Key::Identifier(Identifier::new(IdentifierInner {
                loc,
                name,
                comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            }))
        }

        if peek::token(env) == &TokenKind::TEllipsis {
            // Spread prop
            let leading = peek::comments(env);
            let (loc, (argument, errs)) = with_loc(None, env, |env| {
                expect::token(env, TokenKind::TEllipsis)?;
                parse_assignment_cover(env)
            })?;

            return Ok((
                expression::object::Property::SpreadProperty(expression::object::SpreadProperty {
                    loc,
                    argument,
                    comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                }),
                errs,
            ));
        }

        let start_loc = peek::loc(env).dupe();

        let (async_, leading_async, async_key) = if peek::token(env) == &TokenKind::TAsync {
            let async_loc = peek::loc(env).dupe();
            let leading = peek::comments(env);
            eat::token(env)?;
            let async_continuation = peek::token(env).clone();
            let async_is_key = matches!(
                async_continuation,
                TokenKind::TAssign
                    | TokenKind::TColon
                    | TokenKind::TLessThan
                    | TokenKind::TLparen
                    | TokenKind::TComma
                    | TokenKind::TRcurly
            ) || peek::is_line_terminator(env);
            if async_is_key {
                let trailing = eat::trailing_comments(env);
                (
                    false,
                    Vec::new(),
                    Some(identifier_key_from_consumed_token(
                        async_loc,
                        FlowSmolStr::new_inline("async"),
                        leading,
                        trailing,
                    )),
                )
            } else {
                (true, leading, None)
            }
        } else {
            (false, Vec::new(), None)
        };
        let (generator, leading_generator) = declaration_parser::parse_generator(env)?;

        match (async_, generator, peek::token(env)) {
            (false, false, _) if async_key.is_some() => {
                init(env, start_loc, async_key.unwrap(), false, false, Vec::new())
            }
            (false, false, TokenKind::TIdentifier { raw, .. }) if raw == "get" => {
                let leading = peek::comments(env);
                let (_, mut obj_key) = key(env, false)?;

                match peek::token(env) {
                    TokenKind::TAssign
                    | TokenKind::TColon
                    | TokenKind::TLessThan
                    | TokenKind::TLparen
                    | TokenKind::TComma
                    | TokenKind::TRcurly => init(env, start_loc, obj_key, false, false, Vec::new()),
                    _ => {
                        comment_attachment::object_key_remove_trailing(env, &mut obj_key);
                        Ok((get(env, start_loc, leading)?, PatternCoverErrors::empty()))
                    }
                }
            }
            (false, false, TokenKind::TIdentifier { raw, .. }) if raw == "set" => {
                let leading = peek::comments(env);
                let (_, mut obj_key) = key(env, false)?;

                match peek::token(env) {
                    TokenKind::TAssign
                    | TokenKind::TColon
                    | TokenKind::TLessThan
                    | TokenKind::TLparen
                    | TokenKind::TComma
                    | TokenKind::TRcurly => init(env, start_loc, obj_key, false, false, Vec::new()),
                    _ => {
                        comment_attachment::object_key_remove_trailing(env, &mut obj_key);
                        Ok((set(env, start_loc, leading)?, PatternCoverErrors::empty()))
                    }
                }
            }
            _ => {
                let leading = [leading_async, leading_generator].concat();
                let (_, obj_key) = key(env, false)?;
                init(env, start_loc, obj_key, async_, generator, leading)
            }
        }
    }

    fn properties(
        env: &mut ParserEnv,
        rest_trailing_comma: Option<Loc>,
    ) -> Result<
        (
            Vec<expression::object::Property<Loc, Loc>>,
            PatternCoverErrors,
        ),
        Rollback,
    > {
        let mut props = Vec::new();
        let mut errors = PatternCoverErrors::empty();
        let mut rest_trailing_comma = rest_trailing_comma;

        loop {
            match peek::token(env) {
                TokenKind::TEof | TokenKind::TRcurly => {
                    if let Some(loc) = rest_trailing_comma {
                        errors
                            .if_patt
                            .push((loc, ParseError::TrailingCommaAfterRestElement));
                    }
                    break;
                }
                _ => {
                    let (prop, prop_errors) = property(env)?;
                    rest_trailing_comma = match &prop {
                        expression::object::Property::SpreadProperty(_)
                            if peek::token(env) == &TokenKind::TComma =>
                        {
                            Some(peek::loc(env).dupe())
                        }
                        _ => None,
                    };
                    errors.append(prop_errors);
                    match peek::token(env) {
                        TokenKind::TRcurly | TokenKind::TEof => {}
                        TokenKind::TComma => {
                            eat::token(env)?;
                        }
                        _ => {
                            // we could use `expect::error(env, &TokenKind::TComma)`, but we're
                            // in a weird cover grammar situation where we're storing errors in
                            // `PatternCoverErrors`. if we used `expect::error`, the errors would
                            // end up out of order.
                            let err = expect::get_error(env, &TokenKind::TComma);
                            // if the unexpected token is a semicolon, consume it to aid
                            // recovery. using a semicolon instead of a comma is a common
                            // mistake.
                            eat::maybe(env, TokenKind::TSemicolon)?;
                            errors.if_patt.push(err.clone());
                            errors.if_expr.push(err);
                        }
                    }
                    props.push(prop);
                }
            }
        }

        Ok((props, errors))
    }

    let (loc, (expr, errs)) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TLcurly)?;
        let (props, errs) = properties(env, None)?;
        let internal = peek::comments(env);
        expect::token(env, TokenKind::TRcurly)?;
        let trailing = eat::trailing_comments(env);
        Ok((
            expression::Object {
                properties: props.into(),
                comments: ast_utils::mk_comments_with_internal_opt(
                    Some(leading.into()),
                    Some(trailing.into()),
                    Some(internal.into()),
                ),
            },
            errs,
        ))
    })?;
    Ok((loc, expr, errs))
}

fn check_property_name(
    env: &mut ParserEnv,
    loc: Loc,
    name: &str,
    static_: bool,
) -> Result<(), Rollback> {
    if name == "constructor" || (name == "prototype" && static_) {
        env.error_at(
            loc,
            ParseError::InvalidClassMemberName {
                name: name.to_owned(),
                static_,
                method_: false,
                private_: false,
            },
        )?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrivateMemberKind {
    Method,
    Field,
    Getter,
    Setter,
}

fn check_private_names(
    env: &mut ParserEnv,
    seen_names: &mut HashMap<FlowSmolStr, PrivateMemberKind>,
    private_name: &PrivateName<Loc>,
    kind: PrivateMemberKind,
) -> Result<(), Rollback> {
    let loc = private_name.loc.dupe();
    let name = &private_name.name;
    if name == "constructor" {
        env.error_at(
            loc,
            ParseError::InvalidClassMemberName {
                name: name.as_str().to_owned(),
                static_: false,
                method_: matches!(kind, PrivateMemberKind::Method),
                private_: true,
            },
        )?;
        return Ok(());
    }
    if let Some(&seen_kind) = seen_names.get(name) {
        match (kind, seen_kind) {
            (PrivateMemberKind::Getter, PrivateMemberKind::Setter)
            | (PrivateMemberKind::Setter, PrivateMemberKind::Getter) => {
                // one getter and one setter are allowed as long as it's not used as a field
            }
            _ => {
                env.error_at(
                    loc.dupe(),
                    ParseError::DuplicatePrivateFields(name.as_str().to_owned()),
                )?;
            }
        }
        seen_names.insert(name.dupe(), PrivateMemberKind::Field);
    } else {
        seen_names.insert(name.dupe(), kind);
    }

    Ok(())
}

pub(super) fn class_implements(
    env: &mut ParserEnv,
    attach_leading: bool,
) -> Result<class::Implements<Loc, Loc>, Rollback> {
    fn interfaces(
        env: &mut ParserEnv,
    ) -> Result<Vec<class::implements::Interface<Loc, Loc>>, Rollback> {
        let mut interfaces = Vec::new();
        loop {
            let (loc, mut interface) = with_loc(None, env, |env| {
                let (_, generic) = type_parser::parse_generic(env)?;
                let types::Generic { id, targs, .. } = generic;
                Ok(class::implements::Interface {
                    loc: LOC_NONE,
                    id,
                    targs,
                })
            })?;
            interface.loc = loc;
            interfaces.push(interface);
            if peek::token(env) == &TokenKind::TComma {
                expect::token(env, TokenKind::TComma)?;
            } else {
                break;
            }
        }
        Ok(interfaces)
    }

    let (loc, mut i) = with_loc(None, env, |env| {
        let leading = if attach_leading {
            peek::comments(env)
        } else {
            Vec::new()
        };
        expect::token(env, TokenKind::TImplements)?;
        let interfaces = interfaces(env)?;
        Ok(class::Implements {
            loc: LOC_NONE,
            interfaces: interfaces.into(),
            comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
        })
    })?;
    i.loc = loc;
    Ok(i)
}

fn class_extends(
    env: &mut ParserEnv,
    leading: Vec<Comment<Loc>>,
) -> Result<class::Extends<Loc, Loc>, Rollback> {
    let (loc, mut e) = with_loc(None, env, |env| {
        let mut expr = env.with_allow_yield(false, |env| {
            env.with_no_record(true, expression_parser::left_hand_side)
        })?;
        if peek::token(env) == &TokenKind::TLessThan {
            comment_attachment::expression_remove_trailing(env, &mut expr);
        }
        let targs = type_parser::parse_type_args(env)?;
        Ok(class::Extends {
            loc: LOC_NONE,
            expr,
            targs,
            comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
        })
    })?;
    e.loc = loc;
    Ok(e)
}

/// https://tc39.es/ecma262/#prod-ClassHeritage
fn class_heritage(
    env: &mut ParserEnv,
) -> Result<
    (
        Option<class::Extends<Loc, Loc>>,
        Option<class::Implements<Loc, Loc>>,
    ),
    Rollback,
> {
    // Parse extends clause
    let extends = if peek::token(env) == &TokenKind::TExtends {
        let leading = peek::comments(env);
        eat::token(env)?;
        let mut extends = class_extends(env, leading)?;
        if let Some(mut remover) = comment_attachment::trailing_and_remover(env).remover {
            extends = remover.map_class_extends(&extends);
        }
        Some(extends)
    } else {
        None
    };

    // Parse implements clause
    let implements = if peek::token(env) == &TokenKind::TImplements {
        if !env.should_parse_types() {
            env.error(ParseError::UnexpectedTypeInterface)?;
        }
        let mut implements = class_implements(env, true)?;
        comment_attachment::class_implements_remove_trailing(env, &mut implements);
        Some(implements)
    } else {
        None
    };

    Ok((extends, implements))
}

fn string_value_of_key(key: &expression::object::Key<Loc, Loc>) -> Option<(&Loc, &str)> {
    match key {
        expression::object::Key::Identifier(id) => Some((&id.loc, &id.name)),
        expression::object::Key::StringLiteral((loc, lit)) => Some((loc, &lit.value)),
        _ => None,
    }
}

fn adjust_private_method_key_start(key: &mut expression::object::Key<Loc, Loc>, start_loc: &Loc) {
    if let expression::object::Key::PrivateName(private_name) = key {
        private_name.loc.start = start_loc.start;
    }
}

// In the ES6 draft, all elements are methods. No properties (though there
// are getter and setters allowed
fn class_element(env: &mut ParserEnv) -> Result<class::BodyElement<Loc, Loc>, Rollback> {
    fn method_kind_of_key(
        env: &mut ParserEnv,
        static_: bool,
        async_: bool,
        generator: bool,
        key: &expression::object::Key<Loc, Loc>,
    ) -> Result<class::MethodKind, Rollback> {
        match (static_, string_value_of_key(key)) {
            (false, Some((key_loc, "constructor"))) => {
                if async_ {
                    env.error_at(key_loc.dupe(), ParseError::ConstructorCannotBeAsync)?;
                }
                if generator {
                    env.error_at(key_loc.dupe(), ParseError::ConstructorCannotBeGenerator)?;
                }
                Ok(class::MethodKind::Constructor)
            }
            (true, Some((key_loc, name))) if name == "prototype" => {
                let name = name.to_owned();
                env.error_at(
                    key_loc.dupe(),
                    ParseError::InvalidClassMemberName {
                        name,
                        static_: true,
                        method_: true,
                        private_: false,
                    },
                )?;
                Ok(class::MethodKind::Method)
            }
            _ => Ok(class::MethodKind::Method),
        }
    }

    fn make_bodyless_declare_accessor(
        env: &mut ParserEnv,
        start_loc: &Loc,
        sig_loc: &Loc,
        kind: class::MethodKind,
        key: &expression::object::Key<Loc, Loc>,
        tparams: &Option<types::TypeParams<Loc, Loc>>,
        params: &function::Params<Loc, Loc>,
        return_annot: &function::ReturnAnnot<Loc, Loc>,
        static_: bool,
        override_: bool,
        ts_accessibility: Option<&class::ts_accessibility::TSAccessibility<Loc>>,
        leading: &[Comment<Loc>],
    ) -> Result<Option<class::BodyElement<Loc, Loc>>, Rollback> {
        match declaration_parser::convert_function_params_to_type_params(params) {
            Ok(type_params) => {
                if !env.should_parse_types() {
                    env.error(ParseError::UnexpectedTypeDeclaration)?;
                }
                let return_ = match return_annot {
                    function::ReturnAnnot::Available(annot) => {
                        types::function::ReturnAnnotation::Available(annot.clone())
                    }
                    function::ReturnAnnot::TypeGuard(tg) => {
                        types::function::ReturnAnnotation::TypeGuard(tg.guard.clone())
                    }
                    function::ReturnAnnot::Missing(loc) => {
                        let default_type = match kind {
                            class::MethodKind::Set => types::TypeInner::Void {
                                loc: loc.dupe(),
                                comments: None,
                            },
                            _ => types::TypeInner::Any {
                                loc: loc.dupe(),
                                comments: None,
                            },
                        };
                        types::function::ReturnAnnotation::Available(types::Annotation {
                            loc: loc.dupe(),
                            annotation: types::Type::new(default_type),
                        })
                    }
                };
                let func = types::Function {
                    tparams: tparams.clone(),
                    params: type_params,
                    return_,
                    comments: None,
                    effect: function::Effect::Arbitrary,
                };
                let annot = types::Annotation {
                    loc: sig_loc.dupe(),
                    annotation: types::Type::new(types::TypeInner::Function {
                        loc: sig_loc.dupe(),
                        inner: Arc::new(func),
                    }),
                };
                eat::maybe(env, TokenKind::TSemicolon)?;
                Ok(Some(class::BodyElement::DeclareMethod(
                    class::DeclareMethod {
                        loc: Loc::between(start_loc, sig_loc),
                        kind,
                        key: key.clone(),
                        annot,
                        static_,
                        override_,
                        optional: false,
                        ts_accessibility: ts_accessibility.cloned(),
                        comments: ast_utils::mk_comments_opt(Some(leading.to_vec().into()), None),
                    },
                )))
            }
            Err(_) => Ok(None),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn make_bodyless_declare_method(
        env: &mut ParserEnv,
        start_loc: &Loc,
        sig_loc: &Loc,
        kind: class::MethodKind,
        key: &expression::object::Key<Loc, Loc>,
        tparams: &Option<types::TypeParams<Loc, Loc>>,
        params: &function::Params<Loc, Loc>,
        return_annot: &function::ReturnAnnot<Loc, Loc>,
        static_: bool,
        override_: bool,
        optional: bool,
        ts_accessibility: Option<&class::ts_accessibility::TSAccessibility<Loc>>,
        leading: &[Comment<Loc>],
    ) -> Result<Option<class::BodyElement<Loc, Loc>>, Rollback> {
        match declaration_parser::convert_function_params_to_type_params(params) {
            Ok(type_params) => {
                let return_ = match return_annot {
                    function::ReturnAnnot::Available(annot) => {
                        types::function::ReturnAnnotation::Available(annot.clone())
                    }
                    function::ReturnAnnot::TypeGuard(tg) => {
                        types::function::ReturnAnnotation::TypeGuard(tg.guard.clone())
                    }
                    function::ReturnAnnot::Missing(loc) => {
                        if env.is_d_ts() {
                            types::function::ReturnAnnotation::Missing(loc.dupe())
                        } else {
                            types::function::ReturnAnnotation::Available(types::Annotation {
                                loc: loc.dupe(),
                                annotation: types::Type::new(types::TypeInner::Any {
                                    loc: loc.dupe(),
                                    comments: None,
                                }),
                            })
                        }
                    }
                };
                let func = types::Function {
                    tparams: tparams.clone(),
                    params: type_params,
                    return_,
                    comments: None,
                    effect: function::Effect::Arbitrary,
                };
                let annot = types::Annotation {
                    loc: sig_loc.dupe(),
                    annotation: types::Type::new(types::TypeInner::Function {
                        loc: sig_loc.dupe(),
                        inner: Arc::new(func),
                    }),
                };
                eat::maybe(env, TokenKind::TSemicolon)?;
                Ok(Some(class::BodyElement::DeclareMethod(
                    class::DeclareMethod {
                        loc: Loc::between(start_loc, sig_loc),
                        kind,
                        key: key.clone(),
                        annot,
                        static_,
                        override_,
                        optional,
                        ts_accessibility: ts_accessibility.cloned(),
                        comments: ast_utils::mk_comments_opt(Some(leading.to_vec().into()), None),
                    },
                )))
            }
            Err(_) => Ok(None),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn accessor(
        env: &mut ParserEnv,
        start_loc: Loc,
        accessor_key_start_loc: Loc,
        decorators: Vec<class::Decorator<Loc, Loc>>,
        static_: bool,
        override_: bool,
        ts_accessibility: Option<class::ts_accessibility::TSAccessibility<Loc>>,
        leading: Vec<Comment<Loc>>,
        is_getter: bool,
    ) -> Result<class::BodyElement<Loc, Loc>, Rollback> {
        if env.in_ambient_context() {
            // In ambient context, parse key + signature, then check for bodyless
            let async_ = false;
            let generator = false;
            let (key_loc, mut obj_key) = key(env, true)?;
            comment_attachment::object_key_remove_trailing(env, &mut obj_key);
            adjust_private_method_key_start(&mut obj_key, &accessor_key_start_loc);
            match (static_, string_value_of_key(&obj_key)) {
                (false, Some((key_loc, "constructor"))) => {
                    env.error_at(key_loc.dupe(), ParseError::ConstructorCannotBeAccessor)?;
                }
                (true, Some((key_loc, name))) if name == "prototype" => {
                    let name = name.to_owned();
                    env.error_at(
                        key_loc.dupe(),
                        ParseError::InvalidClassMemberName {
                            name,
                            static_: true,
                            method_: false,
                            private_: false,
                        },
                    )?;
                }
                _ => {}
            }
            let (sig_loc, (tparams, params, return_annot)) = with_loc(None, env, |env| {
                let mut tparams = type_parser::parse_type_params(env)?;
                comment_attachment::type_params_remove_trailing(env, tparams.as_mut());
                let mut params = declaration_parser::parse_function_params(env, async_, generator)?;
                if peek::token(env) != &TokenKind::TColon {
                    comment_attachment::function_params_remove_trailing(env, &mut params);
                }
                match (is_getter, &params) {
                    (true, function::Params { this_: Some(_), .. }) => {
                        env.error_at(key_loc.dupe(), ParseError::GetterMayNotHaveThisParam)?;
                    }
                    (false, function::Params { this_: Some(_), .. }) => {
                        env.error_at(key_loc.dupe(), ParseError::SetterMayNotHaveThisParam)?;
                    }
                    (
                        true,
                        function::Params {
                            loc: _,
                            params,
                            rest: None,
                            this_: None,
                            comments: _,
                        },
                    ) if params.is_empty() => {}
                    (false, function::Params { rest: Some(_), .. }) => {
                        env.error_at(key_loc.dupe(), ParseError::SetterArity)?;
                    }
                    (
                        false,
                        function::Params {
                            loc: _,
                            params,
                            rest: None,
                            this_: None,
                            comments: _,
                        },
                    ) if params.len() == 1 => {}
                    (true, _) => {
                        env.error_at(key_loc.dupe(), ParseError::GetterArity)?;
                    }
                    (false, _) => {
                        env.error_at(key_loc.dupe(), ParseError::SetterArity)?;
                    }
                }
                let mut return_annot = type_parser::parse_function_return_annotation_opt(env)?;
                comment_attachment::return_annotation_remove_trailing(env, &mut return_annot);
                Ok((tparams, params, return_annot))
            })?;
            let kind = if is_getter {
                class::MethodKind::Get
            } else {
                class::MethodKind::Set
            };
            let make_accessor_method =
                |env: &mut ParserEnv| -> Result<class::BodyElement<Loc, Loc>, Rollback> {
                    let value = with_loc(Some(sig_loc.dupe()), env, |env| {
                        let simple_params = is_simple_parameter_list(&params);
                        let (body, contains_use_strict) = declaration_parser::parse_function_body(
                            env,
                            async_,
                            generator,
                            false,
                            simple_params,
                        )?;
                        declaration_parser::strict_function_post_check(
                            env,
                            contains_use_strict,
                            None,
                            &params,
                        )?;
                        Ok(function::Function {
                            id: None,
                            params: params.clone(),
                            body,
                            generator,
                            async_,
                            effect_: function::Effect::Arbitrary,
                            predicate: None,
                            return_: return_annot.clone(),
                            tparams: tparams.clone(),
                            sig_loc: sig_loc.dupe(),
                            comments: None,
                        })
                    })?;
                    Ok(class::BodyElement::Method(class::Method {
                        loc: Loc::between(&start_loc, &value.0),
                        kind,
                        key: obj_key.clone(),
                        value,
                        static_,
                        override_,
                        ts_accessibility: ts_accessibility.clone(),
                        decorators: decorators.clone().into(),
                        comments: ast_utils::mk_comments_opt(Some(leading.clone().into()), None),
                    }))
                };
            let is_bodyless =
                peek::token(env) == &TokenKind::TSemicolon || peek::is_implicit_semicolon(env);
            if is_bodyless {
                match make_bodyless_declare_accessor(
                    env,
                    &start_loc,
                    &sig_loc,
                    kind,
                    &obj_key,
                    &tparams,
                    &params,
                    &return_annot,
                    static_,
                    override_,
                    ts_accessibility.as_ref(),
                    &leading,
                )? {
                    Some(decl) => Ok(decl),
                    None => make_accessor_method(env),
                }
            } else {
                make_accessor_method(env)
            }
        } else {
            // Not in ambient context - use normal getter_or_setter
            let (loc, (mut key, value)) = with_loc(Some(start_loc), env, |env| {
                getter_or_setter(env, true, is_getter)
            })?;
            adjust_private_method_key_start(&mut key, &accessor_key_start_loc);
            match (static_, string_value_of_key(&key)) {
                (false, Some((key_loc, "constructor"))) => {
                    env.error_at(key_loc.dupe(), ParseError::ConstructorCannotBeAccessor)?;
                }
                (true, Some((key_loc, name))) if name == "prototype" => {
                    let name = name.to_owned();
                    env.error_at(
                        key_loc.dupe(),
                        ParseError::InvalidClassMemberName {
                            name,
                            static_: true,
                            method_: false,
                            private_: false,
                        },
                    )?;
                }
                _ => {}
            }
            Ok(class::BodyElement::Method(class::Method {
                loc,
                kind: if is_getter {
                    class::MethodKind::Get
                } else {
                    class::MethodKind::Set
                },
                key,
                value,
                static_,
                override_,
                ts_accessibility,
                decorators: decorators.into(),
                comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
            }))
        }
    }

    fn error_unsupported_variance(
        env: &mut ParserEnv,
        variance: &Option<Variance<Loc>>,
    ) -> Result<(), Rollback> {
        if let Some(v) = variance {
            env.error_at(v.loc.dupe(), ParseError::UnexpectedVariance)?;
        }
        Ok(())
    }

    fn error_unsupported_declare(
        env: &mut ParserEnv,
        declare: &Option<Loc>,
    ) -> Result<(), Rollback> {
        if let Some(loc) = declare {
            env.error_at(loc.dupe(), ParseError::DeclareClassElement)?;
        }
        Ok(())
    }

    fn property_end_and_semicolon(
        env: &mut ParserEnv,
        mut key: expression::object::Key<Loc, Loc>,
        mut annot: types::AnnotationOrHint<Loc, Loc>,
        mut value: class::property::Value<Loc, Loc>,
    ) -> Result<
        (
            expression::object::Key<Loc, Loc>,
            types::AnnotationOrHint<Loc, Loc>,
            class::property::Value<Loc, Loc>,
            Vec<Comment<Loc>>,
        ),
        Rollback,
    > {
        match peek::token(env) {
            TokenKind::TLbracket | TokenKind::TLparen => {
                env.error_unexpected(None)?;
                Ok((key, annot, value, Vec::new()))
            }
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
                Ok((key, annot, value, trailing))
            }
            _ => {
                let should_run_remover = match peek::token(env) {
                    TokenKind::TEof | TokenKind::TRcurly => false,
                    _ => true,
                };
                match (&mut annot, &mut value) {
                    (_, class::property::Value::Initialized(expr)) => {
                        if should_run_remover {
                            comment_attachment::expression_remove_trailing(env, expr);
                        }
                    }
                    (types::AnnotationOrHint::Available(annot), _) => {
                        if should_run_remover {
                            if let Some(mut remover) =
                                comment_attachment::trailing_and_remover(env).remover
                            {
                                *annot = remover.map_type_annotation(annot);
                            }
                        }
                    }
                    _ => {
                        if should_run_remover {
                            comment_attachment::object_key_remove_trailing(env, &mut key);
                        }
                    }
                };
                Ok((key, annot, value, Vec::new()))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn property(
        env: &mut ParserEnv,
        start_loc: Loc,
        decorators: Vec<class::Decorator<Loc, Loc>>,
        key: expression::object::Key<Loc, Loc>,
        static_: bool,
        declare: Option<Loc>,
        variance: Option<Variance<Loc>>,
        ts_accessibility: Option<class::ts_accessibility::TSAccessibility<Loc>>,
        abstract_: bool,
        override_: bool,
        optional: bool,
        definite: Option<Loc>,
        leading: Vec<Comment<Loc>>,
    ) -> Result<class::BodyElement<Loc, Loc>, Rollback> {
        let (loc, (key, annot, value, comments)) = with_loc(Some(start_loc), env, |env| {
            let annot = type_parser::parse_annotation_opt(env)?;

            let value = match (declare.as_ref(), peek::token(env)) {
                (None, TokenKind::TAssign) => {
                    eat::token(env)?;
                    let expr = env
                        .with_allow_super(AllowedSuper::SuperProp, main_parser::parse_expression)?;
                    class::property::Value::Initialized(expr)
                }
                (Some(_), TokenKind::TAssign) => {
                    env.error(ParseError::DeclareClassFieldInitializer)?;
                    eat::token(env)?;
                    class::property::Value::Declared
                }
                (None, _) => class::property::Value::Uninitialized,
                (Some(_), _) => class::property::Value::Declared,
            };
            let (key, annot, value, trailing) = property_end_and_semicolon(env, key, annot, value)?;
            let comments = ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into()));
            Ok((key, annot, value, comments))
        })?;

        if let Some(definite_loc) = &definite
            && let Some(error) = main_parser::definite_assignment_error(
                DefiniteAssignmentTarget::ClassField,
                declare.is_none() && !static_ && !abstract_ && !env.in_ambient_context(),
                matches!(value, class::property::Value::Initialized(_)),
                matches!(annot, types::AnnotationOrHint::Available(_)),
            )
        {
            env.error_at(definite_loc.dupe(), error)?;
        }
        let definite = definite.is_some();

        if abstract_ {
            if let class::property::Value::Initialized(_) = &value {
                env.error_at(loc.dupe(), ParseError::AbstractPropertyWithInitializer)?;
            }
            match key {
                expression::object::Key::PrivateName(priv_name) => {
                    env.error_at(priv_name.loc.dupe(), ParseError::AbstractPrivateMember)?;
                    // Recover as a regular PrivateField, dropping the abstract modifier.
                    Ok(class::BodyElement::PrivateField(class::PrivateField {
                        loc,
                        key: priv_name,
                        value,
                        annot,
                        static_,
                        override_,
                        optional,
                        definite,
                        variance,
                        ts_accessibility,
                        decorators: decorators.into(),
                        comments,
                    }))
                }
                _ => Ok(class::BodyElement::AbstractProperty(
                    class::AbstractProperty {
                        loc,
                        key,
                        annot,
                        override_,
                        ts_accessibility,
                        variance,
                        comments,
                    },
                )),
            }
        } else {
            match key {
                expression::object::Key::PrivateName(priv_name) => {
                    Ok(class::BodyElement::PrivateField(class::PrivateField {
                        loc,
                        key: priv_name,
                        value,
                        annot,
                        static_,
                        override_,
                        optional,
                        definite,
                        variance,
                        ts_accessibility,
                        decorators: decorators.into(),
                        comments,
                    }))
                }
                _ => Ok(class::BodyElement::Property(class::Property {
                    loc,
                    key,
                    value,
                    annot,
                    static_,
                    override_,
                    optional,
                    definite,
                    variance,
                    ts_accessibility,
                    decorators: decorators.into(),
                    comments,
                })),
            }
        }
    }

    fn is_asi(env: &mut ParserEnv) -> bool {
        match peek::token(env) {
            TokenKind::TLessThan | TokenKind::TLparen => false,
            _ => peek::is_implicit_semicolon(env),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn init(
        env: &mut ParserEnv,
        start_loc: Loc,
        decorators: Vec<class::Decorator<Loc, Loc>>,
        mut key: expression::object::Key<Loc, Loc>,
        async_: bool,
        generator: bool,
        method_key_start_loc: Option<Loc>,
        static_: bool,
        abstract_: bool,
        override_: bool,
        declare: Option<Loc>,
        variance: Option<Variance<Loc>>,
        ts_accessibility: Option<class::ts_accessibility::TSAccessibility<Loc>>,
        leading: Vec<Comment<Loc>>,
    ) -> Result<class::BodyElement<Loc, Loc>, Rollback> {
        if !async_ && !generator && {
            match peek::token(env) {
                TokenKind::TColon
                | TokenKind::TAssign
                | TokenKind::TSemicolon
                | TokenKind::TRcurly => true,
                _ => is_asi(env),
            }
        } {
            return property(
                env,
                start_loc,
                decorators,
                key,
                static_,
                declare,
                variance,
                ts_accessibility,
                abstract_,
                override_,
                false,
                None,
                leading,
            );
        }
        if !async_
            && !generator
            && let Some(definite_loc) = main_parser::parse_definite_assignment_assertion(env)?
        {
            return property(
                env,
                start_loc,
                decorators,
                key,
                static_,
                declare,
                variance,
                ts_accessibility,
                abstract_,
                override_,
                false,
                Some(definite_loc),
                leading,
            );
        }
        if !async_
            && !generator
            && peek::token(env) == &TokenKind::TPling
            && peek::class_optional_method_continuation_is_call_or_type_args(env)
        {
            if !env.should_parse_types() {
                env.error(ParseError::UnexpectedTypeModifier)?;
            }
            eat::token(env)?;
            error_unsupported_declare(env, &declare)?;
            error_unsupported_variance(env, &variance)?;
            comment_attachment::object_key_remove_trailing(env, &mut key);
            let kind = method_kind_of_key(env, static_, async_, generator, &key)?;
            if kind == class::MethodKind::Constructor {
                env.error(ParseError::ConstructorCannotBeOptional)?;
            }
            if abstract_ {
                env.error(ParseError::OptionalMethodCannotBeAbstract)?;
            }
            // `override constructor?()` is rejected for the same reason
            // `override constructor()` is in the non-T_PLING branch below.
            // Strip the modifier so the rest of the class still type-checks.
            let override_ = if override_ && kind == class::MethodKind::Constructor {
                env.error_at(start_loc.dupe(), ParseError::OverrideOnConstructor)?;
                false
            } else {
                override_
            };
            let (sig_loc, (tparams, params, return_annot)) = with_loc(None, env, |env| {
                let mut tparams = type_parser::parse_type_params(env)?;
                comment_attachment::type_params_remove_trailing(env, tparams.as_mut());
                let mut params = declaration_parser::parse_function_params(env, false, false)?;
                if peek::token(env) != &TokenKind::TColon {
                    comment_attachment::function_params_remove_trailing(env, &mut params);
                }
                let mut return_annot = type_parser::parse_function_return_annotation_opt(env)?;
                comment_attachment::return_annotation_remove_trailing(env, &mut return_annot);
                Ok((tparams, params, return_annot))
            })?;
            return match make_bodyless_declare_method(
                env,
                &start_loc,
                &sig_loc,
                kind,
                &key,
                &tparams,
                &params,
                &return_annot,
                static_,
                override_,
                true,
                ts_accessibility.as_ref(),
                &leading,
            )? {
                Some(element) => Ok(element),
                None => {
                    env.error_unexpected(None)?;
                    eat::maybe(env, TokenKind::TSemicolon)?;
                    Ok(class::BodyElement::Property(class::Property {
                        loc: Loc::between(&start_loc, &sig_loc),
                        key,
                        value: class::property::Value::Declared,
                        annot: types::AnnotationOrHint::Missing(peek::loc(env).dupe()),
                        static_,
                        override_,
                        optional: true,
                        definite: false,
                        variance: None,
                        ts_accessibility,
                        decorators: decorators.into(),
                        comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                    }))
                }
            };
        }
        if !async_ && !generator && peek::token(env) == &TokenKind::TPling {
            if !env.should_parse_types() {
                env.error(ParseError::UnexpectedTypeModifier)?;
            }
            eat::token(env)?;
            return property(
                env,
                start_loc,
                decorators,
                key,
                static_,
                declare,
                variance,
                ts_accessibility,
                abstract_,
                override_,
                true,
                None,
                leading,
            );
        }
        if is_asi(env) {
            // an uninitialized, unannotated property
            return property(
                env,
                start_loc,
                decorators,
                key,
                static_,
                declare,
                variance,
                ts_accessibility,
                abstract_,
                override_,
                false,
                None,
                leading,
            );
        }

        error_unsupported_declare(env, &declare)?;
        error_unsupported_variance(env, &variance)?;

        if let Some(method_key_start_loc) = &method_key_start_loc {
            adjust_private_method_key_start(&mut key, method_key_start_loc);
        }

        let kind = method_kind_of_key(env, static_, async_, generator, &key)?;
        // `override constructor()` is a parse error. Recover by dropping
        // the `override` modifier so the rest of the class still
        // type-checks.
        let override_ = if override_ && kind == class::MethodKind::Constructor {
            env.error_at(start_loc.dupe(), ParseError::OverrideOnConstructor)?;
            false
        } else {
            override_
        };
        let allow_super = match kind {
            class::MethodKind::Constructor => AllowedSuper::SuperPropOrCall,
            _ => AllowedSuper::SuperProp,
        };
        env.with_allow_super(allow_super, |env| {
            comment_attachment::object_key_remove_trailing(env, &mut key);
            // Parse method signature: type params, params, return annotation
            let (sig_loc, (tparams, params, return_annot)) = with_loc(None, env, |env| {
                let mut tparams = type_parser::parse_type_params(env)?;
                comment_attachment::type_params_remove_trailing(env, tparams.as_mut());

                let mut params = declaration_parser::parse_function_params(env, async_, generator)?;
                if peek::token(env) != &TokenKind::TColon {
                    comment_attachment::function_params_remove_trailing(env, &mut params);
                }

                if let class::MethodKind::Constructor = kind {
                    if let Some(this_param) = &params.this_ {
                        // Disallow this param annotations for constructors
                        env.error_at(
                            this_param.loc.dupe(),
                            ParseError::ThisParamBannedInConstructor,
                        )?;
                        params.this_ = None;
                    }
                }

                let mut return_annot = type_parser::parse_function_return_annotation_opt(env)?;
                comment_attachment::return_annotation_remove_trailing(env, &mut return_annot);
                Ok((tparams, params, return_annot))
            })?;

            // Helper to build method function type
            let is_d_ts = env.is_d_ts();
            let make_method_func_type = |type_params: types::function::Params<Loc, Loc>| {
                let return_annot = match &return_annot {
                    function::ReturnAnnot::Available(annot) => {
                        types::function::ReturnAnnotation::Available(annot.clone())
                    }
                    function::ReturnAnnot::TypeGuard(tg) => {
                        types::function::ReturnAnnotation::TypeGuard(tg.guard.clone())
                    }
                    function::ReturnAnnot::Missing(loc) => {
                        if is_d_ts {
                            types::function::ReturnAnnotation::Missing(loc.dupe())
                        } else {
                            types::function::ReturnAnnotation::Available(types::Annotation {
                                loc: LOC_NONE,
                                annotation: types::Type::new(types::TypeInner::Any {
                                    loc: LOC_NONE,
                                    comments: None,
                                }),
                            })
                        }
                    }
                };
                let func = types::Function {
                    tparams: tparams.clone(),
                    params: type_params,
                    return_: return_annot,
                    comments: None,
                    effect: function::Effect::Arbitrary,
                };
                let annot_loc = match &tparams {
                    Some(tp) => tp.loc.dupe(),
                    None => params.loc.dupe(),
                };
                (annot_loc, func)
            };

            // Check for bodyless method: semicolon instead of body.
            // Also accept implicit semicolons (ASI) in ambient contexts and
            // for abstract methods. A `{` after a newline is still a method
            // body, which is invalid for an abstract method.
            let is_bodyless_method = (peek::token(env) == &TokenKind::TSemicolon
                || ((env.in_ambient_context() || abstract_)
                    && (!abstract_ || peek::token(env) != &TokenKind::TLcurly)
                    && peek::is_implicit_semicolon(env)))
                && !async_
                && !generator
                && (kind != class::MethodKind::Constructor || env.is_d_ts());

            // Wrap function type as Type.annotation for DeclareMethod
            let make_method_value =
                |env: &mut ParserEnv| -> Result<(Loc, function::Function<Loc, Loc>), Rollback> {
                    with_loc(Some(sig_loc.dupe()), env, |env| {
                        let simple_params = is_simple_parameter_list(&params);
                        let (body, contains_use_strict) = declaration_parser::parse_function_body(
                            env,
                            async_,
                            generator,
                            false,
                            simple_params,
                        )?;
                        declaration_parser::strict_function_post_check(
                            env,
                            contains_use_strict,
                            None,
                            &params,
                        )?;

                        Ok(function::Function {
                            id: None,
                            params: params.clone(),
                            body,
                            generator,
                            async_,
                            effect_: function::Effect::Arbitrary,
                            // TODO: add support for method predicates
                            predicate: None,
                            return_: return_annot.clone(),
                            tparams: tparams.clone(),
                            sig_loc: sig_loc.dupe(),
                            comments: None,
                        })
                    })
                };

            let make_method =
                |env: &mut ParserEnv| -> Result<class::BodyElement<Loc, Loc>, Rollback> {
                    let value = make_method_value(env)?;
                    let loc = Loc::between(&start_loc, &value.0);
                    Ok(class::BodyElement::Method(class::Method {
                        loc,
                        kind,
                        key: key.clone(),
                        value,
                        static_,
                        override_,
                        ts_accessibility: ts_accessibility.clone(),
                        decorators: decorators.clone().into(),
                        comments: ast_utils::mk_comments_opt(Some(leading.clone().into()), None),
                    }))
                };

            if abstract_ && is_bodyless_method {
                // Abstract method - try to convert params to type params
                match declaration_parser::convert_function_params_to_type_params(&params) {
                    Ok(type_params) => {
                        match &key {
                            expression::object::Key::PrivateName(priv_name) => {
                                env.error_at(
                                    priv_name.loc.dupe(),
                                    ParseError::AbstractPrivateMember,
                                )?;
                            }
                            _ => {}
                        }
                        eat::maybe(env, TokenKind::TSemicolon)?;
                        let (annot_loc, func) = make_method_func_type(type_params);
                        let loc = Loc::between(&start_loc, &sig_loc);
                        Ok(class::BodyElement::AbstractMethod(class::AbstractMethod {
                            loc,
                            key,
                            annot: (annot_loc, func),
                            override_,
                            ts_accessibility,
                            comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                        }))
                    }
                    // Params couldn't be converted - fall back to normal method with body
                    Err(_) => make_method(env),
                }
            } else if abstract_ {
                // Abstract method with a body - error
                env.error(ParseError::AbstractMethodWithBody)?;
                make_method(env)
            } else if is_bodyless_method
                && kind != class::MethodKind::Get
                && kind != class::MethodKind::Set
                && decorators.is_empty()
                && match &return_annot {
                    function::ReturnAnnot::Missing(_) => env.is_d_ts(),
                    _ => true,
                }
            {
                // Implicit declare method
                match make_bodyless_declare_method(
                    env,
                    &start_loc,
                    &sig_loc,
                    kind,
                    &key,
                    &tparams,
                    &params,
                    &return_annot,
                    static_,
                    override_,
                    false,
                    ts_accessibility.as_ref(),
                    &leading,
                )? {
                    Some(element) => Ok(element),
                    // Normal method with body
                    None => make_method(env),
                }
            } else {
                // Normal method with body
                make_method(env)
            }
        })
    }

    fn token_after_current_implies_identifier(env: &mut ParserEnv) -> bool {
        peek::token_after_current_implies_identifier(env)
    }

    fn type_modifier_continues(env: &mut ParserEnv) -> bool {
        env.should_parse_types() || !peek::has_line_terminator_after_current(env)
    }

    fn implies_identifier(env: &mut ParserEnv) -> bool {
        match peek::token(env) {
            TokenKind::TLessThan
            | TokenKind::TColon
            | TokenKind::TAssign
            | TokenKind::TSemicolon
            | TokenKind::TLparen
            | TokenKind::TPling
            | TokenKind::TNot
            | TokenKind::TRcurly => true,
            _ => false,
        }
    }

    let start_loc = peek::loc(env).dupe();
    let decorators = parse_decorator_list(env)?;

    let (declare, leading_declare) = if peek::token(env) == &TokenKind::TDeclare
        && !token_after_current_implies_identifier(env)
        && type_modifier_continues(env)
    {
        if !env.should_parse_types() {
            env.error(ParseError::UnexpectedTypeDeclaration)?;
        }
        let declare_loc = peek::loc(env).dupe();
        let leading = peek::comments(env);
        eat::token(env)?;
        (Some(declare_loc), leading)
    } else {
        (None, Vec::new())
    };

    let is_public = matches!(peek::token(env), TokenKind::TPublic)
        && peek::class_member_continuation_allows_modifier(env)
        && type_modifier_continues(env);
    let is_private = matches!(peek::token(env), TokenKind::TPrivate)
        && peek::class_member_continuation_allows_modifier(env)
        && type_modifier_continues(env);
    let is_protected = matches!(peek::token(env), TokenKind::TProtected)
        && peek::class_member_continuation_allows_modifier(env)
        && type_modifier_continues(env);
    if (is_public || is_private || is_protected) && !env.should_parse_types() {
        env.error(ParseError::UnexpectedTypeModifier)?;
    }
    let (ts_accessibility, leading_accessibility) = if is_public {
        let loc = peek::loc(env).dupe();
        let leading = peek::comments(env);
        eat::token(env)?;
        let trailing = eat::trailing_comments(env);
        (
            Some(class::ts_accessibility::TSAccessibility {
                loc,
                kind: class::ts_accessibility::Kind::Public,
                comments: ast_utils::mk_comments_opt(
                    Some(leading.clone().into()),
                    Some(trailing.into()),
                ),
            }),
            leading,
        )
    } else if is_private {
        let loc = peek::loc(env).dupe();
        let leading = peek::comments(env);
        eat::token(env)?;
        let trailing = eat::trailing_comments(env);
        (
            Some(class::ts_accessibility::TSAccessibility {
                loc,
                kind: class::ts_accessibility::Kind::Private,
                comments: ast_utils::mk_comments_opt(
                    Some(leading.clone().into()),
                    Some(trailing.into()),
                ),
            }),
            leading,
        )
    } else if is_protected {
        let loc = peek::loc(env).dupe();
        let leading = peek::comments(env);
        eat::token(env)?;
        let trailing = eat::trailing_comments(env);
        (
            Some(class::ts_accessibility::TSAccessibility {
                loc,
                kind: class::ts_accessibility::Kind::Protected,
                comments: ast_utils::mk_comments_opt(
                    Some(leading.clone().into()),
                    Some(trailing.into()),
                ),
            }),
            leading,
        )
    } else {
        (None, Vec::new())
    };

    let static_ = peek::token(env) == &TokenKind::TStatic
        && peek::class_static_continuation_allows_modifier(env);
    let leading_static = if static_ {
        let leading = peek::comments(env);
        eat::token(env)?;
        leading
    } else {
        Vec::new()
    };

    // Parse `abstract` then `override`. The reverse order is rejected
    // at parse time as an unexpected identifier — locking in a single
    // canonical order while `override` has no users to migrate.
    let abstract_ = matches!(
        peek::token(env),
        TokenKind::TIdentifier { raw, .. } if raw == "abstract"
    ) && peek::class_member_continuation_allows_modifier(env)
        && type_modifier_continues(env);
    let leading_abstract = if abstract_ {
        if !env.should_parse_types() {
            env.error(ParseError::UnexpectedTypeModifier)?;
        }
        let leading = peek::comments(env);
        eat::token(env)?;
        leading
    } else {
        Vec::new()
    };

    let override_ = matches!(
        peek::token(env),
        TokenKind::TIdentifier { raw, .. } if raw == "override"
    ) && peek::class_member_continuation_allows_modifier(env)
        && type_modifier_continues(env);
    let leading_override = if override_ {
        if !env.should_parse_types() {
            env.error(ParseError::UnexpectedTypeModifier)?;
        }
        let leading = peek::comments(env);
        eat::token(env)?;
        leading
    } else {
        Vec::new()
    };
    // Error if both static and abstract
    if static_ && abstract_ {
        env.error(ParseError::StaticAbstractMethod)?;
    }

    if static_ && declare.is_none() && peek::token(env) == &TokenKind::TLcurly {
        eat::token(env)?;
        let (loc, mut static_block) = with_loc(Some(start_loc), env, |env| {
            let internal = peek::comments(env);
            let body =
                statement_parser::parse_statement_list(env, |token: &TokenKind| match token {
                    TokenKind::TRcurly => true,
                    _ => false,
                })?;
            expect::token(env, TokenKind::TRcurly)?;
            let trailing = eat::trailing_comments(env);
            let comments = ast_utils::mk_comments_with_internal_opt(
                Some(leading_static.into()),
                Some(trailing.into()),
                Some(internal.into()),
            );
            Ok(class::StaticBlock {
                loc: LOC_NONE,
                body: body.into(),
                comments,
            })
        })?;
        static_block.loc = loc;

        return Ok(class::BodyElement::StaticBlock(static_block));
    }

    let async_ = if let TokenKind::TAsync = peek::token(env) {
        !token_after_current_implies_identifier(env)
            && !peek::has_line_terminator_after_current(env)
    } else {
        false
    };
    let async_start_loc = if async_ {
        Some(peek::loc(env).dupe())
    } else {
        None
    };
    let leading_async = if async_ {
        let leading = peek::comments(env);
        eat::token(env)?;
        leading
    } else {
        Vec::new()
    };
    let mut generator_start_loc = if peek::token(env) == &TokenKind::TMult {
        Some(peek::loc(env).dupe())
    } else {
        None
    };
    let (mut generator, mut leading_generator) = declaration_parser::parse_generator(env)?;

    let parse_property_variance_keyword = matches!(
        peek::token(env),
        TokenKind::TIdentifier { raw, .. } if raw == "readonly" || raw == "writeonly"
    ) && peek::current_token_is_followed_by_object_key(
        env, true,
    ) && type_modifier_continues(env);
    let variance = declaration_parser::parse_variance(
        env,
        parse_property_variance_keyword,
        async_,
        generator,
    )?;

    if !generator && variance.is_some() {
        let result_generator_start_loc = if peek::token(env) == &TokenKind::TMult {
            Some(peek::loc(env).dupe())
        } else {
            None
        };
        let result = declaration_parser::parse_generator(env)?;
        generator = result.0;
        leading_generator = result.1;
        if generator {
            generator_start_loc = result_generator_start_loc;
        }
    }
    let method_key_start_loc = async_start_loc.or(generator_start_loc);

    let leading = [
        leading_declare,
        leading_accessibility,
        leading_static,
        leading_abstract,
        leading_override,
        leading_async,
        leading_generator,
    ]
    .concat();

    let in_ambient_context = env.in_ambient_context();
    match (async_, generator, peek::token(env)) {
        (false, false, TokenKind::TIdentifier { raw, .. }) if raw == "get" => {
            let leading_get = peek::comments(env);
            let (accessor_key_start_loc, mut obj_key) = key(env, true)?;

            if implies_identifier(env) {
                init(
                    env,
                    start_loc,
                    decorators,
                    obj_key,
                    async_,
                    generator,
                    method_key_start_loc.clone(),
                    static_,
                    abstract_,
                    override_,
                    declare,
                    variance,
                    ts_accessibility,
                    leading,
                )
            } else {
                if !env.in_ambient_context() {
                    error_unsupported_declare(env, &declare)?;
                }
                error_unsupported_variance(env, &variance)?;
                comment_attachment::object_key_remove_trailing(env, &mut obj_key);
                accessor(
                    env,
                    start_loc,
                    accessor_key_start_loc,
                    decorators,
                    static_,
                    override_,
                    ts_accessibility,
                    [leading, leading_get].concat(),
                    true,
                )
            }
        }
        (false, false, TokenKind::TIdentifier { raw, .. }) if raw == "set" => {
            let leading_set = peek::comments(env);
            let (accessor_key_start_loc, mut obj_key) = key(env, true)?;

            if implies_identifier(env) {
                init(
                    env,
                    start_loc,
                    decorators,
                    obj_key,
                    async_,
                    generator,
                    method_key_start_loc.clone(),
                    static_,
                    abstract_,
                    override_,
                    declare,
                    variance,
                    ts_accessibility,
                    leading,
                )
            } else {
                if !env.in_ambient_context() {
                    error_unsupported_declare(env, &declare)?;
                }
                error_unsupported_variance(env, &variance)?;
                comment_attachment::object_key_remove_trailing(env, &mut obj_key);
                accessor(
                    env,
                    start_loc,
                    accessor_key_start_loc,
                    decorators,
                    static_,
                    override_,
                    ts_accessibility,
                    [leading, leading_set].concat(),
                    false,
                )
            }
        }
        (false, false, TokenKind::TLbracket) if in_ambient_context => {
            // Consume `[`, then disambiguate: index signature vs computed property.
            // Index signature: [id: type]: type — the token after the identifier is T_COLON.
            // Computed property: [expr] — anything else.
            let leading_lbracket = peek::comments(env);
            expect::token(env, TokenKind::TLbracket)?;
            if peek::ambient_computed_property_is_indexer(env) {
                if !env.should_parse_types() {
                    env.error(ParseError::UnexpectedTypeAnnotation)?;
                }
                // Index signature: [key: type]: type
                // Reject modifiers that are not valid on index signatures.
                error_unsupported_declare(env, &declare)?;
                if let Some(ref accessibility) = ts_accessibility {
                    let keyword = match accessibility.kind {
                        class::ts_accessibility::Kind::Public => "public",
                        class::ts_accessibility::Kind::Protected => "protected",
                        class::ts_accessibility::Kind::Private => "private",
                    };
                    env.error_at(
                        accessibility.loc.dupe(),
                        ParseError::IndexSignatureInvalidModifier(keyword.to_owned()),
                    )?;
                }
                if abstract_ {
                    env.error_at(
                        start_loc.dupe(),
                        ParseError::IndexSignatureInvalidModifier("abstract".to_owned()),
                    )?;
                }
                let (loc, mut indexer) = with_loc(Some(start_loc.dupe()), env, |env| {
                    let id = identifier_name(env)?;
                    expect::token(env, TokenKind::TColon)?;
                    let key_type = type_parser::parse_type(env)?;
                    expect::token(env, TokenKind::TRbracket)?;
                    let optional = eat::maybe(env, TokenKind::TPling)?;
                    let trailing = eat::trailing_comments(env);
                    expect::token(env, TokenKind::TColon)?;
                    let value = type_parser::parse_type(env)?;
                    eat::maybe(env, TokenKind::TSemicolon)?;
                    Ok(types::object::Indexer {
                        loc: LOC_NONE,
                        id: Some(id),
                        key: key_type,
                        value,
                        static_,
                        variance,
                        optional,
                        comments: ast_utils::mk_comments_opt(
                            Some([leading.clone(), leading_lbracket.clone()].concat().into()),
                            Some(trailing.into()),
                        ),
                    })
                })?;
                indexer.loc = loc;
                Ok(class::BodyElement::IndexSignature(indexer))
            } else {
                // Computed property — `[` already consumed, parse expression and `]`
                let (loc, mut computed_key) = with_loc(Some(start_loc.dupe()), env, |env| {
                    let expr = env.with_no_in(false, expression_parser::assignment)?;
                    expect::token(env, TokenKind::TRbracket)?;
                    let trailing = eat::trailing_comments(env);
                    Ok(ComputedKey {
                        loc: LOC_NONE,
                        expression: expr,
                        comments: ast_utils::mk_comments_opt(
                            Some(leading_lbracket.clone().into()),
                            Some(trailing.into()),
                        ),
                    })
                })?;
                computed_key.loc = loc;
                let key = expression::object::Key::Computed(computed_key);
                init(
                    env,
                    start_loc,
                    decorators,
                    key,
                    async_,
                    generator,
                    method_key_start_loc.clone(),
                    static_,
                    abstract_,
                    override_,
                    declare,
                    variance,
                    ts_accessibility,
                    leading,
                )
            }
        }
        _ => {
            let (_, obj_key) = key(env, true)?;
            init(
                env,
                start_loc,
                decorators,
                obj_key,
                async_,
                generator,
                method_key_start_loc,
                static_,
                abstract_,
                override_,
                declare,
                variance,
                ts_accessibility,
                leading,
            )
        }
    }
}

fn class_body(
    env: &mut ParserEnv,
    expression: bool,
    abstract_: bool,
) -> Result<class::Body<Loc, Loc>, Rollback> {
    fn class_elements(
        env: &mut ParserEnv,
        abstract_: bool,
    ) -> Result<Vec<class::BodyElement<Loc, Loc>>, Rollback> {
        let mut elements = Vec::new();
        let mut seen_constructor = false;
        let mut private_names = HashMap::new();

        loop {
            match peek::token(env) {
                TokenKind::TEof | TokenKind::TRcurly => break,
                TokenKind::TSemicolon => {
                    // Skip empty elements
                    eat::token(env)?;
                }
                _ => {
                    let element = class_element(env)?;

                    match &element {
                        class::BodyElement::Method(method) => match method.kind {
                            class::MethodKind::Constructor => {
                                if !method.static_ {
                                    if seen_constructor {
                                        env.error_at(
                                            method.loc.dupe(),
                                            ParseError::DuplicateConstructor,
                                        )?;
                                    }
                                    seen_constructor = true;
                                }
                            }
                            class::MethodKind::Method => {
                                if let expression::object::Key::PrivateName(ref name) = method.key {
                                    check_private_names(
                                        env,
                                        &mut private_names,
                                        name,
                                        PrivateMemberKind::Method,
                                    )?;
                                }
                            }
                            class::MethodKind::Get => {
                                if let expression::object::Key::PrivateName(ref name) = method.key {
                                    check_private_names(
                                        env,
                                        &mut private_names,
                                        name,
                                        PrivateMemberKind::Getter,
                                    )?;
                                }
                            }
                            class::MethodKind::Set => {
                                if let expression::object::Key::PrivateName(ref name) = method.key {
                                    check_private_names(
                                        env,
                                        &mut private_names,
                                        name,
                                        PrivateMemberKind::Setter,
                                    )?;
                                }
                            }
                        },
                        class::BodyElement::Property(property) => match &property.key {
                            expression::object::Key::Identifier(id) => {
                                check_property_name(
                                    env,
                                    id.loc.dupe(),
                                    &id.name,
                                    property.static_,
                                )?;
                            }
                            expression::object::Key::StringLiteral((loc, lit)) => {
                                check_property_name(env, loc.dupe(), &lit.value, property.static_)?;
                            }
                            _ => {}
                        },
                        class::BodyElement::PrivateField(field) => {
                            check_private_names(
                                env,
                                &mut private_names,
                                &field.key,
                                PrivateMemberKind::Field,
                            )?;
                        }
                        class::BodyElement::StaticBlock(_) => {
                            // No validation needed for static blocks
                        }
                        // DeclareMethod is a bodyless method signature, no private name checking needed
                        class::BodyElement::DeclareMethod(_) => {}
                        class::BodyElement::AbstractMethod(m) => {
                            if !abstract_ {
                                env.error_at(
                                    m.loc.dupe(),
                                    ParseError::AbstractMethodInNonAbstractClass,
                                )?;
                            }
                        }
                        class::BodyElement::AbstractProperty(p) => {
                            if !abstract_ {
                                env.error_at(
                                    p.loc.dupe(),
                                    ParseError::AbstractPropertyInNonAbstractClass,
                                )?;
                            }
                        }
                        class::BodyElement::IndexSignature(_) => {}
                    }

                    elements.push(element);
                }
            }
        }

        Ok(elements)
    }

    let (loc, mut body) = with_loc(None, env, |env| {
        let leading = peek::comments(env);

        if eat::maybe(env, TokenKind::TLcurly)? {
            env.enter_class();
            let body = class_elements(env, abstract_)?;
            env.exit_class()?;

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

            Ok(class::Body {
                loc: LOC_NONE,
                body: body.into(),
                comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            })
        } else {
            expect::error(env, &TokenKind::TLcurly)?;
            Ok(class::Body {
                loc: LOC_NONE,
                body: Vec::new().into(),
                comments: None,
            })
        }
    })?;
    body.loc = loc;
    Ok(body)
}

fn parse_class(
    env: &mut ParserEnv,
    decorators: Vec<class::Decorator<Loc, Loc>>,
    optional_id: bool,
    expression: bool,
) -> Result<class::Class<Loc, Loc>, Rollback> {
    // All parts of a class definition are strict (10.2.1)
    env.with_strict(true, |env| {
        let mut all_decorators = decorators;
        all_decorators.extend(parse_decorator_list(env)?);
        let leading = peek::comments(env);
        let abstract_ = match peek::token(env) {
            TokenKind::TIdentifier { raw, .. } if raw == "abstract" => {
                if !env.should_parse_types() {
                    env.error(ParseError::UnexpectedTypeModifier)?;
                }
                eat::token(env)?;
                true
            }
            _ => false,
        };
        expect::token(env, TokenKind::TClass)?;
        let id = match (optional_id, peek::token(env)) {
            (
                true,
                TokenKind::TExtends
                | TokenKind::TImplements
                | TokenKind::TLessThan
                | TokenKind::TLcurly,
            ) => None,
            _ => {
                if peek::is_identifier(env) {
                    let mut id = main_parser::parse_identifier(env, None)?;
                    comment_attachment::id_remove_trailing(env, &mut id);
                    Some(id)
                } else {
                    // error, but don't consume a token like Parse.identifier does. this helps
                    // with recovery, and the parser won't get stuck because we consumed the
                    // `class` token above.
                    env.error_nameless_declaration("class")?;
                    Some(Identifier::new(IdentifierInner {
                        loc: peek::loc(env).dupe(),
                        name: FlowSmolStr::new_inline(""),
                        comments: None,
                    }))
                }
            }
        };
        let mut tparams = type_parser::parse_type_params(env)?;
        comment_attachment::type_params_remove_trailing(env, tparams.as_mut());
        let (extends, implements) = class_heritage(env)?;
        let body = class_body(env, expression, abstract_)?;
        Ok(class::Class {
            id,
            body,
            tparams,
            extends,
            implements,
            class_decorators: all_decorators.into(),
            abstract_,
            comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
        })
    })
}

pub(super) fn class_declaration(
    env: &mut ParserEnv,
    decorators: Vec<class::Decorator<Loc, Loc>>,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, c) = with_loc(None, env, |env| {
        let optional_id = env.in_export_default();
        parse_class(env, decorators, optional_id, false)
    })?;
    Ok(statement::Statement::new(
        statement::StatementInner::ClassDeclaration {
            loc,
            inner: Arc::new(c),
        },
    ))
}

pub(super) fn class_expression(
    env: &mut ParserEnv,
) -> Result<expression::Expression<Loc, Loc>, Rollback> {
    let (loc, c) = with_loc(None, env, |env| parse_class(env, Vec::new(), true, true))?;
    Ok(expression::Expression::new(ExpressionInner::Class {
        loc,
        inner: Arc::new(c),
    }))
}

fn record_body(
    env: &mut ParserEnv,
) -> Result<statement::record_declaration::Body<Loc, Loc>, Rollback> {
    use statement::record_declaration::BodyElement;

    fn elements(env: &mut ParserEnv, acc: &mut Vec<BodyElement<Loc, Loc>>) -> Result<(), Rollback> {
        loop {
            match peek::token(env) {
                TokenKind::TEof | TokenKind::TRcurly => return Ok(()),
                _ => {
                    fn maybe_eat_and_get_comments(
                        env: &mut ParserEnv,
                        token: TokenKind,
                    ) -> Result<(bool, Vec<Comment<Loc>>), Rollback> {
                        let cond = peek::token(env) == &token
                            && peek::record_modifier_allows_modifier(env);
                        let comments = if cond {
                            let leading = peek::comments(env);
                            eat::token(env)?;
                            leading
                        } else {
                            Vec::new()
                        };
                        Ok((cond, comments))
                    }

                    let start_loc = peek::loc(env).dupe();
                    let (static_, leading_static) =
                        maybe_eat_and_get_comments(env, TokenKind::TStatic)?;
                    let (async_, leading_async) =
                        maybe_eat_and_get_comments(env, TokenKind::TAsync)?;
                    let (mut generator, mut leading_generator) =
                        declaration_parser::parse_generator(env)?;

                    let parse_property_variance_keyword = matches!(
                        peek::token(env),
                        TokenKind::TIdentifier { raw, .. } if raw == "readonly" || raw == "writeonly"
                    )
                        && peek::current_token_is_followed_by_object_key(env, false);
                    let variance = declaration_parser::parse_variance(
                        env,
                        parse_property_variance_keyword,
                        async_,
                        generator,
                    )?;
                    if !generator && variance.is_some() {
                        let result = declaration_parser::parse_generator(env)?;
                        generator = result.0;
                        leading_generator = result.1;
                    }

                    let leading_key = peek::comments(env);
                    let leading = [
                        leading_static,
                        leading_async,
                        leading_generator,
                        leading_key,
                    ]
                    .concat();
                    if peek::token(env) == &TokenKind::TPound {
                        let error_loc = peek::loc(env).dupe();
                        eat::token(env)?;
                        env.error_at(error_loc, ParseError::RecordPrivateElementUnsupported)?;
                    }
                    let (key_loc, key) = key(env, false)?;
                    let key = match key {
                        expression::object::Key::Computed(_) => {
                            env.error_at(
                                key_loc.dupe(),
                                ParseError::RecordComputedPropertyUnsupported,
                            )?;
                            expression::object::Key::Identifier(Identifier::new(IdentifierInner {
                                loc: key_loc.dupe(),
                                name: FlowSmolStr::new(""),
                                comments: None,
                            }))
                        }
                        _ => key,
                    };
                    let check_invalid_name =
                        |env: &mut ParserEnv, method_: bool| -> Result<(), Rollback> {
                            if let Some((key_loc, name)) = string_value_of_key(&key) {
                                if name == "constructor" || (name == "prototype" && static_) {
                                    env.error_at(
                                        key_loc.dupe(),
                                        ParseError::RecordInvalidPropertyName {
                                            name: name.to_string(),
                                            static_,
                                            method_,
                                        },
                                    )?;
                                }
                            }
                            Ok(())
                        };

                    let empty_invalid_syntax =
                        statement::record_declaration::InvalidPropertySyntax {
                            invalid_variance: None,
                            invalid_optional: None,
                            invalid_suffix_semicolon: None,
                        };

                    let mut invalid_syntax =
                        variance.map(|v| statement::record_declaration::InvalidPropertySyntax {
                            invalid_variance: Some(v),
                            invalid_optional: None,
                            invalid_suffix_semicolon: None,
                        });

                    if peek::token(env) == &TokenKind::TPling {
                        let error_loc = peek::loc(env).dupe();
                        eat::token(env)?;
                        let invalid_syntax_val =
                            invalid_syntax.get_or_insert(empty_invalid_syntax.clone());
                        invalid_syntax_val.invalid_optional = Some(error_loc);
                    }

                    fn end_property(
                        env: &mut ParserEnv,
                        invalid_syntax: Option<
                            statement::record_declaration::InvalidPropertySyntax<Loc>,
                        >,
                    ) -> Result<
                        Option<statement::record_declaration::InvalidPropertySyntax<Loc>>,
                        Rollback,
                    > {
                        match peek::token(env) {
                            TokenKind::TEof | TokenKind::TRcurly => Ok(invalid_syntax),
                            TokenKind::TSemicolon => {
                                let semicolon_loc = peek::loc(env).dupe();
                                eat::token(env)?;
                                let empty_invalid_syntax =
                                    statement::record_declaration::InvalidPropertySyntax {
                                        invalid_variance: None,
                                        invalid_optional: None,
                                        invalid_suffix_semicolon: None,
                                    };
                                let mut invalid_syntax_val =
                                    invalid_syntax.unwrap_or(empty_invalid_syntax);
                                invalid_syntax_val.invalid_suffix_semicolon = Some(semicolon_loc);
                                Ok(Some(invalid_syntax_val))
                            }
                            _ => {
                                expect::token(env, TokenKind::TComma)?;
                                Ok(invalid_syntax)
                            }
                        }
                    }
                    match peek::token(env) {
                        TokenKind::TColon if static_ => {
                            check_invalid_name(env, false)?;
                            let (loc, mut prop) = with_loc(Some(start_loc.dupe()), env, |env| {
                                let annot = type_parser::parse_annotation(env)?;
                                expect::token(env, TokenKind::TAssign)?;
                                let value = expression_parser::assignment(env)?;
                                let invalid_syntax_final = end_property(env, invalid_syntax)?;
                                let comments =
                                    ast_utils::mk_comments_opt(Some(leading.into()), None);
                                Ok(statement::record_declaration::StaticProperty {
                                    loc: LOC_NONE,
                                    key,
                                    annot,
                                    value,
                                    comments,
                                    invalid_syntax: invalid_syntax_final,
                                })
                            })?;
                            prop.loc = loc;
                            acc.push(BodyElement::StaticProperty(prop));
                        }
                        TokenKind::TColon if !async_ && !generator => {
                            check_invalid_name(env, false)?;
                            let (loc, mut prop) = with_loc(Some(start_loc.dupe()), env, |env| {
                                let annot = type_parser::parse_annotation(env)?;
                                let default_value = if eat::maybe(env, TokenKind::TAssign)? {
                                    Some(expression_parser::assignment(env)?)
                                } else {
                                    None
                                };
                                let invalid_syntax_final = end_property(env, invalid_syntax)?;
                                let comments =
                                    ast_utils::mk_comments_opt(Some(leading.into()), None);
                                Ok(statement::record_declaration::Property {
                                    loc: LOC_NONE,
                                    key,
                                    annot,
                                    default_value,
                                    comments,
                                    invalid_syntax: invalid_syntax_final,
                                })
                            })?;
                            prop.loc = loc;
                            acc.push(BodyElement::Property(prop));
                        }
                        TokenKind::TComma | TokenKind::TSemicolon => {
                            env.error_at(key_loc, ParseError::RecordPropertyAnnotationRequired)?;
                            end_property(env, invalid_syntax)?;
                        }
                        TokenKind::TAssign => {
                            env.error_at(key_loc, ParseError::RecordPropertyAnnotationRequired)?;
                            eat::token(env)?;
                            let _ = expression_parser::assignment(env)?;
                            end_property(env, invalid_syntax)?;
                        }
                        _ => {
                            check_invalid_name(env, true)?;
                            let (loc, mut meth) = with_loc(Some(start_loc.dupe()), env, |env| {
                                let value = parse_method(env, async_, generator, leading)?;
                                let comments =
                                    ast_utils::mk_comments_opt(Some(Vec::new().into()), None);
                                Ok(class::Method {
                                    loc: LOC_NONE,
                                    key: key.clone(),
                                    value,
                                    kind: class::MethodKind::Method,
                                    static_,
                                    override_: false,
                                    ts_accessibility: None,
                                    decorators: Vec::new().into(),
                                    comments,
                                })
                            })?;
                            meth.loc = loc;
                            acc.push(BodyElement::Method(meth));
                        }
                    }
                }
            }
        }
    }

    let (loc, mut body_result) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        if eat::maybe(env, TokenKind::TLcurly)? {
            let mut body = Vec::new();
            elements(env, &mut body)?;
            let trailing = match peek::token(env) {
                TokenKind::TRcurly | TokenKind::TEof => eat::trailing_comments(env),
                _ => {
                    if peek::is_line_terminator(env) {
                        eat::comments_until_next_line(env)
                    } else {
                        Vec::new()
                    }
                }
            };
            expect::token(env, TokenKind::TRcurly)?;
            Ok(statement::record_declaration::Body {
                loc: LOC_NONE,
                body: body.into(),
                comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            })
        } else {
            expect::error(env, &TokenKind::TLcurly)?;
            Ok(statement::record_declaration::Body {
                loc: LOC_NONE,
                body: Vec::new().into(),
                comments: None,
            })
        }
    })?;
    body_result.loc = loc;
    Ok(body_result)
}

pub(super) fn record_declaration(
    env: &mut ParserEnv,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, decl) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TRecord)?;
        let id = main_parser::parse_identifier(env, None)?;
        let mut tparams = type_parser::parse_type_params(env)?;
        comment_attachment::type_params_remove_trailing(env, tparams.as_mut());
        if peek::token(env) == &TokenKind::TExtends {
            let extends_loc = peek::loc(env).dupe();
            env.error_at(extends_loc, ParseError::RecordExtendsUnsupported)?;
            let leading = peek::comments(env);
            eat::token(env)?;
            let _ = class_extends(env, leading)?;
        }
        let invalid_syntax = if peek::token(env) == &TokenKind::TAssign {
            let invalid_infix_equals = Some(peek::loc(env).dupe());
            eat::token(env)?;
            Some(statement::record_declaration::InvalidSyntax {
                invalid_infix_equals,
            })
        } else {
            None
        };
        let implements = if peek::token(env) == &TokenKind::TImplements {
            let mut implements = class_implements(env, true)?;
            comment_attachment::class_implements_remove_trailing(env, &mut implements);
            Some(implements)
        } else {
            None
        };
        let body = record_body(env)?;
        let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
        Ok(statement::RecordDeclaration {
            id,
            tparams,
            implements,
            body,
            comments,
            invalid_syntax,
        })
    })?;
    Ok(statement::Statement::new(
        statement::StatementInner::RecordDeclaration {
            loc,
            inner: Arc::new(decl),
        },
    ))
}
