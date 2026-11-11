/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// Stub functions based on DECLARATION module type from parser_common.ml
use std::collections::HashSet;
use std::sync::Arc;

use dupe::Dupe;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;

use crate::ast::*;
use crate::ast_utils;
use crate::comment_attachment;
use crate::enum_parser;
use crate::expression_parser;
use crate::loc::LOC_NONE;
use crate::loc::Loc;
use crate::main_parser;
use crate::parse_error::ParseError;
use crate::parser_common::assert_identifier_name_is_identifier;
use crate::parser_common::identifier_name;
use crate::parser_common::is_simple_parameter_list;
use crate::parser_common::with_loc;
use crate::parser_env::ParserEnv;
use crate::parser_env::eat;
use crate::parser_env::expect;
use crate::parser_env::is_restricted;
use crate::parser_env::is_strict_reserved;
use crate::parser_env::peek;
use crate::parser_env::try_parse::Rollback;
use crate::pattern_parser;
use crate::token::TokenKind;
use crate::type_parser;

fn check_param(
    env: &mut ParserEnv,
    param_names: &mut HashSet<FlowSmolStr>,
    pattern: &pattern::Pattern<Loc, Loc>,
) -> Result<(), Rollback> {
    fn pattern_check(
        env: &mut ParserEnv,
        param_names: &mut HashSet<FlowSmolStr>,
        pat: &pattern::Pattern<Loc, Loc>,
    ) -> Result<(), Rollback> {
        match pat {
            pattern::Pattern::Object { loc: _, inner } => object_check(env, param_names, inner),
            pattern::Pattern::Array { loc: _, inner } => array_check(env, param_names, inner),
            pattern::Pattern::Identifier { loc: _, inner } => {
                identifier_pattern_check(env, param_names, inner)
            }
            pattern::Pattern::Expression { loc, .. } => {
                env.error_at(loc.dupe(), ParseError::ExpectedPatternFoundExpression)?;
                Ok(())
            }
        }
    }

    fn object_check(
        env: &mut ParserEnv,
        param_names: &mut HashSet<FlowSmolStr>,
        obj: &pattern::Object<Loc, Loc>,
    ) -> Result<(), Rollback> {
        for prop in obj.properties.iter() {
            object_property_check(env, param_names, prop)?;
        }
        Ok(())
    }

    fn object_property_check(
        env: &mut ParserEnv,
        param_names: &mut HashSet<FlowSmolStr>,
        prop: &pattern::object::Property<Loc, Loc>,
    ) -> Result<(), Rollback> {
        match prop {
            pattern::object::Property::NormalProperty(property) => {
                pattern_check(env, param_names, &property.pattern)
            }
            pattern::object::Property::RestElement(rest) => {
                pattern_check(env, param_names, &rest.argument)
            }
        }
    }

    fn array_check(
        env: &mut ParserEnv,
        param_names: &mut HashSet<FlowSmolStr>,
        arr: &pattern::Array<Loc, Loc>,
    ) -> Result<(), Rollback> {
        for elem in arr.elements.iter() {
            array_element_check(env, param_names, elem)?;
        }
        Ok(())
    }

    fn array_element_check(
        env: &mut ParserEnv,
        param_names: &mut HashSet<FlowSmolStr>,
        elem: &pattern::array::Element<Loc, Loc>,
    ) -> Result<(), Rollback> {
        match elem {
            pattern::array::Element::Hole(_) => Ok(()),
            pattern::array::Element::NormalElement(element) => {
                pattern_check(env, param_names, &element.argument)
            }
            pattern::array::Element::RestElement(rest) => {
                pattern_check(env, param_names, &rest.argument)
            }
        }
    }

    fn identifier_pattern_check(
        env: &mut ParserEnv,
        param_names: &mut HashSet<FlowSmolStr>,
        id: &pattern::Identifier<Loc, Loc>,
    ) -> Result<(), Rollback> {
        identifier_check(env, param_names, &id.name)
    }

    fn identifier_check(
        env: &mut ParserEnv,
        param_names: &mut HashSet<FlowSmolStr>,
        id: &Identifier<Loc, Loc>,
    ) -> Result<(), Rollback> {
        let name = &id.name;
        let loc = id.loc.dupe();
        if param_names.contains(name) {
            env.error_at(loc.dupe(), ParseError::StrictParamDupe)?;
        }
        identifier_no_dupe_check(env, &loc, name)?;
        param_names.insert(name.dupe());
        Ok(())
    }

    fn identifier_no_dupe_check(
        env: &mut ParserEnv,
        loc: &Loc,
        name: &str,
    ) -> Result<(), Rollback> {
        if is_restricted(name) {
            env.strict_error_at((loc.dupe(), ParseError::StrictParamName))?;
        }
        if is_strict_reserved(name) {
            env.strict_error_at((loc.dupe(), ParseError::StrictReservedWord))?;
        }
        Ok(())
    }

    pattern_check(env, param_names, pattern)
}

/// Errors if there are any duplicate formal parameters
/// https://tc39.es/ecma262/#sec-parameter-lists-static-semantics-early-errors
pub(super) fn check_unique_formal_parameters(
    env: &mut ParserEnv,
    params: &function::Params<Loc, Loc>,
) -> Result<(), Rollback> {
    let mut param_names = HashSet::new();
    for param in params.params.iter() {
        match param {
            function::Param::RegularParam { argument, .. } => {
                check_param(env, &mut param_names, argument)?;
            }
            function::Param::ParamProperty { loc, property } => match &property.key {
                expression::object::Key::Identifier(name) => {
                    let id_loc = name.loc.dupe();
                    let synthetic_pattern = pattern::Pattern::Identifier {
                        loc: id_loc.dupe(),
                        inner: Arc::new(pattern::Identifier {
                            name: name.clone(),
                            annot: property.annot.clone(),
                            optional: false,
                        }),
                    };
                    check_param(env, &mut param_names, &synthetic_pattern)?;
                }
                _ => {
                    env.error_at(loc.dupe(), ParseError::ExpectedPatternFoundExpression)?;
                }
            },
        }
    }
    if let Some(ref rest) = params.rest {
        check_param(env, &mut param_names, &rest.argument)?;
    }
    Ok(())
}

