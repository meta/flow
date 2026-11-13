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

use crate::ast::types::TypeInner;
use crate::ast::types::TypeParams;
use crate::ast::*;
use crate::ast_utils;
use crate::ast_utils::mk_comments_opt;
use crate::ast_visitor::AstVisitor;
use crate::comment_attachment;
use crate::expression_parser;
use crate::loc::LOC_NONE;
use crate::loc::Loc;
use crate::main_parser;
use crate::object_parser;
use crate::parse_error::FunctionTypeParensContext;
use crate::parse_error::ParseError;
use crate::parser_common;
use crate::parser_common::*;
use crate::parser_env::try_parse::Rollback;
use crate::parser_env::*;
use crate::pattern_parser;
use crate::token;
use crate::token::NumberType;
use crate::token::TokenKind;

fn maybe_variance(
    env: &mut ParserEnv,
    parse_property_variance_keyword: bool,
    parse_in_out: bool,
) -> Result<Option<Variance<Loc>>, Rollback> {
    let loc = peek::loc(env).dupe();
    Ok(match peek::token(env) {
        TokenKind::TPlus => {
            let leading = peek::comments(env);
            eat::token(env)?;
            Some(Variance {
                loc,
                kind: VarianceKind::Plus,
                comments: mk_comments_opt(Some(leading.into()), None),
            })
        }
        TokenKind::TMinus => {
            let leading = peek::comments(env);
            eat::token(env)?;
            Some(Variance {
                loc,
                kind: VarianceKind::Minus,
                comments: mk_comments_opt(Some(leading.into()), None),
            })
        }
        TokenKind::TReadonly if parse_property_variance_keyword => {
            let leading = peek::comments(env);
            eat::token(env)?;
            Some(Variance {
                loc,
                kind: VarianceKind::Readonly,
                comments: mk_comments_opt(Some(leading.into()), None),
            })
        }
        TokenKind::TWriteonly if parse_property_variance_keyword => {
            let leading = peek::comments(env);
            eat::token(env)?;
            Some(Variance {
                loc,
                kind: VarianceKind::Writeonly,
                comments: mk_comments_opt(Some(leading.into()), None),
            })
        }
        TokenKind::TIdentifier { raw, .. } if raw == "in" && parse_in_out => {
            if peek::current_token_is_followed_by_type_identifier(env) {
                let leading = peek::comments(env);
                eat::token(env)?;
                let (kind, loc) = match peek::token(env) {
                    TokenKind::TIdentifier { raw, .. } if raw == "out" => {
                        let end_loc = peek::loc(env).dupe();
                        eat::token(env)?;
                        (VarianceKind::InOut, Loc::between(&loc, &end_loc))
                    }
                    _ => (VarianceKind::In, loc),
                };
                Some(Variance {
                    loc,
                    kind,
                    comments: mk_comments_opt(Some(leading.into()), None),
                })
            } else {
                None
            }
        }
        TokenKind::TIdentifier { raw, .. } if raw == "out" && parse_in_out => {
            if peek::current_token_is_followed_by_type_identifier(env) {
                let leading = peek::comments(env);
                eat::token(env)?;
                Some(Variance {
                    loc,
                    kind: VarianceKind::Out,
                    comments: mk_comments_opt(Some(leading.into()), None),
                })
            } else {
                None
            }
        }
        _ => None,
    })
}

fn maybe_const(env: &mut ParserEnv) -> Result<Option<(Loc, Option<Syntax<Loc, ()>>)>, Rollback> {
    match peek::token(env) {
        TokenKind::TConst => {
            let (loc, comments) = with_loc(None, env, |env| {
                let leading = peek::comments(env);
                eat::token(env)?;
                Ok(mk_comments_opt(Some(leading.into()), None))
            })?;
            Ok(Some((loc, comments)))
        }
        _ => Ok(None),
    }
}

fn number_singleton(
    env: &mut ParserEnv,
    neg: Option<Vec<Comment<Loc>>>,
    kind: NumberType,
    value: f64,
    raw: FlowSmolStr,
) -> Result<NumberLiteral<Loc>, Rollback> {
    if matches!(kind, NumberType::LegacyOctal) {
        env.strict_error(ParseError::StrictOctalLiteral)?;
    }
    let leading = peek::comments(env);
    eat::token(env)?;
    let trailing = eat::trailing_comments(env);
    let (value, raw, comments) = match neg {
        None => {
            let comments = mk_comments_opt(Some(leading.into()), Some(trailing.into()));
            (value, raw, comments)
        }
        Some(leading_neg) => {
            let leading = [leading_neg, leading].concat();
            let comments = mk_comments_opt(Some(leading.into()), Some(trailing.into()));
            (-value, FlowSmolStr::new(format!("-{}", raw)), comments)
        }
    };
    Ok(NumberLiteral {
        value,
        raw,
        comments,
    })
}

fn bigint_singleton(
    env: &mut ParserEnv,
    neg: Option<Vec<Comment<Loc>>>,
    value: Option<i64>,
    raw: FlowSmolStr,
) -> Result<BigIntLiteral<Loc>, Rollback> {
    let leading = peek::comments(env);
    eat::token(env)?;
    let trailing = eat::trailing_comments(env);
    let (value, raw, comments) = match neg {
        None => {
            let comments = mk_comments_opt(Some(leading.into()), Some(trailing.into()));
            (value, raw, comments)
        }
        Some(leading_neg) => {
            let leading = [leading_neg, leading].concat();
            let comments = mk_comments_opt(Some(leading.into()), Some(trailing.into()));
            (
                value.map(|v| -v),
                FlowSmolStr::new(format!("-{}", raw)),
                comments,
            )
        }
    };
    Ok(BigIntLiteral {
        value,
        raw,
        comments,
    })
}

fn type_inner(env: &mut ParserEnv) -> Result<types::Type<Loc, Loc>, Rollback> {
    conditional(env)
}

fn annotation(env: &mut ParserEnv) -> Result<types::Annotation<Loc, Loc>, Rollback> {
    if !env.should_parse_types() {
        env.error(ParseError::UnexpectedTypeAnnotation)?;
    }
    let (loc, annotation) = with_loc(None, env, |env| {
        expect::token(env, TokenKind::TColon)?;
        type_inner(env)
    })?;
    Ok(types::Annotation { loc, annotation })
}

fn function_return_annotation(
    env: &mut ParserEnv,
) -> Result<function::ReturnAnnot<Loc, Loc>, Rollback> {
    if !env.should_parse_types() {
        env.error(ParseError::UnexpectedTypeAnnotation)?;
    }
    let start_loc = peek::loc(env).dupe();
    expect::token(env, TokenKind::TColon)?;
    if is_start_of_type_guard(env) {
        Ok(function::ReturnAnnot::TypeGuard(type_guard_annotation(
            env, start_loc,
        )?))
    } else {
        let (loc, annotation) = with_loc(Some(start_loc), env, type_inner)?;
        Ok(function::ReturnAnnot::Available(types::Annotation {
            loc,
            annotation,
        }))
    }
}

fn conditional(env: &mut ParserEnv) -> Result<types::Type<Loc, Loc>, Rollback> {
    let start_loc = peek::loc(env).dupe();
    env.with_no_conditional_type(false, |env| {
        let check_type = parse_union_no_wrap(env)?;
        conditional_with(env, start_loc, check_type)
    })
}

fn conditional_with(
    env: &mut ParserEnv,
    start_loc: Loc,
    check_type: types::Type<Loc, Loc>,
) -> Result<types::Type<Loc, Loc>, Rollback> {
    if env.oxc_type_feature_parser_rules() && peek::is_line_terminator(env) {
        return Ok(check_type);
    }
    match peek::token(env) {
        TokenKind::TExtends => {
            let (loc, conditional) = with_loc(Some(start_loc), env, |env| {
                expect::token(env, TokenKind::TExtends)?;
                let extends_type = env.with_no_conditional_type(true, parse_union_no_wrap)?;
                expect::token_opt(env, TokenKind::TPling)?;
                let true_type = type_inner(env)?;
                expect::token_opt(env, TokenKind::TColon)?;
                let false_type = type_inner(env)?;
                let trailing = eat::trailing_comments(env);
                Ok(types::Conditional {
                    check_type,
                    extends_type,
                    true_type,
                    false_type,
                    comments: mk_comments_opt(None, Some(trailing.into())),
                })
            })?;
            Ok(types::Type::new(TypeInner::Conditional {
                loc,
                inner: Arc::new(conditional),
            }))
        }
        _ => Ok(check_type),
    }
}

pub(super) fn parse_union_no_wrap(env: &mut ParserEnv) -> Result<types::Type<Loc, Loc>, Rollback> {
    let start_loc = peek::loc(env).dupe();
    let has_leading_operator = peek::token(env) == &TokenKind::TBitOr;
    let leading = if has_leading_operator {
        let leading = peek::comments(env);
        eat::token(env)?;
        leading
    } else {
        Vec::new()
    };
    let left = env.with_function_type_parens_context(
        has_leading_operator.then_some(FunctionTypeParensContext::Union),
        intersection,
    )?;
    union_with(env, leading, start_loc, left)
}

fn union_with(
    env: &mut ParserEnv,
    leading: Vec<Comment<Loc>>,
    start_loc: Loc,
    left: types::Type<Loc, Loc>,
) -> Result<types::Type<Loc, Loc>, Rollback> {
    fn unions(
        env: &mut ParserEnv,
        leading: Vec<Comment<Loc>>,
        mut acc: Vec<types::Type<Loc, Loc>>,
    ) -> Result<types::Union<Loc, Loc>, Rollback> {
        while eat::maybe(env, TokenKind::TBitOr)? {
            acc.push(env.with_function_type_parens_context(
                Some(FunctionTypeParensContext::Union),
                intersection,
            )?);
        }
        let mut types = acc.into_iter().collect::<Vec<_>>();
        if types.len() < 2 {
            unreachable!("union should have at least 2 types")
        }
        let t0 = types.remove(0);
        let t1 = types.remove(0);
        Ok(types::Union {
            types: (t0, t1, types),
            comments: mk_comments_opt(Some(leading.into()), None),
        })
    }

    if peek::token(env) == &TokenKind::TBitOr {
        let (loc, union_type) =
            with_loc(Some(start_loc), env, |env| unions(env, leading, vec![left]))?;
        Ok(types::Type::new(TypeInner::Union {
            loc,
            inner: Arc::new(union_type),
        }))
    } else {
        Ok(left)
    }
}

fn intersection(env: &mut ParserEnv) -> Result<types::Type<Loc, Loc>, Rollback> {
    let start_loc = peek::loc(env).dupe();
    let has_leading_operator = peek::token(env) == &TokenKind::TBitAnd;
    let leading = if has_leading_operator {
        let leading = peek::comments(env);
        eat::token(env)?;
        leading
    } else {
        Vec::new()
    };
    let left = if has_leading_operator {
        env.with_function_type_parens_context(
            Some(FunctionTypeParensContext::Intersection),
            anon_function_without_parens,
        )?
    } else {
        anon_function_without_parens(env)?
    };
    intersection_with(env, leading, start_loc, left)
}

fn intersection_with(
    env: &mut ParserEnv,
    leading: Vec<Comment<Loc>>,
    start_loc: Loc,
    left: types::Type<Loc, Loc>,
) -> Result<types::Type<Loc, Loc>, Rollback> {
    fn intersections(
        env: &mut ParserEnv,
        leading: Vec<Comment<Loc>>,
        mut acc: Vec<types::Type<Loc, Loc>>,
    ) -> Result<types::Intersection<Loc, Loc>, Rollback> {
        while eat::maybe(env, TokenKind::TBitAnd)? {
            acc.push(env.with_function_type_parens_context(
                Some(FunctionTypeParensContext::Intersection),
                anon_function_without_parens,
            )?);
        }
        let mut types = acc.into_iter().collect::<Vec<_>>();
        if types.len() < 2 {
            unreachable!("intersection should have at least 2 types")
        }
        let t0 = types.remove(0);
        let t1 = types.remove(0);
        Ok(types::Intersection {
            types: (t0, t1, types),
            comments: mk_comments_opt(Some(leading.into()), None),
        })
    }

    if peek::token(env) == &TokenKind::TBitAnd {
        let (loc, intersection_type) = with_loc(Some(start_loc), env, |env| {
            intersections(env, leading, vec![left])
        })?;
        Ok(types::Type::new(TypeInner::Intersection {
            loc,
            inner: Arc::new(intersection_type),
        }))
    } else {
        Ok(left)
    }
}

fn anon_function_without_parens(env: &mut ParserEnv) -> Result<types::Type<Loc, Loc>, Rollback> {
    let param = prefix(env)?;
    anon_function_without_parens_with(env, param)
}

fn anon_function_without_parens_with(
    env: &mut ParserEnv,
    param: types::Type<Loc, Loc>,
) -> Result<types::Type<Loc, Loc>, Rollback> {
    if matches!(peek::token(env), TokenKind::TArrow) {
        if !env.no_anon_function_type() {
            let param_loc = param.loc().dupe();
            let param = anonymous_function_param(env, param);
            let start_loc = param_loc.dupe();
            let tparams = None;
            let params = types::function::Params {
                loc: param_loc,
                params: vec![param].into(),
                this: None,
                rest: None,
                comments: None,
            };
            return function_with_params(
                env,
                start_loc,
                tparams,
                params,
                function::Effect::Arbitrary,
            );
        }
    }
    Ok(param)
}

fn prefix(env: &mut ParserEnv) -> Result<types::Type<Loc, Loc>, Rollback> {
    match peek::token(env) {
        TokenKind::TPling => {
            let (loc, nullable) = with_loc(None, env, |env| {
                let leading = peek::comments(env);
                expect::token(env, TokenKind::TPling)?;
                let argument = env.with_function_type_parens_context(None, prefix)?;
                Ok(types::Nullable {
                    argument,
                    comments: mk_comments_opt(Some(leading.into()), None),
                })
            })?;
            Ok(types::Type::new(TypeInner::Nullable {
                loc,
                inner: Arc::new(nullable),
            }))
        }
        _ => postfix(env),
    }
}

fn postfix(env: &mut ParserEnv) -> Result<types::Type<Loc, Loc>, Rollback> {
    let start_loc = peek::loc(env).dupe();
    let t = primary(env)?;
    postfix_with(env, false, start_loc, t)
}

fn postfix_with(
    env: &mut ParserEnv,
    in_optional_indexed_access: bool,
    start_loc: Loc,
    t: types::Type<Loc, Loc>,
) -> Result<types::Type<Loc, Loc>, Rollback> {
    if peek::is_line_terminator(env) {
        return Ok(t);
    }

    match peek::token(env) {
        TokenKind::TPlingPeriod => {
            eat::token(env)?;
            if peek::token(env) != &TokenKind::TLbracket {
                env.error(ParseError::InvalidOptionalIndexedAccess)?;
            }
            expect::token(env, TokenKind::TLbracket)?;
            postfix_brackets(env, true, true, start_loc, t)
        }
        TokenKind::TLbracket => {
            eat::token(env)?;
            postfix_brackets(env, in_optional_indexed_access, false, start_loc, t)
        }
        TokenKind::TPeriod => {
            if peek::current_token_is_followed_by_lbracket(env) {
                env.error(ParseError::InvalidIndexedAccess { has_bracket: true })?;
                expect::token(env, TokenKind::TPeriod)?;
                expect::token(env, TokenKind::TLbracket)?;
                postfix_brackets(env, in_optional_indexed_access, false, start_loc, t)
            } else {
                env.error(ParseError::InvalidIndexedAccess { has_bracket: false })?;
                Ok(t)
            }
        }
        _ => Ok(t),
    }
}

fn postfix_brackets(
    env: &mut ParserEnv,
    in_optional_indexed_access: bool,
    optional_indexed_access: bool,
    start_loc: Loc,
    t: types::Type<Loc, Loc>,
) -> Result<types::Type<Loc, Loc>, Rollback> {
    let (loc, mut inner) = with_loc(Some(start_loc.dupe()), env, |env| {
        // Legacy Array syntax `Foo[]`
        if !optional_indexed_access && eat::maybe(env, TokenKind::TRbracket)? {
            let trailing = eat::trailing_comments(env);
            Ok(TypeInner::Array {
                loc: LOC_NONE,
                inner: Arc::new(types::Array {
                    argument: t,
                    comments: mk_comments_opt(None, Some(trailing.into())),
                }),
            })
        } else {
            let index = type_inner(env)?;
            expect::token(env, TokenKind::TRbracket)?;
            let trailing = eat::trailing_comments(env);
            let indexed_access = types::IndexedAccess {
                object: t,
                index,
                comments: mk_comments_opt(None, Some(trailing.into())),
            };

            if in_optional_indexed_access {
                Ok(TypeInner::OptionalIndexedAccess {
                    loc: LOC_NONE,
                    inner: Arc::new(types::OptionalIndexedAccess {
                        indexed_access,
                        optional: optional_indexed_access,
                    }),
                })
            } else {
                Ok(TypeInner::IndexedAccess {
                    loc: LOC_NONE,
                    inner: Arc::new(indexed_access),
                })
            }
        }
    })?;
    *inner.loc_mut() = loc;
    postfix_with(
        env,
        in_optional_indexed_access,
        start_loc,
        types::Type::new(inner),
    )
}

fn typeof_expr(env: &mut ParserEnv) -> Result<types::typeof_::Target<Loc, Loc>, Rollback> {
    let id = parser_common::identifier_name(env)?;
    raw_typeof_expr_with_identifier(env, id)
}

fn typeof_qualification(
    env: &mut ParserEnv,
    _start_loc: Loc,
    initial: types::typeof_::Target<Loc, Loc>,
) -> Result<types::typeof_::Target<Loc, Loc>, Rollback> {
    let mut target = initial;
    while peek::token(env) == &TokenKind::TPeriod
        && peek::current_token_is_followed_by_identifier_name(env)
    {
        let (loc, id) = with_loc(Some(target.loc().dupe()), env, |env| {
            expect::token(env, TokenKind::TPeriod)?;
            let id = parser_common::identifier_name(env)?;
            Ok(id)
        })?;
        target = types::typeof_::Target::Qualified(Arc::new(types::typeof_::Qualified {
            loc,
            qualification: target,
            id,
        }));
    }
    Ok(target)
}

fn raw_typeof_expr_with_identifier(
    env: &mut ParserEnv,
    id: Identifier<Loc, Loc>,
) -> Result<types::typeof_::Target<Loc, Loc>, Rollback> {
    let start_loc = id.loc.dupe();
    typeof_qualification(env, start_loc, types::typeof_::Target::Unqualified(id))
}

fn typeof_import_expr(env: &mut ParserEnv) -> Result<types::typeof_::Target<Loc, Loc>, Rollback> {
    let start_loc = peek::loc(env).dupe();
    let leading = peek::comments(env);
    expect::token(env, TokenKind::TImport)?;
    expect::token(env, TokenKind::TLparen)?;

    let source = match peek::token(env).clone() {
        TokenKind::TString(_, value, raw, octal) => {
            if octal {
                env.strict_error(ParseError::StrictOctalLiteral)?;
            }
            let loc = peek::loc(env).dupe();
            eat::token(env)?;
            (
                loc,
                StringLiteral {
                    value,
                    raw,
                    comments: None,
                },
            )
        }
        _ => {
            env.error_unexpected(Some("string literal".to_owned()))?;
            (
                peek::loc(env).dupe(),
                StringLiteral {
                    value: FlowSmolStr::new(""),
                    raw: FlowSmolStr::new("\"\""),
                    comments: None,
                },
            )
        }
    };

    let rparen_loc = peek::loc(env).dupe();
    expect::token(env, TokenKind::TRparen)?;

    let import_loc = Loc::between(&start_loc, &rparen_loc);
    let import_node = types::typeof_::Target::Import(Arc::new(types::generic::ImportType {
        loc: import_loc.dupe(),
        source,
        comments: mk_comments_opt(Some(leading.into()), None),
    }));
    typeof_qualification(env, import_loc, import_node)
}

