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

use crate::ast::expression::ExpressionInner;
use crate::ast::*;
use crate::ast_utils;
use crate::ast_utils::mk_comments_opt;
use crate::comment_attachment;
use crate::declaration_parser;
use crate::flow_lexer::bigint_strip_n;
use crate::jsx_parser;
use crate::loc::LOC_NONE;
use crate::loc::Loc;
use crate::main_parser;
use crate::match_pattern_parser;
use crate::object_parser;
use crate::parse_error::ParseError;
use crate::parser_common::*;
use crate::parser_env::try_parse::Rollback;
use crate::parser_env::*;
use crate::pattern_cover::PatternCover;
use crate::pattern_cover::PatternCoverErrors;
use crate::pattern_cover::as_expression;
use crate::pattern_cover::as_pattern;
use crate::token::BigintType;
use crate::token::NumberType;
use crate::token::TemplatePart;
use crate::token::TokenKind;
use crate::token::parse_binary_to_f64;
use crate::token::parse_hex_to_f64;
use crate::token::parse_octal_to_f64;
use crate::type_parser;

#[derive(Debug, Clone, Copy)]
enum OpPrecedence {
    LeftAssoc(i32),
    RightAssoc(i32),
}

enum GroupCover {
    GroupExpr(expression::Expression<Loc, Loc>),
    GroupTypecast(expression::TypeCast<Loc, Loc>),
}

fn is_tighter(a: OpPrecedence, b: OpPrecedence) -> bool {
    let a_prec = match a {
        OpPrecedence::LeftAssoc(x) => x,
        OpPrecedence::RightAssoc(x) => x - 1,
    };
    let b_prec = match b {
        OpPrecedence::LeftAssoc(x) => x,
        OpPrecedence::RightAssoc(x) => x,
    };
    a_prec >= b_prec
}

pub(super) fn is_assignable_lhs(expr: &expression::Expression<Loc, Loc>) -> bool {
    match expr.deref() {
        // new.target
        ExpressionInner::MetaProperty { inner, .. }
            if inner.meta.name == "new" && inner.property.name == "target" =>
        {
            false
        }
        // import.meta
        ExpressionInner::MetaProperty { inner, .. }
            if inner.meta.name == "import" && inner.property.name == "meta" =>
        {
            false
        }
        // #sec-static-semantics-static-semantics-isvalidsimpleassignmenttarget
        ExpressionInner::Array { .. }
        | ExpressionInner::Identifier { .. }
        | ExpressionInner::Member { .. }
        | ExpressionInner::MetaProperty { .. }
        | ExpressionInner::Object { .. } => true,
        ExpressionInner::Unary { inner, .. }
            if matches!(inner.operator, expression::UnaryOperator::Nonnull) =>
        {
            is_assignable_lhs(&inner.argument)
        }
        ExpressionInner::ArrowFunction { .. }
        | ExpressionInner::AsConstExpression { .. }
        | ExpressionInner::AsExpression { .. }
        | ExpressionInner::Assignment { .. }
        | ExpressionInner::Binary { .. }
        | ExpressionInner::Call { .. }
        | ExpressionInner::Class { .. }
        | ExpressionInner::Conditional { .. }
        | ExpressionInner::Function { .. }
        | ExpressionInner::Import { .. }
        | ExpressionInner::JSXElement { .. }
        | ExpressionInner::JSXFragment { .. }
        | ExpressionInner::StringLiteral { .. }
        | ExpressionInner::BooleanLiteral { .. }
        | ExpressionInner::NullLiteral { .. }
        | ExpressionInner::NumberLiteral { .. }
        | ExpressionInner::BigIntLiteral { .. }
        | ExpressionInner::RegExpLiteral { .. }
        | ExpressionInner::Match { .. }
        | ExpressionInner::ModuleRefLiteral { .. }
        | ExpressionInner::Logical { .. }
        | ExpressionInner::New { .. }
        | ExpressionInner::OptionalCall { .. }
        | ExpressionInner::OptionalMember { .. }
        | ExpressionInner::Record { .. }
        | ExpressionInner::Sequence { .. }
        | ExpressionInner::Super { .. }
        | ExpressionInner::TaggedTemplate { .. }
        | ExpressionInner::TemplateLiteral { .. }
        | ExpressionInner::This { .. }
        | ExpressionInner::TypeCast { .. }
        | ExpressionInner::TSSatisfies { .. }
        | ExpressionInner::Unary { .. }
        | ExpressionInner::Update { .. }
        | ExpressionInner::Yield { .. } => false,
    }
}

/*
AssignmentExpression :
  [+Yield] YieldExpression
  ConditionalExpression
  LeftHandSideExpression = AssignmentExpression
  LeftHandSideExpression AssignmentOperator AssignmentExpression
  ArrowFunctionFunction

  Originally we were parsing this without backtracking, but
  ArrowFunctionExpression got too tricky. Oh well.
*/
pub(super) fn assignment_cover(env: &mut ParserEnv) -> Result<PatternCover, Rollback> {
    fn assignment_but_not_arrow_function_cover(
        env: &mut ParserEnv,
    ) -> Result<PatternCover, Rollback> {
        let start_loc = peek::loc(env).dupe();
        let expr_or_pattern = conditional_cover(env)?;
        match assignment_op(env)? {
            Some(operator) => {
                let (loc, assignment) = with_loc(Some(start_loc), env, |env| {
                    let left = as_pattern(env, None, expr_or_pattern)?;
                    let right = assignment(env)?;
                    Ok(expression::Assignment {
                        operator,
                        left,
                        right,
                        comments: None,
                    })
                })?;
                Ok(PatternCover::CoverExpr(expression::Expression::new(
                    ExpressionInner::Assignment {
                        loc,
                        inner: Arc::new(assignment),
                    },
                )))
            }
            None => Ok(expr_or_pattern),
        }
    }

    fn error_callback(error: ParseError) -> Result<(), Rollback> {
        // So we may or may not be parsing the first part of an arrow function
        // (the part before the =>). We might end up parsing that whole thing or
        // we might end up parsing only part of it and thinking we're done. We
        // need to look at the next token to figure out if we really parsed an
        // assignment expression or if this is just the beginning of an arrow
        // function
        match error {
            // Don't rollback on these errors.
            ParseError::StrictReservedWord => Ok(()),
            // Everything else causes a rollback
            _ => Err(Rollback),
        }
    }

    fn try_assignment_but_not_arrow_function(
        env: &mut ParserEnv,
    ) -> Result<PatternCover, Rollback> {
        env.with_error_callback(error_callback, |env| {
            let ret = assignment_but_not_arrow_function_cover(env)?;
            match peek::token(env) {
                // x => 123
                TokenKind::TArrow => Err(Rollback),
                // (x): number => 123
                TokenKind::TColon => {
                    if env.last_token() == Some(&TokenKind::TRparen) {
                        Err(Rollback)
                    } else {
                        Ok(ret)
                    }
                }
                _ => {
                    // async x => 123 -- and we've already parsed async as an identifier expression
                    if peek::is_identifier(env) {
                        if let PatternCover::CoverExpr(expr) = &ret {
                            if let ExpressionInner::Identifier { inner, .. } = &**expr {
                                if &inner.name == "async" && !peek::is_line_terminator(env) {
                                    return Err(Rollback);
                                }
                            }
                        }
                    }
                    Ok(ret)
                }
            }
        })
    }

    fn try_assignment_or_arrow_function(
        env: &mut ParserEnv,
        try_arrow_function_first: bool,
    ) -> Result<PatternCover, Rollback> {
        match if try_arrow_function_first {
            try_parse::to_parse(env, try_arrow_function)
        } else {
            try_parse::to_parse(env, try_assignment_but_not_arrow_function)
        } {
            try_parse::ParseResult::ParsedSuccessfully(expr) => Ok(expr),
            try_parse::ParseResult::FailedToParse => {
                match if try_arrow_function_first {
                    try_parse::to_parse(env, try_assignment_but_not_arrow_function)
                } else {
                    try_parse::to_parse(env, try_arrow_function)
                } {
                    try_parse::ParseResult::ParsedSuccessfully(expr) => Ok(expr),
                    try_parse::ParseResult::FailedToParse => {
                        // Well shoot. It doesn't parse cleanly as a normal
                        // expression or as an arrow_function. Let's treat it as a
                        // normal assignment expression gone wrong
                        assignment_but_not_arrow_function_cover(env)
                    }
                }
            }
        }
    }

    let is_identifier = peek::is_identifier(env)
        && match peek::token(env) {
            TokenKind::TAwait => !env.allow_await(),
            TokenKind::TYield => !env.allow_yield(),
            _ => true,
        };
    if peek::token(env) == &TokenKind::TYield {
        if env.allow_yield() {
            Ok(PatternCover::CoverExpr(yield_expr(env)?))
        } else {
            assignment_but_not_arrow_function_cover(env)
        }
    } else if peek::token(env) == &TokenKind::TThis {
        if peek::async_arrow_parameter_continuation_is_arrow(env) {
            try_assignment_or_arrow_function(env, false)
        } else {
            assignment_but_not_arrow_function_cover(env)
        }
    } else if matches!(peek::token(env), TokenKind::TLparen | TokenKind::TLessThan) {
        // Ok, we don't know if this is going to be an arrow function or a
        // regular assignment expression. Let's first try to parse it as an
        // assignment expression. If that fails we'll try an arrow function.
        try_assignment_or_arrow_function(env, false)
    } else if is_identifier {
        let is_async = peek::token(env) == &TokenKind::TAsync;
        let arrow_function_possible = if is_async {
            peek::async_token_can_start_arrow_function(env)
        } else {
            peek::async_arrow_parameter_continuation_is_arrow(env)
        };
        if !arrow_function_possible {
            assignment_but_not_arrow_function_cover(env)
        } else {
            // Unless it begins with `async <` in which case we first try parsing
            // it as an arrow function, and then an assignment expression.
            let try_arrow_function_first = is_async
                && env.should_parse_types()
                && peek::current_token_is_followed_by_tless_than(env);
            try_assignment_or_arrow_function(env, try_arrow_function_first)
        }
    } else {
        assignment_but_not_arrow_function_cover(env)
    }
}

pub(super) fn assignment(
    env: &mut ParserEnv,
) -> Result<expression::Expression<Loc, Loc>, Rollback> {
    let cover = assignment_cover(env)?;
    as_expression(env, cover)
}

// Port of the `yield` function from expression_parser.ml (lines 214-261)
// Renamed to `yield_expr` since `yield` is a Rust keyword
fn yield_expr(env: &mut ParserEnv) -> Result<expression::Expression<Loc, Loc>, Rollback> {
    let (loc, yield_expr) = with_loc(None, env, |env| {
        if env.in_formal_parameters() {
            env.error(ParseError::YieldInFormalParameters)?;
        }
        if env.in_match_expression() {
            env.error(ParseError::MatchExpressionYield)?;
        }

        let leading = peek::comments(env);
        let start_loc = peek::loc(env).dupe();
        expect::token(env, TokenKind::TYield)?;
        let end_loc = peek::loc(env).dupe();

        let (argument, delegate) = if peek::is_implicit_semicolon(env) {
            (None, false)
        } else {
            let delegate = eat::maybe(env, TokenKind::TMult)?;
            let has_argument = !matches!(
                peek::token(env),
                TokenKind::TSemicolon
                    | TokenKind::TRbracket
                    | TokenKind::TRcurly
                    | TokenKind::TRparen
                    | TokenKind::TColon
                    | TokenKind::TComma
            );
            let argument = if delegate || has_argument {
                Some(assignment(env)?)
            } else {
                None
            };
            (argument, delegate)
        };

        let trailing = match &argument {
            None => eat::trailing_comments(env),
            Some(_) => Vec::new(),
        };

        Ok(expression::Yield {
            argument,
            delegate,
            comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            result_out: Loc::between(&start_loc, &end_loc),
        })
    })?;
    Ok(expression::Expression::new(ExpressionInner::Yield {
        loc,
        inner: Arc::new(yield_expr),
    }))
}

fn is_lhs(expr: &expression::Expression<Loc, Loc>) -> bool {
    match expr.deref() {
        // new.target
        ExpressionInner::MetaProperty { inner, .. }
            if inner.meta.name == "new" && inner.property.name == "target" =>
        {
            false
        }
        // import.meta
        ExpressionInner::MetaProperty { inner, .. }
            if inner.meta.name == "import" && inner.property.name == "meta" =>
        {
            false
        }
        // #sec-static-semantics-static-semantics-isvalidsimpleassignmenttarget
        ExpressionInner::Identifier { .. }
        | ExpressionInner::Member { .. }
        | ExpressionInner::MetaProperty { .. } => true,
        ExpressionInner::Unary { inner, .. }
            if matches!(inner.operator, expression::UnaryOperator::Nonnull) =>
        {
            is_lhs(&inner.argument)
        }
        ExpressionInner::Array { .. }
        | ExpressionInner::ArrowFunction { .. }
        | ExpressionInner::AsConstExpression { .. }
        | ExpressionInner::AsExpression { .. }
        | ExpressionInner::Assignment { .. }
        | ExpressionInner::Binary { .. }
        | ExpressionInner::Call { .. }
        | ExpressionInner::Class { .. }
        | ExpressionInner::Conditional { .. }
        | ExpressionInner::Function { .. }
        | ExpressionInner::Import { .. }
        | ExpressionInner::JSXElement { .. }
        | ExpressionInner::JSXFragment { .. }
        | ExpressionInner::StringLiteral { .. }
        | ExpressionInner::BooleanLiteral { .. }
        | ExpressionInner::NullLiteral { .. }
        | ExpressionInner::NumberLiteral { .. }
        | ExpressionInner::BigIntLiteral { .. }
        | ExpressionInner::RegExpLiteral { .. }
        | ExpressionInner::ModuleRefLiteral { .. }
        | ExpressionInner::Logical { .. }
        | ExpressionInner::Match { .. }
        | ExpressionInner::New { .. }
        | ExpressionInner::Object { .. }
        | ExpressionInner::OptionalCall { .. }
        | ExpressionInner::OptionalMember { .. }
        | ExpressionInner::Record { .. }
        | ExpressionInner::Sequence { .. }
        | ExpressionInner::Super { .. }
        | ExpressionInner::TaggedTemplate { .. }
        | ExpressionInner::TemplateLiteral { .. }
        | ExpressionInner::This { .. }
        | ExpressionInner::TypeCast { .. }
        | ExpressionInner::TSSatisfies { .. }
        | ExpressionInner::Unary { .. }
        | ExpressionInner::Update { .. }
        | ExpressionInner::Yield { .. } => false,
    }
}