/// This does the same check as check_unique_formal_parameters. However, it converts the component
/// params to a single object destructure, then runs the check. This is done to best match the behavior
/// of components still using function syntax.
pub(super) fn check_unique_component_formal_parameters(
    env: &mut ParserEnv,
    params: &statement::component_params::Params<Loc, Loc>,
) -> Result<(), Rollback> {
    let obj_pattern = pattern::Pattern::Object {
        loc: LOC_NONE,
        inner: Arc::new(pattern::Object {
            optional: false,
            properties: params
                .params
                .iter()
                .map(|param| {
                    let key = match &param.name {
                        statement::component_params::ParamName::StringLiteral((loc, lit)) => {
                            pattern::object::Key::StringLiteral((loc.dupe(), lit.clone()))
                        }
                        statement::component_params::ParamName::Identifier(id) => {
                            pattern::object::Key::Identifier(id.dupe())
                        }
                    };
                    pattern::object::Property::NormalProperty(pattern::object::NormalProperty {
                        loc: param.loc.dupe(),
                        key,
                        pattern: param.local.clone(),
                        default: param.default.clone(),
                        shorthand: param.shorthand,
                    })
                })
                .collect(),
            annot: types::AnnotationOrHint::Missing(LOC_NONE),
            comments: None,
        }),
    };
    let mut param_names = HashSet::new();
    check_param(env, &mut param_names, &obj_pattern)?;
    if let Some(ref rest) = params.rest {
        check_param(env, &mut param_names, &rest.argument)?;
    }

    Ok(())
}