fn typeof_arg(env: &mut ParserEnv) -> Result<Option<types::typeof_::Target<Loc, Loc>>, Rollback> {
    eat::push_lex_mode(env, LexMode::Normal);
    let result = match peek::token(env).clone() {
        TokenKind::TLparen => {
            eat::token(env)?;
            let typeof_result = typeof_arg(env)?;
            expect::token(env, TokenKind::TRparen)?;
            typeof_result
        }
        TokenKind::TImport => Some(typeof_import_expr(env)?),
        TokenKind::TThis => {
            let id = parser_common::identifier_name(env)?;
            Some(raw_typeof_expr_with_identifier(env, id)?)
        }
        _ if peek::is_identifier(env) => Some(typeof_expr(env)?),
        _ => {
            env.error(ParseError::InvalidTypeof)?;
            None
        }
    };
    eat::pop_lex_mode(env);
    Ok(result)
}

fn typeof_type(env: &mut ParserEnv) -> Result<types::Type<Loc, Loc>, Rollback> {
    let (loc, mut inner) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TTypeof)?;
        match typeof_arg(env)? {
            None => Ok(TypeInner::Any {
                loc: LOC_NONE,
                comments: None,
            }),
            Some(argument) => {
                let targs = if peek::is_line_terminator(env) {
                    None
                } else {
                    type_args(env)?
                };
                let trailing = eat::trailing_comments(env);
                Ok(TypeInner::Typeof {
                    loc: LOC_NONE,
                    inner: Arc::new(types::Typeof {
                        argument,
                        targs,
                        comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
                    }),
                })
            }
        }
    })?;
    *inner.loc_mut() = loc;
    Ok(types::Type::new(inner))
}

// Port of primary from type_parser.ml (lines 400-562)
fn primary(env: &mut ParserEnv) -> Result<types::Type<Loc, Loc>, Rollback> {
    let loc = peek::loc(env).dupe();
    match peek::token(env).clone() {
        TokenKind::TMult => {
            let leading = peek::comments(env);
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            Ok(types::Type::new(TypeInner::Exists {
                loc: loc.dupe(),
                comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            }))
        }
        TokenKind::TLessThan => {
            let t = function_type(env)?;
            error_on_unparenthesized_function_type(env, &t);
            Ok(t)
        }
        TokenKind::TLparen => function_or_group(env),
        TokenKind::TLcurly | TokenKind::TLcurlybar => {
            let (loc, obj) = object_type(env, false, true, true)?;
            Ok(types::Type::new(TypeInner::Object {
                loc,
                inner: Arc::new(obj),
            }))
        }
        TokenKind::TInterface => {
            let (loc, interface) = with_loc(None, env, |env| {
                let leading = peek::comments(env);
                eat::token(env)?;
                let (extends, body) = interface_helper(env)?;
                Ok(types::Interface {
                    extends: extends.into(),
                    body,
                    comments: mk_comments_opt(Some(leading.into()), None),
                })
            })?;
            Ok(types::Type::new(TypeInner::Interface {
                loc,
                inner: Arc::new(interface),
            }))
        }
        TokenKind::TTypeof => typeof_type(env),
        TokenKind::TImport if peek::current_token_is_followed_by_lparen(env) => {
            import_type_generic(env)
        }
        TokenKind::TLbracket => tuple(env),
        TokenKind::TIdentifier { raw, .. }
            if raw == "component" && env.parse_options().components =>
        {
            let (loc, component) = with_loc(None, env, |env| {
                let leading = peek::comments(env);
                expect::identifier(env, "component")?;
                let mut tparams = type_params(env)?;
                comment_attachment::type_params_remove_trailing(env, tparams.as_mut());
                let mut params = component_param_list(env)?;
                let renders = if peek::is_renders_ident(env) {
                    let mut renders = renders_annotation_opt(env)?;
                    comment_attachment::component_renders_annotation_remove_trailing(
                        env,
                        &mut renders,
                    );
                    renders
                } else {
                    let missing_annotation = renders_annotation_opt(env)?;
                    comment_attachment::component_type_params_remove_trailing(env, &mut params);
                    missing_annotation
                };
                Ok(types::Component {
                    tparams,
                    params,
                    renders,
                    comments: mk_comments_opt(Some(leading.into()), None),
                })
            })?;
            Ok(types::Type::new(TypeInner::Component {
                loc,
                inner: Arc::new(component),
            }))
        }
        TokenKind::TIdentifier { raw, .. } if raw == "renders" => {
            let (loc, renders) = with_loc(None, env, render_type)?;
            Ok(types::Type::new(TypeInner::Renders {
                loc,
                inner: Arc::new(renders),
            }))
        }
        TokenKind::TRendersQuestion | TokenKind::TRendersStar => {
            let (loc, renders) = with_loc(None, env, render_type)?;
            Ok(types::Type::new(TypeInner::Renders {
                loc,
                inner: Arc::new(renders),
            }))
        }
        TokenKind::TIdentifier { raw, .. }
            if raw == "unique" && peek::identifier_token_is_followed_by_symbol_type(env) =>
        {
            let (loc, mut inner) = with_loc(None, env, |env| {
                let leading = peek::comments(env);
                eat::token(env)?;
                let trailing = eat::trailing_comments(env);
                let symbol_loc = peek::loc(env).dupe();
                eat::token(env)?;
                Ok(TypeInner::UniqueSymbol {
                    loc: LOC_NONE,
                    symbol_loc,
                    comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
                })
            })?;
            *inner.loc_mut() = loc;
            Ok(types::Type::new(inner))
        }
        TokenKind::TIdentifier { raw, .. } if raw == "hook" && env.parse_options().components => {
            if peek::token_after_current_starts_type_call(env) {
                hook(env)
            } else {
                let (loc, g) = generic(env)?;
                Ok(types::Type::new(TypeInner::Generic {
                    loc,
                    inner: Arc::new(g),
                }))
            }
        }
        TokenKind::TNew => {
            if peek::token_after_current_starts_type_call(env) {
                let t = constructor_type(env, false)?;
                error_on_unparenthesized_function_type(env, &t);
                Ok(t)
            } else {
                let (loc, g) = generic(env)?;
                Ok(types::Type::new(TypeInner::Generic {
                    loc,
                    inner: Arc::new(g),
                }))
            }
        }
        TokenKind::TIdentifier { raw, .. } if raw == "abstract" => {
            if peek::token_after_current_is_new(env) {
                let t = constructor_type(env, true)?;
                error_on_unparenthesized_function_type(env, &t);
                Ok(t)
            } else {
                let (loc, g) = generic(env)?;
                Ok(types::Type::new(TypeInner::Generic {
                    loc,
                    inner: Arc::new(g),
                }))
            }
        }
        TokenKind::TIdentifier { .. } | TokenKind::TExtends | TokenKind::TStatic => {
            // `extends` is reserved, but recover by treating it as an identifier
            // `static` is reserved, but recover by treating it as an identifier
            let (loc, g) = generic(env)?;
            Ok(types::Type::new(TypeInner::Generic {
                loc,
                inner: Arc::new(g),
            }))
        }
        TokenKind::TString(_, value, raw, octal) => {
            if octal {
                env.strict_error(ParseError::StrictOctalLiteral)?;
            }
            let leading = peek::comments(env);
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            Ok(types::Type::new(TypeInner::StringLiteral {
                loc: loc.dupe(),
                literal: StringLiteral {
                    value,
                    raw,
                    comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
                },
            }))
        }
        TokenKind::TMinus => negate(env),
        TokenKind::TNumberSingletonType { kind, value, raw } => {
            let (loc, literal) = with_loc(None, env, |env| {
                number_singleton(env, None, kind, value, raw)
            })?;
            Ok(types::Type::new(TypeInner::NumberLiteral { loc, literal }))
        }
        TokenKind::TBigintSingletonType {
            kind: _,
            value,
            raw,
        } => {
            let (loc, literal) =
                with_loc(None, env, |env| bigint_singleton(env, None, value, raw))?;
            Ok(types::Type::new(TypeInner::BigIntLiteral { loc, literal }))
        }
        TokenKind::TTrue | TokenKind::TFalse => {
            let leading = peek::comments(env);
            let value = peek::token(env) == &TokenKind::TTrue;
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            Ok(types::Type::new(TypeInner::BooleanLiteral {
                loc: loc.dupe(),
                literal: BooleanLiteral {
                    value,
                    comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
                },
            }))
        }
        TokenKind::TKeyof => {
            let (loc, keyof) = with_loc(None, env, |env| {
                let leading = peek::comments(env);
                eat::token(env)?;
                let trailing = eat::trailing_comments(env);
                let argument = env.with_function_type_parens_context(
                    Some(FunctionTypeParensContext::TypeOperator),
                    prefix,
                )?;
                Ok(types::Keyof {
                    argument,
                    comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
                })
            })?;
            Ok(types::Type::new(TypeInner::Keyof {
                loc,
                inner: Arc::new(keyof),
            }))
        }
        TokenKind::TReadonly => {
            let (loc, readonly) = with_loc(None, env, |env| {
                let leading = peek::comments(env);
                eat::token(env)?;
                let trailing = eat::trailing_comments(env);
                let argument = if token_needs_identifier_reparse_before_label(env) {
                    generic_type_from_current_identifier_name(env)?
                } else {
                    env.with_function_type_parens_context(
                        Some(FunctionTypeParensContext::TypeOperator),
                        prefix,
                    )?
                };
                Ok(types::ReadOnly {
                    argument,
                    comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
                })
            })?;
            Ok(types::Type::new(TypeInner::ReadOnly {
                loc,
                inner: Arc::new(readonly),
            }))
        }
        TokenKind::TInfer => {
            let (loc, infer) = with_loc(None, env, |env| {
                let leading = peek::comments(env);
                eat::token(env)?;
                let trailing = eat::trailing_comments(env);
                let (tparam_loc, mut tparam) = with_loc(None, env, |env| {
                    let name = type_identifier(env)?;
                    let fallback = types::AnnotationOrHint::Missing(peek::loc(env).dupe());
                    let bound = try_parse::or_else(
                        env,
                        |env| {
                            if !eat::maybe(env, TokenKind::TExtends)? {
                                return Err(Rollback);
                            }
                            let bound = parse_union_no_wrap(env)?;
                            if env.no_conditional_type() || peek::token(env) != &TokenKind::TPling {
                                Ok(types::AnnotationOrHint::Available(types::Annotation {
                                    loc: bound.loc().dupe(),
                                    annotation: bound,
                                }))
                            } else {
                                Err(Rollback)
                            }
                        },
                        fallback,
                    );
                    Ok(types::TypeParam {
                        loc: LOC_NONE,
                        name,
                        bound,
                        bound_kind: types::type_param::BoundKind::Extends,
                        variance: None,
                        default: None,
                        const_: None,
                    })
                })?;
                tparam.loc = tparam_loc;
                Ok(types::Infer {
                    tparam,
                    comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
                })
            })?;
            Ok(types::Type::new(TypeInner::Infer {
                loc,
                inner: Arc::new(infer),
            }))
        }
        TokenKind::TTemplatePart(part) => {
            let part = part.clone();
            let (template_loc, template) = template_literal_type(env, part)?;
            Ok(types::Type::new(TypeInner::TemplateLiteral {
                loc: template_loc,
                inner: Arc::new(template),
            }))
        }
        _ => {
            if let Some(prim) = primitive(env, loc.dupe())? {
                Ok(prim)
            } else {
                env.error_unexpected(Some("a type".to_owned()))?;
                Ok(types::Type::new(TypeInner::Any {
                    loc,
                    comments: None,
                }))
            }
        }
    }
}

fn template_literal_type(
    env: &mut ParserEnv,
    part: token::TemplatePart,
) -> Result<(Loc, types::TypeTemplateLiteral<Loc, Loc>), Rollback> {
    fn template_parts(
        env: &mut ParserEnv,
        mut quasis: Vec<types::type_template_literal::Element<Loc>>,
        mut types_acc: Vec<types::Type<Loc, Loc>>,
    ) -> Result<
        (
            Loc,
            Vec<types::type_template_literal::Element<Loc>>,
            Vec<types::Type<Loc, Loc>>,
        ),
        Rollback,
    > {
        loop {
            let t = parse_type(env)?;
            let t_loc = t.loc().dupe();
            types_acc.push(t);
            let prev_lex_cursor = peek::lex_cursor(env);
            match peek::token(env) {
                TokenKind::TRcurly => {
                    eat::rescan_as_template_from(env, prev_lex_cursor);
                    let (loc, part, is_tail) = match peek::token(env) {
                        TokenKind::TTemplatePart(template_part) => {
                            let template_part = template_part.clone();
                            let loc = template_part.loc.dupe();
                            eat::token(env)?;
                            let element = types::type_template_literal::Element {
                                loc: loc.dupe(),
                                value: types::type_template_literal::Value {
                                    cooked: template_part.value,
                                    raw: template_part.raw,
                                },
                                tail: template_part.tail,
                            };
                            (loc, element, template_part.tail)
                        }
                        t => unreachable!("Expected template part, but got {:?}", t),
                    };
                    quasis.push(part);
                    if is_tail {
                        return Ok((loc, quasis, types_acc));
                    }
                }
                _ => {
                    // Malformed template
                    env.error_unexpected(Some("a template literal part".to_owned()))?;
                    let imaginary_quasi = types::type_template_literal::Element {
                        loc: t_loc.dupe(),
                        value: types::type_template_literal::Value {
                            raw: FlowSmolStr::new_inline(""),
                            cooked: FlowSmolStr::new_inline(""),
                        },
                        tail: true,
                    };
                    quasis.push(imaginary_quasi);
                    return Ok((t_loc, quasis, types_acc));
                }
            }
        }
    }

    let start_loc = part.loc.dupe();
    let cooked = part.value.dupe();
    let raw = part.raw.dupe();
    let is_tail = part.tail;
    let leading = peek::comments(env);
    expect::token(env, TokenKind::TTemplatePart(part))?;
    let (end_loc, quasis, types) = {
        let head = types::type_template_literal::Element {
            loc: start_loc.dupe(),
            value: types::type_template_literal::Value { cooked, raw },
            tail: is_tail,
        };
        if is_tail {
            (start_loc.dupe(), vec![head], vec![])
        } else {
            template_parts(env, vec![head], vec![])?
        }
    };
    let trailing = eat::trailing_comments(env);
    let loc = Loc::between(&start_loc, &end_loc);
    Ok((
        loc,
        types::TypeTemplateLiteral {
            quasis: quasis.into(),
            types: types.into(),
            comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        },
    ))
}

fn negate(env: &mut ParserEnv) -> Result<types::Type<Loc, Loc>, Rollback> {
    let (loc, mut inner) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        eat::token(env)?;
        match peek::token(env).clone() {
            TokenKind::TNumberSingletonType { kind, value, raw } => {
                let raw = raw.to_owned();
                let literal = number_singleton(env, Some(leading), kind, value, raw)?;
                Ok(TypeInner::NumberLiteral {
                    loc: LOC_NONE,
                    literal,
                })
            }
            TokenKind::TBigintSingletonType {
                kind: _,
                value,
                raw,
            } => {
                let raw = raw.to_owned();
                let literal = bigint_singleton(env, Some(leading), value, raw)?;
                Ok(TypeInner::BigIntLiteral {
                    loc: LOC_NONE,
                    literal,
                })
            }
            _ => {
                env.error_unexpected(Some("a number literal type".to_owned()))?;
                Ok(TypeInner::Any {
                    loc: LOC_NONE,
                    comments: None,
                })
            }
        }
    })?;
    *inner.loc_mut() = loc;
    Ok(types::Type::new(inner))
}

fn is_primitive(token: &TokenKind) -> bool {
    matches!(
        token,
        TokenKind::TAnyType
            | TokenKind::TMixedType
            | TokenKind::TEmptyType
            | TokenKind::TBooleanType { .. }
            | TokenKind::TNumberType
            | TokenKind::TBigintType
            | TokenKind::TStringType
            | TokenKind::TSymbolType
            | TokenKind::TVoidType
            | TokenKind::TNull
            | TokenKind::TUnknownType
            | TokenKind::TNeverType
            | TokenKind::TUndefinedType
    )
}

fn generic_of_primitive(
    env: &mut ParserEnv,
    name: &'static str,
) -> Option<types::Generic<Loc, Loc>> {
    let leading = peek::comments(env);
    let (loc, _) = with_loc(None, env, |env| {
        eat::token(env)?;
        Ok(())
    })
    .ok()?;
    let trailing = eat::trailing_comments(env);
    let comments = mk_comments_opt(Some(leading.into()), Some(trailing.into()));
    let id = Identifier::new(IdentifierInner {
        loc: loc.dupe(),
        name: FlowSmolStr::new_inline(name),
        comments: comments.clone(),
    });
    Some(types::Generic {
        id: types::generic::Identifier::Unqualified(id),
        targs: None,
        comments,
    })
}

fn primitive(env: &mut ParserEnv, loc: Loc) -> Result<Option<types::Type<Loc, Loc>>, Rollback> {
    let leading = peek::comments(env);
    Ok(match peek::token(env) {
        TokenKind::TAnyType => {
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            Some(types::Type::new(TypeInner::Any {
                loc,
                comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            }))
        }
        TokenKind::TMixedType => {
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            Some(types::Type::new(TypeInner::Mixed {
                loc,
                comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            }))
        }
        TokenKind::TEmptyType => {
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            Some(types::Type::new(TypeInner::Empty {
                loc,
                comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            }))
        }
        TokenKind::TBooleanType(b) => {
            let raw = match b {
                token::BoolOrBoolean::Bool => types::BooleanRaw::Bool,
                token::BoolOrBoolean::Boolean => types::BooleanRaw::Boolean,
            };
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            let comments = mk_comments_opt(Some(leading.into()), Some(trailing.into()));
            Some(types::Type::new(TypeInner::Boolean { loc, raw, comments }))
        }
        TokenKind::TNumberType => {
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            Some(types::Type::new(TypeInner::Number {
                loc,
                comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            }))
        }
        TokenKind::TBigintType => {
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            Some(types::Type::new(TypeInner::BigInt {
                loc,
                comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            }))
        }
        TokenKind::TStringType => {
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            Some(types::Type::new(TypeInner::String {
                loc,
                comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            }))
        }
        TokenKind::TSymbolType => {
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            Some(types::Type::new(TypeInner::Symbol {
                loc,
                comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            }))
        }
        TokenKind::TVoidType => {
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            Some(types::Type::new(TypeInner::Void {
                loc,
                comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            }))
        }
        TokenKind::TNull => {
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            Some(types::Type::new(TypeInner::Null {
                loc,
                comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            }))
        }
        TokenKind::TUnknownType => {
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            Some(types::Type::new(TypeInner::Unknown {
                loc,
                comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            }))
        }
        TokenKind::TNeverType => {
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            Some(types::Type::new(TypeInner::Never {
                loc,
                comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            }))
        }
        TokenKind::TUndefinedType => {
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            Some(types::Type::new(TypeInner::Undefined {
                loc,
                comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            }))
        }
        TokenKind::TAsserts => generic_of_primitive(env, "asserts").map(|g| {
            types::Type::new(TypeInner::Generic {
                loc,
                inner: Arc::new(g),
            })
        }),
        TokenKind::TImplies => generic_of_primitive(env, "implies").map(|g| {
            types::Type::new(TypeInner::Generic {
                loc,
                inner: Arc::new(g),
            })
        }),
        TokenKind::TIs => generic_of_primitive(env, "is").map(|g| {
            types::Type::new(TypeInner::Generic {
                loc,
                inner: Arc::new(g),
            })
        }),
        _ => None,
    })
}