// Helper function to parse assignment operator
fn assignment_op(
    env: &mut ParserEnv,
) -> Result<Option<Option<expression::AssignmentOperator>>, Rollback> {
    use expression::AssignmentOperator::*;
    let op = match peek::token(env) {
        TokenKind::TRshift3Assign => Some(Some(RShift3Assign)),
        TokenKind::TRshiftAssign => Some(Some(RShiftAssign)),
        TokenKind::TLshiftAssign => Some(Some(LShiftAssign)),
        TokenKind::TBitXorAssign => Some(Some(BitXorAssign)),
        TokenKind::TBitOrAssign => Some(Some(BitOrAssign)),
        TokenKind::TBitAndAssign => Some(Some(BitAndAssign)),
        TokenKind::TModAssign => Some(Some(ModAssign)),
        TokenKind::TDivAssign => Some(Some(DivAssign)),
        TokenKind::TMultAssign => Some(Some(MultAssign)),
        TokenKind::TExpAssign => Some(Some(ExpAssign)),
        TokenKind::TMinusAssign => Some(Some(MinusAssign)),
        TokenKind::TPlusAssign => Some(Some(PlusAssign)),
        TokenKind::TNullishAssign => Some(Some(NullishAssign)),
        TokenKind::TAndAssign => Some(Some(AndAssign)),
        TokenKind::TOrAssign => Some(Some(OrAssign)),
        TokenKind::TAssign => Some(None),
        _ => None,
    };
    if op.is_some() {
        eat::token(env)?;
    }
    Ok(op)
}

/* ConditionalExpression :
 *   LogicalExpression
 *   LogicalExpression ? AssignmentExpression : AssignmentExpression
 */
fn conditional_cover(env: &mut ParserEnv) -> Result<PatternCover, Rollback> {
    let start_loc = peek::loc(env).dupe();
    let expr = logical_cover(env)?;
    if peek::token(env) == &TokenKind::TPling {
        eat::token(env)?;

        // no_in is ignored for the consequent
        let consequent = env.with_no_in(false, |env_no_in| assignment(env_no_in))?;
        expect::token(env, TokenKind::TColon)?;
        let (loc, alternate) = with_loc(Some(start_loc), env, assignment)?;
        Ok(PatternCover::CoverExpr(expression::Expression::new(
            ExpressionInner::Conditional {
                loc,
                inner: Arc::new(expression::Conditional {
                    test: as_expression(env, expr)?,
                    consequent,
                    alternate,
                    comments: None,
                }),
            },
        )))
    } else {
        Ok(expr)
    }
}

pub(super) fn conditional(
    env: &mut ParserEnv,
) -> Result<expression::Expression<Loc, Loc>, Rollback> {
    let cover = conditional_cover(env)?;
    as_expression(env, cover)
}

/*
 * LogicalANDExpression :
 *   BinaryExpression
 *   LogicalANDExpression && BitwiseORExpression
 *
 * LogicalORExpression :
 *   LogicalANDExpression
 *   LogicalORExpression || LogicalANDExpression
 *   LogicalORExpression ?? LogicalANDExpression
 *
 * LogicalExpression :
 *   LogicalORExpression
 */
fn logical_cover(env: &mut ParserEnv) -> Result<PatternCover, Rollback> {
    fn make_logical(
        env: &mut ParserEnv,
        left: PatternCover,
        right: PatternCover,
        operator: expression::LogicalOperator,
        loc: Loc,
    ) -> Result<PatternCover, Rollback> {
        let left = as_expression(env, left)?;
        let right = as_expression(env, right)?;
        Ok(PatternCover::CoverExpr(expression::Expression::new(
            ExpressionInner::Logical {
                loc,
                inner: Arc::new(expression::Logical {
                    operator,
                    left,
                    right,
                    comments: None,
                }),
            },
        )))
    }

    fn logical_and(
        env: &mut ParserEnv,
        mut left: PatternCover,
        mut lloc: Loc,
    ) -> Result<(Loc, PatternCover), Rollback> {
        loop {
            match peek::token(env) {
                TokenKind::TAnd => {
                    eat::token(env)?;
                    let (rloc, right) = with_loc(None, env, binary_cover)?;
                    let loc = Loc::between(&lloc, &rloc).dupe();
                    left = make_logical(
                        env,
                        left,
                        right,
                        expression::LogicalOperator::And,
                        loc.dupe(),
                    )?;
                    // `a && b ?? c` is an error, but to recover, try to parse it like `(a && b) ?? c`.
                    let (new_loc, new_left) = coalesce(env, false, left, loc)?;
                    lloc = new_loc;
                    left = new_left;
                }
                _ => return Ok((lloc, left)),
            }
        }
    }

    fn logical_or(
        env: &mut ParserEnv,
        mut left: PatternCover,
        mut lloc: Loc,
    ) -> Result<(Loc, PatternCover), Rollback> {
        loop {
            match peek::token(env) {
                TokenKind::TOr => {
                    eat::token(env)?;
                    let (rloc, right) = with_loc(None, env, binary_cover)?;
                    let (rloc, right) = logical_and(env, right, rloc)?;
                    let loc = Loc::between(&lloc, &rloc).dupe();
                    left = make_logical(
                        env,
                        left,
                        right,
                        expression::LogicalOperator::Or,
                        loc.dupe(),
                    )?;
                    // `a || b ?? c` is an error, but to recover, try to parse it like `(a || b) ?? c`.
                    let (new_loc, new_left) = coalesce(env, false, left, loc)?;
                    lloc = new_loc;
                    left = new_left;
                }
                _ => return Ok((lloc, left)),
            }
        }
    }

    fn coalesce(
        env: &mut ParserEnv,
        mut allowed: bool,
        mut left: PatternCover,
        mut lloc: Loc,
    ) -> Result<(Loc, PatternCover), Rollback> {
        loop {
            match peek::token(env) {
                TokenKind::TPlingPling => {
                    if !allowed {
                        env.error(ParseError::NullishCoalescingUnexpectedLogical(
                            "??".to_string(),
                        ))?;
                    }

                    expect::token(env, TokenKind::TPlingPling)?;
                    let (rloc, right) = with_loc(None, env, binary_cover)?;
                    let (rloc, right) = match peek::token(env) {
                        TokenKind::TAnd | TokenKind::TOr => {
                            // `a ?? b || c` is an error. To recover, treat it like `a ?? (b || c)`.
                            let token_value = match peek::token(env) {
                                TokenKind::TAnd => "&&",
                                TokenKind::TOr => "||",
                                _ => unreachable!(),
                            };
                            env.error(ParseError::NullishCoalescingUnexpectedLogical(
                                token_value.to_string(),
                            ))?;
                            let (rloc, right) = logical_and(env, right, rloc)?;
                            logical_or(env, right, rloc)?
                        }
                        _ => (rloc, right),
                    };
                    let loc = Loc::between(&lloc, &rloc).dupe();
                    left = make_logical(
                        env,
                        left,
                        right,
                        expression::LogicalOperator::NullishCoalesce,
                        loc.dupe(),
                    )?;
                    lloc = loc;
                    allowed = true;
                }
                _ => return Ok((lloc, left)),
            }
        }
    }

    let (loc, left) = with_loc(None, env, binary_cover)?;
    let (_, left) = match peek::token(env) {
        TokenKind::TPlingPling => coalesce(env, true, left, loc)?,
        _ => {
            let (loc, left) = logical_and(env, left, loc)?;
            logical_or(env, left, loc)?
        }
    };
    Ok(left)
}

fn binary_cover(env: &mut ParserEnv) -> Result<PatternCover, Rollback> {
    fn binary_op(
        env: &mut ParserEnv,
    ) -> Result<Option<(expression::BinaryOperator, OpPrecedence)>, Rollback> {
        use expression::BinaryOperator::*;
        let ret = match peek::token(env) {
            // Most BinaryExpression operators are left associative
            // Lowest pri
            TokenKind::TBitOr => Some((BitOr, OpPrecedence::LeftAssoc(2))),
            TokenKind::TBitXor => Some((Xor, OpPrecedence::LeftAssoc(3))),
            TokenKind::TBitAnd => Some((BitAnd, OpPrecedence::LeftAssoc(4))),
            TokenKind::TEqual => Some((Equal, OpPrecedence::LeftAssoc(5))),
            TokenKind::TStrictEqual => Some((StrictEqual, OpPrecedence::LeftAssoc(5))),
            TokenKind::TNotEqual => Some((NotEqual, OpPrecedence::LeftAssoc(5))),
            TokenKind::TStrictNotEqual => Some((StrictNotEqual, OpPrecedence::LeftAssoc(5))),
            TokenKind::TLessThan => Some((LessThan, OpPrecedence::LeftAssoc(6))),
            TokenKind::TLessThanEqual => Some((LessThanEqual, OpPrecedence::LeftAssoc(6))),
            TokenKind::TGreaterThan => Some((GreaterThan, OpPrecedence::LeftAssoc(6))),
            TokenKind::TGreaterThanEqual => Some((GreaterThanEqual, OpPrecedence::LeftAssoc(6))),
            TokenKind::TIn => {
                if env.no_in() {
                    None
                } else {
                    Some((In, OpPrecedence::LeftAssoc(6)))
                }
            }
            TokenKind::TInstanceof => Some((Instanceof, OpPrecedence::LeftAssoc(6))),
            TokenKind::TLshift => Some((LShift, OpPrecedence::LeftAssoc(7))),
            TokenKind::TRshift => Some((RShift, OpPrecedence::LeftAssoc(7))),
            TokenKind::TRshift3 => Some((RShift3, OpPrecedence::LeftAssoc(7))),
            TokenKind::TPlus => Some((Plus, OpPrecedence::LeftAssoc(8))),
            TokenKind::TMinus => Some((Minus, OpPrecedence::LeftAssoc(8))),
            TokenKind::TMult => Some((Mult, OpPrecedence::LeftAssoc(9))),
            TokenKind::TDiv => Some((Div, OpPrecedence::LeftAssoc(9))),
            TokenKind::TMod => Some((Mod, OpPrecedence::LeftAssoc(9))),
            TokenKind::TExp => Some((Exp, OpPrecedence::RightAssoc(10))),
            // Highest priority
            _ => None,
        };
        if ret.is_some() {
            eat::token(env)?;
        }
        Ok(ret)
    }

    fn make_binary(
        left: expression::Expression<Loc, Loc>,
        right: expression::Expression<Loc, Loc>,
        operator: expression::BinaryOperator,
        loc: Loc,
    ) -> expression::Expression<Loc, Loc> {
        expression::Expression::new(ExpressionInner::Binary {
            loc,
            inner: Arc::new(expression::Binary {
                operator,
                left,
                right,
                comments: None,
            }),
        })
    }

    type StackItem = (
        expression::Expression<Loc, Loc>,
        (expression::BinaryOperator, OpPrecedence),
        Loc,
    );

    fn add_to_stack(
        mut right: expression::Expression<Loc, Loc>,
        (rop, rpri): (expression::BinaryOperator, OpPrecedence),
        mut rloc: Loc,
        stack: &mut Vec<StackItem>,
    ) {
        loop {
            if let Some((left, (lop, _lpri), lloc)) =
                stack.pop_if(|(_, (_, lpri), _)| is_tighter(*lpri, rpri))
            {
                let loc = Loc::between(&lloc, &rloc);
                right = make_binary(left, right, lop, loc.dupe());
                rloc = loc;
            } else {
                stack.push((right, (rop, rpri), rloc));
                return;
            }
        }
    }

    fn collapse_stack(
        mut right: expression::Expression<Loc, Loc>,
        mut rloc: Loc,
        stack: Vec<StackItem>,
    ) -> expression::Expression<Loc, Loc> {
        for (left, (lop, _), lloc) in stack.into_iter().rev() {
            let loc = Loc::between(&lloc, &rloc);
            right = make_binary(left, right, lop, loc.dupe());
            rloc = loc;
        }
        right
    }

    let mut stack = Vec::new();
    loop {
        let (mut expr_loc, (is_unary, mut expr)) = with_loc(None, env, |env| {
            let is_unary = peek_unary_op(env)?.is_some();
            let expr = env.with_no_in(false, unary_cover)?;
            Ok((is_unary, expr))
        })?;

        if peek::token(env) == &TokenKind::TLessThan {
            if let PatternCover::CoverExpr(expr) = &expr {
                if matches!(&**expr, ExpressionInner::JSXElement { .. }) {
                    env.error(ParseError::AdjacentJSXElements)?;
                }
            }
        }

        while let Some(keyword) = match peek::token(env) {
            TokenKind::TIdentifier { raw, .. } => {
                let keyword = raw.to_owned();
                if (&keyword == "as" || &keyword == "satisfies")
                    && (env.should_parse_types() || !peek::is_line_terminator(env))
                {
                    if !env.should_parse_types() {
                        env.error(ParseError::UnexpectedTypeOperator)?;
                    }
                    Some(keyword)
                } else {
                    None
                }
            }
            _ => None,
        } {
            eat::token(env)?;
            let as_expression_expr = as_expression(env, expr)?;
            let (new_expr, new_expr_loc) = match stack
                .pop_if(|(_, (_, lpri), _)| is_tighter(*lpri, OpPrecedence::LeftAssoc(6)))
            {
                Some((left, (lop, _lpri), lloc)) => {
                    let loc = Loc::between(&lloc, &expr_loc);
                    (make_binary(left, as_expression_expr, lop, loc.dupe()), loc)
                }
                None => (as_expression_expr, expr_loc.dupe()),
            };
            expr = if keyword == "satisfies" {
                let annot = type_parser::parse_type(env)?;
                let loc = Loc::between(&new_expr_loc, annot.loc());
                expr_loc = loc.dupe();
                PatternCover::CoverExpr(expression::Expression::new(ExpressionInner::TSSatisfies {
                    loc,
                    inner: Arc::new(expression::TSSatisfies {
                        expression: new_expr,
                        annot: types::Annotation {
                            loc: annot.loc().dupe(),
                            annotation: annot,
                        },
                        comments: None,
                    }),
                }))
            } else if peek::token(env) == &TokenKind::TConst {
                let loc = Loc::between(&new_expr_loc, peek::loc(env));
                eat::token(env)?;
                expr_loc = loc.dupe();
                PatternCover::CoverExpr(expression::Expression::new(
                    ExpressionInner::AsConstExpression {
                        loc,
                        inner: Arc::new(expression::AsConstExpression {
                            expression: new_expr,
                            comments: None,
                        }),
                    },
                ))
            } else {
                let annot = type_parser::parse_type(env)?;
                let loc = Loc::between(&new_expr_loc, annot.loc());
                expr_loc = loc.dupe();
                PatternCover::CoverExpr(expression::Expression::new(
                    ExpressionInner::AsExpression {
                        loc,
                        inner: Arc::new(expression::AsExpression {
                            expression: new_expr,
                            annot: types::Annotation {
                                loc: annot.loc().dupe(),
                                annotation: annot,
                            },
                            comments: None,
                        }),
                    },
                ))
            };
        }

        match (stack.is_empty(), binary_op(env)?) {
            (true, None) => return Ok(expr),
            (_, None) => {
                let expr_result = as_expression(env, expr)?;
                return Ok(PatternCover::CoverExpr(collapse_stack(
                    expr_result,
                    expr_loc,
                    stack,
                )));
            }
            (_, Some((rop, rpri))) => {
                if is_unary && matches!(rop, expression::BinaryOperator::Exp) {
                    env.error_at(expr_loc.dupe(), ParseError::InvalidLHSInExponentiation)?;
                }
                let expr_result = as_expression(env, expr)?;
                add_to_stack(expr_result, (rop, rpri), expr_loc, &mut stack);
            }
        }
    }
}