enum ParamType<'a> {
    FunctionParams(&'a function::Params<Loc, Loc>),
    ComponentParams(&'a statement::component_params::Params<Loc, Loc>),
}

fn strict_post_check(
    env: &mut ParserEnv,
    contains_use_strict: bool,
    id: Option<&Identifier<Loc, Loc>>,
    params: ParamType,
) -> Result<(), Rollback> {
    let strict_mode = env.in_strict_mode();
    let simple = match params {
        ParamType::FunctionParams(p) => is_simple_parameter_list(p),
        ParamType::ComponentParams(_) => {
            // Component params are equivalent to an object destructure so not simple
            false
        }
    };
    // If we were already in strict mode and therefore already threw strict
    // errors, we want to do these checks outside of strict mode. If we
    // were in non-strict mode but the function contains "use strict", then
    // we want to do these checks in strict mode
    env.with_strict(!strict_mode && contains_use_strict, |env| {
        if contains_use_strict || strict_mode || !simple {
            if let Some(id) = id {
                let IdentifierInner {
                    loc,
                    name,
                    comments: _,
                } = &**id;
                if is_restricted(name) {
                    env.strict_error_at((loc.dupe(), ParseError::StrictFunctionName))?;
                }
                if is_strict_reserved(name) {
                    env.strict_error_at((loc.dupe(), ParseError::StrictReservedWord))?;
                }
            }
            match params {
                ParamType::FunctionParams(p) => check_unique_formal_parameters(env, p)?,
                ParamType::ComponentParams(p) => check_unique_component_formal_parameters(env, p)?,
            }
        }
        Ok(())
    })
}

pub(super) fn strict_function_post_check(
    env: &mut ParserEnv,
    contains_use_strict: bool,
    id: Option<&Identifier<Loc, Loc>>,
    params: &function::Params<Loc, Loc>,
) -> Result<(), Rollback> {
    strict_post_check(
        env,
        contains_use_strict,
        id,
        ParamType::FunctionParams(params),
    )
}

pub(super) fn strict_component_post_check(
    env: &mut ParserEnv,
    contains_use_strict: bool,
    id: &Identifier<Loc, Loc>,
    params: &statement::component_params::Params<Loc, Loc>,
) -> Result<(), Rollback> {
    strict_post_check(
        env,
        contains_use_strict,
        Some(id),
        ParamType::ComponentParams(params),
    )
}

fn rest_param(
    allow_optional: bool,
    env: &mut ParserEnv,
    t: &TokenKind,
) -> Result<Option<(Loc, pattern::Pattern<Loc, Loc>, Option<Syntax<Loc, ()>>)>, Rollback> {
    if *t == TokenKind::TEllipsis {
        let leading = peek::comments(env);
        let (loc, id) = with_loc(None, env, |env| {
            expect::token(env, TokenKind::TEllipsis)?;
            pattern_parser::pattern(env, allow_optional, ParseError::StrictParamName)
        })?;
        let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
        Ok(Some((loc, id, comments)))
    } else {
        Ok(None)
    }
}

pub(super) fn parse_function_params(
    env: &mut ParserEnv,
    await_: bool,
    yield_: bool,
) -> Result<function::Params<Loc, Loc>, Rollback> {
    fn param_property(
        env: &mut ParserEnv,
        ts_accessibility: Option<class::ts_accessibility::TSAccessibility<Loc>>,
    ) -> Result<class::Property<Loc, Loc>, Rollback> {
        if let Some(accessibility) = &ts_accessibility
            && !env.should_parse_types()
        {
            env.error_at(accessibility.loc.dupe(), ParseError::UnexpectedTypeModifier)?;
        }
        let variance = parse_variance(env, true, false, false)?;
        let leading = peek::comments(env);
        let (
            _,
            pattern::Identifier {
                name: name_id,
                annot,
                optional,
            },
        ) = main_parser::parse_identifier_with_type(env, true, None)?;
        let name_loc = name_id.loc.dupe();
        let key = expression::object::Key::Identifier(Identifier::new(IdentifierInner {
            loc: name_loc.dupe(),
            name: name_id.name.dupe(),
            comments: None,
        }));
        let value = if peek::token(env) == &TokenKind::TAssign {
            expect::token(env, TokenKind::TAssign)?;
            class::property::Value::Initialized(expression_parser::assignment(env)?)
        } else {
            class::property::Value::Uninitialized
        };
        let comments = ast_utils::mk_comments_opt(Some(leading.into()), None);
        Ok(class::Property {
            loc: name_loc,
            key,
            value,
            annot,
            static_: false,
            override_: false,
            optional,
            definite: false,
            variance,
            ts_accessibility,
            decorators: [].into(),
            comments,
        })
    }

    fn param(env: &mut ParserEnv) -> Result<function::Param<Loc, Loc>, Rollback> {
        let (loc, param) = with_loc(None, env, |env| {
            if peek::token(env) == &TokenKind::TThis {
                env.error(ParseError::ThisParamMustBeFirst)?;
            }

            // Check conditions before the match to avoid borrow checker issues
            let is_accessibility_with_ident =
                matches!(
                    peek::token(env),
                    TokenKind::TPublic | TokenKind::TPrivate | TokenKind::TProtected
                ) && peek::current_token_is_followed_by_identifier(env);

            let is_readonly_with_ident = matches!(
                peek::token(env),
                TokenKind::TIdentifier { raw, .. } if raw == "readonly"
            ) && peek::current_token_is_followed_by_identifier(env);

            let is_writeonly_with_ident = matches!(
                peek::token(env),
                TokenKind::TIdentifier { raw, .. } if raw == "writeonly"
            ) && peek::current_token_is_followed_by_identifier(env);

            if is_accessibility_with_ident {
                let (acc_loc, (kind, leading)) = with_loc(None, env, |env| {
                    let leading = peek::comments(env);
                    let kind = match peek::token(env) {
                        TokenKind::TPublic => class::ts_accessibility::Kind::Public,
                        TokenKind::TPrivate => class::ts_accessibility::Kind::Private,
                        TokenKind::TProtected => class::ts_accessibility::Kind::Protected,
                        _ => unreachable!("Must be one of the above"),
                    };
                    eat::token(env)?;
                    Ok((kind, leading))
                })?;
                let ts_accessibility = class::ts_accessibility::TSAccessibility {
                    loc: acc_loc,
                    kind,
                    comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                };
                let property = param_property(env, Some(ts_accessibility))?;
                Ok(function::Param::ParamProperty {
                    loc: LOC_NONE,
                    property,
                })
            } else if is_readonly_with_ident || is_writeonly_with_ident {
                let property = param_property(env, None)?;
                Ok(function::Param::ParamProperty {
                    loc: LOC_NONE,
                    property,
                })
            } else {
                let argument = pattern_parser::pattern(env, true, ParseError::StrictParamName)?;
                let default = if peek::token(env) == &TokenKind::TAssign {
                    expect::token(env, TokenKind::TAssign)?;
                    Some(expression_parser::assignment(env)?)
                } else {
                    None
                };
                Ok(function::Param::RegularParam {
                    loc: LOC_NONE,
                    argument,
                    default,
                })
            }
        })?;
        // Update the loc in the returned param
        Ok(match param {
            function::Param::RegularParam {
                loc: _,
                argument,
                default,
            } => function::Param::RegularParam {
                loc,
                argument,
                default,
            },
            function::Param::ParamProperty { loc: _, property } => {
                function::Param::ParamProperty { loc, property }
            }
        })
    }

    fn param_list(
        env: &mut ParserEnv,
    ) -> Result<
        (
            Vec<function::Param<Loc, Loc>>,
            Option<function::RestParam<Loc, Loc>>,
        ),
        Rollback,
    > {
        let mut params = Vec::new();
        loop {
            match peek::token(env) {
                TokenKind::TEof | TokenKind::TRparen | TokenKind::TEllipsis => {
                    let t = peek::token(env).clone();
                    let rest = rest_param(false, env, &t)?.map(|(loc, id, comments)| {
                        function::RestParam {
                            loc,
                            argument: id,
                            comments,
                        }
                    });
                    if peek::token(env) != &TokenKind::TRparen {
                        env.error(ParseError::ParameterAfterRestParameter)?;
                    }
                    return Ok((params, rest));
                }
                _ => {
                    let the_param = param(env)?;
                    if peek::token(env) != &TokenKind::TRparen {
                        expect::token(env, TokenKind::TComma)?;
                    }
                    params.push(the_param);
                }
            }
        }
    }

    fn this_param_annotation(
        env: &mut ParserEnv,
    ) -> Result<Option<function::ThisParam<Loc, Loc>>, Rollback> {
        if env.should_parse_types() && peek::token(env) == &TokenKind::TThis {
            let leading = peek::comments(env);
            let (this_loc, this_param) = with_loc(None, env, |env| {
                expect::token(env, TokenKind::TThis)?;
                if peek::token(env) != &TokenKind::TColon {
                    env.error(ParseError::ThisParamAnnotationRequired)?;
                    Ok(None)
                } else {
                    Ok(Some(type_parser::parse_annotation(env)?))
                }
            })?;
            match this_param {
                None => Ok(None),
                Some(annot) => {
                    if peek::token(env) == &TokenKind::TComma {
                        eat::token(env)?;
                    }
                    Ok(Some(function::ThisParam {
                        loc: this_loc,
                        annot,
                        comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                    }))
                }
            }
        } else {
            Ok(None)
        }
    }

    let (loc, (params, rest, comments, this_)) = with_loc(None, env, |env| {
        env.with_allow_await(await_, |env| {
            env.with_allow_yield(yield_, |env| {
                env.with_in_formal_parameters(true, |env| {
                    let leading = peek::comments(env);
                    expect::token(env, TokenKind::TLparen)?;
                    let this_ = this_param_annotation(env)?;
                    let (params, rest) = param_list(env)?;
                    let internal = peek::comments(env);
                    expect::token(env, TokenKind::TRparen)?;
                    let trailing = eat::trailing_comments(env);
                    Ok((
                        params,
                        rest,
                        ast_utils::mk_comments_with_internal_opt(
                            Some(leading.into()),
                            Some(trailing.into()),
                            Some(internal.into()),
                        ),
                        this_,
                    ))
                })
            })
        })
    })?;
    Ok(function::Params {
        loc,
        params: params.into(),
        rest,
        comments,
        this_,
    })
}

fn function_or_component_body(
    env: &mut ParserEnv,
    async_: bool,
    generator: bool,
    expression: bool,
    simple_params: bool,
) -> Result<((Loc, statement::Block<Loc, Loc>), bool), Rollback> {
    env.enter_function(async_, generator, simple_params, |env| {
        main_parser::parse_function_block_body(env, expression)
    })
}

pub(super) fn parse_function_body(
    env: &mut ParserEnv,
    async_: bool,
    generator: bool,
    expression: bool,
    simple_params: bool,
) -> Result<(function::Body<Loc, Loc>, bool), Rollback> {
    let (body_block, contains_use_strict) =
        function_or_component_body(env, async_, generator, expression, simple_params)?;
    Ok((function::Body::BodyBlock(body_block), contains_use_strict))
}

pub(super) fn parse_variance(
    env: &mut ParserEnv,
    parse_property_variance_keyword: bool,
    is_async: bool,
    is_generator: bool,
) -> Result<Option<Variance<Loc>>, Rollback> {
    let loc = peek::loc(env).dupe();
    let variance = match peek::token(env) {
        TokenKind::TPlus => {
            let leading = peek::comments(env);
            eat::token(env)?;
            Some(Variance {
                loc: loc.dupe(),
                kind: VarianceKind::Plus,
                comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
            })
        }
        TokenKind::TMinus => {
            let leading = peek::comments(env);
            eat::token(env)?;
            Some(Variance {
                loc: loc.dupe(),
                kind: VarianceKind::Minus,
                comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
            })
        }
        TokenKind::TIdentifier { raw, .. }
            if parse_property_variance_keyword && raw == "readonly" =>
        {
            let leading = peek::comments(env);
            eat::token(env)?;
            Some(Variance {
                loc: loc.dupe(),
                kind: VarianceKind::Readonly,
                comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
            })
        }
        TokenKind::TIdentifier { raw, .. }
            if parse_property_variance_keyword && raw == "writeonly" =>
        {
            let leading = peek::comments(env);
            eat::token(env)?;
            Some(Variance {
                loc: loc.dupe(),
                kind: VarianceKind::Writeonly,
                comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
            })
        }
        _ => None,
    };
    if let Some(variance) = &variance
        && !env.should_parse_types()
    {
        env.error_at(variance.loc.dupe(), ParseError::UnexpectedTypeModifier)?;
    }
    match variance {
        Some(ref v) if is_async || is_generator => {
            env.error_at(v.loc.dupe(), ParseError::UnexpectedVariance)?;
            Ok(None)
        }
        _ => Ok(variance),
    }
}

pub(super) fn parse_generator(env: &mut ParserEnv) -> Result<(bool, Vec<Comment<Loc>>), Rollback> {
    if peek::token(env) == &TokenKind::TMult {
        let leading = peek::comments(env);
        eat::token(env)?;
        Ok((true, leading))
    } else {
        Ok((false, vec![]))
    }
}

/// Returns true and consumes a token if the token is `async` and the token after it is on
/// the same line (see https://tc39.es/ecma262/#sec-async-function-definitions)
pub(super) fn parse_async(env: &mut ParserEnv) -> Result<(bool, Vec<Comment<Loc>>), Rollback> {
    if peek::async_token_continues_on_same_line(env) {
        let leading = peek::comments(env);
        eat::token(env)?;
        Ok((true, leading))
    } else {
        Ok((false, vec![]))
    }
}

/// Convert a Function.Param to Type.Function.Param for implicit declare function.
/// Returns Ok type_param or Err error_reason.
fn convert_function_param_to_type_param(
    param: &function::Param<Loc, Loc>,
) -> Result<types::function::Param<Loc, Loc>, &'static str> {
    match param {
        function::Param::RegularParam {
            loc,
            argument,
            default: _,
        } => match argument {
            pattern::Pattern::Identifier { inner, .. } => {
                let pattern::Identifier {
                    name,
                    annot,
                    optional,
                    ..
                } = inner.as_ref();
                match annot {
                    types::AnnotationOrHint::Available(types::Annotation {
                        annotation, ..
                    }) => Ok(types::function::Param {
                        loc: loc.dupe(),
                        param: types::function::ParamKind::Labeled {
                            name: name.clone(),
                            annot: annotation.clone(),
                            optional: *optional,
                        },
                    }),
                    types::AnnotationOrHint::Missing(_) => {
                        Err("parameter is missing a type annotation")
                    }
                }
            }
            pattern::Pattern::Object { inner, .. }
                if matches!(inner.annot, types::AnnotationOrHint::Available(_)) =>
            {
                Ok(types::function::Param {
                    loc: loc.dupe(),
                    param: types::function::ParamKind::Destructuring(argument.clone()),
                })
            }
            pattern::Pattern::Array { inner, .. }
                if matches!(inner.annot, types::AnnotationOrHint::Available(_)) =>
            {
                Ok(types::function::Param {
                    loc: loc.dupe(),
                    param: types::function::ParamKind::Destructuring(argument.clone()),
                })
            }
            pattern::Pattern::Object { .. } | pattern::Pattern::Array { .. } => {
                Err("destructuring parameter is missing a type annotation")
            }
            _ => Err("complex parameter patterns are not allowed"),
        },
        function::Param::ParamProperty { .. } => Err("parameter properties are not allowed"),
    }
}