fn tuple(env: &mut ParserEnv) -> Result<types::Type<Loc, Loc>, Rollback> {
    enum ElementResult {
        TupleElement(types::tuple::Element<Loc, Loc>),
        InexactTupleMarker,
    }

    fn element(env: &mut ParserEnv) -> Result<ElementResult, Rollback> {
        let (loc, mut element_result) = with_loc(None, env, |env| {
            if eat::maybe(env, TokenKind::TEllipsis)? {
                match peek::token(env) {
                    TokenKind::TEof | TokenKind::TRbracket => Ok(ElementResult::InexactTupleMarker),
                    TokenKind::TComma => {
                        env.error_unexpected(Some(
                            "the end of a tuple type (no trailing comma is allowed in inexact tuple type).".to_owned(),
                        ))?;
                        eat::token(env)?;
                        Ok(ElementResult::InexactTupleMarker)
                    }
                    _ => {
                        let annot = type_annotation_before_label(env)?;
                        let (name, annot) = match peek::token(env) {
                            TokenKind::TPling | TokenKind::TColon => {
                                let name = reparse_type_annotation_as_identifier(env, &annot)?;
                                if peek::token(env) == &TokenKind::TPling {
                                    env.error(ParseError::InvalidTupleOptionalSpread)?;
                                    eat::token(env)?;
                                }
                                expect::token(env, TokenKind::TColon)?;
                                (Some(name), type_inner(env)?)
                            }
                            _ => (None, annot),
                        };
                        Ok(ElementResult::TupleElement(
                            types::tuple::Element::SpreadElement {
                                loc: LOC_NONE,
                                element: types::tuple::SpreadElement { name, annot },
                            },
                        ))
                    }
                }
            } else {
                let is_plus = matches!(peek::token(env), TokenKind::TPlus);
                let is_minus_with_identifier = matches!(peek::token(env), TokenKind::TMinus)
                    && peek::current_token_is_followed_by_identifier(env);
                let readonly_loc = if peek::token(env) == &TokenKind::TReadonly {
                    Some(peek::loc(env).dupe())
                } else {
                    None
                };
                // writeonly foo: T
                let is_writeonly_with_identifier =
                    matches!(peek::token(env), TokenKind::TWriteonly)
                        && peek::current_token_is_followed_by_identifier(env);
                let variance = if is_plus || is_minus_with_identifier {
                    // `-1` is a valid type but not a valid tuple label.
                    //   But `-foo` is only valid as a tuple label.
                    maybe_variance(env, false, false)?
                } else if is_writeonly_with_identifier {
                    maybe_variance(env, true, false)?
                } else {
                    None
                };

                let error_invalid_tuple_variance = |env: &mut ParserEnv,
                                                    variance: &Option<Variance<Loc>>,
                                                    annot: &types::Type<Loc, Loc>|
                 -> Result<(), Rollback> {
                    if variance.is_some() {
                        env.error_at(annot.loc().dupe(), ParseError::InvalidTupleVariance)?;
                    }
                    Ok(())
                };

                let annot = type_annotation_before_label(env)?;
                let readonly_type_prefix =
                    readonly_loc.is_some() && matches!(annot.deref(), TypeInner::ReadOnly { .. });
                let (variance, annot) = match (readonly_loc, annot.deref()) {
                    (Some(loc), TypeInner::ReadOnly { inner, .. })
                        if peek::token(env) == &TokenKind::TColon
                            || (peek::token(env) == &TokenKind::TPling
                                && peek::token_after_current_is_colon(env)) =>
                    {
                        (
                            Some(Variance {
                                loc,
                                kind: VarianceKind::Readonly,
                                comments: inner.comments.dupe(),
                            }),
                            inner.argument.dupe(),
                        )
                    }
                    _ => (variance, annot),
                };
                match peek::token(env) {
                    TokenKind::TPling => {
                        let optional_loc = peek::loc(env).dupe();
                        eat::token(env)?; // eat T_PLING
                        if peek::token(env) == &TokenKind::TColon {
                            let name = reparse_type_annotation_as_identifier(env, &annot)?;
                            // foo?: type — labeled optional element
                            eat::token(env)?;
                            let annot = type_inner(env)?;
                            Ok(ElementResult::TupleElement(
                                types::tuple::Element::LabeledElement {
                                    loc: LOC_NONE,
                                    element: types::tuple::LabeledElement {
                                        name,
                                        annot,
                                        variance,
                                        optional: true,
                                    },
                                },
                            ))
                        } else {
                            // Foo? — optional unlabeled element with identifier type
                            error_invalid_tuple_variance(env, &variance, &annot)?;
                            if readonly_type_prefix {
                                env.error_at(
                                    optional_loc,
                                    ParseError::OptionalReadonlyTupleElementNeedsParens,
                                )?;
                            }
                            Ok(ElementResult::TupleElement(
                                types::tuple::Element::UnlabeledElement {
                                    loc: LOC_NONE,
                                    annot,
                                    optional: true,
                                },
                            ))
                        }
                    }
                    TokenKind::TColon => {
                        let name = reparse_type_annotation_as_identifier(env, &annot)?;
                        expect::token(env, TokenKind::TColon)?;
                        let annot = type_inner(env)?;
                        Ok(ElementResult::TupleElement(
                            types::tuple::Element::LabeledElement {
                                loc: LOC_NONE,
                                element: types::tuple::LabeledElement {
                                    name,
                                    annot,
                                    variance,
                                    optional: false,
                                },
                            },
                        ))
                    }
                    _ => {
                        error_invalid_tuple_variance(env, &variance, &annot)?;
                        Ok(ElementResult::TupleElement(
                            types::tuple::Element::UnlabeledElement {
                                loc: LOC_NONE,
                                annot,
                                optional: false,
                            },
                        ))
                    }
                }
            }
        })?;
        match &mut element_result {
            ElementResult::TupleElement(element) => {
                *element.loc_mut() = loc;
            }
            ElementResult::InexactTupleMarker => {}
        }
        Ok(element_result)
    }

    fn elements(
        env: &mut ParserEnv,
    ) -> Result<(Vec<types::tuple::Element<Loc, Loc>>, bool), Rollback> {
        let mut elements = Vec::new();
        loop {
            match peek::token(env) {
                TokenKind::TEof | TokenKind::TRbracket => return Ok((elements, false)),
                _ => match element(env)? {
                    ElementResult::InexactTupleMarker => return Ok((elements, true)),
                    ElementResult::TupleElement(element) => {
                        elements.push(element);
                        // Trailing comma support (like [number, string,])
                        if peek::token(env) != &TokenKind::TRbracket {
                            expect::token(env, TokenKind::TComma)?;
                        }
                    }
                },
            }
        }
    }

    let (loc, tuple) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TLbracket)?;
        let (elements, inexact) = env.with_no_anon_function_type(false, |env| elements(env))?;
        expect::token(env, TokenKind::TRbracket)?;
        let trailing = eat::trailing_comments(env);

        Ok(types::Tuple {
            elements: elements.into(),
            inexact,
            comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        })
    })?;
    Ok(types::Type::new(TypeInner::Tuple {
        loc,
        inner: Arc::new(tuple),
    }))
}

fn render_type(env: &mut ParserEnv) -> Result<types::Renders<Loc, Loc>, Rollback> {
    let leading = peek::comments(env);
    let variant = match peek::token(env) {
        TokenKind::TIdentifier { raw, .. } if raw == "renders" => types::RendersVariant::Normal,
        TokenKind::TRendersQuestion => types::RendersVariant::Maybe,
        TokenKind::TRendersStar => types::RendersVariant::Star,
        _ => {
            unreachable!(
                "You should only call render_type after making sure the next token is a renders variant"
            )
        }
    };
    let operator_loc = peek::loc(env).dupe();
    eat::token(env)?;
    let trailing = eat::trailing_comments(env);
    let argument = prefix(env)?;
    Ok(types::Renders {
        operator_loc,
        argument,
        comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        variant,
    })
}

fn anonymous_function_param(
    env: &mut ParserEnv,
    annot: types::Type<Loc, Loc>,
) -> types::function::Param<Loc, Loc> {
    if env.oxc_type_feature_parser_rules() {
        env.error_at_without_rollback(annot.loc().dupe(), ParseError::MissingFunctionTypeParamName);
    }
    types::function::Param {
        loc: annot.loc().dupe(),
        param: types::function::ParamKind::Anonymous(annot),
    }
}

fn error_on_unparenthesized_function_type(env: &mut ParserEnv, t: &types::Type<Loc, Loc>) {
    if !env.oxc_type_feature_parser_rules() {
        return;
    }
    let Some(context) = env.function_type_parens_context() else {
        return;
    };
    let constructor = matches!(t.deref(), TypeInner::ConstructorType { .. });
    env.error_at_without_rollback(
        t.loc().dupe(),
        ParseError::FunctionTypeNeedsParens {
            constructor,
            context,
        },
    );
}

fn primitive_type_annotation_as_identifier(
    loc: &Loc,
    name: &'static str,
    comments: &Option<Syntax<Loc, ()>>,
) -> Identifier<Loc, Loc> {
    Identifier::new(IdentifierInner {
        loc: loc.dupe(),
        name: FlowSmolStr::new_inline(name),
        comments: comments.clone(),
    })
}

fn type_annotation_as_identifier(annot: &types::Type<Loc, Loc>) -> Option<Identifier<Loc, Loc>> {
    match annot.deref() {
        TypeInner::Any { loc, comments } => Some(primitive_type_annotation_as_identifier(
            loc, "any", comments,
        )),
        TypeInner::Mixed { loc, comments } => Some(primitive_type_annotation_as_identifier(
            loc, "mixed", comments,
        )),
        TypeInner::Empty { loc, comments } => Some(primitive_type_annotation_as_identifier(
            loc, "empty", comments,
        )),
        TypeInner::Boolean { loc, raw, comments } => {
            let name = match raw {
                types::BooleanRaw::Boolean => "boolean",
                types::BooleanRaw::Bool => "bool",
            };
            Some(primitive_type_annotation_as_identifier(loc, name, comments))
        }
        TypeInner::Number { loc, comments } => Some(primitive_type_annotation_as_identifier(
            loc, "number", comments,
        )),
        TypeInner::BigInt { loc, comments } => Some(primitive_type_annotation_as_identifier(
            loc, "bigint", comments,
        )),
        TypeInner::String { loc, comments } => Some(primitive_type_annotation_as_identifier(
            loc, "string", comments,
        )),
        TypeInner::Symbol { loc, comments } => Some(primitive_type_annotation_as_identifier(
            loc, "symbol", comments,
        )),
        TypeInner::Void { loc, comments } => Some(primitive_type_annotation_as_identifier(
            loc, "void", comments,
        )),
        TypeInner::Null { loc, comments } => Some(primitive_type_annotation_as_identifier(
            loc, "null", comments,
        )),
        TypeInner::Unknown { loc, comments } => Some(primitive_type_annotation_as_identifier(
            loc, "unknown", comments,
        )),
        TypeInner::Never { loc, comments } => Some(primitive_type_annotation_as_identifier(
            loc, "never", comments,
        )),
        TypeInner::Undefined { loc, comments } => Some(primitive_type_annotation_as_identifier(
            loc,
            "undefined",
            comments,
        )),
        TypeInner::Generic { inner, .. } if inner.targs.is_none() => match &inner.id {
            types::generic::Identifier::Unqualified(id) => Some(id.dupe()),
            _ => None,
        },
        _ => None,
    }
}

fn reparse_type_annotation_as_identifier(
    env: &mut ParserEnv,
    annot: &types::Type<Loc, Loc>,
) -> Result<Identifier<Loc, Loc>, Rollback> {
    match type_annotation_as_identifier(annot) {
        Some(id) => Ok(id),
        None => {
            env.error_at(
                annot.loc().dupe(),
                ParseError::UnexpectedWithExpected("type".to_owned(), "an identifier".to_owned()),
            )?;
            Ok(Identifier::new(IdentifierInner {
                loc: annot.loc().dupe(),
                name: FlowSmolStr::new_inline(""),
                comments: None,
            }))
        }
    }
}

fn function_param_from_type_annotation(
    env: &mut ParserEnv,
    left: types::Type<Loc, Loc>,
) -> Result<types::function::Param<Loc, Loc>, Rollback> {
    let start_loc = left.loc().dupe();
    let name = reparse_type_annotation_as_identifier(env, &left)?;
    let (loc, (optional, annot)) = with_loc(Some(start_loc), env, |env| {
        let optional = eat::maybe(env, TokenKind::TPling)?;
        expect::token(env, TokenKind::TColon)?;
        let annot = type_inner(env)?;
        Ok((optional, annot))
    })?;
    Ok(types::function::Param {
        loc,
        param: types::function::ParamKind::Labeled {
            name,
            annot,
            optional,
        },
    })
}

fn generic_type_from_current_identifier_name(
    env: &mut ParserEnv,
) -> Result<types::Type<Loc, Loc>, Rollback> {
    let (loc, id) = with_loc(None, env, parser_common::identifier_name)?;
    Ok(types::Type::new(TypeInner::Generic {
        loc,
        inner: Arc::new(types::Generic {
            id: types::generic::Identifier::Unqualified(id),
            targs: None,
            comments: None,
        }),
    }))
}

fn token_needs_identifier_reparse_before_label(env: &mut ParserEnv) -> bool {
    let token_needs_reparse = match peek::token(env) {
        TokenKind::TStatic
        | TokenKind::TReadonly
        | TokenKind::TWriteonly
        | TokenKind::TKeyof
        | TokenKind::TInfer
        | TokenKind::TAsserts
        | TokenKind::TIs
        | TokenKind::TImplies
        | TokenKind::TVoidType
        | TokenKind::TTypeof => true,
        TokenKind::TIdentifier { raw, .. } => {
            raw == "component" || raw == "renders" || raw == "function"
        }
        _ => false,
    };
    token_needs_reparse && peek::token_after_current_is_pling_or_colon(env)
}

fn type_annotation_before_label(env: &mut ParserEnv) -> Result<types::Type<Loc, Loc>, Rollback> {
    if token_needs_identifier_reparse_before_label(env) {
        generic_type_from_current_identifier_name(env)
    } else {
        type_inner(env)
    }
}

fn function_param_with_current_token_identifier(
    env: &mut ParserEnv,
) -> Result<types::function::Param<Loc, Loc>, Rollback> {
    let (loc, (name, optional, annot)) = with_loc(None, env, |env| {
        eat::push_lex_mode(env, LexMode::Normal);
        let name = main_parser::parse_identifier(env, None)?;
        eat::pop_lex_mode(env);
        if !env.should_parse_types() {
            env.error(ParseError::UnexpectedTypeAnnotation)?;
        }
        let optional = eat::maybe(env, TokenKind::TPling)?;
        expect::token(env, TokenKind::TColon)?;
        let annot = type_inner(env)?;
        Ok((name, optional, annot))
    })?;
    Ok(types::function::Param {
        loc,
        param: types::function::ParamKind::Labeled {
            name,
            annot,
            optional,
        },
    })
}

fn function_param_with_renders_question(
    env: &mut ParserEnv,
) -> Result<types::function::Param<Loc, Loc>, Rollback> {
    let (loc, (name, annot)) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        let mut name_loc = peek::loc(env).dupe();
        name_loc.end.column -= 1;
        eat::token(env)?;
        if !env.should_parse_types() {
            env.error(ParseError::UnexpectedTypeAnnotation)?;
        }
        expect::token(env, TokenKind::TColon)?;
        let annot = type_inner(env)?;
        let name = Identifier::new(IdentifierInner {
            loc: name_loc,
            name: FlowSmolStr::new_inline("renders"),
            comments: mk_comments_opt(Some(leading.into()), None),
        });
        Ok((name, annot))
    })?;
    Ok(types::function::Param {
        loc,
        param: types::function::ParamKind::Labeled {
            name,
            annot,
            optional: true,
        },
    })
}

fn destructuring_function_param(
    env: &mut ParserEnv,
    allow_optional: bool,
) -> Result<types::function::Param<Loc, Loc>, Rollback> {
    let (loc, param) = with_loc(None, env, |env| {
        let pattern = env.with_ambient_context(true, |env| {
            pattern_parser::pattern(env, allow_optional, ParseError::StrictParamName)
        })?;
        Ok(types::function::ParamKind::Destructuring(pattern))
    })?;
    Ok(types::function::Param { loc, param })
}

fn function_param_list_without_parens(
    env: &mut ParserEnv,
    mut params: Vec<types::function::Param<Loc, Loc>>,
) -> Result<types::function::Params<Loc, Loc>, Rollback> {
    fn param(
        env: &mut ParserEnv,
        allow_optional: bool,
        is_rest: bool,
    ) -> Result<types::function::Param<Loc, Loc>, Rollback> {
        match peek::token(env).clone() {
            TokenKind::TLcurly | TokenKind::TLbracket => {
                if env.is_ts() {
                    destructuring_function_param(env, allow_optional)
                } else if is_rest {
                    match try_parse::to_parse(env, |env| {
                        env.with_error_callback(
                            |_| Err(Rollback),
                            |env| {
                                let annot = type_inner(env)?;
                                if peek::token(env) == &TokenKind::TColon {
                                    return Err(Rollback);
                                }
                                Ok(anonymous_function_param(env, annot))
                            },
                        )
                    }) {
                        try_parse::ParseResult::ParsedSuccessfully(param) => Ok(param),
                        try_parse::ParseResult::FailedToParse => {
                            destructuring_function_param(env, allow_optional)
                        }
                    }
                } else {
                    let annot = type_inner(env)?;
                    Ok(anonymous_function_param(env, annot))
                }
            }
            _ => {
                if token_needs_identifier_reparse_before_label(env) {
                    return function_param_with_current_token_identifier(env);
                }
                let annot = type_inner(env)?;
                if matches!(peek::token(env), TokenKind::TPling | TokenKind::TColon) {
                    function_param_from_type_annotation(env, annot)
                } else {
                    Ok(anonymous_function_param(env, annot))
                }
            }
        }
    }

    let mut this = None;
    loop {
        match peek::token(env).clone() {
            TokenKind::TEof | TokenKind::TEllipsis | TokenKind::TRparen => {
                let rest = if peek::token(env) == &TokenKind::TEllipsis {
                    let (loc, mut param) = with_loc(None, env, |env| {
                        let leading = peek::comments(env);
                        expect::token(env, TokenKind::TEllipsis)?;
                        let argument = param(env, false, true)?;
                        Ok(types::function::RestParam {
                            loc: LOC_NONE,
                            argument,
                            comments: mk_comments_opt(Some(leading.into()), None),
                        })
                    })?;
                    param.loc = loc;
                    Some(param)
                } else {
                    None
                };
                return Ok(types::function::Params {
                    loc: LOC_NONE,
                    params: params.into(),
                    rest,
                    this,
                    comments: None,
                });
            }
            token
                if (matches!(token, TokenKind::TThis)
                    || matches!(&token, TokenKind::TIdentifier { raw, .. } if raw == "this"))
                    && peek::token_after_current_is_pling_or_colon(env) =>
            {
                if this.is_some() || !params.is_empty() {
                    env.error(ParseError::ThisParamMustBeFirst)?;
                }
                let (loc, mut this_param) = with_loc(None, env, |env| {
                    let leading = peek::comments(env);
                    eat::token(env)?;
                    if peek::token(env) == &TokenKind::TPling {
                        env.error(ParseError::ThisParamMayNotBeOptional)?;
                    }
                    let annot = annotation(env)?;
                    Ok(types::function::ThisParam {
                        loc: LOC_NONE,
                        annot,
                        comments: mk_comments_opt(Some(leading.into()), None),
                    })
                })?;
                this_param.loc = loc;
                if peek::token(env) != &TokenKind::TRparen {
                    expect::token(env, TokenKind::TComma)?;
                }
                this = Some(this_param);
            }
            TokenKind::TIdentifier { raw, .. } if raw == "this" => {
                params.push(param(env, true, false)?);
                if peek::token(env) != &TokenKind::TRparen {
                    expect::token(env, TokenKind::TComma)?;
                }
            }
            _ => {
                params.push(param(env, true, false)?);
                if peek::token(env) != &TokenKind::TRparen {
                    expect::token(env, TokenKind::TComma)?;
                }
            }
        }
    }
}