fn peek_unary_op(env: &mut ParserEnv) -> Result<Option<expression::UnaryOperator>, Rollback> {
    Ok(match peek::token(env) {
        TokenKind::TNot => Some(expression::UnaryOperator::Not),
        TokenKind::TBitNot => Some(expression::UnaryOperator::BitNot),
        TokenKind::TPlus => Some(expression::UnaryOperator::Plus),
        TokenKind::TMinus => Some(expression::UnaryOperator::Minus),
        TokenKind::TTypeof => Some(expression::UnaryOperator::Typeof),
        TokenKind::TVoid => Some(expression::UnaryOperator::Void),
        TokenKind::TDelete => Some(expression::UnaryOperator::Delete),
        TokenKind::TAwait => {
            if env.allow_await() {
                if env.in_formal_parameters() {
                    env.error(ParseError::AwaitInAsyncFormalParameters)?;
                }
                if env.in_match_expression() {
                    env.error(ParseError::MatchExpressionAwait)?;
                }
                Some(expression::UnaryOperator::Await)
            } else {
                None
            }
        }
        _ => None,
    })
}

fn unary_cover(env: &mut ParserEnv) -> Result<PatternCover, Rollback> {
    let start_loc = peek::loc(env).dupe();
    let leading = peek::comments(env);
    let op = peek_unary_op(env)?;

    match op {
        None => {
            // Check for update operators (++ and --)
            let update_op = match peek::token(env) {
                TokenKind::TIncr => Some(expression::UpdateOperator::Increment),
                TokenKind::TDecr => Some(expression::UpdateOperator::Decrement),
                _ => None,
            };

            match update_op {
                None => postfix_cover(env),
                Some(operator) => {
                    eat::token(env)?;
                    let (loc, argument) = with_loc(Some(start_loc), env, unary)?;
                    if !is_lhs(&argument) {
                        env.error_at(argument.loc().dupe(), ParseError::InvalidLHSInAssignment)?;
                    }
                    match &*argument {
                        ExpressionInner::Identifier { inner, .. } if is_restricted(&inner.name) => {
                            env.strict_error(ParseError::StrictLHSPrefix)?;
                        }
                        _ => {}
                    }
                    Ok(PatternCover::CoverExpr(expression::Expression::new(
                        ExpressionInner::Update {
                            loc,
                            inner: Arc::new(expression::Update {
                                operator,
                                prefix: true,
                                argument,
                                comments: mk_comments_opt(Some(leading.into()), None),
                            }),
                        },
                    )))
                }
            }
        }
        Some(operator) => {
            eat::token(env)?;
            let (loc, argument) = with_loc(Some(start_loc), env, unary)?;
            match (operator, &*argument) {
                (expression::UnaryOperator::Delete, ExpressionInner::Identifier { .. }) => {
                    env.strict_error_at((loc.dupe(), ParseError::StrictDelete))?;
                }
                (expression::UnaryOperator::Delete, ExpressionInner::Member { inner, .. }) => {
                    if matches!(
                        inner.property,
                        expression::member::Property::PropertyPrivateName(_)
                    ) {
                        env.error_at(loc.dupe(), ParseError::PrivateDelete)?;
                    }
                }
                _ => {}
            }
            Ok(PatternCover::CoverExpr(expression::Expression::new(
                ExpressionInner::Unary {
                    loc,
                    inner: Arc::new(expression::Unary {
                        operator,
                        argument,
                        comments: mk_comments_opt(Some(leading.into()), None),
                    }),
                },
            )))
        }
    }
}

fn unary(env: &mut ParserEnv) -> Result<expression::Expression<Loc, Loc>, Rollback> {
    let cover = unary_cover(env)?;
    as_expression(env, cover)
}

fn postfix_cover(env: &mut ParserEnv) -> Result<PatternCover, Rollback> {
    let argument = left_hand_side_cover(env)?;

    // No line terminator allowed before operator
    if peek::is_line_terminator(env) {
        return Ok(argument);
    }

    let update_op = match peek::token(env) {
        TokenKind::TIncr => Some(expression::UpdateOperator::Increment),
        TokenKind::TDecr => Some(expression::UpdateOperator::Decrement),
        _ => None,
    };

    match update_op {
        None => Ok(argument),
        Some(operator) => {
            let argument = as_expression(env, argument)?;
            if !is_lhs(&argument) {
                env.error_at(argument.loc().dupe(), ParseError::InvalidLHSInAssignment)?;
            }
            match &*argument {
                ExpressionInner::Identifier { inner, .. } if is_restricted(&inner.name) => {
                    env.strict_error(ParseError::StrictLHSPostfix)?;
                }
                _ => {}
            }
            let end_loc = peek::loc(env).dupe();
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            let loc = Loc::between(argument.loc(), &end_loc);
            Ok(PatternCover::CoverExpr(expression::Expression::new(
                ExpressionInner::Update {
                    loc,
                    inner: Arc::new(expression::Update {
                        operator,
                        prefix: false,
                        argument,
                        comments: mk_comments_opt(None, Some(trailing.into())),
                    }),
                },
            )))
        }
    }
}

fn left_hand_side_cover(env: &mut ParserEnv) -> Result<PatternCover, Rollback> {
    let start_loc = peek::loc(env).dupe();
    let allow_new = !env.no_new();
    let expr = env.with_no_new(false, |env| {
        Ok(match peek::token(env) {
            TokenKind::TNew if allow_new => PatternCover::CoverExpr(new_expression(env)?),
            TokenKind::TImport => PatternCover::CoverExpr(import_expr(env)?),
            TokenKind::TSuper => PatternCover::CoverExpr(super_expr(env)?),
            _ => {
                if peek::is_function(env) {
                    PatternCover::CoverExpr(function_expr(env)?)
                } else {
                    primary_cover(env)?
                }
            }
        })
    })?;
    call_cover(env, true, false, start_loc, expr)
}

pub(super) fn left_hand_side(
    env: &mut ParserEnv,
) -> Result<expression::Expression<Loc, Loc>, Rollback> {
    let cover = left_hand_side_cover(env)?;
    as_expression(env, cover)
}

pub(super) fn decorator(env: &mut ParserEnv) -> Result<expression::Expression<Loc, Loc>, Rollback> {
    env.with_no_call(false, |env| {
        env.with_no_in(false, |env| {
            if peek::token(env) == &TokenKind::TLparen {
                return group(env);
            }

            let id = main_parser::parse_identifier(env, None)?;
            let start_loc = id.loc.dupe();
            let mut expression = expression::Expression::new(ExpressionInner::Identifier {
                loc: start_loc.dupe(),
                inner: id,
            });

            while eat::maybe(env, TokenKind::TPeriod)? {
                let (property_loc, property) = static_member_property(env)?;
                expression = expression::Expression::new(ExpressionInner::Member {
                    loc: Loc::between(&start_loc, &property_loc),
                    inner: Arc::new(expression::Member {
                        object: expression,
                        property,
                        comments: None,
                    }),
                });
            }

            if peek::token(env) == &TokenKind::TLparen {
                comment_attachment::expression_remove_trailing(env, &mut expression);
                let arguments = arguments(env)?;
                expression = expression::Expression::new(ExpressionInner::Call {
                    loc: Loc::between(&start_loc, &arguments.loc),
                    inner: Arc::new(expression::Call {
                        callee: expression,
                        targs: None,
                        arguments,
                        comments: None,
                    }),
                });
            }

            Ok(expression)
        })
    })
}

fn super_expr(env: &mut ParserEnv) -> Result<expression::Expression<Loc, Loc>, Rollback> {
    let allowed_mode = env.allow_super();
    let (allowed, call_allowed) = match allowed_mode {
        AllowedSuper::NoSuper => (false, false),
        AllowedSuper::SuperProp => (true, false),
        AllowedSuper::SuperPropOrCall => (true, true),
    };

    let loc = peek::loc(env).dupe();
    let leading = peek::comments(env);
    expect::token(env, TokenKind::TSuper)?;
    let trailing = eat::trailing_comments(env);
    let super_expr = expression::Expression::new(ExpressionInner::Super {
        loc: loc.dupe(),
        inner: Arc::new(expression::Super {
            comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        }),
    });

    match peek::token(env) {
        TokenKind::TPeriod | TokenKind::TLbracket => {
            let super_expr = if !allowed {
                env.error_at(loc.dupe(), ParseError::UnexpectedSuper)?;
                expression::Expression::new(ExpressionInner::Identifier {
                    loc: loc.dupe(),
                    inner: Identifier::new(IdentifierInner {
                        loc: loc.dupe(),
                        name: FlowSmolStr::new_inline("super"),
                        comments: None,
                    }),
                })
            } else {
                super_expr
            };
            call(env, true, loc, super_expr)
        }
        TokenKind::TLparen => {
            let super_expr = if !call_allowed {
                env.error_at(loc.dupe(), ParseError::UnexpectedSuperCall)?;
                expression::Expression::new(ExpressionInner::Identifier {
                    loc: loc.dupe(),
                    inner: Identifier::new(IdentifierInner {
                        loc: loc.dupe(),
                        name: FlowSmolStr::new_inline("super"),
                        comments: None,
                    }),
                })
            } else {
                super_expr
            };
            call(env, true, loc, super_expr)
        }
        _ => {
            if !allowed {
                env.error_at(loc.dupe(), ParseError::UnexpectedSuper)?;
            } else {
                env.error_unexpected(Some("either a call or access of `super`".to_owned()))?;
            }
            Ok(super_expr)
        }
    }
}

fn import_expr(env: &mut ParserEnv) -> Result<expression::Expression<Loc, Loc>, Rollback> {
    let (loc, mut inner) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        let start_loc = peek::loc(env).dupe();
        expect::token(env, TokenKind::TImport)?;

        if eat::maybe(env, TokenKind::TPeriod)? {
            // `import.defer(specifier)` — deferred eval of a dynamic import
            if matches!(peek::token(env), TokenKind::TIdentifier { raw, .. } if raw == "defer") {
                eat::token(env)?;
                return import_call(env, leading, Some(statement::ImportPhase::Defer));
            }
            // `import.<phase>(...)` for a phase this parser does not
            // implement. The call parens are what distinguish a phase from a
            // mistyped `import.meta`, which still reports as such. Reported
            // here rather than let through to the meta-property path, which
            // would emit a MetaProperty and a misleading "expected the
            // identifier `meta`".
            let unsupported_phase = match peek::token(env) {
                TokenKind::TIdentifier { raw, .. } if raw != "meta" => Some(raw.to_string()),
                _ => None,
            };
            if let Some(phase) = unsupported_phase
                && peek::token_after_current_is_lparen(env)
            {
                let phase_loc = peek::loc(env).dupe();
                eat::token(env)?;
                env.error_at(phase_loc, ParseError::ImportPhaseUnsupported(phase))?;
                return import_call(env, leading, None);
            }
            // import.meta
            let import_ident = Identifier::new(IdentifierInner {
                loc: start_loc.dupe(),
                name: FlowSmolStr::new_inline("import"),
                comments: None,
            });
            let meta_loc = peek::loc(env).dupe();
            expect::identifier(env, "meta")?;
            let meta_ident = Identifier::new(IdentifierInner {
                loc: meta_loc.dupe(),
                name: FlowSmolStr::new_inline("meta"),
                comments: None,
            });
            let trailing = eat::trailing_comments(env);
            Ok(ExpressionInner::MetaProperty {
                loc: LOC_NONE,
                inner: Arc::new(expression::MetaProperty {
                    meta: import_ident,
                    property: meta_ident,
                    comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
                }),
            })
        } else {
            import_call(env, leading, None)
        }
    })?;
    *inner.loc_mut() = loc;
    Ok(expression::Expression::new(inner))
}