/// Convert Function.Params to Type.Function.Params for implicit declare function.
/// Returns Ok type_params or Err if any conversion fails.
pub(super) fn convert_function_params_to_type_params(
    params: &function::Params<Loc, Loc>,
) -> Result<types::function::Params<Loc, Loc>, ()> {
    let mut converted = Vec::new();
    let mut has_error = false;

    for param in params.params.iter() {
        match convert_function_param_to_type_param(param) {
            Ok(p) => converted.push(p),
            Err(_) => has_error = true,
        }
    }

    // Convert this_ param
    let type_this = params
        .this_
        .as_ref()
        .map(|this_param| types::function::ThisParam {
            loc: this_param.loc.dupe(),
            annot: this_param.annot.clone(),
            comments: this_param.comments.dupe(),
        });

    // Convert rest param
    let (type_rest, rest_error) = match &params.rest {
        Some(rest_param) => {
            // Create a temporary Param to convert the rest argument
            let temp_param = function::Param::RegularParam {
                loc: rest_param.loc.dupe(),
                argument: rest_param.argument.clone(),
                default: None,
            };
            match convert_function_param_to_type_param(&temp_param) {
                Ok(type_param) => {
                    let rest = types::function::RestParam {
                        loc: rest_param.loc.dupe(),
                        argument: type_param,
                        comments: rest_param.comments.dupe(),
                    };
                    (Some(rest), false)
                }
                Err(_) => (None, true),
            }
        }
        None => (None, false),
    };

    if has_error || rest_error {
        Err(())
    } else {
        Ok(types::function::Params {
            loc: params.loc.dupe(),
            this: type_this,
            params: converted.into(),
            rest: type_rest,
            comments: params.comments.dupe(),
        })
    }
}