fn function_param_list(env: &mut ParserEnv) -> Result<types::function::Params<Loc, Loc>, Rollback> {
    let (loc, mut params) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TLparen)?;
        let mut params = function_param_list_without_parens(env, Vec::new())?;
        let internal = peek::comments(env);
        expect::token(env, TokenKind::TRparen)?;
        let trailing = eat::trailing_comments(env);
        params.comments = ast_utils::mk_comments_with_internal_opt(
            Some(leading.into()),
            Some(trailing.into()),
            Some(internal.into()),
        );
        Ok(params)
    })?;
    params.loc = loc;
    Ok(params)
}

fn function_param_list_with_first(
    env: &mut ParserEnv,
    param: types::function::Param<Loc, Loc>,
) -> Result<types::function::Params<Loc, Loc>, Rollback> {
    if peek::token(env) != &TokenKind::TRparen {
        expect::token(env, TokenKind::TComma)?;
    }
    function_param_list_without_parens(env, vec![param])
}

fn component_param_list_without_parens(
    env: &mut ParserEnv,
) -> Result<types::component_params::Params<Loc, Loc>, Rollback> {
    fn param_name(
        env: &mut ParserEnv,
    ) -> Result<statement::component_params::ParamName<Loc, Loc>, Rollback> {
        match peek::token(env).clone() {
            TokenKind::TString(_, value, raw, octal) => {
                let value = value.dupe();
                let raw = raw.dupe();
                let loc = peek::loc(env).dupe();
                if octal {
                    env.strict_error(ParseError::StrictOctalLiteral)?;
                }
                eat::token(env)?;
                let trailing = eat::trailing_comments(env);
                Ok(statement::component_params::ParamName::StringLiteral((
                    loc,
                    StringLiteral {
                        value,
                        raw,
                        comments: mk_comments_opt(None, Some(trailing.into())),
                    },
                )))
            }
            _ => {
                eat::push_lex_mode(env, LexMode::Normal);
                let ident = parser_common::identifier_name(env)?;
                eat::pop_lex_mode(env);
                Ok(statement::component_params::ParamName::Identifier(ident))
            }
        }
    }

    fn param(env: &mut ParserEnv) -> Result<types::component_params::Param<Loc, Loc>, Rollback> {
        let (loc, mut param) = with_loc(None, env, |env| {
            let name = param_name(env)?;
            let optional = eat::maybe(env, TokenKind::TPling)?;
            let annot = annotation(env)?;
            Ok(types::component_params::Param {
                loc: LOC_NONE,
                name,
                annot,
                optional,
            })
        })?;
        param.loc = loc;
        Ok(param)
    }

    let mut params = Vec::new();
    loop {
        match peek::token(env) {
            TokenKind::TEof | TokenKind::TEllipsis | TokenKind::TRparen => {
                let rest = if peek::token(env) == &TokenKind::TEllipsis {
                    let (loc, mut rest) = with_loc(None, env, |env| {
                        let leading = peek::comments(env);
                        expect::token(env, TokenKind::TEllipsis)?;
                        let left = type_annotation_before_label(env)?;
                        let (argument, optional, annot) = match peek::token(env) {
                            TokenKind::TColon | TokenKind::TPling => {
                                let ident = reparse_type_annotation_as_identifier(env, &left)?;
                                let optional = eat::maybe(env, TokenKind::TPling)?;
                                expect::token(env, TokenKind::TColon)?;
                                (Some(ident), optional, type_inner(env)?)
                            }
                            _ => (None, false, left),
                        };
                        if peek::token(env) == &TokenKind::TComma {
                            eat::token(env)?;
                        }
                        Ok(types::component_params::RestParam {
                            loc: LOC_NONE,
                            argument,
                            annot,
                            optional,
                            comments: mk_comments_opt(Some(leading.into()), None),
                        })
                    })?;
                    rest.loc = loc;
                    Some(rest)
                } else {
                    None
                };
                return Ok(types::component_params::Params {
                    loc: LOC_NONE,
                    params: params.into(),
                    rest,
                    comments: None,
                });
            }
            _ => {
                params.push(param(env)?);
                if peek::token(env) != &TokenKind::TRparen {
                    expect::token(env, TokenKind::TComma)?;
                }
            }
        }
    }
}

fn component_param_list(
    env: &mut ParserEnv,
) -> Result<types::component_params::Params<Loc, Loc>, Rollback> {
    let (loc, mut params) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TLparen)?;
        let mut params = component_param_list_without_parens(env)?;
        let internal = peek::comments(env);
        expect::token(env, TokenKind::TRparen)?;
        let trailing = eat::trailing_comments(env);
        params.comments = ast_utils::mk_comments_with_internal_opt(
            Some(leading.into()),
            Some(trailing.into()),
            Some(internal.into()),
        );
        Ok(params)
    })?;
    params.loc = loc;
    Ok(params)
}

enum ParamListOrType {
    ParamList(types::function::Params<Loc, Loc>),
    Type(types::Type<Loc, Loc>),
}

// Port of param_list_or_type from type_parser.ml (lines 965-1051)
fn param_list_or_type(env: &mut ParserEnv) -> Result<ParamListOrType, Rollback> {
    let leading = peek::comments(env);
    expect::token(env, TokenKind::TLparen)?;

    let ret = env.with_no_anon_function_type(false, |env| {
        match peek::token(env).clone() {
            TokenKind::TEof | TokenKind::TEllipsis => {
                // (... is definitely the beginning of a param list
                Ok(ParamListOrType::ParamList(
                    function_param_list_without_parens(env, Vec::new())?,
                ))
            }
            TokenKind::TRparen => {
                // () is definitely a param list
                Ok(ParamListOrType::ParamList(types::function::Params {
                    loc: LOC_NONE,
                    this: None,
                    params: Vec::new().into(),
                    rest: None,
                    comments: None,
                }))
            }
            TokenKind::TThis if peek::token_after_current_is_pling_or_colon(env) => Ok(
                ParamListOrType::ParamList(function_param_list_without_parens(env, Vec::new())?),
            ),
            TokenKind::TIdentifier { raw, .. }
                if raw == "this" && peek::token_after_current_is_pling_or_colon(env) =>
            {
                Ok(ParamListOrType::ParamList(
                    function_param_list_without_parens(env, Vec::new())?,
                ))
            }
            TokenKind::TRendersQuestion => {
                if peek::token_after_current_is_colon(env) {
                    // Ok this is definitely a parameter
                    let param = function_param_with_renders_question(env)?;
                    Ok(ParamListOrType::ParamList(function_param_list_with_first(
                        env, param,
                    )?))
                } else {
                    Ok(ParamListOrType::Type(type_inner(env)?))
                }
            }
            TokenKind::TIdentifier { raw, .. } if raw == "renders" => {
                if peek::token_after_current_is_pling_or_colon(env) {
                    // Ok this is definitely a parameter
                    let param = function_param_with_current_token_identifier(env)?;
                    Ok(ParamListOrType::ParamList(function_param_list_with_first(
                        env, param,
                    )?))
                } else {
                    Ok(ParamListOrType::Type(type_inner(env)?))
                }
            }
            TokenKind::TIdentifier { raw, .. } if raw == "component" => {
                if env.parse_options().components {
                    if peek::token_after_current_starts_type_call(env) {
                        Ok(ParamListOrType::Type(type_inner(env)?))
                    } else {
                        function_param_or_generic_type(env)
                    }
                } else {
                    // This could be a function parameter or a generic type
                    function_param_or_generic_type(env)
                }
            }
            TokenKind::TNew
            | TokenKind::TReadonly
            | TokenKind::TWriteonly
            | TokenKind::TKeyof
            | TokenKind::TInfer
            | TokenKind::TAsserts
            | TokenKind::TIs
            | TokenKind::TImplies => {
                if peek::token_after_current_is_pling_or_colon(env) {
                    let param = function_param_with_current_token_identifier(env)?;
                    Ok(ParamListOrType::ParamList(function_param_list_with_first(
                        env, param,
                    )?))
                } else {
                    Ok(ParamListOrType::Type(type_inner(env)?))
                }
            }
            TokenKind::TIdentifier { raw, .. } if raw == "abstract" => {
                if peek::token_after_current_is_new(env) {
                    Ok(ParamListOrType::Type(type_inner(env)?))
                } else {
                    function_param_or_generic_type(env)
                }
            }
            TokenKind::TStatic if peek::token_after_current_is_pling_or_colon(env) => {
                let param = function_param_with_current_token_identifier(env)?;
                Ok(ParamListOrType::ParamList(function_param_list_with_first(
                    env, param,
                )?))
            }
            TokenKind::TIdentifier { .. } | TokenKind::TStatic => {
                // This could be a function parameter or a generic type
                function_param_or_generic_type(env)
            }
            _ => {
                if matches!(peek::token(env), TokenKind::TLcurly | TokenKind::TLbracket)
                    && env.is_ts()
                {
                    // In TypeScript, ({...} or ([... could be a parenthesized type or
                    // a destructuring function parameter. Try parsing as type first.
                    fn error_callback(_err: ParseError) -> Result<(), Rollback> {
                        Err(Rollback)
                    }
                    let try_type =
                        |env: &mut ParserEnv| -> Result<types::Type<Loc, Loc>, Rollback> {
                            env.with_error_callback(error_callback, |env| {
                                let t = type_inner(env)?;
                                // After parsing, verify we're at ) or , — otherwise this isn't
                                // a simple parenthesized type
                                match peek::token(env) {
                                    TokenKind::TRparen | TokenKind::TComma => {}
                                    _ => return Err(Rollback),
                                }
                                Ok(t)
                            })
                        };
                    match try_parse::to_parse(env, try_type) {
                        try_parse::ParseResult::ParsedSuccessfully(t) => {
                            Ok(ParamListOrType::Type(t))
                        }
                        try_parse::ParseResult::FailedToParse => {
                            // Type parse failed. Try destructuring. If that also fails,
                            // re-parse as type to produce type-level error messages.
                            match try_parse::to_parse(env, |env| {
                                env.with_error_callback(error_callback, |env| {
                                    function_param_list_without_parens(env, Vec::new())
                                })
                            }) {
                                try_parse::ParseResult::ParsedSuccessfully(params) => {
                                    Ok(ParamListOrType::ParamList(params))
                                }
                                try_parse::ParseResult::FailedToParse => {
                                    Ok(ParamListOrType::Type(type_inner(env)?))
                                }
                            }
                        }
                    }
                } else if is_primitive(peek::token(env)) {
                    // Don't know if this is (number) or (number: number). The first
                    // is a type, the second is a param.
                    function_param_or_generic_type(env)
                } else {
                    // All params start with an identifier or `...`
                    Ok(ParamListOrType::Type(type_inner(env)?))
                }
            }
        }
    })?;

    // Now that we allow anonymous parameters in function types, we need to
    // disambiguate a little bit more
    let ret = match ret {
        ParamListOrType::ParamList(_) => ret,
        ParamListOrType::Type(_) if env.no_anon_function_type() => ret,
        ParamListOrType::Type(t) => match peek::token(env) {
            TokenKind::TComma => {
                // Reinterpret `(type,` as a ParamList
                expect::token(env, TokenKind::TComma)?;
                let param = anonymous_function_param(env, t);
                ParamListOrType::ParamList(function_param_list_without_parens(env, vec![param])?)
            }
            _ => ParamListOrType::Type(t),
        },
    };

    let internal = peek::comments(env);
    expect::token(env, TokenKind::TRparen)?;
    let trailing = eat::trailing_comments(env);

    let ret = match ret {
        ParamListOrType::ParamList(mut params) => {
            params.comments = ast_utils::mk_comments_with_internal_opt(
                Some(leading.into()),
                Some(trailing.into()),
                Some(internal.into()),
            );
            ParamListOrType::ParamList(params)
        }
        ParamListOrType::Type(t)
            if !env.no_anon_function_type() && peek::token(env) == &TokenKind::TArrow =>
        {
            // Reinterpret `(type) =>` as a ParamList after consuming `)`, so the
            // arrow is the current token instead of token 2.
            let mut params = types::function::Params {
                loc: LOC_NONE,
                params: vec![anonymous_function_param(env, t)].into(),
                rest: None,
                this: None,
                comments: None,
            };
            params.comments = ast_utils::mk_comments_with_internal_opt(
                Some(leading.into()),
                Some(trailing.into()),
                Some(internal.into()),
            );
            ParamListOrType::ParamList(params)
        }
        ParamListOrType::Type(t) => ParamListOrType::Type(add_comments(t, leading, trailing)),
    };

    Ok(ret)
}

fn function_param_or_generic_type(env: &mut ParserEnv) -> Result<ParamListOrType, Rollback> {
    if token_needs_identifier_reparse_before_label(env) {
        let param = function_param_with_current_token_identifier(env)?;
        return Ok(ParamListOrType::ParamList(function_param_list_with_first(
            env, param,
        )?));
    }
    let t = type_inner(env)?;
    if matches!(peek::token(env), TokenKind::TPling | TokenKind::TColon) {
        let param = function_param_from_type_annotation(env, t)?;
        Ok(ParamListOrType::ParamList(function_param_list_with_first(
            env, param,
        )?))
    } else {
        Ok(ParamListOrType::Type(t))
    }
}

fn function_or_group(env: &mut ParserEnv) -> Result<types::Type<Loc, Loc>, Rollback> {
    let start_loc = peek::loc(env).dupe();
    match param_list_or_type(env)? {
        ParamListOrType::ParamList(params) => {
            let t =
                function_with_params(env, start_loc, None, params, function::Effect::Arbitrary)?;
            error_on_unparenthesized_function_type(env, &t);
            Ok(t)
        }
        ParamListOrType::Type(t) => Ok(t),
    }
}

fn function_type(env: &mut ParserEnv) -> Result<types::Type<Loc, Loc>, Rollback> {
    let start_loc = peek::loc(env).dupe();
    let mut tparams = type_params(env)?;
    comment_attachment::type_params_remove_trailing(env, tparams.as_mut());
    let params = function_param_list(env)?;
    function_with_params(env, start_loc, tparams, params, function::Effect::Arbitrary)
}

fn function_with_params(
    env: &mut ParserEnv,
    start_loc: Loc,
    tparams: Option<TypeParams<Loc, Loc>>,
    params: types::function::Params<Loc, Loc>,
    effect: function::Effect,
) -> Result<types::Type<Loc, Loc>, Rollback> {
    let (loc, func) = with_loc(Some(start_loc), env, |env| {
        expect::token(env, TokenKind::TArrow)?;
        let return_ = function_return_type(env)?;
        Ok(types::Function {
            params,
            return_,
            tparams,
            comments: None,
            effect,
        })
    })?;
    Ok(types::Type::new(TypeInner::Function {
        loc,
        inner: Arc::new(func),
    }))
}

fn hook(env: &mut ParserEnv) -> Result<types::Type<Loc, Loc>, Rollback> {
    let start_loc = peek::loc(env).dupe();
    eat::token(env)?;
    let mut tparams = type_params(env)?;
    comment_attachment::type_params_remove_trailing(env, tparams.as_mut());
    let params = function_param_list(env)?;
    function_with_params(env, start_loc, tparams, params, function::Effect::Hook)
}

fn constructor_type(
    env: &mut ParserEnv,
    abstract_: bool,
) -> Result<types::Type<Loc, Loc>, Rollback> {
    let start_loc = peek::loc(env).dupe();
    if abstract_ {
        eat::token(env)?;
    }
    eat::token(env)?;
    let mut tparams = type_params(env)?;
    comment_attachment::type_params_remove_trailing(env, tparams.as_mut());
    let params = function_param_list(env)?;
    if params.this.is_some() {
        env.error_at(
            start_loc.dupe(),
            ParseError::ThisParamBannedInConstructorType,
        )?;
    }
    let effect = function::Effect::Arbitrary;
    let (loc, func) = with_loc(Some(start_loc), env, |env| {
        expect::token(env, TokenKind::TArrow)?;
        let return_ = function_return_type(env)?;
        Ok(types::Function {
            params,
            return_,
            tparams,
            comments: None,
            effect,
        })
    })?;
    Ok(types::Type::new(TypeInner::ConstructorType {
        loc,
        abstract_,
        inner: Arc::new(func),
    }))
}

fn function_return_type(
    env: &mut ParserEnv,
) -> Result<types::function::ReturnAnnotation<Loc, Loc>, Rollback> {
    if is_start_of_type_guard(env) {
        Ok(types::function::ReturnAnnotation::TypeGuard(type_guard(
            env,
        )?))
    } else {
        let t = type_inner(env)?;
        Ok(types::function::ReturnAnnotation::Available(
            types::Annotation {
                loc: t.loc().dupe(),
                annotation: t,
            },
        ))
    }
}

fn type_guard(env: &mut ParserEnv) -> Result<types::TypeGuard<Loc, Loc>, Rollback> {
    fn parse_is_type_guard(
        env: &mut ParserEnv,
    ) -> Result<(Option<types::Type<Loc, Loc>>, Vec<Comment<Loc>>), Rollback> {
        let internal = peek::comments(env);
        expect::token(env, TokenKind::TIs)?;
        let internal2 = peek::comments(env);
        let internal = [internal, internal2].concat();
        let t = type_inner(env)?;
        Ok((Some(t), internal))
    }

    let (loc, mut type_guard) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        let kind = if eat::maybe(env, TokenKind::TAsserts)? {
            types::TypeGuardKind::Asserts
        } else if eat::maybe(env, TokenKind::TImplies)? {
            types::TypeGuardKind::Implies
        } else {
            types::TypeGuardKind::Default
        };

        // Parse the identifier part as normal code, since this can be any name that
        // a parameter can be.
        eat::push_lex_mode(env, LexMode::Normal);
        let param = parser_common::identifier_name(env)?;
        eat::pop_lex_mode(env);

        let (t, internal) = if kind == types::TypeGuardKind::Implies {
            parse_is_type_guard(env)?
        } else {
            match peek::token(env) {
                TokenKind::TIs => parse_is_type_guard(env)?,
                _ => (None, Vec::new()),
            }
        };

        let guard = (param, t);
        let comments = ast_utils::mk_comments_with_internal_opt(
            Some(leading.into()),
            None,
            if internal.is_empty() {
                None
            } else {
                Some(internal.into())
            },
        );

        Ok(types::TypeGuard {
            loc: LOC_NONE,
            kind,
            guard,
            comments,
        })
    })?;
    type_guard.loc = loc;
    Ok(type_guard)
}

fn type_guard_annotation(
    env: &mut ParserEnv,
    start_loc: Loc,
) -> Result<types::TypeGuardAnnotation<Loc, Loc>, Rollback> {
    let (loc, guard) = with_loc(Some(start_loc), env, type_guard)?;
    Ok(types::TypeGuardAnnotation { loc, guard })
}

fn object_key_after_current_in_normal_mode(env: &mut ParserEnv, is_class: bool) -> bool {
    eat::push_lex_mode(env, LexMode::Normal);
    let result = peek::current_token_is_followed_by_object_key(env, is_class);
    eat::pop_lex_mode(env);
    result
}