/// Parses the argument list of a dynamic import, from the opening paren. The
/// caller has already consumed `import` and, for a phase-modified call, the
/// `.defer` that follows it.
fn import_call(
    env: &mut ParserEnv,
    leading: Vec<Comment<Loc>>,
    phase: Option<statement::ImportPhase>,
) -> Result<ExpressionInner<Loc, Loc>, Rollback> {
    let leading_arg = peek::comments(env);
    expect::token(env, TokenKind::TLparen)?;
    let argument = add_comments(env.with_no_in(false, assignment)?, Some(leading_arg), None);
    let options = if eat::maybe(env, TokenKind::TComma)? {
        Some(env.with_no_in(false, assignment)?)
    } else {
        None
    };
    expect::token(env, TokenKind::TRparen)?;
    let trailing = eat::trailing_comments(env);
    Ok(ExpressionInner::Import {
        loc: LOC_NONE,
        inner: Arc::new(expression::Import {
            argument,
            options,
            phase,
            comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        }),
    })
}

pub(super) fn call_cover(
    env: &mut ParserEnv,
    allow_optional_chain: bool,
    in_optional_chain: bool,
    start_loc: Loc,
    left: PatternCover,
) -> Result<PatternCover, Rollback> {
    let (left, pending_call) = member_cover(
        env,
        allow_optional_chain,
        in_optional_chain,
        start_loc.dupe(),
        left,
    )?;

    fn left_to_callee(
        env: &mut ParserEnv,
        left: PatternCover,
    ) -> Result<expression::Expression<Loc, Loc>, Rollback> {
        let mut expr = as_expression(env, left)?;
        comment_attachment::expression_remove_trailing(env, &mut expr);
        Ok(expr)
    }

    fn should_parse_record(
        env: &mut ParserEnv,
        constructor: &expression::Expression<Loc, Loc>,
    ) -> bool {
        if !env.parse_options().records {
            return false;
        }
        if peek::is_line_terminator(env) {
            return false;
        }
        if env.no_record() {
            return false;
        }
        match &**constructor {
            ExpressionInner::Identifier { inner, .. } => {
                if inner.name.is_empty() {
                    return false;
                }
                let first_char = inner.name.chars().next().unwrap();
                !first_char.is_ascii_lowercase()
            }
            ExpressionInner::Member { .. } => true,
            _ => false,
        }
    }

    fn parse_record(
        env: &mut ParserEnv,
        start_loc: Loc,
        constructor: expression::Expression<Loc, Loc>,
        targs: Option<expression::CallTypeArgs<Loc, Loc>>,
    ) -> Result<PatternCover, Rollback> {
        let (obj_loc, obj, errs) = object_parser::initializer(env)?;
        for (loc, err) in errs.if_expr {
            env.error_at(loc, err)?;
        }
        let loc = Loc::between(&start_loc, &obj_loc);
        Ok(PatternCover::CoverExpr(expression::Expression::new(
            ExpressionInner::Record {
                loc,
                inner: Arc::new(expression::Record {
                    constructor,
                    targs,
                    properties: (obj_loc, obj),
                    comments: None,
                }),
            },
        )))
    }

    let optional = pending_call
        .or_else(|| in_optional_chain.then_some(expression::OptionalCallKind::NonOptional));

    fn arguments_helper(
        env: &mut ParserEnv,
        targs: Option<expression::CallTypeArgs<Loc, Loc>>,
        callee: expression::Expression<Loc, Loc>,
        optional: Option<expression::OptionalCallKind<Loc>>,
        allow_optional_chain: bool,
        start_loc: Loc,
    ) -> Result<PatternCover, Rollback> {
        let args = arguments(env)?;
        let loc = Loc::between(&start_loc, &args.loc);
        let call = expression::Call {
            callee,
            targs,
            arguments: args,
            comments: None,
        };
        let in_optional_chain = optional.is_some();
        let call = if let Some(optional) = optional {
            expression::Expression::new(ExpressionInner::OptionalCall {
                loc: loc.dupe(),
                inner: Arc::new(expression::OptionalCall {
                    call,
                    filtered_out: loc,
                    optional,
                }),
            })
        } else {
            expression::Expression::new(ExpressionInner::Call {
                loc,
                inner: Arc::new(call),
            })
        };
        call_cover(
            env,
            allow_optional_chain,
            in_optional_chain,
            start_loc,
            PatternCover::CoverExpr(call),
        )
    }

    if env.no_call() {
        return Ok(left);
    }

    match peek::token(env) {
        TokenKind::TLparen => {
            let callee = left_to_callee(env, left)?;
            arguments_helper(env, None, callee, optional, allow_optional_chain, start_loc)
        }
        TokenKind::TLcurly => {
            if should_parse_record(
                env,
                match &left {
                    PatternCover::CoverExpr(expression)
                    | PatternCover::CoverPatt(expression, _) => expression,
                },
            ) {
                let callee = left_to_callee(env, left)?;
                parse_record(env, start_loc, callee, None)
            } else {
                Ok(left)
            }
        }
        TokenKind::TLshift | TokenKind::TLessThan => {
            let fallback = if peek::token(env) == &TokenKind::TLessThan
                && let Some(expression::OptionalCallKind::AssertNonnull { assertion_loc }) =
                    &optional
                && let PatternCover::CoverExpr(argument) = &left
            {
                let loc = assertion_loc.dupe();
                let argument = argument.dupe();
                PatternCover::CoverExpr(expression::Expression::new(ExpressionInner::Unary {
                    loc,
                    inner: Arc::new(expression::Unary {
                        operator: expression::UnaryOperator::Nonnull,
                        argument,
                        comments: None,
                    }),
                }))
            } else {
                left.clone()
            };
            let is_optional_type_call =
                matches!(optional, Some(expression::OptionalCallKind::Optional));
            if env.should_parse_ambiguous_types()
                || (env.should_parse_types() && is_optional_type_call)
            {
                // If we are parsing types, then f<T>(e) is a function call with a
                // type application. If we aren't, it's a nested binary expression.
                // Parameterized call syntax is ambiguous, so we fall back to
                // standard parsing if it fails.
                Ok(env.with_error_callback(
                    |_| Err(Rollback),
                    |env| {
                        // Parameterized call syntax is ambiguous, so we fall back to
                        // standard parsing if it fails.
                        try_parse::or_else(
                            env,
                            |env| {
                                let callee = left_to_callee(env, left)?;
                                let targs = call_type_args(env)?;
                                if peek::token(env) == &TokenKind::TLcurly
                                    && should_parse_record(env, &callee)
                                {
                                    parse_record(env, start_loc, callee, targs)
                                } else if let TokenKind::TTemplatePart(part) = peek::token(env) {
                                    let part = part.clone();
                                    let expr = tagged_template(
                                        env,
                                        targs,
                                        start_loc.dupe(),
                                        callee,
                                        part,
                                    )?;
                                    call_cover(
                                        env,
                                        true,
                                        false,
                                        start_loc,
                                        PatternCover::CoverExpr(expr),
                                    )
                                } else {
                                    arguments_helper(
                                        env,
                                        targs,
                                        callee,
                                        optional,
                                        allow_optional_chain,
                                        start_loc,
                                    )
                                }
                            },
                            fallback,
                        )
                    },
                ))
            } else {
                Ok(fallback)
            }
        }
        _ => Ok(left),
    }
}

fn call(
    env: &mut ParserEnv,
    allow_optional_chain: bool,
    start_loc: Loc,
    left: expression::Expression<Loc, Loc>,
) -> Result<expression::Expression<Loc, Loc>, Rollback> {
    let cover = call_cover(
        env,
        allow_optional_chain,
        false,
        start_loc,
        PatternCover::CoverExpr(left),
    )?;
    as_expression(env, cover)
}

fn new_expression(env: &mut ParserEnv) -> Result<expression::Expression<Loc, Loc>, Rollback> {
    let (loc, mut inner) = with_loc(None, env, |env| {
        let start_loc = peek::loc(env).dupe();
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TNew)?;

        if env.in_function() && peek::token(env) == &TokenKind::TPeriod {
            let trailing = eat::trailing_comments(env);
            eat::token(env)?;
            let meta = Identifier::new(IdentifierInner {
                loc: start_loc.dupe(),
                name: FlowSmolStr::new_inline("new"),
                comments: ast_utils::mk_comments_opt(Some(leading.into()), Some(trailing.into())),
            });
            match peek::token(env) {
                TokenKind::TIdentifier { raw, .. } if raw == "target" => {
                    let property = main_parser::parse_identifier(env, None)?;
                    Ok(ExpressionInner::MetaProperty {
                        loc: LOC_NONE,
                        inner: Arc::new(expression::MetaProperty {
                            meta,
                            property,
                            comments: None,
                        }),
                    })
                }
                _ => {
                    env.error_unexpected(Some("the identifier `target`".to_owned()))?;
                    eat::token(env)?;
                    // skip unknown identifier
                    Ok(ExpressionInner::Identifier {
                        loc: LOC_NONE,
                        inner: meta,
                    })
                }
            }
        } else {
            let callee_loc = peek::loc(env).dupe();
            let expr = match peek::token(env) {
                TokenKind::TNew => new_expression(env)?,
                TokenKind::TSuper => env.with_no_call(true, super_expr)?,
                _ => {
                    if peek::is_function(env) {
                        function_expr(env)?
                    } else {
                        primary(env)?
                    }
                }
            };
            let callee =
                env.with_no_call(true, |env| member(env, false, callee_loc.dupe(), expr))?;

            // You can do something like
            //   new raw`42`
            let mut callee = match peek::token(env) {
                TokenKind::TTemplatePart(part) => {
                    let part = part.clone();
                    tagged_template(env, None, callee_loc, callee, part)?
                }
                _ => callee,
            };

            // Remove trailing comments if the callee is followed by args or type args
            if peek::token(env) == &TokenKind::TLparen
                || (env.should_parse_ambiguous_types() && peek::token(env) == &TokenKind::TLessThan)
            {
                comment_attachment::expression_remove_trailing(env, &mut callee);
            }

            let targs = if env.should_parse_ambiguous_types() {
                // If we are parsing types, then new C<T>(e) is a constructor with a
                // type application. If we aren't, it's a nested binary expression.
                env.with_error_callback(
                    |_| Err(Rollback),
                    |env| try_parse::or_else(env, call_type_args, None),
                )
            } else {
                None
            };

            let arguments = if peek::token(env) == &TokenKind::TLparen {
                Some(arguments(env)?)
            } else {
                None
            };

            let comments = mk_comments_opt(Some(leading.into()), None);
            Ok(ExpressionInner::New {
                loc: LOC_NONE,
                inner: Arc::new(expression::New {
                    callee,
                    targs,
                    arguments,
                    comments,
                }),
            })
        }
    })?;
    *inner.loc_mut() = loc;
    Ok(expression::Expression::new(inner))
}

pub(super) fn call_type_args(
    env: &mut ParserEnv,
) -> Result<Option<expression::CallTypeArgs<Loc, Loc>>, Rollback> {
    fn args(
        env: &mut ParserEnv,
    ) -> Result<
        (
            Arc<[expression::CallTypeArg<Loc, Loc>]>,
            Option<Syntax<Loc, Arc<[Comment<Loc>]>>>,
        ),
        Rollback,
    > {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TLessThan)?;
        let arguments = {
            let mut arguments = Vec::new();
            loop {
                match peek::token(env) {
                    TokenKind::TEof | TokenKind::TGreaterThan => break,
                    _ => {
                        arguments.push(match peek::token(env) {
                            TokenKind::TIdentifier { value, .. } if value == "_" => {
                                let loc = peek::loc(env).dupe();
                                let leading = peek::comments(env);
                                expect::identifier(env, "_")?;
                                let trailing = eat::trailing_comments(env);
                                expression::CallTypeArg::Implicit(expression::CallTypeArgImplicit {
                                    loc,
                                    comments: mk_comments_opt(
                                        Some(leading.into()),
                                        Some(trailing.into()),
                                    ),
                                })
                            }
                            _ => expression::CallTypeArg::Explicit(type_parser::parse_type(env)?),
                        });
                        if peek::token(env) != &TokenKind::TGreaterThan {
                            expect::token(env, TokenKind::TComma)?;
                        }
                    }
                }
            }
            arguments
        };
        let internal = peek::comments(env);
        expect::token(env, TokenKind::TGreaterThan)?;
        let trailing = if peek::token(env) == &TokenKind::TLparen {
            comment_attachment::trailing_and_remover(env).trailing
        } else {
            eat::trailing_comments(env)
        };

        let comments = ast_utils::mk_comments_with_internal_opt(
            Some(leading.into()),
            Some(trailing.into()),
            Some(internal.into()),
        );
        Ok((arguments.into(), comments))
    }

    eat::push_lex_mode(env, LexMode::Type);
    let node = if peek::token(env) == &TokenKind::TLessThan {
        with_loc(None, env, args).map(|(loc, (arguments, comments))| {
            Some(expression::CallTypeArgs {
                loc,
                arguments,
                comments,
            })
        })
    } else {
        Ok(None)
    };
    eat::pop_lex_mode(env);
    node
}