pub(super) fn parse_function(
    env: &mut ParserEnv,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, stmt) = with_loc(None, env, |env| {
        let (async_, leading_async) = parse_async(env)?;
        let (sig_loc, (generator, effect_, tparams, id, params, return_, predicate, leading)) =
            with_loc(None, env, |env| {
                let leading_function = peek::comments(env);
                let token = peek::token(env).clone();
                let (effect_, (generator, leading_generator)) = match token {
                    TokenKind::TFunction => {
                        eat::token(env)?;
                        let (is_generator, gen_comments) = parse_generator(env)?;
                        Ok((function::Effect::Arbitrary, (is_generator, gen_comments)))
                    }
                    TokenKind::TIdentifier { ref raw, .. } if raw == "hook" => {
                        eat::token(env)?;
                        Ok((function::Effect::Hook, (false, Vec::new())))
                    }
                    ref t => {
                        expect::error(env, t)?;
                        let (is_gen, gen_comments) = parse_generator(env)?;
                        Ok((function::Effect::Arbitrary, (is_gen, gen_comments)))
                    }
                }?;
                let leading = [leading_async, leading_function, leading_generator].concat();
                let in_default_export = env.in_export_default();
                let (tparams, id) = match (in_default_export, peek::token(env)) {
                    (true, TokenKind::TLparen) => (None, None),
                    (true, TokenKind::TLessThan) => {
                        let mut tparams = type_parser::parse_type_params(env)?;
                        comment_attachment::type_params_remove_trailing(env, tparams.as_mut());
                        let id = if peek::token(env) == &TokenKind::TLparen {
                            None
                        } else {
                            let mut id = main_parser::parse_identifier(
                                env,
                                Some(ParseError::StrictFunctionName),
                            )?;
                            comment_attachment::id_remove_trailing(env, &mut id);
                            Some(id)
                        };
                        (tparams, id)
                    }
                    _ => {
                        let id = if peek::is_identifier(env) {
                            let mut id = main_parser::parse_identifier(
                                env,
                                Some(ParseError::StrictFunctionName),
                            )?;
                            comment_attachment::id_remove_trailing(env, &mut id);
                            id
                        } else {
                            // don't consume the identifier here like Parse.identifier does.
                            env.error_nameless_declaration("function")?;
                            Identifier::new(IdentifierInner {
                                loc: peek::loc(env).dupe(),
                                name: FlowSmolStr::new_inline(""),
                                comments: None,
                            })
                        };
                        let mut tparams = type_parser::parse_type_params(env)?;
                        comment_attachment::type_params_remove_trailing(env, tparams.as_mut());
                        (tparams, Some(id))
                    }
                };
                let mut params = parse_function_params(env, async_, generator)?;
                if peek::token(env) != &TokenKind::TColon {
                    comment_attachment::function_params_remove_trailing(env, &mut params);
                }
                let (mut return_, mut predicate) =
                    type_parser::parse_function_return_annotation_and_predicate_opt(env)?;
                if predicate.is_none() {
                    comment_attachment::return_annotation_remove_trailing(env, &mut return_);
                } else {
                    comment_attachment::predicate_remove_trailing(env, predicate.as_mut());
                }
                Ok((
                    generator, effect_, tparams, id, params, return_, predicate, leading,
                ))
            })?;

        // We check all conversion conditions BEFORE consuming the semicolon. If conversion
        // would fail, we fall through to normal body parsing which will emit appropriate errors.
        // We also accept implicit semicolons (ASI).
        let implicit_declare_conversion = if (env.in_ambient_context() || env.is_ts())
            && peek::token(env) != &TokenKind::TLcurly
            && (peek::token(env) == &TokenKind::TSemicolon || peek::is_implicit_semicolon(env))
            && !async_
            && !generator
            && effect_ != function::Effect::Hook
            && !matches!(return_, function::ReturnAnnot::Missing(_))
        {
            convert_function_params_to_type_params(&params)
        } else {
            Err(())
        };

        match implicit_declare_conversion {
            Ok(type_params) => {
                // Consume the semicolon if explicit, or handle ASI trailing comments
                let trailing = if peek::token(env) == &TokenKind::TSemicolon {
                    eat::token(env)?;
                    if peek::is_line_terminator(env) {
                        eat::comments_until_next_line(env)
                    } else {
                        Vec::new()
                    }
                } else {
                    // Implicit semicolon (ASI): don't consume any token
                    eat::comments_until_next_line(env)
                };

                // Use tparams location for annot if present, otherwise params location
                let annot_loc = match &tparams {
                    Some(tp) => tp.loc.dupe(),
                    None => params.loc.dupe(),
                };
                let return_annot = match &return_ {
                    function::ReturnAnnot::Available(annot) => {
                        types::function::ReturnAnnotation::Available(annot.clone())
                    }
                    function::ReturnAnnot::TypeGuard(tg) => {
                        types::function::ReturnAnnotation::TypeGuard(tg.guard.clone())
                    }
                    function::ReturnAnnot::Missing(_) => {
                        // Should not happen - already checked above
                        types::function::ReturnAnnotation::Available(types::Annotation {
                            loc: LOC_NONE,
                            annotation: types::Type::new(types::TypeInner::Any {
                                loc: LOC_NONE,
                                comments: None,
                            }),
                        })
                    }
                };

                let fn_type = types::Function {
                    tparams,
                    params: type_params,
                    return_: return_annot,
                    comments: None,
                    effect: effect_,
                };

                let annot = types::Annotation {
                    loc: annot_loc.dupe(),
                    annotation: types::Type::new(types::TypeInner::Function {
                        loc: annot_loc.dupe(),
                        inner: Arc::new(fn_type),
                    }),
                };

                Ok(ImplicitDeclareResult::DeclareFunction(
                    statement::DeclareFunction {
                        id,
                        annot,
                        predicate,
                        comments: ast_utils::mk_comments_opt(
                            Some(leading.into()),
                            Some(trailing.into()),
                        ),
                        implicit_declare: true,
                    },
                ))
            }
            Err(()) => {
                // Normal function: parse body
                let simple_params = is_simple_parameter_list(&params);
                let (body, contains_use_strict) =
                    parse_function_body(env, async_, generator, false, simple_params)?;
                strict_function_post_check(env, contains_use_strict, id.as_ref(), &params)?;
                Ok(ImplicitDeclareResult::FunctionDeclaration(
                    function::Function {
                        id,
                        params,
                        body,
                        generator,
                        effect_,
                        async_,
                        predicate,
                        return_,
                        tparams,
                        sig_loc,
                        comments: ast_utils::mk_comments_opt(Some(leading.into()), None),
                    },
                ))
            }
        }
    })?;
    match stmt {
        ImplicitDeclareResult::DeclareFunction(decl) => Ok(statement::Statement::new(
            statement::StatementInner::DeclareFunction {
                loc,
                inner: Arc::new(decl),
            },
        )),
        ImplicitDeclareResult::FunctionDeclaration(func) => Ok(statement::Statement::new(
            statement::StatementInner::FunctionDeclaration {
                loc,
                inner: Arc::new(func),
            },
        )),
    }
}