fn object_type(
    env: &mut ParserEnv,
    _is_class: bool,
    _allow_exact: bool,
    _allow_spread: bool,
) -> Result<(Loc, types::Object<Loc, Loc>), Rollback> {
    fn methodish(
        env: &mut ParserEnv,
        start_loc: Loc,
        tparams: Option<TypeParams<Loc, Loc>>,
    ) -> Result<(Loc, types::Function<Loc, Loc>), Rollback> {
        with_loc(Some(start_loc), env, |env| {
            let params = function_param_list(env)?;
            let return_ = if *peek::token(env) != TokenKind::TColon {
                types::function::ReturnAnnotation::Missing(peek::loc_skip_lookahead(env))
            } else {
                expect::token(env, TokenKind::TColon)?;
                function_return_type(env)?
            };
            Ok(types::Function {
                params,
                return_,
                tparams,
                comments: None,
                effect: function::Effect::Arbitrary,
            })
        })
    }

    fn method_property(
        env: &mut ParserEnv,
        start_loc: Loc,
        static_: Option<Loc>,
        abstract_: bool,
        override_: bool,
        mut key: expression::object::Key<Loc, Loc>,
        optional: bool,
        ts_accessibility: Option<class::ts_accessibility::TSAccessibility<Loc>>,
        leading: Vec<Comment<Loc>>,
    ) -> Result<types::object::Property<Loc, Loc>, Rollback> {
        comment_attachment::object_key_remove_trailing(env, &mut key);
        let key = key;
        let mut tparams = type_params(env)?;
        comment_attachment::type_params_remove_trailing(env, tparams.as_mut());
        let value = methodish(env, start_loc.dupe(), tparams)?;
        let value_loc = value.0.dupe();
        let value = types::Type::new(TypeInner::Function {
            loc: value_loc.dupe(),
            inner: Arc::new(value.1),
        });

        Ok(types::object::Property::NormalProperty(
            types::object::NormalProperty {
                loc: value_loc,
                key,
                value: types::object::PropertyValue::Init(Some(value)),
                optional,
                static_: static_.is_some(),
                proto: false,
                method: true,
                // abstract;
                abstract_,
                // override;
                override_,
                variance: None,
                ts_accessibility,
                init: None,
                comments: mk_comments_opt(Some(leading.into()), None),
            },
        ))
    }

    fn call_property(
        env: &mut ParserEnv,
        start_loc: Loc,
        static_: Option<Loc>,
        leading: Vec<Comment<Loc>>,
    ) -> Result<types::object::Property<Loc, Loc>, Rollback> {
        let (loc, mut prop) = with_loc(Some(start_loc), env, |env| {
            let method_start_loc = peek::loc(env).dupe();
            let mut tparams = type_params(env)?;
            comment_attachment::type_params_remove_trailing(env, tparams.as_mut());
            let value = methodish(env, method_start_loc, tparams)?;
            Ok(types::object::CallProperty {
                loc: LOC_NONE,
                value,
                static_: static_.is_some(),
                comments: mk_comments_opt(Some(leading.into()), None),
            })
        })?;
        prop.loc = loc;
        Ok(types::object::Property::CallProperty(prop))
    }

    fn init_property(
        env: &mut ParserEnv,
        start_loc: Loc,
        variance: Option<Variance<Loc>>,
        static_: Option<Loc>,
        proto: Option<Loc>,
        abstract_: bool,
        override_: bool,
        ts_accessibility: Option<class::ts_accessibility::TSAccessibility<Loc>>,
        leading: Vec<Comment<Loc>>,
        key_loc: Loc,
        key: expression::object::Key<Loc, Loc>,
        is_class: bool,
    ) -> Result<types::object::Property<Loc, Loc>, Rollback> {
        if !env.should_parse_types() {
            env.error(ParseError::UnexpectedTypeAnnotation)?;
        }

        let (loc, mut prop) = with_loc(Some(start_loc), env, |env| {
            let optional = eat::maybe(env, TokenKind::TPling)?;
            let ate_colon = if is_class {
                eat::maybe(env, TokenKind::TColon)?
            } else {
                expect::token_maybe(env, TokenKind::TColon)?
            };
            let value = if ate_colon {
                types::object::PropertyValue::Init(Some(type_inner(env)?))
            } else if is_class {
                types::object::PropertyValue::Init(None)
            } else {
                types::object::PropertyValue::Init(Some(types::Type::new(TypeInner::Any {
                    loc: key_loc,
                    comments: None,
                })))
            };
            let init = if is_class && eat::maybe(env, TokenKind::TAssign)? {
                eat::push_lex_mode(env, LexMode::Normal);
                let init_expr = expression_parser::assignment(env)?;
                eat::pop_lex_mode(env);
                Some(init_expr)
            } else {
                None
            };
            Ok(types::object::NormalProperty {
                loc: LOC_NONE,
                key,
                value,
                optional,
                static_: static_.is_some(),
                proto: proto.is_some(),
                method: false,
                abstract_,
                override_,
                variance,
                ts_accessibility,
                init,
                comments: mk_comments_opt(Some(leading.into()), None),
            })
        })?;
        prop.loc = loc;

        Ok(types::object::Property::NormalProperty(prop))
    }

    fn getter_or_setter(
        env: &mut ParserEnv,
        is_getter: bool,
        leading: Vec<Comment<Loc>>,
        override_: bool,
        ts_accessibility: Option<class::ts_accessibility::TSAccessibility<Loc>>,
        start_loc: Loc,
        static_: Option<Loc>,
        (key_loc, mut key): (Loc, expression::object::Key<Loc, Loc>),
    ) -> Result<types::object::Property<Loc, Loc>, Rollback> {
        let (loc, mut prop) = with_loc(Some(start_loc.dupe()), env, |env| {
            comment_attachment::object_key_remove_trailing(env, &mut key);
            let (value_loc, func) = methodish(env, start_loc, None)?;

            if !is_getter
                && env.oxc_type_feature_parser_rules()
                && let [param] = &*func.params.params
            {
                let optional = match &param.param {
                    types::function::ParamKind::Anonymous(_) => false,
                    types::function::ParamKind::Labeled { optional, .. } => *optional,
                    types::function::ParamKind::Destructuring(pattern) => {
                        ast_utils::pattern_optional(pattern)
                    }
                };
                if optional {
                    env.error_at_without_rollback(
                        param.loc.dupe(),
                        ParseError::SetterMayNotHaveOptionalParam,
                    );
                }
            }

            match (is_getter, &func.params) {
                (true, types::function::Params { this: Some(_), .. }) => {
                    env.error_at(key_loc, ParseError::GetterMayNotHaveThisParam)?;
                }
                (false, types::function::Params { this: Some(_), .. }) => {
                    env.error_at(key_loc, ParseError::SetterMayNotHaveThisParam)?;
                }
                (
                    true,
                    types::function::Params {
                        params,
                        rest: None,
                        this: None,
                        ..
                    },
                ) if params.is_empty() => {}
                (false, types::function::Params { rest: Some(_), .. }) => {
                    // rest params don't make sense on a setter
                    env.error_at(key_loc, ParseError::SetterArity)?;
                }
                (false, types::function::Params { params, .. }) if params.len() == 1 => {}
                (true, _) => {
                    env.error_at(key_loc, ParseError::GetterArity)?;
                }
                (false, _) => {
                    env.error_at(key_loc, ParseError::SetterArity)?;
                }
            }

            let value_type = if is_getter {
                types::object::PropertyValue::Get(value_loc, func)
            } else {
                types::object::PropertyValue::Set(value_loc, func)
            };

            Ok(types::object::NormalProperty {
                loc: LOC_NONE,
                key,
                value: value_type,
                optional: false,
                static_: static_.is_some(),
                proto: false,
                method: false,
                abstract_: false,
                override_,
                variance: None,
                ts_accessibility,
                init: None,
                comments: mk_comments_opt(Some(leading.into()), None),
            })
        })?;
        prop.loc = loc;

        Ok(types::object::Property::NormalProperty(prop))
    }

    fn indexer_property(
        env: &mut ParserEnv,
        start_loc: Loc,
        static_: Option<Loc>,
        variance: Option<Variance<Loc>>,
        leading: Vec<Comment<Loc>>,
    ) -> Result<types::object::Property<Loc, Loc>, Rollback> {
        let id = if peek::token_after_current_is_colon(env) {
            let id = parser_common::identifier_name(env)?;
            expect::token(env, TokenKind::TColon)?;
            Some(id)
        } else {
            None
        };
        let key = type_inner(env)?;
        expect::token(env, TokenKind::TRbracket)?;
        finish_indexer(env, start_loc, static_, variance, leading, id, key)
    }

    /// Finish an indexer once its key type has been parsed and the closing `]`
    /// consumed: an optional `?`, then `: <value type>`.
    fn finish_indexer(
        env: &mut ParserEnv,
        start_loc: Loc,
        static_: Option<Loc>,
        variance: Option<Variance<Loc>>,
        leading: Vec<Comment<Loc>>,
        id: Option<Identifier<Loc, Loc>>,
        key: types::Type<Loc, Loc>,
    ) -> Result<types::object::Property<Loc, Loc>, Rollback> {
        let (loc, mut indexer) = with_loc(Some(start_loc), env, move |env| {
            let optional = eat::maybe(env, TokenKind::TPling)?;
            let trailing = eat::trailing_comments(env);
            expect::token(env, TokenKind::TColon)?;
            let value = type_inner(env)?;
            Ok(types::object::Indexer {
                loc: LOC_NONE,
                id,
                key,
                value,
                static_: static_.is_some(),
                variance,
                optional,
                comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            })
        })?;
        indexer.loc = loc;
        Ok(types::object::Property::Indexer(indexer))
    }

    // Port of mapped_type from type_parser.ml (lines 1309-1361)
    fn mapped_type(
        env: &mut ParserEnv,
        start_loc: Loc,
        variance: Option<Variance<Loc>>,
        variance_op: Option<types::object::MappedTypeVarianceOp>,
        leading: Vec<Comment<Loc>>,
    ) -> Result<types::object::Property<Loc, Loc>, Rollback> {
        let (loc, mapped_type) = with_loc(Some(start_loc), env, |env| {
            let key_id = type_identifier(env)?;
            let key_name_loc = key_id.loc.dupe();
            let key_tparam = types::TypeParam {
                loc: key_name_loc.dupe(),
                name: key_id,
                bound: types::AnnotationOrHint::Missing(key_name_loc.dupe()),
                variance: None,
                default: None,
                bound_kind: types::type_param::BoundKind::Colon,
                const_: None,
            };

            // We already checked in bracket_property that the next token was
            // an "in" identifier. Now we eat it.
            eat::token(env)?;
            let source_type = type_inner(env)?;
            // as <type>
            let name_type = match peek::token(env) {
                TokenKind::TIdentifier { raw, .. } if raw == "as" => {
                    eat::token(env)?;
                    Some(type_inner(env)?)
                }
                _ => None,
            };
            expect::token(env, TokenKind::TRbracket)?;

            let optional = match peek::token(env) {
                TokenKind::TPling => {
                    eat::token(env)?;
                    types::object::MappedTypeOptionalFlag::Optional
                }
                TokenKind::TPlus => {
                    eat::token(env)?;
                    expect::token(env, TokenKind::TPling)?;
                    types::object::MappedTypeOptionalFlag::PlusOptional
                }
                TokenKind::TMinus => {
                    eat::token(env)?;
                    expect::token(env, TokenKind::TPling)?;
                    types::object::MappedTypeOptionalFlag::MinusOptional
                }
                _ => types::object::MappedTypeOptionalFlag::NoOptionalFlag,
            };

            expect::token(env, TokenKind::TColon)?;
            let prop_type = type_inner(env)?;
            let trailing = eat::trailing_comments(env);

            Ok(types::object::MappedType {
                loc: LOC_NONE,
                key_tparam,
                source_type,
                prop_type,
                name_type,
                variance,
                variance_op,
                optional,
                comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            })
        })?;

        Ok(types::object::Property::MappedType(
            types::object::MappedType { loc, ..mapped_type },
        ))
    }

    fn well_known_symbol_property(
        env: &mut ParserEnv,
        start_loc: Loc,
        static_: Option<Loc>,
        abstract_: bool,
        override_: bool,
        variance: Option<Variance<Loc>>,
        ts_accessibility: Option<class::ts_accessibility::TSAccessibility<Loc>>,
        leading: Vec<Comment<Loc>>,
        is_class: bool,
    ) -> Result<types::object::Property<Loc, Loc>, Rollback> {
        // We've already consumed '['. Now consume 'Symbol', '.', and the property name.
        let sym_loc = peek::loc(env).dupe();
        eat::token(env)?;
        // consume 'Symbol'
        expect::token(env, TokenKind::TPeriod)?;
        let name_loc = peek::loc(env).dupe();
        let name = match peek::token(env) {
            TokenKind::TIdentifier { raw, .. } => raw.dupe(),
            _ => {
                env.error_unexpected(Some("a symbol name".to_owned()))?;
                FlowSmolStr::new_inline("")
            }
        };
        eat::token(env)?;
        // Build a Member expression AST: Symbol.X
        let member_expr = {
            let object = expression::Expression::new(expression::ExpressionInner::Identifier {
                loc: sym_loc.dupe(),
                inner: Identifier::new(IdentifierInner {
                    loc: sym_loc.dupe(),
                    name: FlowSmolStr::new_inline("Symbol"),
                    comments: None,
                }),
            });
            let property = expression::member::Property::PropertyIdentifier(Identifier::new(
                IdentifierInner {
                    loc: name_loc.dupe(),
                    name: name.dupe(),
                    comments: None,
                },
            ));
            expression::Expression::new(expression::ExpressionInner::Member {
                loc: Loc::between(&sym_loc, &name_loc),
                inner: Arc::new(expression::Member {
                    object,
                    property,
                    comments: None,
                }),
            })
        };
        let computed_loc = Loc::between(&start_loc, peek::loc(env));
        expect::token(env, TokenKind::TRbracket)?;
        let key = expression::object::Key::Computed(ComputedKey {
            loc: computed_loc.dupe(),
            expression: member_expr,
            comments: mk_comments_opt(Some(leading.into()), None),
        });
        computed_key_property(
            env,
            start_loc,
            static_,
            abstract_,
            override_,
            variance,
            ts_accessibility,
            computed_loc,
            key,
            is_class,
        )
    }

    fn bracket_property(
        env: &mut ParserEnv,
        abstract_: bool,
        override_: bool,
        start_loc: Loc,
        static_: Option<Loc>,
        variance: Option<Variance<Loc>>,
        variance_op: Option<types::object::MappedTypeVarianceOp>,
        ts_accessibility: Option<class::ts_accessibility::TSAccessibility<Loc>>,
        leading: Vec<Comment<Loc>>,
        is_class: bool,
    ) -> Result<types::object::Property<Loc, Loc>, Rollback> {
        let leading = [leading, peek::comments(env)].concat();
        expect::token(env, TokenKind::TLbracket)?;
        let ith_1 = peek::computed_type_property_continuation(env);
        match (peek::token(env), &ith_1) {
            (_, TokenKind::TIdentifier { raw, .. }) if raw == "in" && static_.is_none() => {
                mapped_type(env, start_loc, variance, variance_op, leading)
            }
            (_, TokenKind::TColon) => {
                if variance_op.is_some() {
                    error_unexpected_variance(env, variance.as_ref())?;
                }
                indexer_property(env, start_loc, static_, variance, leading)
            }
            (TokenKind::TIdentifier { raw, .. }, TokenKind::TPeriod) if raw == "Symbol" => {
                if variance_op.is_some() {
                    error_unexpected_variance(env, variance.as_ref())?;
                }
                well_known_symbol_property(
                    env,
                    start_loc,
                    static_,
                    abstract_,
                    override_,
                    variance,
                    ts_accessibility,
                    leading,
                    is_class,
                )
            }
            _ => {
                if variance_op.is_some() {
                    error_unexpected_variance(env, variance.as_ref())?;
                }
                if env.is_ts() {
                    computed_property(
                        env,
                        start_loc,
                        static_,
                        abstract_,
                        override_,
                        variance,
                        ts_accessibility,
                        leading,
                        is_class,
                    )
                } else {
                    // In a `.js`/lib position `[T]: V` is an indexer and
                    // `[expr](): T` is a computed method, told apart only by what
                    // follows `]`. Parse the bracket content once as a type, then
                    // dispatch on the next token, converting a literal or
                    // (possibly-qualified) reference key into its value-expression
                    // form for the method case. No backtracking is needed.
                    js_computed_or_indexer(
                        env,
                        start_loc,
                        static_,
                        abstract_,
                        override_,
                        variance,
                        ts_accessibility,
                        leading,
                        is_class,
                    )
                }
            }
        }
    }

    fn internal_slot(
        env: &mut ParserEnv,
        start_loc: Loc,
        static_: Option<Loc>,
        leading: Vec<Comment<Loc>>,
    ) -> Result<types::object::Property<Loc, Loc>, Rollback> {
        let (loc, mut slot) = with_loc(Some(start_loc.dupe()), env, |env| {
            let leading = [leading, peek::comments(env)].concat();
            expect::token(env, TokenKind::TLbracket)?;
            expect::token(env, TokenKind::TLbracket)?;
            let id = parser_common::identifier_name(env)?;
            expect::token(env, TokenKind::TRbracket)?;
            expect::token(env, TokenKind::TRbracket)?;

            let (optional, method, value, trailing) = match peek::token(env) {
                TokenKind::TLessThan | TokenKind::TLparen => {
                    let mut tparams = type_params(env)?;
                    comment_attachment::type_params_remove_trailing(env, tparams.as_mut());
                    let (fn_loc, func) = methodish(env, start_loc, tparams)?;
                    let value = types::Type::new(TypeInner::Function {
                        loc: fn_loc,
                        inner: Arc::new(func),
                    });
                    (false, true, value, Vec::new())
                }
                _ => {
                    let optional = eat::maybe(env, TokenKind::TPling)?;
                    let trailing = eat::trailing_comments(env);
                    expect::token(env, TokenKind::TColon)?;
                    let value = type_inner(env)?;
                    (optional, false, value, trailing)
                }
            };

            // Expects the T_ELLIPSIS has already been eaten
            Ok(types::object::InternalSlot {
                loc: LOC_NONE,
                id,
                value,
                optional,
                static_: static_.is_some(),
                method,
                comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            })
        })?;
        slot.loc = loc;

        Ok(types::object::Property::InternalSlot(slot))
    }

    // Port of spread_property from type_parser.ml (lines 1418-1430)
    fn spread_property(
        env: &mut ParserEnv,
        start_loc: Loc,
        leading: Vec<Comment<Loc>>,
    ) -> Result<types::object::Property<Loc, Loc>, Rollback> {
        let (loc, mut spread) = with_loc(Some(start_loc), env, |env| {
            Ok(types::object::SpreadProperty {
                loc: LOC_NONE,
                argument: type_inner(env)?,
                comments: mk_comments_opt(Some(leading.into()), None),
            })
        })?;
        spread.loc = loc;

        Ok(types::object::Property::SpreadProperty(spread))
    }

    fn semicolon(env: &mut ParserEnv, exact: bool) -> Result<(), Rollback> {
        match peek::token(env) {
            TokenKind::TComma | TokenKind::TSemicolon => {
                eat::token(env)?;
            }
            TokenKind::TRcurlybar if exact => {}
            TokenKind::TRcurly if !exact => {}
            _ => {
                if !((env.is_ts() || env.oxc_type_feature_parser_rules())
                    && peek::is_implicit_semicolon(env))
                {
                    expect::error(env, &TokenKind::TComma)?
                }
            }
        }
        Ok(())
    }

    fn error_unexpected_variance(
        env: &mut ParserEnv,
        variance: Option<&Variance<Loc>>,
    ) -> Result<(), Rollback> {
        if let Some(v) = variance {
            env.error_at(v.loc.dupe(), ParseError::UnexpectedVariance)?;
        }
        Ok(())
    }

    fn computed_key_property(
        env: &mut ParserEnv,
        start_loc: Loc,
        static_: Option<Loc>,
        abstract_: bool,
        override_: bool,
        variance: Option<Variance<Loc>>,
        ts_accessibility: Option<class::ts_accessibility::TSAccessibility<Loc>>,
        computed_loc: Loc,
        key: expression::object::Key<Loc, Loc>,
        is_class: bool,
    ) -> Result<types::object::Property<Loc, Loc>, Rollback> {
        match peek::token(env) {
            TokenKind::TLessThan | TokenKind::TLparen => {
                error_unexpected_variance(env, variance.as_ref())?;
                method_property(
                    env,
                    start_loc,
                    static_,
                    abstract_,
                    override_,
                    key,
                    false,
                    ts_accessibility,
                    Vec::new(),
                )
            }
            TokenKind::TPling => {
                if peek::class_optional_method_continuation_is_call_or_type_args(env) {
                    eat::token(env)?;
                    error_unexpected_variance(env, variance.as_ref())?;
                    method_property(
                        env,
                        start_loc,
                        static_,
                        abstract_,
                        override_,
                        key,
                        true,
                        ts_accessibility,
                        Vec::new(),
                    )
                } else {
                    init_property(
                        env,
                        start_loc,
                        variance,
                        static_,
                        None,
                        abstract_,
                        override_,
                        ts_accessibility,
                        Vec::new(),
                        computed_loc,
                        key,
                        is_class,
                    )
                }
            }
            _ => init_property(
                env,
                start_loc,
                variance,
                static_,
                None,
                abstract_,
                override_,
                ts_accessibility,
                Vec::new(),
                computed_loc,
                key,
                is_class,
            ),
        }
    }

    /// Whether the token after the just-consumed `]` begins a method: `(` or `<`
    /// type params, or an optional `?` followed by either.
    fn method_continuation(env: &mut ParserEnv) -> bool {
        match peek::token(env) {
            TokenKind::TLparen | TokenKind::TLessThan => true,
            TokenKind::TPling => peek::class_optional_method_continuation_is_call_or_type_args(env),
            _ => false,
        }
    }

    /// Parse a computed-key member `[expr]...` in a TypeScript type, interface,
    /// or `declare class` body. Unlike Flow's unnamed indexers, TypeScript index
    /// signatures have a `name: type` inside the brackets and are dispatched
    /// separately. The `[` has already been consumed.
    fn computed_property(
        env: &mut ParserEnv,
        start_loc: Loc,
        static_: Option<Loc>,
        abstract_: bool,
        override_: bool,
        variance: Option<Variance<Loc>>,
        ts_accessibility: Option<class::ts_accessibility::TSAccessibility<Loc>>,
        leading: Vec<Comment<Loc>>,
        is_class: bool,
    ) -> Result<types::object::Property<Loc, Loc>, Rollback> {
        eat::push_lex_mode(env, LexMode::Normal);
        let expr = expression_parser::assignment(env)?;
        eat::pop_lex_mode(env);
        let computed_loc = Loc::between(&start_loc, peek::loc(env));
        expect::token(env, TokenKind::TRbracket)?;
        let key = expression::object::Key::Computed(ComputedKey {
            loc: computed_loc.dupe(),
            expression: expr,
            comments: mk_comments_opt(Some(leading.into()), None),
        });
        computed_key_property(
            env,
            start_loc,
            static_,
            abstract_,
            override_,
            variance,
            ts_accessibility,
            computed_loc,
            key,
            is_class,
        )
    }

    /// Parse an unnamed bracketed member `[...]` in a `.js`/lib type, interface,
    /// or `declare class` body, disambiguating a computed method `[expr](): T`
    /// from an indexer `[T]: V` by the token after `]`. The `[` has already been
    /// consumed. The bracket content is parsed once as a type; for the method
    /// case a literal or (possibly-qualified) reference key is turned into its
    /// value-expression form, and any other key is an error.
    fn js_computed_or_indexer(
        env: &mut ParserEnv,
        start_loc: Loc,
        static_: Option<Loc>,
        abstract_: bool,
        override_: bool,
        variance: Option<Variance<Loc>>,
        ts_accessibility: Option<class::ts_accessibility::TSAccessibility<Loc>>,
        leading: Vec<Comment<Loc>>,
        is_class: bool,
    ) -> Result<types::object::Property<Loc, Loc>, Rollback> {
        let key_type = type_inner(env)?;
        let computed_loc = Loc::between(&start_loc, peek::loc(env));
        expect::token(env, TokenKind::TRbracket)?;
        if !method_continuation(env) {
            if env.oxc_type_feature_parser_rules() {
                env.error_at_without_rollback(
                    key_type.loc().dupe(),
                    ParseError::MissingIndexerKeyName,
                );
            }
            return finish_indexer(env, start_loc, static_, variance, leading, None, key_type);
        }
        let expression = match computed_key_expression_of_type(&key_type) {
            Some(expression) => expression,
            None => {
                env.error_at(
                    computed_loc.dupe(),
                    ParseError::Unexpected(
                        "computed key, expected a literal or a possibly-qualified reference"
                            .to_owned(),
                    ),
                )?;
                // Recover with a string-literal key rather than an identifier: a
                // literal key is a named property that needs no resolution, so the
                // parse error is not followed by a spurious cannot-resolve error.
                expression::Expression::new(expression::ExpressionInner::StringLiteral {
                    loc: computed_loc.dupe(),
                    inner: Arc::new(StringLiteral {
                        value: FlowSmolStr::new_inline(""),
                        raw: FlowSmolStr::new_inline("\"\""),
                        comments: None,
                    }),
                })
            }
        };
        let key = expression::object::Key::Computed(ComputedKey {
            loc: computed_loc.dupe(),
            expression,
            comments: mk_comments_opt(Some(leading.into()), None),
        });
        computed_key_property(
            env,
            start_loc,
            static_,
            abstract_,
            override_,
            variance,
            ts_accessibility,
            computed_loc,
            key,
            is_class,
        )
    }

    /// Turn a type parsed as a computed key into its value-level expression, or
    /// `None` if the type has no value form. A literal becomes a literal
    /// expression, and a primitive keyword or (possibly-qualified) reference
    /// becomes the matching identifier or member chain, so a bare `string` in
    /// key position reads as the value `string`, just as in a `.d.ts` body.
    fn computed_key_expression_of_type(
        t: &types::Type<Loc, Loc>,
    ) -> Option<expression::Expression<Loc, Loc>> {
        match t.deref() {
            TypeInner::StringLiteral { loc, literal } => Some(expression::Expression::new(
                expression::ExpressionInner::StringLiteral {
                    loc: loc.dupe(),
                    inner: Arc::new(literal.clone()),
                },
            )),
            // A negative numeric literal type carries its sign inside the single
            // literal node (`-1`), but the value AST spells it as unary minus over
            // a positive literal, matching how the expression parser reads `[-1]`.
            TypeInner::NumberLiteral { loc, literal } => {
                Some(match literal.raw.strip_prefix('-') {
                    Some(unsigned_raw) => negated_key_expression(
                        loc,
                        expression::ExpressionInner::NumberLiteral {
                            loc: loc.dupe(),
                            inner: Arc::new(NumberLiteral {
                                value: -literal.value,
                                raw: FlowSmolStr::new(unsigned_raw),
                                comments: literal.comments.dupe(),
                            }),
                        },
                    ),
                    None => {
                        expression::Expression::new(expression::ExpressionInner::NumberLiteral {
                            loc: loc.dupe(),
                            inner: Arc::new(literal.clone()),
                        })
                    }
                })
            }
            TypeInner::BigIntLiteral { loc, literal } => {
                Some(match literal.raw.strip_prefix('-') {
                    Some(unsigned_raw) => negated_key_expression(
                        loc,
                        expression::ExpressionInner::BigIntLiteral {
                            loc: loc.dupe(),
                            inner: Arc::new(BigIntLiteral {
                                value: literal.value.map(|v| -v),
                                raw: FlowSmolStr::new(unsigned_raw),
                                comments: literal.comments.dupe(),
                            }),
                        },
                    ),
                    None => {
                        expression::Expression::new(expression::ExpressionInner::BigIntLiteral {
                            loc: loc.dupe(),
                            inner: Arc::new(literal.clone()),
                        })
                    }
                })
            }
            TypeInner::BooleanLiteral { loc, literal } => Some(expression::Expression::new(
                expression::ExpressionInner::BooleanLiteral {
                    loc: loc.dupe(),
                    inner: Arc::new(literal.clone()),
                },
            )),
            // `null` is a literal, not a value identifier: match the expression
            // parser and a `.d.ts` body, which both read `[null]` as the null
            // literal rather than a reference to a name `null`.
            TypeInner::Null { loc, comments } => Some(expression::Expression::new(
                expression::ExpressionInner::NullLiteral {
                    loc: loc.dupe(),
                    inner: Arc::new(comments.clone()),
                },
            )),
            // `void` is a reserved word, not a value identifier, so it has no
            // value form. A `.d.ts` body likewise rejects `[void]`, since `void`
            // there is the unary operator with no operand.
            TypeInner::Void { .. } => None,
            TypeInner::Generic { inner, .. } if inner.targs.is_none() => {
                computed_key_expression_of_type_reference(&inner.id)
            }
            // A bare primitive keyword (`string`, `number`, ...) is a value
            // identifier in key position, reusing the same type-as-identifier
            // reparse as `.d.ts` computed keys and function-parameter labels.
            _ => type_annotation_as_identifier(t).map(|id| {
                expression::Expression::new(expression::ExpressionInner::Identifier {
                    loc: id.loc.dupe(),
                    inner: id,
                })
            }),
        }
    }

    /// Turn a (possibly-qualified) type reference into the matching value
    /// expression: an unqualified name becomes an identifier, a qualified name a
    /// member chain. An `import(...)`-rooted reference has no value form.
    fn computed_key_expression_of_type_reference(
        id: &types::generic::Identifier<Loc, Loc>,
    ) -> Option<expression::Expression<Loc, Loc>> {
        match id {
            types::generic::Identifier::Unqualified(id) => Some(expression::Expression::new(
                expression::ExpressionInner::Identifier {
                    loc: id.loc.dupe(),
                    inner: id.dupe(),
                },
            )),
            types::generic::Identifier::Qualified(q) => {
                let object = computed_key_expression_of_type_reference(&q.qualification)?;
                Some(expression::Expression::new(
                    expression::ExpressionInner::Member {
                        loc: q.loc.dupe(),
                        inner: Arc::new(expression::Member {
                            object,
                            property: expression::member::Property::PropertyIdentifier(q.id.dupe()),
                            comments: None,
                        }),
                    },
                ))
            }
            types::generic::Identifier::ImportTypeAnnot(_) => None,
        }
    }

    /// Wrap a positive-literal argument in a unary-minus expression, reproducing
    /// the `Unary(-, <literal>)` shape the expression parser gives a negative key.
    fn negated_key_expression(
        loc: &Loc,
        argument: expression::ExpressionInner<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        expression::Expression::new(expression::ExpressionInner::Unary {
            loc: loc.dupe(),
            inner: Arc::new(expression::Unary {
                operator: expression::UnaryOperator::Minus,
                argument: expression::Expression::new(argument),
                comments: None,
            }),
        })
    }

    fn error_unexpected_proto(env: &mut ParserEnv, proto: Option<&Loc>) -> Result<(), Rollback> {
        if let Some(loc) = proto {
            env.error_at(loc.dupe(), ParseError::UnexpectedProto)?;
        }
        Ok(())
    }

    fn error_invalid_property_name(
        env: &mut ParserEnv,
        is_class: bool,
        static_: &Option<Loc>,
        key: &expression::object::Key<Loc, Loc>,
    ) -> Result<(), Rollback> {
        let is_static = static_.is_some();
        if let expression::object::Key::Identifier(id) = key {
            let name = &id.name;
            if is_class && (name == "constructor" || (is_static && name == "prototype")) {
                env.error_at(
                    id.loc.dupe(),
                    ParseError::InvalidClassMemberName {
                        name: name.as_str().to_owned(),
                        static_: is_static,
                        method_: false,
                        private_: false,
                    },
                )?;
            }
        }
        Ok(())
    }

    fn properties(
        env: &mut ParserEnv,
        is_class: bool,
        allow_inexact: bool,
        allow_spread: bool,
        exact: bool,
        inexact: bool,
        internal: Vec<Comment<Loc>>,
    ) -> Result<
        (
            Vec<types::object::Property<Loc, Loc>>,
            bool,
            Vec<Comment<Loc>>,
        ),
        Rollback,
    > {
        // no `static ...A`
        assert!(!(is_class && allow_spread));
        // allow_inexact implies allow_spread
        assert!(!allow_inexact || allow_spread);
        let mut props = Vec::new();

        loop {
            let start_loc = peek::loc(env).dupe();
            match peek::token(env) {
                TokenKind::TEof => return Ok((props, inexact, internal)),
                TokenKind::TRcurlybar if exact => return Ok((props, inexact, internal)),
                TokenKind::TRcurly if !exact => return Ok((props, inexact, internal)),
                TokenKind::TEllipsis if allow_spread => {
                    let leading = peek::comments(env);
                    eat::token(env)?;
                    match peek::token(env) {
                        TokenKind::TComma
                        | TokenKind::TSemicolon
                        | TokenKind::TRcurly
                        | TokenKind::TRcurlybar => {
                            semicolon(env, exact)?;
                            match peek::token(env) {
                                TokenKind::TRcurly if allow_inexact => {
                                    return Ok((props, true, leading));
                                }
                                TokenKind::TRcurlybar => {
                                    env.error_at(start_loc, ParseError::InexactInsideExact)?;
                                    return Ok((props, inexact, internal));
                                }
                                _ => {
                                    env.error_at(
                                        start_loc,
                                        ParseError::UnexpectedExplicitInexactInObject,
                                    )?;
                                }
                            }
                        }
                        _ => {
                            let prop = spread_property(env, start_loc, leading)?;
                            semicolon(env, exact)?;
                            props.push(prop);
                        }
                    }
                }
                TokenKind::TEllipsis => {
                    // In this case, allow_spread is false, so we may assume allow_inexact is false
                    // based on our assertion at the top of this function.
                    // Thus, any T_ELLIPSIS here is not allowed.
                    eat::token(env)?;
                    match peek::token(env) {
                        TokenKind::TComma
                        | TokenKind::TSemicolon
                        | TokenKind::TRcurly
                        | TokenKind::TRcurlybar => {
                            env.error_at(start_loc, ParseError::InexactInsideNonObject)?;
                            semicolon(env, exact)?;
                        }
                        _ => {
                            let errors_to_add = peek::errors(env);
                            env.add_errors(errors_to_add)?;
                            env.error_at(start_loc, ParseError::UnexpectedSpreadType)?;
                            // It's likely the user is trying to spread something here, so we can
                            // eat what they try to spread to try to continue parsing the remaining
                            // properties.
                            eat::token(env)?;
                            semicolon(env, exact)?;
                        }
                    }
                }
                _ => {
                    let prop = property(
                        env,
                        start_loc,
                        is_class,
                        is_class,
                        is_class,
                        is_class,
                        is_class,
                        None,
                        None,
                        None,
                        None,
                        false,
                        false,
                        None,
                        Vec::new(),
                    )?;
                    if env.oxc_type_feature_parser_rules() && peek::token(env) == &TokenKind::TComma
                    {
                        let comma_loc = peek::loc(env).dupe();
                        if is_class {
                            env.error_at_without_rollback(
                                comma_loc,
                                ParseError::UnexpectedCommaBetweenClassMembers,
                            );
                        } else if matches!(prop, types::object::Property::MappedType(_)) {
                            env.error_at_without_rollback(
                                comma_loc,
                                ParseError::UnexpectedCommaAfterMappedType,
                            );
                        }
                    }
                    semicolon(env, exact)?;
                    props.push(prop);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn property(
        env: &mut ParserEnv,
        start_loc: Loc,
        is_class: bool,
        mut allow_static: bool,
        mut allow_proto: bool,
        mut allow_abstract: bool,
        mut allow_accessibility: bool,
        mut variance: Option<Variance<Loc>>,
        variance_op: Option<types::object::MappedTypeVarianceOp>,
        mut static_: Option<Loc>,
        mut proto: Option<Loc>,
        mut abstract_: bool,
        mut override_: bool,
        mut ts_accessibility: Option<class::ts_accessibility::TSAccessibility<Loc>>,
        mut leading: Vec<Comment<Loc>>,
    ) -> Result<types::object::Property<Loc, Loc>, Rollback> {
        loop {
            if is_class
                && !override_
                && matches!(peek::token(env), TokenKind::TIdentifier { raw, .. } if raw == "override")
            {
                if object_key_after_current_in_normal_mode(env, is_class) {
                    leading = [leading, peek::comments(env)].concat();
                    eat::token(env)?;
                    allow_static = false;
                    allow_proto = false;
                    allow_accessibility = false;
                    override_ = true;
                    continue;
                } else {
                    break;
                }
            }
            if allow_abstract
                && matches!(peek::token(env), TokenKind::TIdentifier { raw, .. } if raw == "abstract")
            {
                if object_key_after_current_in_normal_mode(env, is_class) {
                    if static_.is_some() {
                        env.error(ParseError::StaticAbstractMethod)?;
                    }
                    leading = [leading, peek::comments(env)].concat();
                    eat::token(env)?;
                    allow_static = false;
                    allow_proto = false;
                    allow_abstract = false;
                    allow_accessibility = false;
                    abstract_ = true;
                    continue;
                } else {
                    break;
                }
            }
            match peek::token(env) {
                TokenKind::TPlus | TokenKind::TMinus if variance.is_none() => {
                    variance = maybe_variance(env, false, false)?;
                    allow_static = false;
                    allow_proto = false;
                    allow_abstract = false;
                    allow_accessibility = false;
                }
                TokenKind::TStatic if allow_static => {
                    assert!(variance.is_none()); // if we parsed variance, allow_static = false
                    static_ = Some(peek::loc(env).dupe());
                    leading = [leading, peek::comments(env)].concat();
                    eat::token(env)?;
                    allow_static = false;
                    allow_proto = false;
                    allow_accessibility = false;
                }
                TokenKind::TIdentifier { raw, .. } if raw == "proto" && allow_proto => {
                    assert!(variance.is_none()); // if we parsed variance, allow_proto = false
                    proto = Some(peek::loc(env).dupe());
                    leading = [leading, peek::comments(env)].concat();
                    eat::token(env)?;
                    allow_static = false;
                    allow_proto = false;
                    allow_accessibility = false;
                }
                TokenKind::TIdentifier { raw, .. }
                    if (raw == "private" || raw == "protected" || raw == "public")
                        && allow_accessibility =>
                {
                    let kind = match raw.as_str() {
                        "private" => class::ts_accessibility::Kind::Private,
                        "protected" => class::ts_accessibility::Kind::Protected,
                        "public" => class::ts_accessibility::Kind::Public,
                        _ => unreachable!("Must be one of the above"),
                    };
                    if object_key_after_current_in_normal_mode(env, is_class)
                        || peek::token_after_current_is_static(env)
                    {
                        let acc_loc = peek::loc(env).dupe();
                        leading = [leading, peek::comments(env)].concat();
                        eat::token(env)?;
                        ts_accessibility = Some(class::ts_accessibility::TSAccessibility {
                            loc: acc_loc,
                            kind,
                            comments: None,
                        });
                        allow_static = is_class;
                        allow_proto = false;
                        allow_accessibility = false;
                    } else {
                        break;
                    }
                }
                TokenKind::TReadonly | TokenKind::TWriteonly => {
                    let variance_allows = match &variance {
                        None => true,
                        Some(v) => {
                            matches!(v.kind, VarianceKind::Plus | VarianceKind::Minus)
                        }
                    };
                    if variance_allows && object_key_after_current_in_normal_mode(env, is_class) {
                        let variance_op = match &variance {
                            Some(v) if v.kind == VarianceKind::Plus => {
                                Some(types::object::MappedTypeVarianceOp::Add)
                            }
                            Some(v) if v.kind == VarianceKind::Minus => {
                                Some(types::object::MappedTypeVarianceOp::Remove)
                            }
                            _ => None,
                        };
                        variance = maybe_variance(env, true, false)?;
                        return property(
                            env,
                            start_loc,
                            is_class,
                            false,
                            false,
                            false,
                            false,
                            variance,
                            variance_op,
                            static_,
                            proto,
                            abstract_,
                            override_,
                            ts_accessibility,
                            leading,
                        );
                    } else {
                        break;
                    }
                }
                TokenKind::TPound if is_class => {
                    error_unexpected_proto(env, proto.as_ref())?;
                    error_unexpected_variance(env, variance.as_ref())?;
                    if static_.is_some() {
                        env.error(ParseError::UnexpectedStatic)?;
                    }
                    let leading = [leading, peek::comments(env)].concat();
                    let key = private_identifier(env)?;
                    let loc = Loc::between(&start_loc, &key.loc);
                    return Ok(types::object::Property::PrivateField(
                        types::object::PrivateField {
                            loc,
                            key,
                            comments: mk_comments_opt(Some(leading.into()), None),
                        },
                    ));
                }
                TokenKind::TLbracket => {
                    error_unexpected_proto(env, proto.as_ref())?;
                    return if peek::current_token_is_followed_by_lbracket(env) {
                        error_unexpected_variance(env, variance.as_ref())?;
                        internal_slot(env, start_loc, static_, leading)
                    } else {
                        bracket_property(
                            env,
                            abstract_,
                            override_,
                            start_loc,
                            static_,
                            variance,
                            variance_op,
                            ts_accessibility,
                            leading,
                            is_class,
                        )
                    };
                }
                TokenKind::TLessThan | TokenKind::TLparen => {
                    // Note that `static(): void` is a static callable property if we
                    // successfully parsed the static modifier above.
                    error_unexpected_proto(env, proto.as_ref())?;
                    error_unexpected_variance(env, variance.as_ref())?;
                    return call_property(env, start_loc, static_, leading);
                }
                _ => {
                    break;
                }
            }
        }
        if let (
            Some(static_loc),
            None,
            TokenKind::TPling | TokenKind::TColon | TokenKind::TAssign,
        ) = (&static_, &proto, peek::token(env))
        {
            // We speculatively parsed `static` as a static modifier, but now
            // we want to parse `static` as the key of a named property.
            let key = expression::object::Key::Identifier(Identifier::new(IdentifierInner {
                loc: static_loc.dupe(),
                name: FlowSmolStr::new_inline("static"),
                comments: mk_comments_opt(Some(leading.clone().into()), None),
            }));
            init_property(
                env,
                start_loc,
                variance,
                None,
                proto,
                false,
                false,
                None,
                Vec::new(),
                static_loc.dupe(),
                key,
                is_class,
            )
        } else if let (
            None,
            Some(proto_loc),
            TokenKind::TPling | TokenKind::TColon | TokenKind::TAssign,
        ) = (&static_, &proto, peek::token(env))
        {
            // We speculatively parsed `proto` as a proto modifier, but now
            // we want to parse `proto` as the key of a named property.
            let key = expression::object::Key::Identifier(Identifier::new(IdentifierInner {
                loc: proto_loc.dupe(),
                name: FlowSmolStr::new_inline("proto"),
                comments: mk_comments_opt(Some(leading.clone().into()), None),
            }));
            init_property(
                env,
                start_loc,
                variance,
                static_,
                None,
                false,
                false,
                None,
                Vec::new(),
                proto_loc.dupe(),
                key,
                is_class,
            )
        } else {
            fn object_key(
                env: &mut ParserEnv,
            ) -> Result<(Loc, expression::object::Key<Loc, Loc>), Rollback> {
                eat::push_lex_mode(env, LexMode::Normal);
                let key = object_parser::key(env, false)?;
                eat::pop_lex_mode(env);
                Ok(key)
            }

            let leading_key = peek::comments(env);
            match object_key(env)? {
                (key_loc, expression::object::Key::Identifier(id))
                    if id.name == "get" || id.name == "set" =>
                {
                    let name = id.name.dupe();
                    let mut key = expression::object::Key::Identifier(id);
                    match peek::token(env).clone() {
                        TokenKind::TLessThan | TokenKind::TLparen => {
                            error_unexpected_proto(env, proto.as_ref())?;
                            error_unexpected_variance(env, variance.as_ref())?;
                            method_property(
                                env,
                                start_loc,
                                static_,
                                abstract_,
                                override_,
                                key,
                                false,
                                ts_accessibility,
                                leading,
                            )
                        }
                        TokenKind::TPling
                            if peek::class_optional_method_continuation_is_call_or_type_args(
                                env,
                            ) =>
                        {
                            eat::token(env)?;
                            error_unexpected_proto(env, proto.as_ref())?;
                            error_unexpected_variance(env, variance.as_ref())?;
                            method_property(
                                env,
                                start_loc,
                                static_,
                                abstract_,
                                override_,
                                key,
                                true,
                                ts_accessibility,
                                leading,
                            )
                        }
                        TokenKind::TColon | TokenKind::TPling | TokenKind::TAssign => {
                            init_property(
                                env,
                                start_loc,
                                variance,
                                static_,
                                proto,
                                abstract_,
                                override_,
                                ts_accessibility,
                                leading,
                                key_loc,
                                key,
                                is_class,
                            )
                        }
                        _ => {
                            comment_attachment::object_key_remove_trailing(env, &mut key);
                            let key = object_key(env)?;
                            let is_getter = name == "get";
                            let leading = [leading, leading_key].concat();
                            error_unexpected_proto(env, proto.as_ref())?;
                            error_unexpected_variance(env, variance.as_ref())?;
                            getter_or_setter(
                                env,
                                is_getter,
                                leading,
                                override_,
                                ts_accessibility,
                                start_loc,
                                static_,
                                key,
                            )
                        }
                    }
                }
                (key_loc, key) => match peek::token(env).clone() {
                    TokenKind::TLessThan | TokenKind::TLparen => {
                        error_unexpected_proto(env, proto.as_ref())?;
                        error_unexpected_variance(env, variance.as_ref())?;
                        method_property(
                            env,
                            start_loc,
                            static_,
                            abstract_,
                            override_,
                            key,
                            false,
                            ts_accessibility,
                            leading,
                        )
                    }
                    TokenKind::TPling
                        if peek::class_optional_method_continuation_is_call_or_type_args(env) =>
                    {
                        eat::token(env)?;
                        error_unexpected_proto(env, proto.as_ref())?;
                        error_unexpected_variance(env, variance.as_ref())?;
                        method_property(
                            env,
                            start_loc,
                            static_,
                            abstract_,
                            override_,
                            key,
                            true,
                            ts_accessibility,
                            leading,
                        )
                    }
                    _ => {
                        error_invalid_property_name(env, is_class, &static_, &key)?;
                        init_property(
                            env,
                            start_loc,
                            variance,
                            static_,
                            proto,
                            abstract_,
                            override_,
                            ts_accessibility,
                            leading,
                            key_loc,
                            key,
                            is_class,
                        )
                    }
                },
            }
        }
    }

    // Main object_type implementation
    let exact = _allow_exact && peek::token(env) == &TokenKind::TLcurlybar;
    let allow_inexact = _allow_exact && !exact;

    with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(
            env,
            if exact {
                TokenKind::TLcurlybar
            } else {
                TokenKind::TLcurly
            },
        )?;

        let (properties, inexact, internal) = env.with_no_anon_function_type(false, |env| {
            properties(
                env,
                _is_class,
                allow_inexact,
                _allow_spread,
                exact,
                false,
                Vec::new(),
            )
        })?;

        let internal = [internal, peek::comments(env)].concat();
        expect::token(
            env,
            if exact {
                TokenKind::TRcurlybar
            } else {
                TokenKind::TRcurly
            },
        )?;
        let trailing = eat::trailing_comments(env);

        Ok(types::Object {
            exact,
            properties: properties.into(),
            inexact,
            comments: ast_utils::mk_comments_with_internal_opt(
                Some(leading.into()),
                Some(trailing.into()),
                if internal.is_empty() {
                    None
                } else {
                    Some(internal.into())
                },
            ),
        })
    })
}

fn interface_helper(
    env: &mut ParserEnv,
) -> Result<
    (
        Vec<(Loc, types::Generic<Loc, Loc>)>,
        (Loc, types::Object<Loc, Loc>),
    ),
    Rollback,
> {
    fn supers(env: &mut ParserEnv) -> Result<Vec<(Loc, types::Generic<Loc, Loc>)>, Rollback> {
        let mut supers = Vec::new();
        loop {
            supers.push(generic(env)?);
            if eat::maybe(env, TokenKind::TComma)? {
                continue;
            } else {
                return Ok(supers);
            }
        }
    }

    let extends = if eat::maybe(env, TokenKind::TExtends)? {
        let mut extends = supers(env)?;
        comment_attachment::generic_type_list_remove_trailing(env, &mut extends);
        extends
    } else {
        Vec::new()
    };

    let body = object_type(env, false, false, false)?;
    Ok((extends, body))
}

pub(super) fn type_identifier(env: &mut ParserEnv) -> Result<Identifier<Loc, Loc>, Rollback> {
    let id = parser_common::identifier_name(env)?;
    if is_reserved_type(&id.name) {
        env.error_at(id.loc.dupe(), ParseError::UnexpectedReservedType)?;
    }
    Ok(id)
}

// Port of bounded_type from type_parser.ml (lines 1768-1788)
fn bounded_type(
    env: &mut ParserEnv,
) -> Result<
    (
        Loc,
        Identifier<Loc, Loc>,
        types::AnnotationOrHint<Loc, Loc>,
        types::type_param::BoundKind,
    ),
    Rollback,
> {
    let (loc, (name, bound, bound_kind)) = with_loc(None, env, |env| {
        let name = type_identifier(env)?;
        let (bound, bound_kind) = match peek::token(env) {
            TokenKind::TColon => (
                types::AnnotationOrHint::Available(annotation(env)?),
                types::type_param::BoundKind::Colon,
            ),
            TokenKind::TExtends => {
                let (loc, bound_type) = with_loc(None, env, |env| {
                    eat::token(env)?;
                    type_inner(env)
                })?;
                (
                    types::AnnotationOrHint::Available(types::Annotation {
                        loc,
                        annotation: bound_type,
                    }),
                    types::type_param::BoundKind::Extends,
                )
            }
            _ => (
                types::AnnotationOrHint::Missing(peek::loc_skip_lookahead(env)),
                types::type_param::BoundKind::Colon,
            ),
        };
        Ok((name, bound, bound_kind))
    })?;
    Ok((loc, name, bound, bound_kind))
}

fn type_params(env: &mut ParserEnv) -> Result<Option<TypeParams<Loc, Loc>>, Rollback> {
    // whether we should consume [token] as a type param. a type param can
    // either start with an identifier or a variance sigil; we'll also parse
    // types like `number` to improve error recovery.
    fn token_is_maybe_param(env: &mut ParserEnv, token: &TokenKind) -> bool {
        token_is_type_identifier(env, token)
            || token_is_variance(token)
            || token_is_reserved_type(token)
    }

    // whether an unexpected [token] should signal the end of the param list.
    // these are tokens that are likely to follow a param list, if the closing
    // > is missing. This improves error recovery when you add type params
    // to an existing node.
    //
    // Note that we're in Lex_mode.TYPE here, so the tokens are those produced
    // by [Flow_lexer.type_token].
    fn token_is_maybe_end_of_list(env: &mut ParserEnv, token: &TokenKind) -> bool {
        match token {
            // Reserved words are lexed as identifiers in Lex_env.TYPE mode (if
            // they're not also reserved types). e.g. `switch` is a T_IDENTIFIER.
            // we're not expecting a type identifier, so let's assume it's a
            // NORMAL-mode keyword and end the list.
            TokenKind::TIdentifier { raw, .. }
                if is_reserved(raw) || is_contextually_reserved(raw) =>
            {
                true
            }
            // adding a type above an enum: `type T<U\nenum ....` (`enum` is not an ES keyword)
            TokenKind::TIdentifier { raw, .. } if raw == "enum" && env.parse_options().enums => {
                true
            }
            // adding a type above another: `type T<U\ntype V ...` (`type` is not an ES keyword)
            TokenKind::TIdentifier { raw, .. } if raw == "type" => true,
            // RHS: `type U<T = default = ...` (this only helps if there's a default!)
            TokenKind::TAssign => true,
            // start of function params: `function f<T|(...)`
            TokenKind::TLparen => true,
            // class heritage: `class C<T| implements ...`
            TokenKind::TIdentifier { raw, .. } if raw == "implements" => true,
            _ => false,
        }
    }

    fn params(
        env: &mut ParserEnv,
        require_default: bool,
    ) -> Result<(Vec<types::TypeParam<Loc, Loc>>, bool), Rollback> {
        let mut params = Vec::new();
        let mut require_default = require_default;

        loop {
            let token = peek::token(env).clone();
            if token_is_maybe_param(env, &token) {
                let (loc, (mut param, new_require_default)) = with_loc(None, env, |env| {
                    let const_ = maybe_const(env)?
                        .map(|(loc, comments)| types::type_param::ConstModifier { loc, comments });
                    let variance = maybe_variance(env, false, true)?;
                    let (bound_loc, name, bound, bound_kind) = bounded_type(env)?;

                    let (default, new_require_default) = match peek::token(env) {
                        TokenKind::TAssign => {
                            eat::token(env)?;
                            (Some(type_inner(env)?), true)
                        }
                        _ => {
                            if require_default {
                                env.error_at(bound_loc, ParseError::MissingTypeParamDefault)?;
                            }
                            (None, require_default)
                        }
                    };

                    Ok((
                        types::TypeParam {
                            loc: LOC_NONE,
                            name,
                            bound,
                            bound_kind,
                            variance,
                            default,
                            const_,
                        },
                        new_require_default,
                    ))
                })?;
                param.loc = loc;

                params.push(param);
                require_default = new_require_default;
            }

            match peek::token(env) {
                TokenKind::TEof | TokenKind::TGreaterThan => {
                    // end of list
                    return Ok((params, require_default));
                }
                TokenKind::TComma => {
                    // handle multiple params
                    eat::token(env)?;
                }
                token => {
                    let token = token.clone();
                    if token_is_maybe_end_of_list(env, &token) {
                        // error recovery: tokens likely to follow a param list
                        expect::error(env, &TokenKind::TGreaterThan)?;
                        return Ok((params, require_default));
                    } else if token_is_maybe_param(env, &token) {
                        expect::error(env, &TokenKind::TComma)?;
                    } else {
                        expect::token(env, TokenKind::TComma)?;
                    }
                }
            }
        }
    }

    if peek::token(env) == &TokenKind::TLessThan {
        if !env.should_parse_types() {
            env.error(ParseError::UnexpectedTypeAnnotation)?;
        }
        let (loc, mut tparams) = with_loc(None, env, |env| {
            let leading = peek::comments(env);
            expect::token(env, TokenKind::TLessThan)?;
            let (params_list, _) = params(env, false)?;
            let internal = peek::comments(env);
            expect::token_opt(env, TokenKind::TGreaterThan)?;
            let trailing = eat::trailing_comments(env);
            let comments = ast_utils::mk_comments_with_internal_opt(
                Some(leading.into()),
                Some(trailing.into()),
                Some(internal.into()),
            );
            Ok(TypeParams {
                loc: LOC_NONE,
                params: params_list.into(),
                comments,
            })
        })?;

        if tparams.params.is_empty() {
            env.error_at(loc.dupe(), ParseError::MissingTypeParam)?;
        }
        tparams.loc = loc;
        Ok(Some(tparams))
    } else {
        Ok(None)
    }
}

fn type_args(env: &mut ParserEnv) -> Result<Option<types::TypeArgs<Loc, Loc>>, Rollback> {
    fn args(env: &mut ParserEnv) -> Result<Vec<types::Type<Loc, Loc>>, Rollback> {
        let mut args = Vec::new();
        loop {
            match peek::token(env) {
                TokenKind::TEof | TokenKind::TGreaterThan => return Ok(args),
                _ => {
                    args.push(type_inner(env)?);
                    if peek::token(env) != &TokenKind::TGreaterThan {
                        expect::token(env, TokenKind::TComma)?;
                    }
                }
            }
        }
    }

    if peek::token(env) == &TokenKind::TLessThan {
        if !env.should_parse_types() {
            env.error(ParseError::UnexpectedTypeArguments)?;
        }
        let (loc, mut type_args) = with_loc(None, env, |env| {
            let leading = peek::comments(env);
            expect::token(env, TokenKind::TLessThan)?;
            let arguments = env.with_no_anon_function_type(false, |env| args(env))?;
            let internal = peek::comments(env);
            expect::token(env, TokenKind::TGreaterThan)?;
            let trailing = eat::trailing_comments(env);
            Ok(types::TypeArgs {
                loc: LOC_NONE,
                arguments: arguments.into(),
                comments: ast_utils::mk_comments_with_internal_opt(
                    Some(leading.into()),
                    Some(trailing.into()),
                    Some(internal.into()),
                ),
            })
        })?;
        type_args.loc = loc;
        Ok(Some(type_args))
    } else {
        Ok(None)
    }
}

pub fn generic(env: &mut ParserEnv) -> Result<(Loc, types::Generic<Loc, Loc>), Rollback> {
    let id = type_identifier(env)?;
    raw_generic_with_identifier(env, id)
}

fn raw_generic_with_identifier(
    env: &mut ParserEnv,
    id: Identifier<Loc, Loc>,
) -> Result<(Loc, types::Generic<Loc, Loc>), Rollback> {
    fn identifier_helper(
        env: &mut ParserEnv,
        mut qualification: (Loc, types::generic::Identifier<Loc, Loc>),
    ) -> Result<(Loc, types::generic::Identifier<Loc, Loc>), Rollback> {
        while peek::token(env) == &TokenKind::TPeriod
            && peek::current_token_is_followed_by_identifier_name(env)
        {
            let (loc, mut q) = with_loc(Some(qualification.0.dupe()), env, |env| {
                expect::token(env, TokenKind::TPeriod)?;
                let id = parser_common::identifier_name(env)?;
                Ok(types::generic::Qualified {
                    loc: LOC_NONE,
                    qualification: qualification.1,
                    id,
                })
            })?;
            q.loc = loc;
            qualification = (
                q.loc.dupe(),
                types::generic::Identifier::Qualified(Arc::new(q)),
            );
        }
        Ok(qualification)
    }

    with_loc(Some(id.loc.dupe()), env, |env| {
        let id = (id.loc.dupe(), types::generic::Identifier::Unqualified(id));
        let (_, id) = identifier_helper(env, id)?;

        let has_targs = type_reference_args_follow(env);
        let id = if !has_targs {
            id
        } else if let Some(mut remover) = comment_attachment::trailing_and_remover(env).remover {
            remover.map_generic_identifier_type(&id)
        } else {
            id
        };

        let targs = if has_targs { type_args(env)? } else { None };
        Ok(types::Generic {
            id,
            targs,
            comments: None,
        })
    })
}

/// Whether type arguments of a type reference follow. TypeScript only reads
/// them from the same line as the reference.
fn type_reference_args_follow(env: &mut ParserEnv) -> bool {
    peek::token(env) == &TokenKind::TLessThan
        && !(env.oxc_type_feature_parser_rules() && peek::is_line_terminator(env))
}

fn import_type_generic(env: &mut ParserEnv) -> Result<types::Type<Loc, Loc>, Rollback> {
    let (loc, mut inner) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        let start_loc = peek::loc(env).dupe();
        // consume 'import'
        eat::token(env)?;
        expect::token(env, TokenKind::TLparen)?;

        let source = match peek::token(env).clone() {
            TokenKind::TString(loc, value, raw, octal) => {
                if octal {
                    env.strict_error(ParseError::StrictOctalLiteral)?;
                }
                eat::token(env)?;
                (
                    loc.dupe(),
                    StringLiteral {
                        value,
                        raw,
                        comments: None,
                    },
                )
            }
            _ => {
                env.error_unexpected(Some("string literal".to_owned()))?;
                (
                    peek::loc(env).dupe(),
                    StringLiteral {
                        value: FlowSmolStr::new_inline(""),
                        raw: FlowSmolStr::new_inline("\"\""),
                        comments: None,
                    },
                )
            }
        };

        let rparen_loc = peek::loc(env).dupe();
        expect::token(env, TokenKind::TRparen)?;

        let import_loc = Loc::between(&start_loc, &rparen_loc);

        let initial =
            types::generic::Identifier::ImportTypeAnnot(Arc::new(types::generic::ImportType {
                loc: import_loc.dupe(),
                source,
                comments: mk_comments_opt(Some(leading.into()), None),
            }));

        let mut qualification = (import_loc.dupe(), initial);
        while peek::token(env) == &TokenKind::TPeriod
            && peek::current_token_is_followed_by_identifier_name(env)
        {
            let (loc, mut q) = with_loc(Some(qualification.0.dupe()), env, |env| {
                expect::token(env, TokenKind::TPeriod)?;
                let id = parser_common::identifier_name(env)?;
                Ok(types::generic::Qualified {
                    loc: LOC_NONE,
                    qualification: qualification.1,
                    id,
                })
            })?;
            q.loc = loc;
            qualification = (
                q.loc.dupe(),
                types::generic::Identifier::Qualified(Arc::new(q)),
            );
        }

        let (_, id) = qualification;
        let has_targs = type_reference_args_follow(env);
        let id = if !has_targs {
            id
        } else if let Some(mut remover) = comment_attachment::trailing_and_remover(env).remover {
            remover.map_generic_identifier_type(&id)
        } else {
            id
        };

        let targs = if has_targs { type_args(env)? } else { None };
        Ok(TypeInner::Generic {
            loc: LOC_NONE,
            inner: Arc::new(types::Generic {
                id,
                targs,
                comments: None,
            }),
        })
    })?;
    *inner.loc_mut() = loc;
    Ok(types::Type::new(inner))
}