pub(super) fn arguments(env: &mut ParserEnv) -> Result<expression::ArgList<Loc, Loc>, Rollback> {
    fn spread_element(
        env: &mut ParserEnv,
    ) -> Result<expression::SpreadElement<Loc, Loc>, Rollback> {
        let (loc, (argument, comments)) = with_loc(None, env, |env| {
            let leading = peek::comments(env);
            expect::token(env, TokenKind::TEllipsis)?;
            let argument = assignment(env)?;
            Ok((argument, mk_comments_opt(Some(leading.into()), None)))
        })?;
        Ok(expression::SpreadElement {
            loc,
            argument,
            comments,
        })
    }

    fn argument(env: &mut ParserEnv) -> Result<expression::ExpressionOrSpread<Loc, Loc>, Rollback> {
        match peek::token(env) {
            TokenKind::TEllipsis => {
                let e = spread_element(env)?;
                Ok(expression::ExpressionOrSpread::Spread(e))
            }
            _ => Ok(expression::ExpressionOrSpread::Expression(assignment(env)?)),
        }
    }

    fn arguments(
        env: &mut ParserEnv,
    ) -> Result<Vec<expression::ExpressionOrSpread<Loc, Loc>>, Rollback> {
        let mut arguments = Vec::new();
        loop {
            match peek::token(env) {
                TokenKind::TEof | TokenKind::TRparen => return Ok(arguments),
                _ => {
                    arguments.push(argument(env)?);
                    if peek::token(env) != &TokenKind::TRparen {
                        expect::token(env, TokenKind::TComma)?;
                    }
                }
            }
        }
    }

    let (loc, (arguments, comments)) = with_loc(None, env, |env| {
        let leading = peek::comments(env);
        expect::token(env, TokenKind::TLparen)?;
        let arguments = arguments(env)?;
        let internal = peek::comments(env);
        expect::token(env, TokenKind::TRparen)?;
        let trailing = eat::trailing_comments(env);
        let comments = ast_utils::mk_comments_with_internal_opt(
            Some(leading.into()),
            Some(trailing.into()),
            Some(internal.into()),
        );
        Ok((arguments.into(), comments))
    })?;
    Ok(expression::ArgList {
        loc,
        arguments,
        comments,
    })
}

fn static_member_property(
    env: &mut ParserEnv,
) -> Result<(Loc, expression::member::Property<Loc, Loc>), Rollback> {
    match peek::token(env) {
        TokenKind::TPound => {
            let id = private_identifier(env)?;
            env.add_used_private(id.name.as_str().to_owned(), id.loc.dupe())?;
            Ok((
                id.loc.dupe(),
                expression::member::Property::PropertyPrivateName(id),
            ))
        }
        _ => {
            let id = identifier_name(env)?;
            Ok((
                id.loc.dupe(),
                expression::member::Property::PropertyIdentifier(id),
            ))
        }
    }
}

fn member_cover(
    env: &mut ParserEnv,
    allow_optional_chain: bool,
    in_optional_chain: bool,
    start_loc: Loc,
    left: PatternCover,
) -> Result<(PatternCover, Option<expression::OptionalCallKind<Loc>>), Rollback> {
    fn dynamic(
        env: &mut ParserEnv,
        allow_optional_chain: bool,
        optional: Option<expression::OptionalMemberKind<Loc>>,
        start_loc: Loc,
        left: PatternCover,
    ) -> Result<PatternCover, Rollback> {
        let expr = env.with_no_call(false, main_parser::parse_expression)?;
        let last_loc = peek::loc(env).dupe();
        expect::token(env, TokenKind::TRbracket)?;
        let trailing = eat::trailing_comments(env);
        let loc = Loc::between(&start_loc, &last_loc);
        let member = expression::Member {
            object: as_expression(env, left)?,
            property: expression::member::Property::PropertyExpression(expr),
            comments: mk_comments_opt(None, Some(trailing.into())),
        };

        let in_optional_chain = optional.is_some();
        let member_expr = match optional {
            Some(optional) => expression::Expression::new(ExpressionInner::OptionalMember {
                loc: loc.dupe(),
                inner: Arc::new(expression::OptionalMember {
                    member,
                    optional,
                    filtered_out: loc.dupe(),
                }),
            }),
            None => expression::Expression::new(ExpressionInner::Member {
                loc: loc.dupe(),
                inner: Arc::new(member),
            }),
        };

        call_cover(
            env,
            allow_optional_chain,
            in_optional_chain,
            start_loc,
            PatternCover::CoverExpr(member_expr),
        )
    }

    fn static_member(
        env: &mut ParserEnv,
        allow_optional_chain: bool,
        optional: Option<expression::OptionalMemberKind<Loc>>,
        start_loc: Loc,
        left: PatternCover,
    ) -> Result<PatternCover, Rollback> {
        let (id_loc, property) = static_member_property(env)?;
        let loc = Loc::between(&start_loc, &id_loc);

        // super.PrivateName is a syntax error
        if let (
            PatternCover::CoverExpr(expr),
            expression::member::Property::PropertyPrivateName(_),
        ) = (&left, &property)
        {
            if matches!(&**expr, ExpressionInner::Super { .. }) {
                env.error_at(loc.dupe(), ParseError::SuperPrivate)?;
            }
        }

        let member = expression::Member {
            object: as_expression(env, left)?,
            property,
            comments: None,
        };

        let in_optional_chain = optional.is_some();
        let member_expr = match optional {
            Some(optional) => expression::Expression::new(ExpressionInner::OptionalMember {
                loc: loc.dupe(),
                inner: Arc::new(expression::OptionalMember {
                    member,
                    optional,
                    filtered_out: loc.dupe(),
                }),
            }),
            None => expression::Expression::new(ExpressionInner::Member {
                loc: loc.dupe(),
                inner: Arc::new(member),
            }),
        };

        call_cover(
            env,
            allow_optional_chain,
            in_optional_chain,
            start_loc,
            PatternCover::CoverExpr(member_expr),
        )
    }

    let default_optional = if in_optional_chain {
        Some(expression::OptionalMemberKind::NonOptional)
    } else {
        None
    };

    let left = assert_operator_cover(env, in_optional_chain, start_loc.dupe(), left)?;

    let mut pending_call = None;
    let left = match peek::token(env) {
        TokenKind::TPlingPeriod => {
            if !allow_optional_chain {
                env.error(ParseError::OptionalChainNew)?;
            }
            expect::token(env, TokenKind::TPlingPeriod)?;
            match peek::token(env) {
                TokenKind::TTemplatePart(_) => {
                    env.error(ParseError::OptionalChainTemplate)?;
                    Ok(left)
                }
                TokenKind::TLparen => {
                    pending_call = Some(expression::OptionalCallKind::Optional);
                    Ok(left)
                }
                TokenKind::TLessThan => {
                    if env.should_parse_types() {
                        pending_call = Some(expression::OptionalCallKind::Optional);
                        Ok(left)
                    } else {
                        static_member(
                            env,
                            allow_optional_chain,
                            Some(expression::OptionalMemberKind::Optional),
                            start_loc,
                            left,
                        )
                    }
                }
                TokenKind::TLbracket => {
                    eat::token(env)?;
                    dynamic(
                        env,
                        allow_optional_chain,
                        Some(expression::OptionalMemberKind::Optional),
                        start_loc,
                        left,
                    )
                }
                _ => static_member(
                    env,
                    allow_optional_chain,
                    Some(expression::OptionalMemberKind::Optional),
                    start_loc,
                    left,
                ),
            }
        }
        TokenKind::TLbracket => {
            eat::token(env)?;
            dynamic(env, allow_optional_chain, default_optional, start_loc, left)
        }
        TokenKind::TPeriod => {
            eat::token(env)?;
            static_member(env, allow_optional_chain, default_optional, start_loc, left)
        }
        TokenKind::TNot if in_optional_chain => {
            if !peek::is_line_terminator(env) {
                let assertion_loc = Loc::between(&start_loc, peek::loc(env));
                let continuation = peek::optional_chain_bang_continuation(env);
                if !env.should_parse_types()
                    && matches!(
                        continuation,
                        TokenKind::TTemplatePart(_)
                            | TokenKind::TLparen
                            | TokenKind::TLessThan
                            | TokenKind::TLbracket
                            | TokenKind::TPeriod
                    )
                {
                    env.error(ParseError::UnexpectedTypeOperator)?;
                }
                match continuation {
                    TokenKind::TTemplatePart(_) => {
                        env.error(ParseError::OptionalChainTemplate)?;
                        eat::token(env)?;
                        Ok(left)
                    }
                    TokenKind::TLparen | TokenKind::TLessThan => {
                        eat::token(env)?;
                        pending_call =
                            Some(expression::OptionalCallKind::AssertNonnull { assertion_loc });
                        Ok(left)
                    }
                    TokenKind::TLbracket => {
                        eat::token(env)?;
                        eat::token(env)?;
                        dynamic(
                            env,
                            allow_optional_chain,
                            Some(expression::OptionalMemberKind::AssertNonnull { assertion_loc }),
                            start_loc,
                            left,
                        )
                    }
                    TokenKind::TPeriod => {
                        eat::token(env)?;
                        eat::token(env)?;
                        static_member(
                            env,
                            allow_optional_chain,
                            Some(expression::OptionalMemberKind::AssertNonnull { assertion_loc }),
                            start_loc,
                            left,
                        )
                    }
                    _ => Ok(left),
                }
            } else {
                Ok(left)
            }
        }
        TokenKind::TTemplatePart(part) => {
            let part = part.clone(); // clone needed: TemplatePart doesn't implement Dupe
            if in_optional_chain {
                env.error(ParseError::OptionalChainTemplate)?;
            }
            let tag = as_expression(env, left)?;
            let expr = tagged_template(env, None, start_loc.dupe(), tag, part)?;
            call_cover(env, true, false, start_loc, PatternCover::CoverExpr(expr))
        }
        _ => Ok(left),
    }?;
    Ok((left, pending_call))
}

fn assert_operator_cover(
    env: &mut ParserEnv,
    in_optional_chain: bool,
    start_loc: Loc,
    left: PatternCover,
) -> Result<PatternCover, Rollback> {
    if peek::token(env) != &TokenKind::TNot || peek::is_line_terminator(env) {
        return Ok(left);
    }
    if !env.should_parse_types() {
        env.error(ParseError::UnexpectedTypeOperator)?;
    }

    let next_token = peek::assert_operator_bang_continuation(env); // clone needed: TokenKind doesn't implement Dupe

    match &next_token {
        TokenKind::TPeriod | TokenKind::TLbracket | TokenKind::TLessThan | TokenKind::TLparen
            if in_optional_chain =>
        {
            Ok(left)
        }
        _ => {
            let argument = as_expression(env, left)?;
            let end_loc = peek::loc(env).dupe();
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            let loc = Loc::between(&start_loc, &end_loc);
            let left =
                PatternCover::CoverExpr(expression::Expression::new(ExpressionInner::Unary {
                    loc,
                    inner: Arc::new(expression::Unary {
                        operator: expression::UnaryOperator::Nonnull,
                        argument,
                        comments: mk_comments_opt(None, Some(trailing.into())),
                    }),
                }));
            assert_operator_cover(env, in_optional_chain, start_loc, left)
        }
    }
}

fn member(
    env: &mut ParserEnv,
    allow_optional_chain: bool,
    start_loc: Loc,
    left: expression::Expression<Loc, Loc>,
) -> Result<expression::Expression<Loc, Loc>, Rollback> {
    let (cover, _) = member_cover(
        env,
        allow_optional_chain,
        false,
        start_loc,
        PatternCover::CoverExpr(left),
    )?;
    as_expression(env, cover)
}

fn function_expr(env: &mut ParserEnv) -> Result<expression::Expression<Loc, Loc>, Rollback> {
    let (loc, func) = with_loc(None, env, |env| {
        let (async_flag, leading_async) = declaration_parser::parse_async(env)?;
        let (sig_loc, (id, params, generator, predicate, return_annot, tparams, leading)) =
            with_loc(None, env, |env| {
                let leading_function = peek::comments(env);
                expect::token(env, TokenKind::TFunction)?;
                let (generator, leading_generator) = declaration_parser::parse_generator(env)?;
                let leading = [leading_async, leading_function, leading_generator].concat();

                // `await` is a keyword in async functions:
                // - proposal-async-iteration/#prod-AsyncGeneratorExpression
                // - #prod-AsyncFunctionExpression
                let allow_await = async_flag;
                // `yield` is a keyword in generator functions:
                //  - proposal-async-iteration/#prod-AsyncGeneratorExpression
                //  - #prod-GeneratorExpression
                let allow_yield = generator;

                let (id, tparams) = if peek::token(env) == &TokenKind::TLparen {
                    (None, None)
                } else {
                    let id = if peek::token(env) == &TokenKind::TLessThan {
                        None
                    } else {
                        env.with_allow_await(allow_await, |env| {
                            env.with_allow_yield(allow_yield, |env| {
                                let mut id = main_parser::parse_identifier(
                                    env,
                                    Some(ParseError::StrictFunctionName),
                                )?;
                                comment_attachment::id_remove_trailing(env, &mut id);
                                Ok(Some(id))
                            })
                        })?
                    };

                    let mut tparams = type_parser::parse_type_params(env)?;
                    comment_attachment::type_params_remove_trailing(env, tparams.as_mut());
                    (id, tparams)
                };

                // #sec-function-definitions-static-semantics-early-errors
                env.with_allow_super(AllowedSuper::NoSuper, |env| {
                    // await is a keyword if *this* is an async function, OR if it's already
                    // a keyword in the current scope (e.g. if this is a non-async function
                    // nested in an async function).
                    let await_ = allow_await || env.allow_await();
                    let mut params =
                        declaration_parser::parse_function_params(env, await_, allow_yield)?;
                    if peek::token(env) != &TokenKind::TColon {
                        comment_attachment::function_params_remove_trailing(env, &mut params);
                    }
                    let (mut return_annot, mut predicate) =
                        type_parser::parse_function_return_annotation_and_predicate_opt(env)?;
                    if predicate.is_none() {
                        comment_attachment::return_annotation_remove_trailing(
                            env,
                            &mut return_annot,
                        );
                    } else {
                        comment_attachment::predicate_remove_trailing(env, predicate.as_mut());
                    }

                    Ok((
                        id,
                        params,
                        generator,
                        predicate,
                        return_annot,
                        tparams,
                        leading,
                    ))
                })
            })?;

        let simple_params = is_simple_parameter_list(&params);

        let (body, contains_use_strict) = declaration_parser::parse_function_body(
            env,
            async_flag,
            generator,
            true,
            simple_params,
        )?;
        declaration_parser::strict_function_post_check(
            env,
            contains_use_strict,
            id.as_ref(),
            &params,
        )?;

        Ok(function::Function {
            id,
            params,
            body,
            generator,
            effect_: function::Effect::Arbitrary,
            async_: async_flag,
            predicate,
            return_: return_annot,
            tparams,
            sig_loc,
            comments: mk_comments_opt(Some(leading.into()), None),
        })
    })?;
    Ok(expression::Expression::new(ExpressionInner::Function {
        loc,
        inner: Arc::new(func),
    }))
}