/// Helper enum for parse_function to return either a DeclareFunction or FunctionDeclaration
enum ImplicitDeclareResult {
    DeclareFunction(statement::DeclareFunction<Loc, Loc>),
    FunctionDeclaration(function::Function<Loc, Loc>),
}

pub(super) fn variable_declaration_list(
    env: &mut ParserEnv,
) -> Result<
    (
        Vec<statement::variable::Declarator<Loc, Loc>>,
        Vec<(Loc, ParseError)>,
    ),
    Rollback,
> {
    fn variable_declaration(
        env: &mut ParserEnv,
    ) -> Result<
        (
            statement::variable::Declarator<Loc, Loc>,
            Option<(Loc, ParseError)>,
        ),
        Rollback,
    > {
        let (loc, (id, init, err)) = with_loc(None, env, |env| {
            let id = pattern_parser::pattern(env, false, ParseError::StrictVarName)?;
            let (init, err) = if eat::maybe(env, TokenKind::TAssign)? {
                (Some(expression_parser::assignment(env)?), None)
            } else {
                match id {
                    pattern::Pattern::Identifier { .. } => (None, None),
                    pattern::Pattern::Object { ref loc, .. }
                    | pattern::Pattern::Array { ref loc, .. }
                    | pattern::Pattern::Expression { ref loc, .. } => (
                        None,
                        Some((loc.dupe(), ParseError::NoUninitializedDestructuring)),
                    ),
                }
            };
            Ok((id, init, err))
        })?;
        Ok((
            statement::variable::Declarator {
                loc,
                id,
                init,
                definite: false,
            },
            err,
        ))
    }

    let mut decls = Vec::new();
    let mut errs = Vec::new();
    loop {
        let (decl, err) = variable_declaration(env)?;
        decls.push(decl);
        if let Some(x) = err {
            errs.push(x);
        }
        if !eat::maybe(env, TokenKind::TComma)? {
            return Ok((decls, errs));
        }
    }
}

fn declarations(
    token: TokenKind,
    env: &mut ParserEnv,
) -> Result<
    (
        Vec<statement::variable::Declarator<Loc, Loc>>,
        Vec<Comment<Loc>>,
        Vec<(Loc, ParseError)>,
    ),
    Rollback,
> {
    let leading = peek::comments(env);
    expect::token(env, token.clone())?;
    let (declarations, errs) = variable_declaration_list(env)?;
    Ok((declarations, leading, errs))
}

pub(super) fn parse_var(
    env: &mut ParserEnv,
) -> Result<
    (
        Vec<statement::variable::Declarator<Loc, Loc>>,
        Vec<Comment<Loc>>,
        Vec<(Loc, ParseError)>,
    ),
    Rollback,
> {
    declarations(TokenKind::TVar, env)
}

pub(super) fn parse_const(
    env: &mut ParserEnv,
) -> Result<
    (
        Vec<statement::variable::Declarator<Loc, Loc>>,
        Vec<Comment<Loc>>,
        Vec<(Loc, ParseError)>,
    ),
    Rollback,