fn function_return_annotation_opt(
    env: &mut ParserEnv,
) -> Result<function::ReturnAnnot<Loc, Loc>, Rollback> {
    match peek::token(env) {
        TokenKind::TColon => function_return_annotation(env),
        _ => Ok(function::ReturnAnnot::Missing(peek::loc_skip_lookahead(
            env,
        ))),
    }
}

fn annotation_opt(env: &mut ParserEnv) -> Result<types::AnnotationOrHint<Loc, Loc>, Rollback> {
    match peek::token(env) {
        TokenKind::TColon => Ok(types::AnnotationOrHint::Available(annotation(env)?)),
        _ => Ok(types::AnnotationOrHint::Missing(peek::loc_skip_lookahead(
            env,
        ))),
    }
}

fn renders_annotation_opt(
    env: &mut ParserEnv,
) -> Result<types::ComponentRendersAnnotation<Loc, Loc>, Rollback> {
    match peek::token(env) {
        TokenKind::TColon => {
            let operator_loc = peek::loc(env).dupe();
            if !env.should_parse_types() {
                env.error(ParseError::UnexpectedTypeAnnotation)?;
            }
            eat::token(env)?;
            let (loc, argument) = with_loc(None, env, type_inner)?;
            let has_nested_render = matches!(&*argument, TypeInner::Renders { .. });
            env.error_at(
                operator_loc.dupe(),
                ParseError::InvalidComponentRenderAnnotation { has_nested_render },
            )?;
            Ok(types::ComponentRendersAnnotation::AvailableRenders(
                loc,
                types::Renders {
                    operator_loc,
                    argument,
                    variant: types::RendersVariant::Normal,
                    comments: None,
                },
            ))
        }
        TokenKind::TIdentifier { raw, .. } if raw == "renders" => {
            if !env.should_parse_types() {
                env.error(ParseError::UnexpectedTypeAnnotation)?;
            }
            let start_loc = peek::loc(env).dupe();
            let (loc, renders) = with_loc(Some(start_loc), env, render_type)?;
            Ok(types::ComponentRendersAnnotation::AvailableRenders(
                loc, renders,
            ))
        }
        TokenKind::TRendersQuestion | TokenKind::TRendersStar => {
            if !env.should_parse_types() {
                env.error(ParseError::UnexpectedTypeAnnotation)?;
            }
            let start_loc = peek::loc(env).dupe();
            let (loc, renders) = with_loc(Some(start_loc), env, render_type)?;
            Ok(types::ComponentRendersAnnotation::AvailableRenders(
                loc, renders,
            ))
        }
        _ => Ok(types::ComponentRendersAnnotation::MissingRenders(
            peek::loc_skip_lookahead(env),
        )),
    }
}