pub(super) fn number(
    env: &mut ParserEnv,
    kind: NumberType,
    raw: &FlowSmolStr,
) -> Result<f64, Rollback> {
    let value = match kind {
        NumberType::LegacyOctal => {
            env.strict_error(ParseError::StrictOctalLiteral)?;
            let mut to_parse = raw.as_str();
            while let Some(s) = to_parse.strip_prefix('0') {
                to_parse = s;
            }
            if to_parse.is_empty() {
                to_parse = "0";
            }
            let without_underscore: String = to_parse.replace('_', "");
            match i64::from_str_radix(&without_underscore, 8) {
                Ok(v) => v as f64,
                Err(_) => unreachable!("Invalid legacy octal {}", to_parse),
            }
        }
        NumberType::LegacyNonOctal => {
            env.strict_error(ParseError::StrictNonOctalLiteral)?;
            match raw.parse::<f64>() {
                Ok(v) => v,
                Err(_) => unreachable!("Invalid legacy non octal number {}", raw),
            }
        }
        NumberType::Binary => {
            let raw = if let Some(raw) = raw.strip_prefix("0b") {
                raw
            } else {
                raw.strip_prefix("0B").expect(raw)
            };
            parse_binary_to_f64(raw)
        }
        NumberType::Octal => {
            let raw = if let Some(raw) = raw.strip_prefix("0o") {
                raw
            } else {
                raw.strip_prefix("0O").expect(raw)
            };
            parse_octal_to_f64(raw)
        }
        NumberType::Hex => {
            let remainder = if let Some(remainder) = raw.strip_prefix("0x") {
                remainder
            } else {
                raw.strip_prefix("0X").expect(raw)
            };
            parse_hex_to_f64(remainder)
        }
        NumberType::Normal(v) => v,
    };
    expect::token(
        env,
        TokenKind::TNumber {
            kind,
            raw: raw.to_owned(),
        },
    )?;
    Ok(value)
}

pub(super) fn bigint(
    env: &mut ParserEnv,
    kind: BigintType,
    raw: &FlowSmolStr,
) -> Result<Option<i64>, Rollback> {
    let postraw = bigint_strip_n(raw);
    // Must convert based on the kind (binary, octal, hex) to get decimal value
    let value = match kind {
        BigintType::BigBinary => {
            let digits = if let Some(d) = postraw.strip_prefix("0b") {
                d
            } else {
                postraw.strip_prefix("0B").unwrap_or(postraw)
            };
            i64::from_str_radix(digits, 2).ok()
        }
        BigintType::BigOctal => {
            let digits = if let Some(d) = postraw.strip_prefix("0o") {
                d
            } else {
                postraw.strip_prefix("0O").unwrap_or(postraw)
            };
            i64::from_str_radix(digits, 8).ok()
        }
        BigintType::BigNormal => {
            // Hex literals also use BigNormal
            if postraw.starts_with("0x") || postraw.starts_with("0X") {
                let digits = if let Some(d) = postraw.strip_prefix("0x") {
                    d
                } else {
                    postraw.strip_prefix("0X").unwrap_or(postraw)
                };
                i64::from_str_radix(digits, 16).ok()
            } else {
                postraw.parse::<i64>().ok()
            }
        }
    };
    expect::token(
        env,
        TokenKind::TBigint {
            kind,
            raw: raw.to_owned(),
        },
    )?;
    Ok(value)
}

fn primary_cover(env: &mut ParserEnv) -> Result<PatternCover, Rollback> {
    let loc = peek::loc(env).dupe();
    let leading = peek::comments(env);
    match peek::token(env).clone() {
        TokenKind::TThis => {
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            Ok(PatternCover::CoverExpr(expression::Expression::new(
                ExpressionInner::This {
                    loc,
                    inner: Arc::new(expression::This {
                        comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
                    }),
                },
            )))
        }
        TokenKind::TNumber { kind, raw } => {
            let value = number(env, kind, &raw)?;
            let trailing = eat::trailing_comments(env);
            let comments = mk_comments_opt(Some(leading.into()), Some(trailing.into()));
            Ok(PatternCover::CoverExpr(expression::Expression::new(
                ExpressionInner::NumberLiteral {
                    loc,
                    inner: Arc::new(NumberLiteral {
                        value,
                        raw,
                        comments,
                    }),
                },
            )))
        }
        TokenKind::TBigint { kind, raw } => {
            let value = bigint(env, kind, &raw)?;
            let trailing = eat::trailing_comments(env);
            let comments = mk_comments_opt(Some(leading.into()), Some(trailing.into()));
            Ok(PatternCover::CoverExpr(expression::Expression::new(
                ExpressionInner::BigIntLiteral {
                    loc,
                    inner: Arc::new(BigIntLiteral {
                        value,
                        raw,
                        comments,
                    }),
                },
            )))
        }
        TokenKind::TString(_, value, raw, octal) => {
            if octal {
                env.strict_error(ParseError::StrictOctalLiteral)?;
            }
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            let comments = mk_comments_opt(Some(leading.into()), Some(trailing.into()));
            let expr = match env.parse_options().module_ref_prefix {
                Some(ref prefix) if value.starts_with(prefix.as_str()) => {
                    let prefix_len = prefix.len();
                    expression::Expression::new(ExpressionInner::ModuleRefLiteral {
                        loc: loc.dupe(),
                        inner: Arc::new(ModuleRefLiteral {
                            value,
                            require_loc: loc.dupe(),
                            def_loc_opt: None,
                            prefix_len,
                            raw,
                            comments,
                        }),
                    })
                }
                _ => expression::Expression::new(ExpressionInner::StringLiteral {
                    loc: loc.dupe(),
                    inner: Arc::new(StringLiteral {
                        value,
                        raw,
                        comments,
                    }),
                }),
            };
            Ok(PatternCover::CoverExpr(expr))
        }
        TokenKind::TTrue | TokenKind::TFalse => {
            let value = peek::token(env) == &TokenKind::TTrue;
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            let comments = mk_comments_opt(Some(leading.into()), Some(trailing.into()));
            Ok(PatternCover::CoverExpr(expression::Expression::new(
                ExpressionInner::BooleanLiteral {
                    loc,
                    inner: Arc::new(BooleanLiteral { value, comments }),
                },
            )))
        }
        TokenKind::TNull => {
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            let comments = mk_comments_opt(Some(leading.into()), Some(trailing.into()));
            Ok(PatternCover::CoverExpr(expression::Expression::new(
                ExpressionInner::NullLiteral {
                    loc,
                    inner: Arc::new(comments),
                },
            )))
        }
        TokenKind::TLparen => Ok(PatternCover::CoverExpr(group(env)?)),
        TokenKind::TLcurly => {
            let (loc, obj, errs) = object_parser::initializer(env)?;
            Ok(PatternCover::CoverPatt(
                expression::Expression::new(ExpressionInner::Object {
                    loc,
                    inner: Arc::new(obj),
                }),
                errs,
            ))
        }
        TokenKind::TLbracket => {
            let (loc, (arr, errs)) = with_loc(None, env, array_initializer)?;
            Ok(PatternCover::CoverPatt(
                expression::Expression::new(ExpressionInner::Array {
                    loc,
                    inner: Arc::new(arr),
                }),
                errs,
            ))
        }
        TokenKind::TDiv | TokenKind::TDivAssign => Ok(PatternCover::CoverExpr(regexp(env)?)),
        TokenKind::TLessThan => {
            let (loc, jsx) = jsx_parser::parse_jsx_element_or_fragment(env)?;
            let expression = match jsx {
                Ok(e) => expression::Expression::new(ExpressionInner::JSXElement {
                    loc,
                    inner: Arc::new(e),
                }),
                Err(f) => expression::Expression::new(ExpressionInner::JSXFragment {
                    loc,
                    inner: Arc::new(f),
                }),
            };
            Ok(PatternCover::CoverExpr(expression))
        }
        TokenKind::TTemplatePart(part) => {
            let part = part.clone();
            let (loc, template) = template_literal(env, part)?;
            Ok(PatternCover::CoverExpr(expression::Expression::new(
                ExpressionInner::TemplateLiteral {
                    loc,
                    inner: Arc::new(template),
                },
            )))
        }
        TokenKind::TClass => Ok(PatternCover::CoverExpr(object_parser::class_expression(
            env,
        )?)),
        TokenKind::TAt if env.parse_options().esproposal_decorators => Ok(PatternCover::CoverExpr(
            object_parser::class_expression(env)?,
        )),
        TokenKind::TIdentifier { raw, .. }
            if raw == "abstract" && peek::abstract_token_is_followed_by_class(env) =>
        {
            Ok(PatternCover::CoverExpr(object_parser::class_expression(
                env,
            )?))
        }
        // `match (`
        TokenKind::TMatch
            if env.parse_options().pattern_matching
                && peek::match_token_is_followed_by_lparen_on_same_line(env) =>
        {
            let leading = peek::comments(env);
            let match_keyword_loc = peek::loc(env).dupe();
            // Consume `match` as an identifier, in case it's a call expression
            let id = main_parser::parse_identifier(env, None)?;
            // Allows trailing comma
            let args = arguments(env)?;
            // `match (<expr>) {`
            if !peek::is_line_terminator(env) && peek::token(env) == &TokenKind::TLcurly {
                let arg = reparse_arguments_as_match_argument(env, args)?;
                let expr = env.with_in_match_expression(true, |env| {
                    match_expression(env, match_keyword_loc, leading, arg)
                })?;
                Ok(PatternCover::CoverExpr(expr))
            } else {
                // It's actually a call expression of the form `match(...)`
                let callee = expression::Expression::new(ExpressionInner::Identifier {
                    loc: match_keyword_loc.dupe(),
                    inner: id,
                });
                let loc = Loc::between(&match_keyword_loc, &args.loc);
                let comments = mk_comments_opt(Some(leading.into()), None);
                let call = expression::Expression::new(ExpressionInner::Call {
                    loc,
                    inner: Arc::new(expression::Call {
                        callee,
                        targs: None,
                        arguments: args,
                        comments,
                    }),
                });
                // Could have a chained call after this
                call_cover(
                    env,
                    true,
                    false,
                    match_keyword_loc,
                    PatternCover::CoverExpr(call),
                )
            }
        }
        _ => {
            if peek::is_identifier(env) {
                let id = main_parser::parse_identifier(env, None)?;
                Ok(PatternCover::CoverExpr(expression::Expression::new(
                    ExpressionInner::Identifier {
                        loc: id.loc.dupe(),
                        inner: id,
                    },
                )))
            } else {
                env.error_unexpected(None)?;

                // Let's get rid of the bad token
                if matches!(peek::token(env), TokenKind::TError { .. }) {
                    eat::token(env)?;
                }

                // Really no idea how to recover from this. I suppose a null
                // expression is as good as anything
                let comments = mk_comments_opt(Some(leading.into()), Some(Arc::from([])));
                Ok(PatternCover::CoverExpr(expression::Expression::new(
                    ExpressionInner::NullLiteral {
                        loc,
                        inner: Arc::new(comments),
                    },
                )))
            }
        }
    }
}

fn primary(env: &mut ParserEnv) -> Result<expression::Expression<Loc, Loc>, Rollback> {
    let cover = primary_cover(env)?;
    as_expression(env, cover)
}