> {
    let leading_comments = peek::comments(env);
    expect::token(env, TokenKind::TConst)?;
    parse_const_after_const(env, leading_comments)
}

pub(super) fn parse_const_after_const(
    env: &mut ParserEnv,
    leading_comments: Vec<Comment<Loc>>,
) -> Result<
    (
        Vec<statement::variable::Declarator<Loc, Loc>>,
        Vec<Comment<Loc>>,
        Vec<(Loc, ParseError)>,
    ),
    Rollback,
> {
    env.with_no_let(true, |env| {
        let (declarations, mut errs) = variable_declaration_list(env)?;
        // Make sure all consts defined are initialized, unless we're in an ambient context with type annotation
        for decl in &declarations {
            if decl.init.is_none() {
                // In ambient context, allow uninitialized const only if there's a type annotation
                if env.in_ambient_context()
                    && crate::ast_utils::pattern_has_type_annotation(&decl.id)
                {
                    // Allowed in ambient context with type annotation
                } else {
                    errs.push((decl.loc.dupe(), ParseError::NoUninitializedConst));
                }
            }
        }
        Ok((declarations, leading_comments, errs))
    })
}

pub(super) fn parse_let(
    env: &mut ParserEnv,
) -> Result<
    (
        Vec<statement::variable::Declarator<Loc, Loc>>,
        Vec<Comment<Loc>>,
        Vec<(Loc, ParseError)>,
    ),
    Rollback,
> {
    env.with_no_let(true, |env| declarations(TokenKind::TLet, env))
}

pub(super) fn parse_enum_declaration(
    env: &mut ParserEnv,
    start_loc: Option<Loc>,
    leading: Option<Vec<Comment<Loc>>>,
    const_: bool,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, decl) = with_loc(start_loc, env, |env| {
        enum_parser::declaration(leading.unwrap_or_default(), const_, env)
    })?;
    Ok(statement::Statement::new(
        statement::StatementInner::EnumDeclaration {
            loc,
            inner: Arc::new(decl),
        },
    ))
}

pub(super) fn parse_component_params(
    env: &mut ParserEnv,
) -> Result<statement::component_params::Params<Loc, Loc>, Rollback> {
    fn param(
        env: &mut ParserEnv,
    ) -> Result<statement::component_params::Param<Loc, Loc>, Rollback> {
        let (loc, param_inner) = with_loc(None, env, |env| {
            let leading = peek::comments(env);
            let (name, local, shorthand) = match peek::token(env).clone() {
                // "prop-key" as propKey
                TokenKind::TString(str_loc, value, str_raw, octal) => {
                    if octal {
                        env.strict_error(ParseError::StrictOctalLiteral)?;
                    }
                    eat::token(env)?;
                    let trailing = eat::trailing_comments(env);
                    let name = statement::component_params::ParamName::StringLiteral((
                        str_loc.dupe(),
                        StringLiteral {
                            value: value.dupe(),
                            raw: str_raw.dupe(),
                            comments: ast_utils::mk_comments_opt(
                                Some(leading.into()),
                                Some(trailing.into()),
                            ),
                        },
                    ));
                    match peek::token(env) {
                        TokenKind::TIdentifier { raw, .. } if raw == "as" => {
                            eat::token(env)?;
                            let local =
                                pattern_parser::pattern(env, true, ParseError::StrictParamName)?;
                            (name, local, false)
                        }
                        TokenKind::TColon | TokenKind::TPling => {
                            // This is an error probably due to someone learning component syntax.
                            // Let's make a good error message and supply a quick fix
                            let optional = peek::token(env) == &TokenKind::TPling;
                            env.error(ParseError::InvalidComponentStringParameterBinding {
                                optional,
                                name: value.as_str().to_owned(),
                            })?;
                            if optional {
                                eat::token(env)?;
                            }
                            let loc = peek::loc(env).dupe();
                            let fallback_ident = Identifier::new(IdentifierInner {
                                loc: loc.dupe(),
                                name: FlowSmolStr::new_inline(""),
                                comments: None,
                            });
                            let annot = type_parser::parse_annotation_opt(env)?;
                            let local = pattern::Pattern::Identifier {
                                loc,
                                inner: Arc::new(pattern::Identifier {
                                    name: fallback_ident,
                                    annot,
                                    optional,
                                }),
                            };
                            (name, local, false)
                        }
                        _ => {
                            env.error(ParseError::InvalidComponentStringParameterBinding {
                                optional: false,
                                name: value.as_str().to_owned(),
                            })?;
                            let loc = peek::loc(env).dupe();
                            let fallback_ident = Identifier::new(IdentifierInner {
                                loc: loc.dupe(),
                                name: FlowSmolStr::new_inline(""),
                                comments: None,
                            });
                            let annot = type_parser::parse_annotation_opt(env)?;
                            let local = pattern::Pattern::Identifier {
                                loc,
                                inner: Arc::new(pattern::Identifier {
                                    name: fallback_ident,
                                    annot,
                                    optional: false,
                                }),
                            };
                            (name, local, false)
                        }
                    }
                }
                TokenKind::TLcurly => {
                    env.error(ParseError::InvalidComponentParamName)?;
                    let fake_name_loc = peek::loc(env).dupe();
                    let fallback_ident = Identifier::new(IdentifierInner {
                        loc: fake_name_loc,
                        name: FlowSmolStr::new_inline(""),
                        comments: None,
                    });
                    let name = statement::component_params::ParamName::Identifier(fallback_ident);
                    let local = pattern_parser::pattern(env, false, ParseError::StrictParamName)?;
                    (name, local, false)
                }
                _ => {
                    let id = identifier_name(env)?;
                    let name = statement::component_params::ParamName::Identifier(id.dupe());
                    if matches!(peek::token(env), TokenKind::TIdentifier { raw, .. } if raw == "as")
                    {
                        expect::identifier(env, "as")?;
                        let local =
                            pattern_parser::pattern(env, true, ParseError::StrictParamName)?;
                        (name, local, false)
                    } else {
                        let (loc, id) = with_loc(Some(id.loc.dupe()), env, |env| {
                            assert_identifier_name_is_identifier(
                                Some(ParseError::StrictParamName),
                                env,
                                &id,
                            )?;
                            let optional = match peek::token(env) {
                                TokenKind::TPling => {
                                    if !env.should_parse_types() {
                                        env.error(ParseError::UnexpectedTypeAnnotation)?;
                                    }
                                    eat::token(env)?;
                                    true
                                }
                                _ => false,
                            };
                            let annot = type_parser::parse_annotation_opt(env)?;
                            Ok(pattern::Identifier {
                                name: id,
                                annot,
                                optional,
                            })
                        })?;
                        let local = pattern::Pattern::Identifier {
                            loc,
                            inner: Arc::new(id),
                        };
                        (name, local, true)
                    }
                }
            };

            let default = if peek::token(env) == &TokenKind::TAssign {
                expect::token(env, TokenKind::TAssign)?;
                Some(expression_parser::assignment(env)?)
            } else {
                None
            };
            Ok((name, local, default, shorthand))
        })?;
        Ok(statement::component_params::Param {
            loc,
            name: param_inner.0,
            local: param_inner.1,
            default: param_inner.2,
            shorthand: param_inner.3,
        })
    }

    fn param_list(
        env: &mut ParserEnv,
    ) -> Result<
        (
            Vec<statement::component_params::Param<Loc, Loc>>,
            Option<statement::component_params::RestParam<Loc, Loc>>,
        ),
        Rollback,
    > {
        let mut params = Vec::new();
        loop {
            match peek::token(env) {
                t @ (TokenKind::TEof | TokenKind::TRparen | TokenKind::TEllipsis) => {
                    let t = t.clone();
                    let rest_param_opt = rest_param(true, env, &t)?;
                    let rest = if let Some((loc, id, comments)) = rest_param_opt {
                        if peek::token(env) == &TokenKind::TComma {
                            eat::token(env)?;
                        }
                        Some(statement::component_params::RestParam {
                            loc,
                            argument: id,
                            comments,
                        })
                    } else {
                        None
                    };
                    if peek::token(env) != &TokenKind::TRparen {
                        env.error(ParseError::ParameterAfterRestParameter)?;
                    }
                    return Ok((params, rest));
                }
                _ => {
                    let the_param = param(env)?;
                    if peek::token(env) != &TokenKind::TRparen {
                        expect::token(env, TokenKind::TComma)?;
                    }
                    params.push(the_param);
                }
            }
        }
    }

    let (loc, (params, rest, comments)) = with_loc(None, env, |env| {
        env.with_in_formal_parameters(true, |env| {
            let leading = peek::comments(env);
            expect::token(env, TokenKind::TLparen)?;
            let (params, rest) = param_list(env)?;
            let internal = peek::comments(env);
            expect::token(env, TokenKind::TRparen)?;
            let trailing = eat::trailing_comments(env);
            Ok((
                params,
                rest,
                ast_utils::mk_comments_with_internal_opt(
                    Some(leading.into()),
                    Some(trailing.into()),
                    Some(internal.into()),
                ),
            ))
        })
    })?;
    Ok(statement::component_params::Params {
        loc,
        params: params.into(),
        rest,
        comments,
    })
}