fn add_comments(
    t: types::Type<Loc, Loc>,
    leading: Vec<Comment<Loc>>,
    trailing: Vec<Comment<Loc>>,
) -> types::Type<Loc, Loc> {
    let outer = mk_comments_opt(Some(leading.into()), Some(trailing.into()));

    match t.deref() {
        TypeInner::Any { loc, comments } => types::Type::new(TypeInner::Any {
            loc: loc.dupe(),
            comments: ast_utils::merge_comments(comments.dupe(), outer),
        }),
        TypeInner::Mixed { loc, comments } => types::Type::new(TypeInner::Mixed {
            loc: loc.dupe(),
            comments: ast_utils::merge_comments(comments.dupe(), outer),
        }),
        TypeInner::Empty { loc, comments } => types::Type::new(TypeInner::Empty {
            loc: loc.dupe(),
            comments: ast_utils::merge_comments(comments.dupe(), outer),
        }),
        TypeInner::Void { loc, comments } => types::Type::new(TypeInner::Void {
            loc: loc.dupe(),
            comments: ast_utils::merge_comments(comments.dupe(), outer),
        }),
        TypeInner::Null { loc, comments } => types::Type::new(TypeInner::Null {
            loc: loc.dupe(),
            comments: ast_utils::merge_comments(comments.dupe(), outer),
        }),
        TypeInner::Number { loc, comments } => types::Type::new(TypeInner::Number {
            loc: loc.dupe(),
            comments: ast_utils::merge_comments(comments.dupe(), outer),
        }),
        TypeInner::BigInt { loc, comments } => types::Type::new(TypeInner::BigInt {
            loc: loc.dupe(),
            comments: ast_utils::merge_comments(comments.dupe(), outer),
        }),
        TypeInner::String { loc, comments } => types::Type::new(TypeInner::String {
            loc: loc.dupe(),
            comments: ast_utils::merge_comments(comments.dupe(), outer),
        }),
        TypeInner::Boolean { loc, raw, comments } => types::Type::new(TypeInner::Boolean {
            loc: loc.dupe(),
            raw: raw.clone(),
            comments: ast_utils::merge_comments(comments.dupe(), outer),
        }),
        TypeInner::Symbol { loc, comments } => types::Type::new(TypeInner::Symbol {
            loc: loc.dupe(),
            comments: ast_utils::merge_comments(comments.dupe(), outer),
        }),
        TypeInner::Exists { loc, comments } => types::Type::new(TypeInner::Exists {
            loc: loc.dupe(),
            comments: ast_utils::merge_comments(comments.dupe(), outer),
        }),
        TypeInner::Unknown { loc, comments } => types::Type::new(TypeInner::Unknown {
            loc: loc.dupe(),
            comments: ast_utils::merge_comments(comments.dupe(), outer),
        }),
        TypeInner::Never { loc, comments } => types::Type::new(TypeInner::Never {
            loc: loc.dupe(),
            comments: ast_utils::merge_comments(comments.dupe(), outer),
        }),
        TypeInner::Undefined { loc, comments } => types::Type::new(TypeInner::Undefined {
            loc: loc.dupe(),
            comments: ast_utils::merge_comments(comments.dupe(), outer),
        }),
        TypeInner::UniqueSymbol {
            loc,
            symbol_loc,
            comments,
        } => types::Type::new(TypeInner::UniqueSymbol {
            loc: loc.dupe(),
            symbol_loc: symbol_loc.dupe(),
            comments: ast_utils::merge_comments(comments.dupe(), outer),
        }),
        TypeInner::Nullable { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = ast_utils::merge_comments(content.comments, outer);
            types::Type::new(TypeInner::Nullable {
                loc: loc.dupe(),
                inner: Arc::new(content),
            })
        }
        TypeInner::Function { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = ast_utils::merge_comments(content.comments, outer);
            types::Type::new(TypeInner::Function {
                loc: loc.dupe(),
                inner: Arc::new(content),
            })
        }
        TypeInner::Component { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = ast_utils::merge_comments(content.comments, outer);
            types::Type::new(TypeInner::Component {
                loc: loc.dupe(),
                inner: Arc::new(content),
            })
        }
        TypeInner::Object { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = ast_utils::merge_comments_with_internal(content.comments, outer);
            types::Type::new(TypeInner::Object {
                loc: loc.dupe(),
                inner: Arc::new(content),
            })
        }
        TypeInner::Interface { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = ast_utils::merge_comments(content.comments, outer);
            types::Type::new(TypeInner::Interface {
                loc: loc.dupe(),
                inner: Arc::new(content),
            })
        }
        TypeInner::Array { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = ast_utils::merge_comments(content.comments, outer);
            types::Type::new(TypeInner::Array {
                loc: loc.dupe(),
                inner: Arc::new(content),
            })
        }
        TypeInner::Conditional { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = ast_utils::merge_comments(content.comments, outer);
            types::Type::new(TypeInner::Conditional {
                loc: loc.dupe(),
                inner: Arc::new(content),
            })
        }
        TypeInner::Infer { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = ast_utils::merge_comments(content.comments, outer);
            types::Type::new(TypeInner::Infer {
                loc: loc.dupe(),
                inner: Arc::new(content),
            })
        }
        TypeInner::Generic { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = ast_utils::merge_comments(content.comments, outer);
            types::Type::new(TypeInner::Generic {
                loc: loc.dupe(),
                inner: Arc::new(content),
            })
        }
        TypeInner::IndexedAccess { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = ast_utils::merge_comments(content.comments, outer);
            types::Type::new(TypeInner::IndexedAccess {
                loc: loc.dupe(),
                inner: Arc::new(content),
            })
        }
        TypeInner::OptionalIndexedAccess { loc, inner } => {
            let mut content = (**inner).clone();
            content.indexed_access.comments =
                ast_utils::merge_comments(content.indexed_access.comments, outer);
            types::Type::new(TypeInner::OptionalIndexedAccess {
                loc: loc.dupe(),
                inner: Arc::new(content),
            })
        }
        TypeInner::Union { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = ast_utils::merge_comments(content.comments, outer);
            types::Type::new(TypeInner::Union {
                loc: loc.dupe(),
                inner: Arc::new(content),
            })
        }
        TypeInner::Intersection { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = ast_utils::merge_comments(content.comments, outer);
            types::Type::new(TypeInner::Intersection {
                loc: loc.dupe(),
                inner: Arc::new(content),
            })
        }
        TypeInner::Typeof { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = ast_utils::merge_comments(content.comments, outer);
            types::Type::new(TypeInner::Typeof {
                loc: loc.dupe(),
                inner: Arc::new(content),
            })
        }
        TypeInner::Keyof { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = ast_utils::merge_comments(content.comments, outer);
            types::Type::new(TypeInner::Keyof {
                loc: loc.dupe(),
                inner: Arc::new(content),
            })
        }
        TypeInner::Renders { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = ast_utils::merge_comments(content.comments, outer);
            types::Type::new(TypeInner::Renders {
                loc: loc.clone(),
                inner: Arc::new(content),
            })
        }
        TypeInner::ReadOnly { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = ast_utils::merge_comments(content.comments, outer);
            types::Type::new(TypeInner::ReadOnly {
                loc: loc.dupe(),
                inner: Arc::new(content),
            })
        }
        TypeInner::Tuple { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = ast_utils::merge_comments(content.comments, outer);
            types::Type::new(TypeInner::Tuple {
                loc: loc.dupe(),
                inner: Arc::new(content),
            })
        }
        TypeInner::StringLiteral { loc, literal } => {
            let mut literal = literal.clone();
            literal.comments = ast_utils::merge_comments(literal.comments, outer);
            types::Type::new(TypeInner::StringLiteral {
                loc: loc.dupe(),
                literal,
            })
        }
        TypeInner::NumberLiteral { loc, literal } => {
            let mut literal = literal.clone();
            literal.comments = ast_utils::merge_comments(literal.comments, outer);
            types::Type::new(TypeInner::NumberLiteral {
                loc: loc.dupe(),
                literal,
            })
        }
        TypeInner::BigIntLiteral { loc, literal } => {
            let mut literal = literal.clone();
            literal.comments = ast_utils::merge_comments(literal.comments, outer);
            types::Type::new(TypeInner::BigIntLiteral {
                loc: loc.dupe(),
                literal,
            })
        }
        TypeInner::BooleanLiteral { loc, literal } => {
            let mut literal = literal.clone();
            literal.comments = ast_utils::merge_comments(literal.comments, outer);
            types::Type::new(TypeInner::BooleanLiteral {
                loc: loc.dupe(),
                literal,
            })
        }
        TypeInner::TemplateLiteral { loc, inner } => {
            let inner = inner.as_ref();
            types::Type::new(TypeInner::TemplateLiteral {
                loc: loc.dupe(),
                inner: Arc::new(types::TypeTemplateLiteral {
                    quasis: inner.quasis.dupe(),
                    types: inner.types.dupe(),
                    comments: ast_utils::merge_comments(inner.comments.dupe(), outer),
                }),
            })
        }
        TypeInner::ConstructorType {
            loc,
            abstract_,
            inner,
        } => {
            let mut func = (**inner).clone();
            func.comments = ast_utils::merge_comments(func.comments, outer);
            types::Type::new(TypeInner::ConstructorType {
                loc: loc.dupe(),
                abstract_: *abstract_,
                inner: Arc::new(func),
            })
        }
    }
}