fn match_expression(
    env: &mut ParserEnv,
    match_keyword_loc: Loc,
    leading: Vec<Comment<Loc>>,
    arg: expression::Expression<Loc, Loc>,
) -> Result<expression::Expression<Loc, Loc>, Rollback> {
    fn case(
        env: &mut ParserEnv,
    ) -> Result<match_::Case<Loc, Loc, expression::Expression<Loc, Loc>>, Rollback> {
        let leading = peek::comments(env);
        let case_match_root_loc = Loc::start_loc(peek::loc(env));
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
        let body = assignment(env)?;
        let invalid_suffix_semicolon = match peek::token(env) {
            TokenKind::TEof | TokenKind::TRcurly => None,
            TokenKind::TSemicolon => {
                let loc = peek::loc(env).dupe();
                eat::token(env)?;
                Some(loc)
            }
            _ => {
                expect::token(env, TokenKind::TComma)?;
                None
            }
        };
        let trailing = eat::trailing_comments(env);
        let comments = mk_comments_opt(Some(leading.into()), Some(trailing.into()));
        let invalid_syntax = match_::InvalidSyntax {
            invalid_prefix_case,
            invalid_infix_colon,
            invalid_suffix_semicolon,
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
    }

    fn case_list(
        env: &mut ParserEnv,
    ) -> Result<Vec<match_::Case<Loc, Loc, expression::Expression<Loc, Loc>>>, Rollback> {
        let mut cases = Vec::new();
        loop {
            match peek::token(env) {
                TokenKind::TEof | TokenKind::TRcurly => return Ok(cases),
                _ => {
                    let (loc, mut case_item) = with_loc(None, env, case)?;
                    case_item.loc = loc;
                    cases.push(case_item);
                }
            }
        }
    }

    let (loc, match_) = with_loc(Some(match_keyword_loc.dupe()), env, |env| {
        expect::token(env, TokenKind::TLcurly)?;
        let cases = case_list(env)?;
        expect::token(env, TokenKind::TRcurly)?;
        let trailing = eat::trailing_comments(env);
        Ok(match_::Match {
            arg,
            cases: cases.into(),
            match_keyword_loc,
            comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        })
    })?;
    Ok(expression::Expression::new(ExpressionInner::Match {
        loc,
        inner: Arc::new(match_),
    }))
}

fn template_literal(
    env: &mut ParserEnv,
    part: TemplatePart,
) -> Result<(Loc, expression::TemplateLiteral<Loc, Loc>), Rollback> {
    fn template_parts(
        env: &mut ParserEnv,
        mut quasis: Vec<expression::template_literal::Element<Loc>>,
        mut expressions: Vec<expression::Expression<Loc, Loc>>,
    ) -> Result<
        (
            Loc,
            Vec<expression::template_literal::Element<Loc>>,
            Vec<expression::Expression<Loc, Loc>>,
        ),
        Rollback,
    > {
        loop {
            let expr = main_parser::parse_expression(env)?;
            let expr_loc = expr.loc().dupe();
            expressions.push(expr);
            let prev_lex_cursor = peek::lex_cursor(env);
            match peek::token(env) {
                TokenKind::TRcurly => {
                    eat::rescan_as_template_from(env, prev_lex_cursor);
                    let (loc, part, is_tail) = match peek::token(env) {
                        TokenKind::TTemplatePart(template_part) => {
                            let template_part = template_part.clone();
                            let loc = template_part.loc.dupe();
                            eat::token(env)?;
                            let element = expression::template_literal::Element {
                                loc: loc.dupe(),
                                value: expression::template_literal::Value {
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
                        return Ok((loc, quasis, expressions));
                    }
                }
                _ => {
                    // Malformed template
                    env.error_unexpected(Some("a template literal part".to_owned()))?;
                    let imaginary_quasi = expression::template_literal::Element {
                        loc: expr_loc.dupe(),
                        value: expression::template_literal::Value {
                            raw: FlowSmolStr::new_inline(""),
                            cooked: FlowSmolStr::new_inline(""),
                        },
                        tail: true,
                    };
                    quasis.push(imaginary_quasi);
                    return Ok((expr_loc, quasis, expressions));
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

    let (end_loc, quasis, expressions) = {
        let head = expression::template_literal::Element {
            loc: start_loc.dupe(),
            value: expression::template_literal::Value { cooked, raw },
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
        expression::TemplateLiteral {
            quasis: quasis.into(),
            expressions: expressions.into(),
            comments: mk_comments_opt(Some(leading.into()), Some(trailing.into())),
        },
    ))
}

fn tagged_template(
    env: &mut ParserEnv,
    targs: Option<expression::CallTypeArgs<Loc, Loc>>,
    start_loc: Loc,
    mut tag: expression::Expression<Loc, Loc>,
    part: TemplatePart,
) -> Result<expression::Expression<Loc, Loc>, Rollback> {
    comment_attachment::expression_remove_trailing(env, &mut tag);
    let (quasi_loc, quasi) = template_literal(env, part)?;
    let loc = Loc::between(&start_loc, &quasi_loc);
    Ok(expression::Expression::new(
        ExpressionInner::TaggedTemplate {
            loc,
            inner: Arc::new(expression::TaggedTemplate {
                tag,
                targs,
                quasi: (quasi_loc, quasi),
                comments: None,
            }),
        },
    ))
}

fn group(env: &mut ParserEnv) -> Result<expression::Expression<Loc, Loc>, Rollback> {
    let leading = peek::comments(env);
    let (loc, cover) = with_loc(None, env, |env| {
        expect::token(env, TokenKind::TLparen)?;
        let expr_start_loc = peek::loc(env).dupe();
        let expression = assignment(env)?;
        let ret = match peek::token(env) {
            TokenKind::TColon => {
                let annot = type_parser::parse_annotation(env)?;
                GroupCover::GroupTypecast(expression::TypeCast {
                    expression,
                    annot,
                    comments: None,
                })
            }
            TokenKind::TComma => {
                GroupCover::GroupExpr(sequence(env, expr_start_loc, vec![expression])?)
            }
            _ => GroupCover::GroupExpr(expression),
        };
        expect::token(env, TokenKind::TRparen)?;
        Ok(ret)
    })?;

    let trailing = eat::trailing_comments(env);
    let ret = match cover {
        GroupCover::GroupExpr(expr) => {
            env.record_parenthesized_expression(loc, expr.loc().dupe());
            expr
        }
        GroupCover::GroupTypecast(cast) => expression::Expression::new(ExpressionInner::TypeCast {
            loc,
            inner: Arc::new(cast),
        }),
    };
    Ok(add_comments(ret, Some(leading), Some(trailing)))
}

fn add_comments(
    expression: expression::Expression<Loc, Loc>,
    leading: Option<Vec<Comment<Loc>>>,
    trailing: Option<Vec<Comment<Loc>>>,
) -> expression::Expression<Loc, Loc> {
    fn merge_comments(
        inner: Option<Syntax<Loc, ()>>,
        leading: Option<Arc<[Comment<Loc>]>>,
        trailing: Option<Arc<[Comment<Loc>]>>,
    ) -> Option<Syntax<Loc, ()>> {
        ast_utils::merge_comments(inner, mk_comments_opt(leading, trailing))
    }

    fn merge_comments_with_internal(
        inner: Option<Syntax<Loc, Arc<[Comment<Loc>]>>>,
        leading: Option<Arc<[Comment<Loc>]>>,
        trailing: Option<Arc<[Comment<Loc>]>>,
    ) -> Option<Syntax<Loc, Arc<[Comment<Loc>]>>> {
        ast_utils::merge_comments_with_internal(inner, mk_comments_opt(leading, trailing))
    }

    let leading: Option<Arc<[Comment<Loc>]>> = leading.map(Arc::from);
    let trailing: Option<Arc<[Comment<Loc>]>> = trailing.map(Arc::from);

    match &*expression {
        ExpressionInner::Array { loc, inner } => {
            let mut arr = (**inner).clone();
            arr.comments = merge_comments_with_internal(arr.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::Array {
                loc: loc.dupe(),
                inner: Arc::new(arr),
            })
        }
        ExpressionInner::ArrowFunction { loc, inner } => {
            let mut func = (**inner).clone();
            func.comments = merge_comments(func.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::ArrowFunction {
                loc: loc.dupe(),
                inner: Arc::new(func),
            })
        }
        ExpressionInner::AsExpression { loc, inner } => {
            let mut as_expr = (**inner).clone();
            as_expr.comments = merge_comments(as_expr.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::AsExpression {
                loc: loc.dupe(),
                inner: Arc::new(as_expr),
            })
        }
        ExpressionInner::AsConstExpression { loc, inner } => {
            let mut as_const = (**inner).clone();
            as_const.comments = merge_comments(as_const.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::AsConstExpression {
                loc: loc.dupe(),
                inner: Arc::new(as_const),
            })
        }
        ExpressionInner::Assignment { loc, inner } => {
            let mut assign = (**inner).clone();
            assign.comments = merge_comments(assign.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::Assignment {
                loc: loc.dupe(),
                inner: Arc::new(assign),
            })
        }
        ExpressionInner::Binary { loc, inner } => {
            let mut bin = (**inner).clone();
            bin.comments = merge_comments(bin.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::Binary {
                loc: loc.dupe(),
                inner: Arc::new(bin),
            })
        }
        ExpressionInner::Call { loc, inner } => {
            let mut call = (**inner).clone();
            call.comments = merge_comments(call.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::Call {
                loc: loc.dupe(),
                inner: Arc::new(call),
            })
        }
        ExpressionInner::Class { loc, inner } => {
            let mut cls = (**inner).clone();
            cls.comments = merge_comments(cls.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::Class {
                loc: loc.dupe(),
                inner: Arc::new(cls),
            })
        }
        ExpressionInner::Conditional { loc, inner } => {
            let mut cond = (**inner).clone();
            cond.comments = merge_comments(cond.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::Conditional {
                loc: loc.dupe(),
                inner: Arc::new(cond),
            })
        }
        ExpressionInner::Function { loc, inner } => {
            let mut func = (**inner).clone();
            func.comments = merge_comments(func.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::Function {
                loc: loc.dupe(),
                inner: Arc::new(func),
            })
        }
        ExpressionInner::Identifier { loc, inner } => {
            let old_id = &**inner;
            let new_id = Identifier::new(IdentifierInner {
                loc: old_id.loc.dupe(),
                name: old_id.name.dupe(),
                comments: merge_comments(old_id.comments.dupe(), leading, trailing),
            });
            expression::Expression::new(ExpressionInner::Identifier {
                loc: loc.dupe(),
                inner: new_id,
            })
        }
        ExpressionInner::Import { loc, inner } => {
            let mut imp = (**inner).clone();
            imp.comments = merge_comments(imp.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::Import {
                loc: loc.dupe(),
                inner: Arc::new(imp),
            })
        }
        ExpressionInner::JSXElement { loc, inner } => {
            let mut jsx = (**inner).clone();
            jsx.comments = merge_comments(jsx.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::JSXElement {
                loc: loc.dupe(),
                inner: Arc::new(jsx),
            })
        }
        ExpressionInner::JSXFragment { loc, inner } => {
            let mut frag = (**inner).clone();
            frag.frag_comments = merge_comments(frag.frag_comments, leading, trailing);
            expression::Expression::new(ExpressionInner::JSXFragment {
                loc: loc.dupe(),
                inner: Arc::new(frag),
            })
        }
        ExpressionInner::StringLiteral { loc, inner } => {
            let mut lit = (**inner).clone();
            lit.comments = merge_comments(lit.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::StringLiteral {
                loc: loc.dupe(),
                inner: Arc::new(lit),
            })
        }
        ExpressionInner::BooleanLiteral { loc, inner } => {
            let mut lit = (**inner).clone();
            lit.comments = merge_comments(lit.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::BooleanLiteral {
                loc: loc.dupe(),
                inner: Arc::new(lit),
            })
        }
        ExpressionInner::NullLiteral { loc, inner } => {
            let comments = merge_comments((**inner).clone(), leading, trailing);
            expression::Expression::new(ExpressionInner::NullLiteral {
                loc: loc.dupe(),
                inner: Arc::new(comments),
            })
        }
        ExpressionInner::NumberLiteral { loc, inner } => {
            let mut lit = (**inner).clone();
            lit.comments = merge_comments(lit.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::NumberLiteral {
                loc: loc.dupe(),
                inner: Arc::new(lit),
            })
        }
        ExpressionInner::BigIntLiteral { loc, inner } => {
            let mut lit = (**inner).clone();
            lit.comments = merge_comments(lit.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::BigIntLiteral {
                loc: loc.dupe(),
                inner: Arc::new(lit),
            })
        }
        ExpressionInner::RegExpLiteral { loc, inner } => {
            let mut lit = (**inner).clone();
            lit.comments = merge_comments(lit.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::RegExpLiteral {
                loc: loc.dupe(),
                inner: Arc::new(lit),
            })
        }
        ExpressionInner::Match { loc, inner } => {
            let mut mtch = (**inner).clone();
            mtch.comments = merge_comments(mtch.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::Match {
                loc: loc.dupe(),
                inner: Arc::new(mtch),
            })
        }
        ExpressionInner::ModuleRefLiteral { loc, inner } => {
            let mut lit = (**inner).clone();
            lit.comments = merge_comments(lit.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::ModuleRefLiteral {
                loc: loc.dupe(),
                inner: Arc::new(lit),
            })
        }
        ExpressionInner::Logical { loc, inner } => {
            let mut log = (**inner).clone();
            log.comments = merge_comments(log.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::Logical {
                loc: loc.dupe(),
                inner: Arc::new(log),
            })
        }
        ExpressionInner::Member { loc, inner } => {
            let mut mem = (**inner).clone();
            mem.comments = merge_comments(mem.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::Member {
                loc: loc.dupe(),
                inner: Arc::new(mem),
            })
        }
        ExpressionInner::MetaProperty { loc, inner } => {
            let mut meta = (**inner).clone();
            meta.comments = merge_comments(meta.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::MetaProperty {
                loc: loc.dupe(),
                inner: Arc::new(meta),
            })
        }
        ExpressionInner::New { loc, inner } => {
            let mut new_expr = (**inner).clone();
            new_expr.comments = merge_comments(new_expr.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::New {
                loc: loc.dupe(),
                inner: Arc::new(new_expr),
            })
        }
        ExpressionInner::Object { loc, inner } => {
            let mut obj = (**inner).clone();
            obj.comments = merge_comments_with_internal(obj.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::Object {
                loc: loc.dupe(),
                inner: Arc::new(obj),
            })
        }
        ExpressionInner::OptionalCall { loc, inner } => {
            let mut opt_call = (**inner).clone();
            opt_call.call.comments = merge_comments(opt_call.call.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::OptionalCall {
                loc: loc.dupe(),
                inner: Arc::new(opt_call),
            })
        }
        ExpressionInner::OptionalMember { loc, inner } => {
            let mut opt_mem = (**inner).clone();
            opt_mem.member.comments = merge_comments(opt_mem.member.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::OptionalMember {
                loc: loc.dupe(),
                inner: Arc::new(opt_mem),
            })
        }
        ExpressionInner::Record { loc, inner } => {
            let mut rec = (**inner).clone();
            rec.comments = merge_comments(rec.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::Record {
                loc: loc.dupe(),
                inner: Arc::new(rec),
            })
        }
        ExpressionInner::Sequence { loc, inner } => {
            let mut seq = (**inner).clone();
            seq.comments = merge_comments(seq.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::Sequence {
                loc: loc.dupe(),
                inner: Arc::new(seq),
            })
        }
        ExpressionInner::Super { loc, inner } => {
            let mut sup = (**inner).clone();
            sup.comments = merge_comments(sup.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::Super {
                loc: loc.dupe(),
                inner: Arc::new(sup),
            })
        }
        ExpressionInner::TaggedTemplate { loc, inner } => {
            let mut tmpl = (**inner).clone();
            tmpl.comments = merge_comments(tmpl.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::TaggedTemplate {
                loc: loc.dupe(),
                inner: Arc::new(tmpl),
            })
        }
        ExpressionInner::TemplateLiteral { loc, inner } => {
            let mut lit = (**inner).clone();
            lit.comments = merge_comments(lit.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::TemplateLiteral {
                loc: loc.dupe(),
                inner: Arc::new(lit),
            })
        }
        ExpressionInner::This { loc, inner } => {
            let mut this = (**inner).clone();
            this.comments = merge_comments(this.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::This {
                loc: loc.dupe(),
                inner: Arc::new(this),
            })
        }
        ExpressionInner::TSSatisfies { loc, inner } => {
            let mut sat = (**inner).clone();
            sat.comments = merge_comments(sat.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::TSSatisfies {
                loc: loc.dupe(),
                inner: Arc::new(sat),
            })
        }
        ExpressionInner::TypeCast { loc, inner } => {
            let mut cast = (**inner).clone();
            cast.comments = merge_comments(cast.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::TypeCast {
                loc: loc.dupe(),
                inner: Arc::new(cast),
            })
        }
        ExpressionInner::Unary { loc, inner } => {
            let mut un = (**inner).clone();
            un.comments = merge_comments(un.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::Unary {
                loc: loc.dupe(),
                inner: Arc::new(un),
            })
        }
        ExpressionInner::Update { loc, inner } => {
            let mut upd = (**inner).clone();
            upd.comments = merge_comments(upd.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::Update {
                loc: loc.dupe(),
                inner: Arc::new(upd),
            })
        }
        ExpressionInner::Yield { loc, inner } => {
            let mut yld = (**inner).clone();
            yld.comments = merge_comments(yld.comments, leading, trailing);
            expression::Expression::new(ExpressionInner::Yield {
                loc: loc.dupe(),
                inner: Arc::new(yld),
            })
        }
    }
}

fn array_initializer(
    env: &mut ParserEnv,
) -> Result<(expression::Array<Loc, Loc>, PatternCoverErrors), Rollback> {
    fn elements(
        env: &mut ParserEnv,
    ) -> Result<
        (
            Vec<expression::ArrayElement<Loc, Loc>>,
            PatternCoverErrors,
            bool,
        ),
        Rollback,
    > {
        // `trailing` tracks whether the most recently consumed token before
        // the eventual T_RBRACKET was a T_COMMA without a following element —
        // i.e. whether the source had a trailing comma like `[1, 2,]`. Reset
        // to false whenever we add an element (since the comma was followed
        // by content); set to true when we consume a comma whose next peek
        // is T_RBRACKET. Mirrors OCaml expression_parser.ml.
        let mut elements = Vec::new();
        let mut errs = PatternCoverErrors::empty();
        let mut trailing = false;
        loop {
            match peek::token(env) {
                TokenKind::TEof | TokenKind::TRbracket => return Ok((elements, errs, trailing)),
                TokenKind::TComma => {
                    let loc = peek::loc(env).dupe();
                    eat::token(env)?;
                    elements.push(expression::ArrayElement::Hole(loc));
                    trailing = false;
                }
                TokenKind::TEllipsis => {
                    let leading = peek::comments(env);
                    let (loc, (argument, mut new_errs)) = with_loc(None, env, |env| {
                        eat::token(env)?;
                        match assignment_cover(env)? {
                            PatternCover::CoverExpr(argument) => {
                                Ok((argument, PatternCoverErrors::empty()))
                            }
                            PatternCover::CoverPatt(argument, new_errs) => Ok((argument, new_errs)),
                        }
                    })?;

                    let elem = expression::ArrayElement::Spread(expression::SpreadElement {
                        loc: loc.dupe(),
                        argument,
                        comments: mk_comments_opt(Some(leading.into()), None),
                    });

                    let is_last = peek::token(env) == &TokenKind::TRbracket;

                    // if this array is interpreted as a pattern, the spread becomes an AssignmentRestElement
                    // which must be the last element. We can easily error about additional elements since
                    // they will be in the element list, but a trailing elision, like `[...x,]`, is not part
                    // of the AST. so, keep track of the error so we can raise it if this is a pattern.
                    if !is_last && peek::array_rest_continuation_is_rbracket(env) {
                        new_errs
                            .if_patt
                            .push((loc, ParseError::ElementAfterRestElement));
                    }
                    if !is_last {
                        expect::token(env, TokenKind::TComma)?;
                        trailing = peek::token(env) == &TokenKind::TRbracket;
                    } else {
                        trailing = false;
                    }
                    elements.push(elem);
                    errs.append(new_errs);
                }
                _ => {
                    let (elem, new_errs) = match assignment_cover(env)? {
                        PatternCover::CoverExpr(elem) => (elem, PatternCoverErrors::empty()),
                        PatternCover::CoverPatt(elem, new_errs) => (elem, new_errs),
                    };
                    if peek::token(env) != &TokenKind::TRbracket {
                        expect::token(env, TokenKind::TComma)?;
                        trailing = peek::token(env) == &TokenKind::TRbracket;
                    } else {
                        trailing = false;
                    }
                    elements.push(expression::ArrayElement::Expression(elem));
                    errs.append(new_errs);
                }
            }
        }
    }

    let leading = peek::comments(env);
    expect::token(env, TokenKind::TLbracket)?;
    let (elems, errs, trailing_comma) = elements(env)?;
    let internal = peek::comments(env);
    expect::token(env, TokenKind::TRbracket)?;
    let trailing = eat::trailing_comments(env);

    Ok((
        expression::Array {
            elements: elems.into(),
            trailing_comma,
            comments: ast_utils::mk_comments_with_internal_opt(
                Some(leading.into()),
                Some(trailing.into()),
                Some(internal.into()),
            ),
        },
        errs,
    ))
}

fn regexp(env: &mut ParserEnv) -> Result<expression::Expression<Loc, Loc>, Rollback> {
    eat::push_lex_mode(env, LexMode::Regexp);
    let loc = peek::loc(env).dupe();
    let leading = peek::comments(env);
    let tkn = peek::token(env).clone();

    let (raw, pattern, raw_flags, trailing) = match tkn {
        TokenKind::TRegexp(_, pattern, flags) => {
            eat::token(env)?;
            let trailing = eat::trailing_comments(env);
            let raw = format!("/{}/{}", pattern, flags);
            (raw, pattern, flags, trailing)
        }
        _ => {
            env.error_unexpected(Some("a regular expression".to_owned()))?;
            (
                String::new(),
                FlowSmolStr::new_inline(""),
                FlowSmolStr::new_inline(""),
                Vec::new(),
            )
        }
    };

    eat::pop_lex_mode(env);

    // Mirror upstream Hermes: invalid regex flags are not a parse error.
    // The literal is syntactically well-formed; flag validation is left to
    // the JS [RegExp] constructor at adapter time. The wire format preserves
    // the raw source flags so consumers can reconstruct the original regex
    // text.
    let flags = raw_flags.as_str().to_owned();

    let comments = mk_comments_opt(Some(leading.into()), Some(trailing.into()));
    Ok(expression::Expression::new(
        ExpressionInner::RegExpLiteral {
            loc,
            inner: Arc::new(RegExpLiteral {
                pattern,
                flags,
                raw,
                comments,
            }),
        },
    ))
}

fn try_arrow_function(env: &mut ParserEnv) -> Result<PatternCover, Rollback> {
    // Certain errors (almost all errors) cause a rollback
    fn error_callback(err: ParseError) -> Result<(), Rollback> {
        match err {
            // Don't rollback on these errors
            ParseError::StrictParamDupe
            | ParseError::StrictParamName
            | ParseError::StrictReservedWord
            | ParseError::ParameterAfterRestParameter
            | ParseError::NewlineBeforeArrow
            | ParseError::AwaitAsIdentifierReference
            | ParseError::AwaitInAsyncFormalParameters
            | ParseError::YieldInFormalParameters
            | ParseError::ThisParamBannedInArrowFunctions => Ok(()),
            // Everything else causes a rollback
            _ => Err(Rollback),
        }
    }

    fn concise_function_body(
        env: &mut ParserEnv,
    ) -> Result<(function::Body<Loc, Loc>, bool), Rollback> {
        match peek::token(env) {
            TokenKind::TLcurly => {
                let (body_block, contains_use_strcit) =
                    main_parser::parse_function_block_body(env, true)?;
                Ok((function::Body::BodyBlock(body_block), contains_use_strcit))
            }
            _ => {
                let expr = assignment(env)?;
                Ok((function::Body::BodyExpression(expr), false))
            }
        }
    }

    env.with_error_callback(error_callback, |env| {
        let start_loc = peek::loc(env).dupe();

        // a T_ASYNC could either be a parameter name or it could be indicating
        // that it's an async function
        let (async_flag, leading) = if !peek::async_arrow_parameter_continuation_is_arrow(env) {
            declaration_parser::parse_async(env)?
        } else {
            (false, Vec::new())
        };

        // await is a keyword if this is an async function, or if we're in one already
        let allow_await = async_flag || env.allow_await();
        env.with_allow_await(allow_await, |env| {
            let allow_yield = env.allow_yield();

            let (sig_loc, (tparams, mut params, return_annot, predicate)) =
                with_loc(None, env, |env| {
                    let mut tparams = type_parser::parse_type_params(env)?;
                    comment_attachment::type_params_remove_trailing(env, tparams.as_mut());

                    // Disallow all fancy features for identifier => body
                    if peek::is_identifier(env) && tparams.is_none() {
                        let name =
                            main_parser::parse_identifier(env, Some(ParseError::StrictParamName))?;
                        let loc = name.loc.dupe();
                        let param = function::Param::RegularParam {
                            loc: loc.dupe(),
                            argument: pattern::Pattern::Identifier {
                                loc: loc.dupe(),
                                inner: Arc::new(pattern::Identifier {
                                    name,
                                    annot: types::AnnotationOrHint::Missing(
                                        peek::loc_skip_lookahead(env),
                                    ),
                                    optional: false,
                                }),
                            },
                            default: None,
                        };

                        let return_annot = function::ReturnAnnot::Missing(Loc {
                            start: loc.end,
                            ..loc.dupe()
                        });
                        let params = function::Params {
                            loc,
                            this_: None,
                            params: vec![param].into(),
                            rest: None,
                            comments: None,
                        };

                        Ok((tparams, params, return_annot, None))
                    } else {
                        let params = declaration_parser::parse_function_params(
                            env,
                            allow_await,
                            allow_yield,
                        )?;
                        declaration_parser::check_unique_formal_parameters(env, &params)?;

                        // There's an ambiguity if you use a function type as the return
                        // type for an arrow function. So we disallow anonymous function
                        // types in arrow function return types unless the function type is
                        // enclosed in parens
                        let (return_annot, predicate) = env.with_no_anon_function_type(
                            true,
                            type_parser::parse_function_return_annotation_and_predicate_opt,
                        )?;
                        Ok((tparams, params, return_annot, predicate))
                    }
                })?;

            // It's hard to tell if an invalid expression was intended to be an
            // arrow function before we see the =>. If there are no params, that
            // implies "()" which is only ever found in arrow params. Similarly,
            // rest params indicate arrow functions. Therefore, if we see a rest
            // param or an empty param list then we can disable the rollback and
            // instead generate errors as if we were parsing an arrow function
            match &params {
                function::Params {
                    loc: _,
                    params: _,
                    rest: Some(_),
                    this_: None,
                    comments: _,
                } => env.without_error_callback(),
                function::Params {
                    loc: _,
                    params,
                    rest: _,
                    this_: None,
                    comments: _,
                } if params.is_empty() => env.without_error_callback(),
                _ => {}
            }

            // Disallow this param annotations in arrow functions
            match params.this_.as_ref() {
                Some(this_) => {
                    let this_loc = this_.loc.dupe();
                    env.error_at(this_loc, ParseError::ThisParamBannedInArrowFunctions)?;
                    params.this_ = None;
                }
                None => {}
            };
            let simple_params = is_simple_parameter_list(&params);

            if peek::is_line_terminator(env) && peek::token(env) == &TokenKind::TArrow {
                env.error(ParseError::NewlineBeforeArrow)?;
            }
            expect::token(env, TokenKind::TArrow)?;

            // Now we know for sure this is an arrow function
            env.without_error_callback();
            // arrow functions can't be generators
            env.enter_function(async_flag, false, simple_params, |env| {
                let (end_loc, (body, contains_use_strict)) =
                    with_loc(Some(start_loc.dupe()), env, concise_function_body)?;
                declaration_parser::strict_function_post_check(
                    env,
                    contains_use_strict,
                    None,
                    &params,
                )?;

                let loc = Loc::between(&start_loc, &end_loc);
                Ok(PatternCover::CoverExpr(expression::Expression::new(
                    ExpressionInner::ArrowFunction {
                        loc,
                        inner: Arc::new(function::Function {
                            id: None,
                            params,
                            body,
                            // arrow functions cannot be generators
                            generator: false,
                            effect_: function::Effect::Arbitrary,
                            async_: async_flag,
                            predicate,
                            return_: return_annot,
                            tparams,
                            sig_loc,
                            comments: mk_comments_opt(Some(leading.into()), None),
                        }),
                    },
                )))
            })
        })
    })
}

pub(super) fn sequence(
    env: &mut ParserEnv,
    start_loc: Loc,
    mut expressions: Vec<expression::Expression<Loc, Loc>>,
) -> Result<expression::Expression<Loc, Loc>, Rollback> {
    let (loc, seq) = with_loc(Some(start_loc), env, |env| {
        loop {
            match peek::token(env) {
                TokenKind::TComma => {
                    eat::token(env)?;
                    let expr = assignment(env)?;
                    expressions.push(expr);
                }
                _ => {
                    return Ok(expression::Sequence {
                        expressions: expressions.into(),
                        comments: None,
                    });
                }
            }
        }
    })?;
    Ok(expression::Expression::new(ExpressionInner::Sequence {
        loc,
        inner: Arc::new(seq),
    }))
}