fn parse_component_body(
    env: &mut ParserEnv,
    async_: bool,
) -> Result<((Loc, statement::Block<Loc, Loc>), bool), Rollback> {
    function_or_component_body(env, async_, false, false, false)
}

pub(super) fn parse_component(
    env: &mut ParserEnv,
) -> Result<statement::Statement<Loc, Loc>, Rollback> {
    let (loc, component) = with_loc(None, env, |env| {
        let (async_, leading_async) = parse_async(env)?;
        let (sig_loc, (tparams, id, params, renders, leading)) = with_loc(None, env, |env| {
            let mut leading = leading_async;
            leading.extend(peek::comments(env));
            expect::identifier(env, "component")?;
            // Components should have at least the same strictness as functions
            let mut id = main_parser::parse_identifier(env, Some(ParseError::StrictFunctionName))?;
            comment_attachment::id_remove_trailing(env, &mut id);
            let mut tparams = type_parser::parse_type_params(env)?;
            comment_attachment::type_params_remove_trailing(env, tparams.as_mut());
            let mut params = parse_component_params(env)?;
            if !peek::is_renders_ident(env) {
                comment_attachment::component_params_remove_trailing(env, &mut params);
            }
            let mut renders = type_parser::parse_renders_annotation_opt(env)?;
            comment_attachment::component_renders_annotation_remove_trailing(env, &mut renders);
            Ok((tparams, id, params, renders, leading))
        })?;
        let (body, contains_use_strict, trailing) = if peek::token(env) != &TokenKind::TLcurly {
            // No body - consume semicolon if present
            let trailing = if peek::token(env) == &TokenKind::TSemicolon {
                eat::token(env)?;
                if peek::is_line_terminator(env) {
                    eat::comments_until_next_line(env)
                } else {
                    vec![]
                }
            } else {
                vec![]
            };
            (None, false, trailing)
        } else {
            let (body, contains_use_strict) = parse_component_body(env, async_)?;
            (Some(body), contains_use_strict, vec![])
        };
        strict_component_post_check(env, contains_use_strict, &id, &params)?;
        Ok(statement::ComponentDeclaration {
            id,
            params,
            body,
            renders,
            tparams,
            sig_loc,
            async_,
            comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        })
    })?;
    Ok(statement::Statement::new(
        statement::StatementInner::ComponentDeclaration {
            loc,
            inner: Arc::new(component),
        },
    ))
}