fn predicate_checks_contents(
    env: &mut ParserEnv,
    leading: Vec<Comment<Loc>>,
) -> Result<types::Predicate<Loc, Loc>, Rollback> {
    if peek::token(env) == &TokenKind::TLparen {
        let leading = [leading, peek::comments(env)].concat();
        expect::token(env, TokenKind::TLparen)?;
        eat::push_lex_mode(env, LexMode::Normal);
        let exp = expression_parser::conditional(env)?;
        eat::pop_lex_mode(env);
        expect::token(env, TokenKind::TRparen)?;
        let trailing = eat::trailing_comments(env);
        Ok(types::Predicate {
            loc: LOC_NONE,
            kind: types::PredicateKind::Declared(Arc::new(exp)),
            comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        })
    } else {
        let trailing = eat::trailing_comments(env);
        Ok(types::Predicate {
            loc: LOC_NONE,
            kind: types::PredicateKind::Inferred,
            comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        })
    }
}

fn predicate(env: &mut ParserEnv) -> Result<types::Predicate<Loc, Loc>, Rollback> {
    let (loc, mut predicate) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TChecks)?;
        predicate_checks_contents(env, leading)
    })?;
    predicate.loc = loc;
    Ok(predicate)
}

fn predicate_opt(env: &mut ParserEnv) -> Result<Option<types::Predicate<Loc, Loc>>, Rollback> {
    env.with_no_anon_function_type(false, |env| {
        Ok(match peek::token(env) {
            TokenKind::TChecks => Some(predicate(env)?),
            _ => None,
        })
    })
}

fn no_annot_predicate(
    env: &mut ParserEnv,
    start_loc: Loc,
) -> Result<types::Predicate<Loc, Loc>, Rollback> {
    env.with_no_anon_function_type(false, |env| {
        let (loc, mut predicate) = with_loc(Some(start_loc), env, |env| {
            let leading = peek::comments(env);
            expect::token(env, TokenKind::TChecks)?;
            predicate_checks_contents(env, leading)
        })?;
        predicate.loc = loc;
        Ok(predicate)
    })
}

fn function_return_annotation_and_predicate(
    env: &mut ParserEnv,
) -> Result<
    (
        function::ReturnAnnot<Loc, Loc>,
        Option<types::Predicate<Loc, Loc>>,
    ),
    Rollback,
> {
    if !env.should_parse_types() {
        env.error(ParseError::UnexpectedTypeAnnotation)?;
    }
    let missing_loc = peek::loc_skip_lookahead(env);
    let start_loc = peek::loc(env).dupe();
    expect::token(env, TokenKind::TColon)?;
    match peek::token(env) {
        TokenKind::TChecks => {
            let predicate = no_annot_predicate(env, start_loc)?;
            Ok((function::ReturnAnnot::Missing(missing_loc), Some(predicate)))
        }
        _ => {
            if is_start_of_type_guard(env) {
                Ok((
                    function::ReturnAnnot::TypeGuard(type_guard_annotation(env, start_loc)?),
                    None,
                ))
            } else {
                let (loc, t) = with_loc(Some(start_loc), env, type_inner)?;
                let mut annotation =
                    function::ReturnAnnot::Available(types::Annotation { loc, annotation: t });
                if peek::token(env) == &TokenKind::TChecks {
                    comment_attachment::return_annotation_remove_trailing(env, &mut annotation);
                }
                let predicate = predicate_opt(env)?;
                Ok((annotation, predicate))
            }
        }
    }
}

fn function_return_annotation_and_predicate_opt(
    env: &mut ParserEnv,
) -> Result<
    (
        function::ReturnAnnot<Loc, Loc>,
        Option<types::Predicate<Loc, Loc>>,
    ),
    Rollback,
> {
    match peek::token(env) {
        TokenKind::TColon => function_return_annotation_and_predicate(env),
        _ => Ok((
            function::ReturnAnnot::Missing(peek::loc_skip_lookahead(env)),
            None,
        )),
    }
}

fn wrap<T, F: FnOnce(&mut ParserEnv) -> Result<T, Rollback>>(
    env: &mut ParserEnv,
    f: F,
) -> Result<T, Rollback> {
    env.with_strict(true, |env| {
        eat::push_lex_mode(env, LexMode::Type);
        let result = f(env)?;
        eat::pop_lex_mode(env);
        Ok(result)
    })
}

pub(super) fn parse_type(env: &mut ParserEnv) -> Result<types::Type<Loc, Loc>, Rollback> {
    wrap(env, type_inner)
}

pub(super) fn parse_type_params(
    env: &mut ParserEnv,
) -> Result<Option<types::TypeParams<Loc, Loc>>, Rollback> {
    wrap(env, type_params)
}

pub(super) fn parse_type_args(
    env: &mut ParserEnv,
) -> Result<Option<types::TypeArgs<Loc, Loc>>, Rollback> {
    wrap(env, type_args)
}

pub(super) fn parse_object_type(
    env: &mut ParserEnv,
    is_class: bool,
) -> Result<(Loc, types::Object<Loc, Loc>), Rollback> {
    wrap(env, |env| object_type(env, is_class, false, false))
}

pub(super) fn parse_interface_helper(
    env: &mut ParserEnv,
) -> Result<
    (
        Vec<(Loc, types::Generic<Loc, Loc>)>,
        (Loc, types::Object<Loc, Loc>),
    ),
    Rollback,
> {
    wrap(env, interface_helper)
}

pub(super) fn parse_function_param_list(
    env: &mut ParserEnv,
) -> Result<types::function::Params<Loc, Loc>, Rollback> {
    wrap(env, function_param_list)
}

pub(super) fn parse_annotation(
    env: &mut ParserEnv,
) -> Result<types::Annotation<Loc, Loc>, Rollback> {
    wrap(env, annotation)
}

pub(super) fn parse_annotation_opt(
    env: &mut ParserEnv,
) -> Result<types::AnnotationOrHint<Loc, Loc>, Rollback> {
    wrap(env, annotation_opt)
}

pub(super) fn parse_function_return_annotation_opt(
    env: &mut ParserEnv,
) -> Result<function::ReturnAnnot<Loc, Loc>, Rollback> {
    wrap(env, function_return_annotation_opt)
}

pub(super) fn parse_predicate_opt(
    env: &mut ParserEnv,
) -> Result<Option<types::Predicate<Loc, Loc>>, Rollback> {
    wrap(env, predicate_opt)
}

pub(super) fn parse_function_return_annotation_and_predicate_opt(
    env: &mut ParserEnv,
) -> Result<
    (
        function::ReturnAnnot<Loc, Loc>,
        Option<types::Predicate<Loc, Loc>>,
    ),
    Rollback,
> {
    wrap(env, function_return_annotation_and_predicate_opt)
}

pub(super) fn parse_generic(
    env: &mut ParserEnv,
) -> Result<(Loc, types::Generic<Loc, Loc>), Rollback> {
    wrap(env, generic)
}

pub(super) fn parse_renders_annotation_opt(
    env: &mut ParserEnv,
) -> Result<types::ComponentRendersAnnotation<Loc, Loc>, Rollback> {
    wrap(env, renders_annotation_opt)
}

pub(super) fn parse_type_guard(
    env: &mut ParserEnv,
) -> Result<types::TypeGuard<Loc, Loc>, Rollback> {
    type_guard(env)
}
