/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::fmt::Write;
use std::ops::Deref;
use std::sync::Arc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_common::js_number::shortest_string_of_float;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::expression::ExpressionInner;
use flow_parser::ast_utils::ident_of_source;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::comment_attachment;
use flow_parser::comment_attachment::CommentsBounds;
use flow_parser::loc::LOC_NONE;
use flow_parser::loc::Loc;

use crate::layout;
use crate::layout::LayoutNode;
use crate::layout::flat_ugly_space;
use crate::layout::fuse;
use crate::layout::fuse_with_space;
use crate::layout::group;
use crate::layout::hardline;
use crate::layout::if_break;
use crate::layout::if_pretty;
use crate::layout::join;
use crate::layout::line;
use crate::layout::new_list;
use crate::layout::pretty_hardline;
use crate::layout::pretty_line;
use crate::layout::pretty_space;
use crate::layout::softline;
use crate::layout::space;
use crate::layout::ugly_space;
use crate::layout::wrap_and_indent;

fn atom(s: &str) -> LayoutNode {
    LayoutNode::atom(s.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrailingCommas {
    /// Wherever possible (including function arguments).
    All,
    /// Where valid in ES5 (objects, arrays, etc.)
    ES5,
    /// No trailing commas
    Off,
}

impl TrailingCommas {
    pub fn enabled_for_function_params(self) -> bool {
        match self {
            TrailingCommas::All => true,
            TrailingCommas::ES5 => false,
            TrailingCommas::Off => false,
        }
    }

    pub fn enabled_for_function_args(self) -> bool {
        match self {
            TrailingCommas::All => true,
            TrailingCommas::ES5 => true,
            TrailingCommas::Off => false,
        }
    }

    pub fn enabled_for_objects(self) -> bool {
        match self {
            TrailingCommas::All => true,
            TrailingCommas::ES5 => true,
            TrailingCommas::Off => false,
        }
    }

    pub fn enabled_for_arrays(self) -> bool {
        match self {
            TrailingCommas::All => true,
            TrailingCommas::ES5 => true,
            TrailingCommas::Off => false,
        }
    }

    pub fn enabled_for_types(self) -> bool {
        match self {
            TrailingCommas::All => true,
            TrailingCommas::ES5 => true,
            TrailingCommas::Off => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Opts {
    pub bracket_spacing: bool,
    pub preserve_formatting: bool,
    pub single_quotes: bool,
    pub trailing_commas: TrailingCommas,
}

// There are some cases where expressions must be wrapped in parens to eliminate
//    ambiguity. We pass whether we're in one of these special cases down through
//    the tree as we generate the layout. Note that these are only necessary as
//    long as the ambiguity can exist, so emitting any wrapper (like a paren or
// bracket) is enough to reset the context back to Normal.
#[derive(Debug, Clone, Copy)]
pub struct ExpressionContext {
    pub left: ExpressionContextLeft,
    pub group: ExpressionContextGroup,
}

// certain contexts where the left parenthesis matters. this is set back to Normal_left
// as soon as we output something (see context_after_token).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpressionContextLeft {
    NormalLeft,
    /// (function x(){});  would become a declaration w/o the paren
    InExpressionStatement,
    /// (new a)`` would become [new (a``)] w/o the paren
    InTaggedTemplate,
    /// x+(+y)  would become [(x++)y] w/o the paren
    InPlusOp,
    /// x-(-y)  would become [(x--)y] w/o the paren
    InMinusOp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpressionContextGroup {
    NormalGroup,
    /// () => ({a: b})  would be a block w/ a labeled statement w/o the parens
    InArrowFunc,
    /// for ((x in y);;);  would become a for-in w/o the parens
    InForInit,
}

pub fn default_opts() -> Opts {
    Opts {
        bracket_spacing: true,
        preserve_formatting: false,
        single_quotes: false,
        trailing_commas: TrailingCommas::All,
    }
}

pub(crate) const NORMAL_CONTEXT: ExpressionContext = ExpressionContext {
    left: ExpressionContextLeft::NormalLeft,
    group: ExpressionContextGroup::NormalGroup,
};

// Some contexts only matter to the left-most token. If we output some other
// token, like an `=`, then we can reset the context. Note that all contexts
// reset when wrapped in parens, brackets, braces, etc, so we don't need to call
// this in those cases, we can just set it back to Normal.
fn context_after_token(ctxt: &ExpressionContext) -> ExpressionContext {
    ExpressionContext {
        left: ExpressionContextLeft::NormalLeft,
        ..*ctxt
    }
}

// JS layout helpers
fn with_semicolon(node: LayoutNode) -> LayoutNode {
    fuse(vec![node, LayoutNode::atom(";".to_string())])
}

fn with_pretty_semicolon(node: LayoutNode) -> LayoutNode {
    fuse(vec![
        node,
        if_pretty(LayoutNode::atom(";".to_string()), LayoutNode::empty()),
    ])
}

fn wrap_in_parens(with_break: bool, item: LayoutNode) -> LayoutNode {
    if with_break {
        group(vec![wrap_and_indent(
            (
                LayoutNode::atom("(".to_string()),
                LayoutNode::atom(")".to_string()),
            ),
            Some(pretty_hardline()),
            vec![item],
        )])
    } else {
        group(vec![
            LayoutNode::atom("(".to_string()),
            item,
            LayoutNode::atom(")".to_string()),
        ])
    }
}

fn wrap_in_parens_on_break(item: LayoutNode) -> LayoutNode {
    group(vec![wrap_and_indent(
        (
            if_break(LayoutNode::atom("(".to_string()), LayoutNode::empty()),
            if_break(LayoutNode::atom(")".to_string()), LayoutNode::empty()),
        ),
        None,
        vec![item],
    )])
}

fn option_layout<T, F: FnOnce(T) -> LayoutNode>(f: F, opt: Option<T>) -> LayoutNode {
    match opt {
        Some(v) => f(v),
        None => LayoutNode::empty(),
    }
}

fn hint<F: FnOnce(&ast::types::Annotation<Loc, Loc>) -> LayoutNode>(
    f: F,
    annot: &ast::types::AnnotationOrHint<Loc, Loc>,
) -> LayoutNode {
    match annot {
        ast::types::AnnotationOrHint::Available(v) => f(v),
        ast::types::AnnotationOrHint::Missing(_) => LayoutNode::empty(),
    }
}

fn deoptionalize<T>(l: Vec<Option<T>>) -> Vec<T> {
    l.into_iter().flatten().collect()
}

// See https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/Operator_Precedence
const MAX_PRECEDENCE: i32 = 22;

const MIN_PRECEDENCE: i32 = 1;

// 0 means always parenthesize, which is not a precedence decision

const PRECEDENCE_OF_ASSIGNMENT: i32 = 3;

fn precedence_of_expression(expr: &ast::expression::Expression<Loc, Loc>) -> i32 {
    match expr.deref() {
        // Expressions that don't involve operators have the highest priority
        ExpressionInner::Array { .. }
        | ExpressionInner::Class { .. }
        | ExpressionInner::Function { .. }
        | ExpressionInner::Identifier { .. }
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
        | ExpressionInner::Object { .. }
        | ExpressionInner::Record { .. }
        | ExpressionInner::Super { .. }
        | ExpressionInner::TemplateLiteral { .. }
        | ExpressionInner::This { .. }
        | ExpressionInner::TypeCast { .. } => MAX_PRECEDENCE,
        // Expressions involving operators
        ExpressionInner::Member { .. }
        | ExpressionInner::OptionalMember { .. }
        | ExpressionInner::MetaProperty { .. }
        | ExpressionInner::Call { .. }
        | ExpressionInner::OptionalCall { .. } => 21,
        ExpressionInner::New { inner, .. } => {
            if inner.arguments.is_some() {
                21
            } else {
                20
            }
        }
        ExpressionInner::TaggedTemplate { .. } | ExpressionInner::Import { .. } => 19,
        ExpressionInner::Update { inner, .. } => {
            if !inner.prefix {
                18
            } else {
                17
            }
        }
        ExpressionInner::Unary { .. } => 17,
        ExpressionInner::AsExpression { .. }
        | ExpressionInner::AsConstExpression { .. }
        | ExpressionInner::TSSatisfies { .. } => 12,
        ExpressionInner::Binary { inner, .. } => {
            use ast::expression::BinaryOperator::*;
            match inner.operator {
                Exp => 16,
                Mult | Div | Mod => 15,
                Plus | Minus => 14,
                LShift | RShift | RShift3 => 13,
                LessThan | LessThanEqual | GreaterThan | GreaterThanEqual | In | Instanceof => 12,
                Equal | NotEqual | StrictEqual | StrictNotEqual => 11,
                BitAnd => 10,
                Xor => 9,
                BitOr => 8,
            }
        }
        ExpressionInner::Logical { inner, .. } => match inner.operator {
            ast::expression::LogicalOperator::And => 7,
            ast::expression::LogicalOperator::Or => 6,
            ast::expression::LogicalOperator::NullishCoalesce => 5,
        },
        ExpressionInner::Conditional { .. } => 4,
        ExpressionInner::Assignment { .. } => PRECEDENCE_OF_ASSIGNMENT,
        // must be >= Assignment, so that `x = () => y = 123` doesn't need parens. It must be lower
        // than a Conditional so that `(() => {}) ? 1 : 2` doesn't become `() => ({} ? 1 : 2)`
        ExpressionInner::ArrowFunction { .. } => PRECEDENCE_OF_ASSIGNMENT,
        ExpressionInner::Yield { .. } => 2,
        ExpressionInner::Sequence { .. } => 0,
    }
}

fn context_needs_parens(
    ctxt: ExpressionContext,
    expr: &ast::expression::Expression<Loc, Loc>,
) -> bool {
    match ctxt {
        // an object body expression in an arrow function needs parens to not
        // make it become a block with label statement.
        ExpressionContext {
            group: ExpressionContextGroup::InArrowFunc,
            ..
        } => matches!(expr.deref(), ExpressionInner::Object { .. }),
        // an `in` binary expression in the init of a for loop needs parens to not
        // make the for loop become a for-in loop.
        ExpressionContext {
            group: ExpressionContextGroup::InForInit,
            ..
        } => matches!(
            expr.deref(),
            ExpressionInner::Binary { inner, .. } if inner.operator == ast::expression::BinaryOperator::In
        ),
        // functions (including async functions, but not arrow functions) and
        //          classes must be wrapped in parens to avoid ambiguity with function and
        ExpressionContext {
            left: ExpressionContextLeft::InExpressionStatement,
            ..
        } => match expr.deref() {
            ExpressionInner::Class { .. }
            | ExpressionInner::Function { .. }
            | ExpressionInner::Object { .. } => true,
            ExpressionInner::Assignment { inner, .. } => {
                matches!(inner.left, ast::pattern::Pattern::Object { .. })
            }
            _ => false,
        },
        ExpressionContext {
            left: ExpressionContextLeft::InTaggedTemplate,
            ..
        } => matches!(
            expr.deref(),
            ExpressionInner::Class { .. }
                | ExpressionInner::Function { .. }
                | ExpressionInner::New { .. }
                | ExpressionInner::Import { .. }
                | ExpressionInner::Object { .. }
        ),
        ExpressionContext {
            left: ExpressionContextLeft::InMinusOp,
            ..
        } => match expr.deref() {
            ExpressionInner::Unary { inner, .. }
                if inner.operator == ast::expression::UnaryOperator::Minus =>
            {
                true
            }
            ExpressionInner::Update { inner, .. }
                if inner.operator == ast::expression::UpdateOperator::Decrement && inner.prefix =>
            {
                true
            }
            _ => false,
        },
        ExpressionContext {
            left: ExpressionContextLeft::InPlusOp,
            ..
        } => match expr.deref() {
            ExpressionInner::Unary { inner, .. }
                if inner.operator == ast::expression::UnaryOperator::Plus =>
            {
                true
            }
            ExpressionInner::Update { inner, .. }
                if inner.operator == ast::expression::UpdateOperator::Increment && inner.prefix =>
            {
                true
            }
            _ => false,
        },
        ExpressionContext {
            left: ExpressionContextLeft::NormalLeft,
            group: ExpressionContextGroup::NormalGroup,
        } => false,
    }
}

fn definitely_needs_parens(
    precedence: i32,
    ctxt: ExpressionContext,
    expr: &ast::expression::Expression<Loc, Loc>,
) -> bool {
    precedence_of_expression(expr) < precedence || context_needs_parens(ctxt, expr)
}

// TODO: this only needs to be shallow; we don't need to walk into function
// or class bodies, for example.

// TODO: use a fold
fn contains_call_expression(expr: &ast::expression::Expression<Loc, Loc>) -> bool {
    match expr.deref() {
        ExpressionInner::Call { .. } => true,
        ExpressionInner::OptionalCall { .. } => true,
        ExpressionInner::Member { inner, .. } => contains_call_expression(&inner.object),
        ExpressionInner::OptionalMember { inner, .. } => {
            contains_call_expression(&inner.member.object)
        }
        _ => false,
    }
}

#[allow(dead_code)]
enum StatementOrComment<'a> {
    Statement(&'a ast::statement::Statement<Loc, Loc>),
    Comment(&'a ast::Comment<Loc>),
}

fn better_quote(prefer_single_quotes: bool, str: &str) -> &'static str {
    let (mut double_count, mut single_count) = (0usize, 0usize);
    for c in str.chars() {
        match c {
            '"' => double_count += 1,
            '\'' => single_count += 1,
            _ => {}
        }
    }
    let double = ("\"", double_count);
    let single = ("'", single_count);
    let (preferred, alternate) = if prefer_single_quotes {
        (single, double)
    } else {
        (double, single)
    };
    if preferred.1 > alternate.1 {
        alternate.0
    } else {
        preferred.0
    }
}

fn utf8_escape(quote: &str, str: &str) -> String {
    // a null character can be printed as \x00 or \0. but if the next character is an ASCII digit,
    // then using \0 would create \01 (for example), which is a legacy octal 1. so, rather than simply
    // fold over the codepoints, we have to look ahead at the next character as well.
    let mut buf = String::with_capacity(str.len());
    let mut chars = str.chars().peekable();
    while let Some(c) = chars.next() {
        let cp = c as u32;
        let next_cp = chars.peek().map(|&nc| nc as u32);
        match cp {
            // SingleEscapeCharacter: http://www.ecma-international.org/ecma-262/6.0/#table-34
            0x0 => {
                let zero = match next_cp {
                    Some(n) if (0x30..=0x39).contains(&n) => "\\x00",
                    _ => "\\0",
                };
                buf.push_str(zero);
            }
            0x8 => {
                buf.push_str("\\b");
            }
            0x9 => {
                buf.push_str("\\t");
            }
            0xA => {
                buf.push_str("\\n");
            }
            0xB => {
                buf.push_str("\\v");
            }
            0xC => {
                buf.push_str("\\f");
            }
            0xD => {
                buf.push_str("\\r");
            }
            0x22 if quote == "\"" => {
                buf.push_str("\\\"");
            }
            0x27 if quote == "'" => {
                buf.push_str("\\'");
            }
            0x5C => {
                buf.push_str("\\\\");
            }
            // printable ascii
            n if 0x1F < n && n < 0x7F => {
                buf.push(c);
            }
            // basic multilingual plane, 2 digits
            n if n < 0x100 => {
                write!(buf, "\\x{:02x}", n).unwrap();
            }
            // basic multilingual plane, 4 digits
            n if n < 0x10000 => {
                write!(buf, "\\u{:04x}", n).unwrap();
            }
            // supplemental planes
            n => {
                // ES5 does not support the \u{} syntax, so print surrogate pairs
                // "\ud83d\udca9" instead of "\u{1f4A9}". if we add a flag to target
                // ES6, we should change this.
                let n2 = n - 0x10000;
                let hi = 0xD800 | (n2 >> 10);
                let lo = 0xDC00 | (n2 & 0x3FF);
                write!(buf, "\\u{:4x}", hi).unwrap();
                write!(buf, "\\u{:4x}", lo).unwrap();
            }
        }
    }
    buf
}

pub fn quote_string(prefer_single_quotes: bool, value: &str) -> String {
    let quote = better_quote(prefer_single_quotes, value);
    format!("{}{}{}", quote, utf8_escape(quote, value), quote)
}

fn is_single_linebreak(prev: &Loc, next: &Loc) -> bool {
    next.start.line == prev.end.line + 1
}

fn is_multi_linebreak(prev: &Loc, next: &Loc) -> bool {
    next.start.line > prev.end.line + 1
}

fn layout_comment(comment: &ast::Comment<Loc>) -> LayoutNode {
    LayoutNode::source_location(
        flow_aloc::ALoc::of_loc(comment.loc.dupe()),
        match comment.kind {
            ast::CommentKind::Line => fuse(vec![
                LayoutNode::atom("//".to_string()),
                LayoutNode::atom(comment.text.to_string()),
                hardline(),
            ]),
            ast::CommentKind::Block => fuse(vec![
                LayoutNode::atom("/*".to_string()),
                LayoutNode::atom(comment.text.to_string()),
                LayoutNode::atom("*/".to_string()),
            ]),
        },
    )
}

fn layout_from_leading_comment(comment: &ast::Comment<Loc>, next_loc: Option<&Loc>) -> LayoutNode {
    let lc = layout_comment(comment);
    match next_loc {
        None => lc,
        Some(next_loc) => {
            let is_single_lb = is_single_linebreak(&comment.loc, next_loc);
            let is_multi_lb = is_multi_linebreak(&comment.loc, next_loc);
            match comment.kind {
                ast::CommentKind::Line if is_multi_lb => fuse(vec![lc, pretty_hardline()]),
                ast::CommentKind::Line => lc,
                ast::CommentKind::Block if is_multi_lb => {
                    fuse(vec![lc, pretty_hardline(), pretty_hardline()])
                }
                ast::CommentKind::Block if is_single_lb => fuse(vec![lc, pretty_hardline()]),
                ast::CommentKind::Block => fuse(vec![lc, pretty_space()]),
            }
        }
    }
}

fn layout_from_trailing_comment(
    comment: &ast::Comment<Loc>,
    prev_loc: &Loc,
    prev_comment_kind: Option<ast::CommentKind>,
) -> LayoutNode {
    let is_single_lb = is_single_linebreak(prev_loc, &comment.loc);
    let is_multi_lb = is_multi_linebreak(prev_loc, &comment.loc);
    let lc = layout_comment(comment);
    match prev_comment_kind {
        Some(ast::CommentKind::Line) if is_multi_lb => fuse(vec![pretty_hardline(), lc]),
        Some(ast::CommentKind::Line) => lc,
        Some(ast::CommentKind::Block) if is_multi_lb => {
            fuse(vec![pretty_hardline(), pretty_hardline(), lc])
        }
        Some(ast::CommentKind::Block) if is_single_lb => fuse(vec![pretty_hardline(), lc]),
        Some(ast::CommentKind::Block) => fuse(vec![pretty_space(), lc]),
        None if is_multi_lb => fuse(vec![pretty_hardline(), pretty_hardline(), lc]),
        None if is_single_lb => fuse(vec![pretty_hardline(), lc]),
        None => fuse(vec![pretty_space(), lc]),
    }
}

fn layout_from_leading_comments(
    comments: &[ast::Comment<Loc>],
    node_loc: Option<&Loc>,
) -> Vec<LayoutNode> {
    fn inner(comments: &[ast::Comment<Loc>], node_loc: Option<&Loc>) -> Vec<LayoutNode> {
        match comments {
            [] => vec![],
            [comment] => vec![layout_from_leading_comment(comment, node_loc)],
            [comment, rest @ ..] => {
                let next_loc = &rest[0].loc;
                let mut result = vec![layout_from_leading_comment(comment, Some(next_loc))];
                result.extend(inner(rest, node_loc));
                result
            }
        }
    }
    inner(comments, node_loc)
}

fn layout_from_trailing_comments(
    comments: &[ast::Comment<Loc>],
    node_loc: &Loc,
) -> Vec<LayoutNode> {
    fn inner(
        comments: &[ast::Comment<Loc>],
        prev_loc: &Loc,
        prev_comment_kind: Option<ast::CommentKind>,
    ) -> Vec<LayoutNode> {
        match comments {
            [] => vec![],
            [comment, rest @ ..] => {
                let mut result = vec![layout_from_trailing_comment(
                    comment,
                    prev_loc,
                    prev_comment_kind,
                )];
                result.extend(inner(rest, &comment.loc, Some(comment.kind)));
                result
            }
        }
    }
    inner(comments, node_loc, None)
}

fn layout_node_with_comments<I: Dupe>(
    current_loc: &Loc,
    comments: &ast::Syntax<Loc, I>,
    layout_node: LayoutNode,
) -> LayoutNode {
    let preceding = layout_from_leading_comments(&comments.leading, Some(current_loc));
    let following = layout_from_trailing_comments(&comments.trailing, current_loc);
    let mut nodes = preceding;
    nodes.push(layout_node);
    nodes.extend(following);
    LayoutNode::concat(nodes)
}

fn layout_node_with_comments_opt<I: Dupe>(
    current_loc: &Loc,
    comments: Option<&ast::Syntax<Loc, I>>,
    layout_node: LayoutNode,
) -> LayoutNode {
    match comments {
        Some(c) => layout_node_with_comments(current_loc, c, layout_node),
        None => layout_node,
    }
}

fn source_location_with_comments<I: Dupe>(
    current_loc: &Loc,
    comments: Option<&ast::Syntax<Loc, I>>,
    layout_node: LayoutNode,
) -> LayoutNode {
    let layout =
        LayoutNode::source_location(flow_aloc::ALoc::of_loc(current_loc.dupe()), layout_node);
    match comments {
        Some(c) => layout_node_with_comments(current_loc, c, layout),
        None => layout,
    }
}

fn internal_comments(
    comments: Option<&ast::Syntax<Loc, Arc<[ast::Comment<Loc>]>>>,
) -> Option<(Loc, CommentsBounds, LayoutNode)> {
    match comments {
        None => None,
        Some(c) if c.internal.is_empty() => None,
        Some(c) => {
            let internal: &[ast::Comment<Loc>] = &c.internal;
            let first_loc = internal[0].loc.dupe();
            Some((
                first_loc,
                CommentsBounds {
                    first_leading: None,
                    last_trailing: None,
                },
                LayoutNode::concat(layout_from_leading_comments(internal, None)),
            ))
        }
    }
}

// Given a set of comment bounds, determine what separator should be inserted before the
//    comments to preserve whether the first comment starts on a new line.
//
//    - If there are no leading comments return the specified separator.
//    - If the first leading comment starts on a new line, ignore the specified separator and
//      return a hard line break.
//    - If the first leading comment does not start on a new line, return the optional same_line
// separator, otherwise return the no_comment separator.
fn comment_aware_separator(
    same_line: Option<LayoutNode>,
    no_comment: LayoutNode,
    comment_bounds: &CommentsBounds,
) -> LayoutNode {
    match comment_bounds.first_leading {
        Some((_, true)) => pretty_hardline(),
        Some((_, false)) => match same_line {
            None => no_comment,
            Some(sep) => sep,
        },
        None => no_comment,
    }
}

fn identifier_with_comments(
    current_loc: &Loc,
    name: &str,
    comments: Option<&ast::Syntax<Loc, ()>>,
) -> LayoutNode {
    let node = LayoutNode::identifier(flow_aloc::ALoc::of_loc(current_loc.dupe()), name.into());
    match comments {
        Some(c) => layout_node_with_comments(current_loc, c, node),
        None => node,
    }
}

// Generate JS layouts

fn identifier(ident: &ast::Identifier<Loc, Loc>) -> LayoutNode {
    identifier_with_comments(&ident.loc, &ident.name, ident.comments.as_ref())
}

// given a list of (Loc, CommentsBounds, LayoutNode) triples, insert newlines between the nodes
// when necessary
fn list_with_newlines(nodes: &[(Loc, CommentsBounds, LayoutNode)]) -> Vec<LayoutNode> {
    list_with_newlines_impl(LayoutNode::empty(), pretty_hardline(), true, nodes)
}

fn list_with_newlines_with_sep(
    sep: LayoutNode,
    sep_linebreak: LayoutNode,
    skip_empty: bool,
    nodes: &[(Loc, CommentsBounds, LayoutNode)],
) -> Vec<LayoutNode> {
    list_with_newlines_impl(sep, sep_linebreak, skip_empty, nodes)
}

fn list_with_newlines_impl(
    sep: LayoutNode,
    sep_linebreak: LayoutNode,
    skip_empty: bool,
    nodes: &[(Loc, CommentsBounds, LayoutNode)],
) -> Vec<LayoutNode> {
    fn helper(
        nodes: &[(Loc, CommentsBounds, LayoutNode)],
        acc: &mut Vec<LayoutNode>,
        prev_loc: Option<Loc>,
        sep: &LayoutNode,
        sep_linebreak: &LayoutNode,
        skip_empty: bool,
    ) {
        if nodes.is_empty() {
            return;
        }
        let (loc, comment_bounds, node) = &nodes[0];
        let rest = &nodes[1..];
        // Expand node's loc to include its attached comments
        let loc = comment_attachment::expand_loc_with_comment_bounds(loc, comment_bounds);
        let has_trailing_line_comment = matches!(
            comment_bounds.last_trailing,
            Some((_, ast::CommentKind::Line))
        );
        // Line comments already end in a hard newline, so another linebreak is not needed
        let sep_node = if rest.is_empty() {
            LayoutNode::empty()
        } else if has_trailing_line_comment {
            sep.dupe()
        } else {
            fuse(vec![sep.dupe(), sep_linebreak.dupe()])
        };
        match (&prev_loc, node) {
            // empty line, don't add anything
            (_, node) if node.is_empty() && skip_empty => {}
            // Location is empty, default to no line break
            (Some(ploc), node) if *ploc == LOC_NONE => {
                acc.push(fuse(vec![node.dupe(), sep_node.dupe()]));
            }
            // Lines are offset by more than one, let's add a line break.
            (Some(ploc), node) if ploc.end.line + 1 < loc.start.line => {
                acc.push(fuse(vec![pretty_hardline(), node.dupe(), sep_node.dupe()]));
            }
            // Hasn't matched, just add the node
            (_, node) => {
                acc.push(fuse(vec![node.dupe(), sep_node.dupe()]));
            }
        }
        helper(rest, acc, Some(loc), sep, sep_linebreak, skip_empty);
    }
    let mut result = Vec::new();
    helper(nodes, &mut result, None, &sep, &sep_linebreak, skip_empty);
    result
}

fn list_add_internal_comments(
    list: &[(Loc, CommentsBounds, LayoutNode)],
    mut list_layout: Vec<LayoutNode>,
    comments: Option<&ast::Syntax<Loc, Arc<[ast::Comment<Loc>]>>>,
) -> Vec<LayoutNode> {
    match internal_comments(comments) {
        None => list_layout,
        Some((comments_loc, _, comments_layout)) => {
            match list.last() {
                None => {
                    list_layout.push(comments_layout);
                    list_layout
                }
                Some((last_item_loc, _, _)) => {
                    // Preserve newlines between internal comments and last item in the list
                    if is_single_linebreak(last_item_loc, &comments_loc) {
                        list_layout.push(pretty_hardline());
                        list_layout.push(comments_layout);
                    } else if is_multi_linebreak(last_item_loc, &comments_loc) {
                        list_layout.push(pretty_hardline());
                        list_layout.push(pretty_hardline());
                        list_layout.push(comments_layout);
                    } else {
                        list_layout.push(comments_layout);
                    }
                    list_layout
                }
            }
        }
    }
}

pub(crate) fn statement_list(
    opts: &Opts,
    pretty_semicolon: bool,
    statements: &[ast::statement::Statement<Loc, Loc>],
) -> Vec<LayoutNode> {
    let mut acc: Vec<(Loc, CommentsBounds, LayoutNode)> = Vec::new();
    for (i, stmt) in statements.iter().enumerate() {
        let loc = stmt.loc().dupe();
        let ps = pretty_semicolon && i == statements.len() - 1;
        let comment_bounds = comment_attachment::statement_comment_bounds(stmt);
        acc.push((loc, comment_bounds, statement(opts, ps, stmt)));
    }
    list_with_newlines(&acc)
}

// The beginning of a statement that does a "test", like `if (test)` or `while (test)`
fn statement_with_test(name: &str, test: LayoutNode) -> LayoutNode {
    group(vec![
        LayoutNode::atom(name.to_string()),
        pretty_space(),
        wrap_and_indent(
            (
                LayoutNode::atom("(".to_string()),
                LayoutNode::atom(")".to_string()),
            ),
            None,
            vec![test],
        ),
    ])
}

// A statement following a "test", like the `statement` in `if (expr) statement` or
// `for (...) statement`. Better names for this are welcome!
fn statement_after_test(
    opts: &Opts,
    pretty_semicolon: bool,
    stmt: &ast::statement::Statement<Loc, Loc>,
) -> LayoutNode {
    use ast::statement::StatementInner;
    match stmt.deref() {
        StatementInner::Empty { .. } => statement(opts, pretty_semicolon, stmt),
        StatementInner::Block { .. } => fuse(vec![
            pretty_space(),
            statement(opts, pretty_semicolon, stmt),
        ]),
        _ => LayoutNode::indent(fuse(vec![
            pretty_line(),
            statement(opts, pretty_semicolon, stmt),
        ])),
    }
}

fn maybe_embed_checksum(nodes: LayoutNode, checksum: Option<&str>) -> LayoutNode {
    match checksum {
        Some(checksum) => {
            let comment = format!("/* {} */", checksum);
            fuse(vec![
                nodes,
                LayoutNode::newline(),
                LayoutNode::atom(comment),
            ])
        }
        None => nodes,
    }
}

pub(crate) fn comment_layout(replacement_for_same_type: bool, c: &ast::Comment<Loc>) -> LayoutNode {
    source_location_with_comments(
        &c.loc,
        None::<&ast::Syntax<Loc, ()>>,
        match c.kind {
            ast::CommentKind::Block => fuse(vec![
                LayoutNode::atom("/*".to_string()),
                LayoutNode::atom(c.text.to_string()),
                LayoutNode::atom("*/".to_string()),
            ]),
            ast::CommentKind::Line => {
                if replacement_for_same_type {
                    fuse(vec![
                        LayoutNode::atom("//".to_string()),
                        LayoutNode::atom(c.text.to_string()),
                    ])
                } else {
                    fuse(vec![
                        LayoutNode::atom("//".to_string()),
                        LayoutNode::atom(c.text.to_string()),
                        hardline(),
                    ])
                }
            }
        },
    )
}

pub fn program(
    opts: &Opts,
    preserve_docblock: bool,
    checksum: Option<&str>,
    prog: &ast::Program<Loc, Loc>,
) -> LayoutNode {
    let loc = &prog.loc;
    let nodes = if preserve_docblock && !prog.all_comments.is_empty() {
        let (directives, statements): (
            Vec<ast::statement::Statement<Loc, Loc>>,
            Vec<ast::statement::Statement<Loc, Loc>>,
        ) = flow_parser::ast_utils::partition_directives(prog.statements.iter().duped().collect());
        let all_comments: &[ast::Comment<Loc>] = match statements.first() {
            None => &prog.all_comments,
            Some(stmt) => {
                &prog.all_comments[..prog
                    .all_comments
                    .iter()
                    .position(|c| c.loc.start >= stmt.loc().start)
                    .unwrap_or(prog.all_comments.len())]
            }
        };
        vec![
            combine_directives_and_comments(opts, &directives, all_comments),
            fuse(statement_list(opts, false, &statements)),
        ]
    } else {
        vec![fuse(statement_list(opts, false, &prog.statements))]
    };
    let nodes = group(vec![join(pretty_hardline(), nodes)]);
    let nodes = maybe_embed_checksum(nodes, checksum);
    let loc = Loc {
        start: flow_parser::loc::Position { line: 1, column: 0 },
        ..loc.dupe()
    };
    source_location_with_comments(&loc, None::<&ast::Syntax<Loc, ()>>, nodes)
}

pub fn program_simple(opts: &Opts, prog: &ast::Program<Loc, Loc>) -> LayoutNode {
    let loc = &prog.loc;
    let nodes = group(vec![fuse(statement_list(opts, false, &prog.statements))]);
    let loc = Loc {
        start: flow_parser::loc::Position { line: 1, column: 0 },
        ..loc.dupe()
    };
    source_location_with_comments(&loc, None::<&ast::Syntax<Loc, ()>>, nodes)
}

fn combine_directives_and_comments(
    opts: &Opts,
    directives: &[ast::statement::Statement<Loc, Loc>],
    comments: &[ast::Comment<Loc>],
) -> LayoutNode {
    enum Item<'a> {
        Statement(&'a ast::statement::Statement<Loc, Loc>),
        Comment(&'a ast::Comment<Loc>),
    }
    let mut merged: Vec<(Loc, Item)> = Vec::new();
    for s in directives {
        merged.push((s.loc().dupe(), Item::Statement(s)));
    }
    for c in comments {
        merged.push((c.loc.dupe(), Item::Comment(c)));
    }
    merged.sort_by(|(a, _), (b, _)| a.cmp(b));

    let nodes: Vec<(Loc, CommentsBounds, LayoutNode)> = merged
        .into_iter()
        .map(|(loc, item)| match item {
            Item::Statement(s) => {
                let bounds = comment_attachment::statement_comment_bounds(s);
                (loc, bounds, statement(opts, false, s))
            }
            Item::Comment(c) => (
                loc,
                CommentsBounds {
                    first_leading: None,
                    last_trailing: None,
                },
                comment_layout(false, c),
            ),
        })
        .collect();
    fuse(list_with_newlines(&nodes))
}

// Renders a statement
//
// Set `pretty_semicolon` when a semicolon is only required in pretty mode. For example,
// a semicolon is never required on the last statement of a statement list, so we can set
// `pretty_semicolon: true` to only print the unnecessary semicolon in pretty mode.
pub fn statement(
    opts: &Opts,
    pretty_semicolon: bool,
    root_stmt: &ast::statement::Statement<Loc, Loc>,
) -> LayoutNode {
    use ast::statement::*;
    let loc = root_stmt.loc();
    let with_semi: fn(LayoutNode) -> LayoutNode = if pretty_semicolon {
        with_pretty_semicolon
    } else {
        with_semicolon
    };
    source_location_with_comments(
        loc,
        None::<&ast::Syntax<Loc, ()>>,
        match root_stmt.deref() {
            StatementInner::Empty { loc, inner } => layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                LayoutNode::atom(";".to_string()),
            ),
            StatementInner::Debugger { loc, inner } => layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                with_semi(LayoutNode::atom("debugger".to_string())),
            ),
            StatementInner::Block { loc, inner } => block(opts, loc, inner),
            StatementInner::Expression { loc, inner } => {
                let opts = &Opts {
                    preserve_formatting: opts.preserve_formatting
                        || inner
                            .directive
                            .as_ref()
                            .is_some_and(|directive| directive.contains('\\')),
                    ..*opts
                };
                let ctxt = ExpressionContext {
                    left: ExpressionContextLeft::InExpressionStatement,
                    ..NORMAL_CONTEXT
                };
                layout_node_with_comments_opt(
                    loc,
                    inner.comments.as_ref(),
                    with_semi(expression_with_parens(opts, 0, &ctxt, &inner.expression)),
                )
            }
            StatementInner::If { loc, inner } => layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                match &inner.alternate {
                    Some(alt) => {
                        let alt_bounds = comment_attachment::if_alternate_statement_comment_bounds(
                            &alt.loc, alt,
                        );
                        let alternate_separator =
                            comment_aware_separator(None, pretty_space(), &alt_bounds);
                        fuse(vec![
                            group(vec![
                                statement_with_test("if", expression(opts, None, &inner.test)),
                                statement_after_test(opts, false, &inner.consequent),
                            ]),
                            alternate_separator,
                            layout_node_with_comments_opt(
                                &alt.loc,
                                alt.comments.as_ref(),
                                fuse_with_space(vec![
                                    LayoutNode::atom("else".to_string()),
                                    statement(opts, pretty_semicolon, &alt.body),
                                ]),
                            ),
                        ])
                    }
                    None => group(vec![
                        statement_with_test("if", expression(opts, None, &inner.test)),
                        statement_after_test(opts, pretty_semicolon, &inner.consequent),
                    ]),
                },
            ),
            StatementInner::Labeled { loc, inner } => layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                fuse(vec![
                    identifier(&inner.label),
                    LayoutNode::atom(":".to_string()),
                    pretty_space(),
                    statement(opts, false, &inner.body),
                ]),
            ),
            StatementInner::Break { loc, inner } => {
                let s_break = LayoutNode::atom("break".to_string());
                layout_node_with_comments_opt(
                    loc,
                    inner.comments.as_ref(),
                    with_semi(match &inner.label {
                        Some(l) => fuse(vec![s_break, space(), identifier(l)]),
                        None => s_break,
                    }),
                )
            }
            StatementInner::Continue { loc, inner } => {
                let s_continue = LayoutNode::atom("continue".to_string());
                layout_node_with_comments_opt(
                    loc,
                    inner.comments.as_ref(),
                    with_semi(match &inner.label {
                        Some(l) => fuse(vec![s_continue, space(), identifier(l)]),
                        None => s_continue,
                    }),
                )
            }
            StatementInner::With { loc, inner } => layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                fuse(vec![
                    statement_with_test("with", expression(opts, None, &inner.object)),
                    statement_after_test(opts, false, &inner.body),
                ]),
            ),
            StatementInner::Match { loc, inner } => {
                let cases: Vec<(Loc, CommentsBounds, LayoutNode)> = inner
                    .cases
                    .iter()
                    .map(|case| {
                        let case_loc = case.loc.dupe();
                        let bounds = comment_attachment::match_statement_case_comment_bounds(case);
                        let layout =
                            match_case(opts, case, &|opts2, body| statement(opts2, false, body));
                        (case_loc, bounds, layout)
                    })
                    .collect();
                let cases = list_with_newlines(&cases);
                let cases_node = wrap_and_indent(
                    (
                        LayoutNode::atom("{".to_string()),
                        LayoutNode::atom("}".to_string()),
                    ),
                    Some(pretty_hardline()),
                    vec![fuse(cases)],
                );
                layout_node_with_comments_opt(
                    loc,
                    inner.comments.as_ref(),
                    fuse(vec![
                        group(vec![
                            LayoutNode::atom("match".to_string()),
                            pretty_space(),
                            wrap_and_indent(
                                (
                                    LayoutNode::atom("(".to_string()),
                                    LayoutNode::atom(")".to_string()),
                                ),
                                None,
                                vec![expression(opts, None, &inner.arg)],
                            ),
                        ]),
                        pretty_space(),
                        cases_node,
                    ]),
                )
            }
            StatementInner::Switch { loc, inner } => {
                let num_cases = inner.cases.len();
                let case_nodes: Vec<(Loc, CommentsBounds, LayoutNode)> = inner
                    .cases
                    .iter()
                    .enumerate()
                    .map(|(i, case)| {
                        let case_loc = case.loc.dupe();
                        let bounds = comment_attachment::switch_case_comment_bounds(case);
                        let last = i == num_cases - 1;
                        let layout = switch_case(opts, last, case);
                        (case_loc, bounds, layout)
                    })
                    .collect();
                let case_nodes = list_with_newlines(&case_nodes);
                let cases_node = wrap_and_indent(
                    (
                        LayoutNode::atom("{".to_string()),
                        LayoutNode::atom("}".to_string()),
                    ),
                    Some(pretty_hardline()),
                    vec![fuse(case_nodes)],
                );
                layout_node_with_comments_opt(
                    loc,
                    inner.comments.as_ref(),
                    fuse(vec![
                        statement_with_test("switch", expression(opts, None, &inner.discriminant)),
                        pretty_space(),
                        cases_node,
                    ]),
                )
            }
            StatementInner::Return { loc, inner } => {
                let s_return = LayoutNode::atom("return".to_string());
                layout_node_with_comments_opt(
                    loc,
                    inner.comments.as_ref(),
                    with_semi(match &inner.argument {
                        Some(arg) => {
                            let arg_layout = match arg.deref() {
                                ast::expression::ExpressionInner::Logical { .. }
                                | ast::expression::ExpressionInner::Binary { .. }
                                | ast::expression::ExpressionInner::Sequence { .. }
                                | ast::expression::ExpressionInner::JSXElement { .. } => {
                                    wrap_in_parens_on_break(expression(opts, None, arg))
                                }
                                _ => {
                                    // If the return argument has a leading comment then we must wrap the argument
                                    // and its comments in parens. Otherwise, the comments could push the argument
                                    // to the next line, meaning the return would be parsed without an argument.
                                    let CommentsBounds {
                                        first_leading: leading,
                                        ..
                                    } = comment_attachment::expression_comment_bounds(arg);
                                    let arg_node = expression(opts, None, arg);
                                    if leading.is_none() {
                                        arg_node
                                    } else {
                                        wrap_in_parens_on_break(arg_node)
                                    }
                                }
                            };
                            fuse_with_space(vec![s_return, arg_layout])
                        }
                        None => s_return,
                    }),
                )
            }
            StatementInner::Throw { loc, inner } => layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                with_semi(fuse_with_space(vec![
                    LayoutNode::atom("throw".to_string()),
                    wrap_in_parens_on_break(expression(opts, None, &inner.argument)),
                ])),
            ),
            StatementInner::Try { loc, inner } => layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                fuse(vec![
                    LayoutNode::atom("try".to_string()),
                    pretty_space(),
                    block(opts, &inner.block.0, &inner.block.1),
                    match &inner.handler {
                        Some(h) => source_location_with_comments(
                            &h.loc,
                            h.comments.as_ref(),
                            match &h.param {
                                Some(p) => fuse(vec![
                                    pretty_space(),
                                    statement_with_test("catch", pattern(opts, None, p)),
                                    pretty_space(),
                                    block(opts, &h.body.0, &h.body.1),
                                ]),
                                None => fuse(vec![
                                    pretty_space(),
                                    LayoutNode::atom("catch".to_string()),
                                    pretty_space(),
                                    block(opts, &h.body.0, &h.body.1),
                                ]),
                            },
                        ),
                        None => LayoutNode::empty(),
                    },
                    match &inner.finalizer {
                        Some((f_loc, f_block)) => fuse(vec![
                            pretty_space(),
                            LayoutNode::atom("finally".to_string()),
                            pretty_space(),
                            block(opts, f_loc, f_block),
                        ]),
                        None => LayoutNode::empty(),
                    },
                ]),
            ),
            StatementInner::While { loc, inner } => layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                fuse(vec![
                    statement_with_test("while", expression(opts, None, &inner.test)),
                    statement_after_test(opts, pretty_semicolon, &inner.body),
                ]),
            ),
            StatementInner::DoWhile { loc, inner } => layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                with_semi(fuse(vec![
                    fuse_with_space(vec![
                        LayoutNode::atom("do".to_string()),
                        statement(opts, false, &inner.body),
                    ]),
                    pretty_space(),
                    LayoutNode::atom("while".to_string()),
                    pretty_space(),
                    group(vec![wrap_and_indent(
                        (
                            LayoutNode::atom("(".to_string()),
                            LayoutNode::atom(")".to_string()),
                        ),
                        None,
                        vec![expression(opts, None, &inner.test)],
                    )]),
                ])),
            ),
            StatementInner::For { loc, inner } => layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                fuse(vec![
                    statement_with_test(
                        "for",
                        join(
                            fuse(vec![LayoutNode::atom(";".to_string()), pretty_line()]),
                            vec![
                                match &inner.init {
                                    Some(for_::Init::InitDeclaration((decl_loc, decl))) => {
                                        let ctxt = ExpressionContext {
                                            group: ExpressionContextGroup::InForInit,
                                            ..NORMAL_CONTEXT
                                        };
                                        variable_declaration(
                                            opts,
                                            None,
                                            Some(&ctxt),
                                            decl_loc,
                                            decl,
                                        )
                                    }
                                    Some(for_::Init::InitExpression(expr)) => {
                                        let ctxt = ExpressionContext {
                                            group: ExpressionContextGroup::InForInit,
                                            ..NORMAL_CONTEXT
                                        };
                                        expression_with_parens(opts, 0, &ctxt, expr)
                                    }
                                    None => LayoutNode::empty(),
                                },
                                match &inner.test {
                                    Some(expr) => expression(opts, None, expr),
                                    None => LayoutNode::empty(),
                                },
                                match &inner.update {
                                    Some(expr) => expression(opts, None, expr),
                                    None => LayoutNode::empty(),
                                },
                            ],
                        ),
                    ),
                    statement_after_test(opts, pretty_semicolon, &inner.body),
                ]),
            ),
            StatementInner::ForIn { loc, inner } => layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                fuse(vec![
                    LayoutNode::atom("for".to_string()),
                    pretty_space(),
                    wrap_in_parens(
                        false,
                        fuse_with_space(vec![
                            match &inner.left {
                                for_in::Left::LeftDeclaration((decl_loc, decl)) => {
                                    variable_declaration(opts, None, None, decl_loc, decl)
                                }
                                for_in::Left::LeftPattern(patt) => pattern(opts, None, patt),
                            },
                            LayoutNode::atom("in".to_string()),
                            expression(opts, None, &inner.right),
                        ]),
                    ),
                    statement_after_test(opts, pretty_semicolon, &inner.body),
                ]),
            ),
            StatementInner::FunctionDeclaration { loc, inner } => function_(opts, loc, inner),
            StatementInner::ComponentDeclaration { loc, inner } => {
                component_declaration(opts, loc, inner)
            }
            StatementInner::VariableDeclaration { loc, inner } => {
                let semicolon = if pretty_semicolon {
                    Some(if_pretty(
                        LayoutNode::atom(";".to_string()),
                        LayoutNode::empty(),
                    ))
                } else {
                    Some(LayoutNode::atom(";".to_string()))
                };
                variable_declaration(opts, semicolon, None, loc, inner)
            }
            StatementInner::ClassDeclaration { loc, inner } => class_base(opts, loc, inner),
            StatementInner::RecordDeclaration { loc, inner } => {
                record_declaration(opts, loc, inner)
            }
            StatementInner::EnumDeclaration { loc, inner } => {
                enum_declaration(LayoutNode::atom("enum".to_string()), loc, inner)
            }
            StatementInner::ForOf { loc, inner } => layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                fuse(vec![
                    LayoutNode::atom("for".to_string()),
                    if inner.await_ {
                        fuse(vec![space(), LayoutNode::atom("await".to_string())])
                    } else {
                        LayoutNode::empty()
                    },
                    pretty_space(),
                    wrap_in_parens(
                        false,
                        fuse_with_space(vec![
                            match &inner.left {
                                for_of::Left::LeftDeclaration((decl_loc, decl)) => {
                                    variable_declaration(opts, None, None, decl_loc, decl)
                                }
                                for_of::Left::LeftPattern(patt) => {
                                    if is_async_pattern_identifier(patt) {
                                        wrap_in_parens(false, pattern(opts, None, patt))
                                    } else {
                                        pattern(opts, None, patt)
                                    }
                                }
                            },
                            LayoutNode::atom("of".to_string()),
                            expression_with_parens(
                                opts,
                                MIN_PRECEDENCE,
                                &NORMAL_CONTEXT,
                                &inner.right,
                            ),
                        ]),
                    ),
                    statement_after_test(opts, pretty_semicolon, &inner.body),
                ]),
            ),
            StatementInner::ImportDeclaration { loc, inner } => {
                import_declaration(opts, loc, inner)
            }
            StatementInner::ImportEqualsDeclaration { loc, inner } => {
                import_equals_declaration(opts, loc, inner)
            }
            StatementInner::ExportNamedDeclaration { loc, inner } => {
                export_declaration(opts, loc, inner)
            }
            StatementInner::ExportDefaultDeclaration { loc, inner } => {
                export_default_declaration(opts, loc, inner)
            }
            StatementInner::ExportAssignment { loc, inner } => export_assignment(opts, loc, inner),
            StatementInner::NamespaceExportDeclaration { loc, inner } => {
                namespace_export_declaration(loc, inner)
            }
            StatementInner::TypeAlias { loc, inner } => type_alias(opts, false, loc, inner),
            StatementInner::OpaqueType { loc, inner } => opaque_type(opts, false, loc, inner),
            StatementInner::InterfaceDeclaration { loc, inner } => {
                interface_declaration(opts, loc, inner)
            }
            StatementInner::DeclareClass { loc, inner } => declare_class(opts, loc, inner),
            StatementInner::DeclareComponent { loc, inner } => declare_component(opts, loc, inner),
            StatementInner::DeclareEnum { loc, inner } => declare_enum(loc, inner),
            StatementInner::DeclareFunction { loc, inner } => declare_function(opts, loc, inner),
            StatementInner::DeclareInterface { loc, inner } => declare_interface(opts, loc, inner),
            StatementInner::DeclareVariable { loc, inner } => declare_variable(opts, loc, inner),
            StatementInner::DeclareModuleExports { loc, inner } => {
                declare_module_exports(opts, loc, inner)
            }
            StatementInner::DeclareModule { loc, inner } => declare_module(opts, loc, inner),
            StatementInner::DeclareNamespace { loc, inner } => declare_namespace(opts, loc, inner),
            StatementInner::DeclareTypeAlias { loc, inner } => type_alias(opts, true, loc, inner),
            StatementInner::DeclareOpaqueType { loc, inner } => opaque_type(opts, true, loc, inner),
            StatementInner::DeclareExportDeclaration { loc, inner } => {
                declare_export_declaration(opts, loc, inner)
            }
        },
    )
}

fn is_async_pattern_identifier(patt: &ast::pattern::Pattern<Loc, Loc>) -> bool {
    match patt {
        ast::pattern::Pattern::Identifier { inner, .. } => {
            inner.name.name.as_ref() == "async"
                && matches!(&inner.annot, ast::types::AnnotationOrHint::Missing(_))
        }
        _ => false,
    }
}

fn block(opts: &Opts, loc: &Loc, b: &ast::statement::Block<Loc, Loc>) -> LayoutNode {
    let statements = statement_list(opts, true, &b.body);
    source_location_with_comments(
        loc,
        b.comments.as_ref(),
        if !statements.is_empty() {
            group(vec![wrap_and_indent(
                (atom("{"), atom("}")),
                Some(pretty_hardline()),
                vec![fuse(statements)],
            )])
        } else {
            match internal_comments(b.comments.as_ref()) {
                None => atom("{}"),
                Some((_, _, comments)) => fuse(vec![atom("{"), comments, atom("}")]),
            }
        },
    )
}

pub fn expression(
    opts: &Opts,
    ctxt: Option<&ExpressionContext>,
    expr: &ast::expression::Expression<Loc, Loc>,
) -> LayoutNode {
    let loc = expr.loc();
    use ast::expression::*;
    let ctxt = ctxt.copied().unwrap_or(NORMAL_CONTEXT);
    let precedence = precedence_of_expression(expr);
    source_location_with_comments(
        loc,
        None::<&ast::Syntax<Loc, ()>>,
        match expr.deref() {
            ExpressionInner::This { loc, inner } => {
                layout_node_with_comments_opt(loc, inner.comments.as_ref(), atom("this"))
            }
            ExpressionInner::Super { loc, inner } => {
                layout_node_with_comments_opt(loc, inner.comments.as_ref(), atom("super"))
            }
            ExpressionInner::Array { loc, inner } => {
                fn element_loc(element: &ArrayElement<Loc, Loc>) -> &Loc {
                    match element {
                        ArrayElement::Hole(loc) => loc,
                        ArrayElement::Expression(expr) => expr.loc(),
                        ArrayElement::Spread(spread) => &spread.loc,
                    }
                }
                let has_trailing_hole =
                    matches!(inner.elements.last(), Some(ArrayElement::Hole(_)));
                let elements: Vec<(Loc, CommentsBounds, LayoutNode)> = inner
                    .elements
                    .iter()
                    .map(|element| {
                        let eloc = element_loc(element);
                        (
                            eloc.dupe(),
                            comment_attachment::array_element_comment_bounds(eloc, element),
                            array_element(opts, element),
                        )
                    })
                    .collect();
                let mut elements_layout =
                    list_with_newlines_with_sep(atom(","), pretty_line(), false, &elements);
                // If the last element is a hole, then we need to manually insert a trailing `,` even in
                // ugly mode. Otherwise add a trailing comma only in pretty mode.
                let trailing_comma = if has_trailing_hole {
                    atom(",")
                } else if !elements.is_empty()
                    && TrailingCommas::enabled_for_arrays(opts.trailing_commas)
                {
                    if_break(atom(","), LayoutNode::empty())
                } else {
                    LayoutNode::empty()
                };
                elements_layout.push(trailing_comma);
                let elements_layout =
                    list_add_internal_comments(&elements, elements_layout, inner.comments.as_ref());
                layout_node_with_comments_opt(
                    loc,
                    inner.comments.as_ref(),
                    group(vec![wrap_and_indent(
                        (atom("["), atom("]")),
                        None,
                        elements_layout,
                    )]),
                )
            }
            ExpressionInner::Object { loc, inner } => object_(opts, loc, inner),
            ExpressionInner::Sequence { loc, inner } => {
                // to get an AST like `x, (y, z)`, then there must've been parens
                //    around the right side. we can force that by bumping the minimum
                let precedence = precedence + 1;
                let layouts: Vec<LayoutNode> = inner
                    .expressions
                    .iter()
                    .map(|e| expression_with_parens(opts, precedence, &ctxt, e))
                    .collect();
                layout_node_with_comments_opt(
                    loc,
                    inner.comments.as_ref(),
                    group(vec![join(fuse(vec![atom(","), pretty_line()]), layouts)]),
                )
            }
            ExpressionInner::Identifier { inner, .. } => identifier(inner),
            ExpressionInner::StringLiteral { loc, inner } => string_literal(opts, loc, inner),
            ExpressionInner::BooleanLiteral { loc, inner } => boolean_literal(loc, inner),
            ExpressionInner::NullLiteral { loc, inner } => null_literal(loc, (**inner).as_ref()),
            ExpressionInner::NumberLiteral { loc, inner } => number_literal(opts, loc, inner),
            ExpressionInner::BigIntLiteral { loc, inner } => bigint_literal(loc, inner),
            ExpressionInner::RegExpLiteral { loc, inner } => regexp_literal(opts, loc, inner),
            ExpressionInner::ModuleRefLiteral { loc, inner } => module_ref_literal(loc, inner),
            ExpressionInner::Function { loc, inner } => function_(opts, loc, inner),
            ExpressionInner::ArrowFunction { loc, inner } => {
                arrow_function(&ctxt, opts, loc, inner)
            }
            ExpressionInner::Assignment { loc, inner } => layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                fuse(vec![
                    pattern(opts, Some(&ctxt), &inner.left),
                    pretty_space(),
                    match inner.operator {
                        None => atom("="),
                        Some(op) => atom(flow_parser::ast_utils::string_of_assignment_operator(op)),
                    },
                    comment_aware_separator(
                        None,
                        pretty_space(),
                        &comment_attachment::expression_comment_bounds(&inner.right),
                    ),
                    {
                        let ctxt = context_after_token(&ctxt);
                        expression_with_parens(opts, precedence, &ctxt, &inner.right)
                    },
                ]),
            ),
            ExpressionInner::Binary { loc, inner } => {
                layout_node_with_comments_opt(
                    loc,
                    inner.comments.as_ref(),
                    fuse_with_space(vec![
                        expression_with_parens(opts, precedence, &ctxt, &inner.left),
                        atom(flow_parser::ast_utils::string_of_binary_operator(
                            inner.operator,
                        )),
                        {
                            match (&inner.operator, inner.right.deref()) {
                                (BinaryOperator::Plus, ExpressionInner::Unary { inner: u, .. })
                                    if u.operator == UnaryOperator::Plus =>
                                {
                                    let ctxt = context_after_token(&ctxt);
                                    let right_separator = comment_aware_separator(
                                        None,
                                        ugly_space(),
                                        &comment_attachment::expression_comment_bounds(
                                            &inner.right,
                                        ),
                                    );
                                    fuse(vec![
                                        right_separator,
                                        expression(opts, Some(&ctxt), &inner.right),
                                    ])
                                }
                                (
                                    BinaryOperator::Minus,
                                    ExpressionInner::Unary { inner: u, .. },
                                ) if u.operator == UnaryOperator::Minus => {
                                    let ctxt = context_after_token(&ctxt);
                                    let right_separator = comment_aware_separator(
                                        None,
                                        ugly_space(),
                                        &comment_attachment::expression_comment_bounds(
                                            &inner.right,
                                        ),
                                    );
                                    fuse(vec![
                                        right_separator,
                                        expression(opts, Some(&ctxt), &inner.right),
                                    ])
                                }
                                (
                                    BinaryOperator::Plus,
                                    ExpressionInner::Update { inner: u, .. },
                                ) if u.prefix && u.operator == UpdateOperator::Increment => {
                                    let ctxt = context_after_token(&ctxt);
                                    let right_separator = comment_aware_separator(
                                        None,
                                        ugly_space(),
                                        &comment_attachment::expression_comment_bounds(
                                            &inner.right,
                                        ),
                                    );
                                    fuse(vec![
                                        right_separator,
                                        expression(opts, Some(&ctxt), &inner.right),
                                    ])
                                }
                                (
                                    BinaryOperator::Minus,
                                    ExpressionInner::Update { inner: u, .. },
                                ) if u.prefix && u.operator == UpdateOperator::Decrement => {
                                    let ctxt = context_after_token(&ctxt);
                                    let right_separator = comment_aware_separator(
                                        None,
                                        ugly_space(),
                                        &comment_attachment::expression_comment_bounds(
                                            &inner.right,
                                        ),
                                    );
                                    fuse(vec![
                                        right_separator,
                                        expression(opts, Some(&ctxt), &inner.right),
                                    ])
                                }
                                _ => {
                                    // to get an AST like `x + (y - z)`, then there must've been parens
                                    //    around the right side. we can force that by bumping the minimum
                                    // precedence to not have parens.
                                    let precedence = precedence + 1;
                                    let ctxt = ExpressionContext {
                                        left: match inner.operator {
                                            BinaryOperator::Minus => {
                                                ExpressionContextLeft::InMinusOp
                                            }
                                            BinaryOperator::Plus => ExpressionContextLeft::InPlusOp,
                                            _ => ExpressionContextLeft::NormalLeft,
                                        },
                                        ..ctxt
                                    };
                                    let right_separator = comment_aware_separator(
                                        None,
                                        LayoutNode::empty(),
                                        &comment_attachment::expression_comment_bounds(
                                            &inner.right,
                                        ),
                                    );
                                    fuse(vec![
                                        right_separator,
                                        expression_with_parens(
                                            opts,
                                            precedence,
                                            &ctxt,
                                            &inner.right,
                                        ),
                                    ])
                                }
                            }
                        },
                    ]),
                )
            }
            ExpressionInner::Call { loc, inner } => call_expr(
                OptionalCallKind::NonOptional,
                precedence,
                &ctxt,
                opts,
                inner,
                loc,
            ),
            ExpressionInner::OptionalCall { loc, inner } => call_expr(
                inner.optional.dupe(),
                precedence,
                &ctxt,
                opts,
                &inner.call,
                loc,
            ),
            ExpressionInner::Conditional { loc, inner } => {
                // increase precedence since conditionals are right-associative
                let test_layout = expression_with_parens(opts, precedence + 1, &ctxt, &inner.test);
                layout_node_with_comments_opt(
                    loc,
                    inner.comments.as_ref(),
                    group(vec![
                        test_layout,
                        LayoutNode::indent(fuse(vec![
                            pretty_line(),
                            atom("?"),
                            pretty_space(),
                            expression_with_parens(opts, MIN_PRECEDENCE, &ctxt, &inner.consequent),
                            pretty_line(),
                            atom(":"),
                            pretty_space(),
                            expression_with_parens(opts, MIN_PRECEDENCE, &ctxt, &inner.alternate),
                        ])),
                    ]),
                )
            }
            ExpressionInner::Logical { loc, inner } => {
                // Logical expressions inside a nullish coalese expression must be wrapped in parens
                let logical_expression_with_parens = |precedence: i32,
                                                      ctxt: &ExpressionContext,
                                                      opts: &Opts,
                                                      e: &ast::expression::Expression<Loc, Loc>|
                 -> LayoutNode {
                    match (inner.operator, e.deref()) {
                        (
                            LogicalOperator::NullishCoalesce,
                            ExpressionInner::Logical {
                                inner: inner_log, ..
                            },
                        ) if inner_log.operator == LogicalOperator::And
                            || inner_log.operator == LogicalOperator::Or =>
                        {
                            let CommentsBounds {
                                first_leading: leading,
                                ..
                            } = comment_attachment::expression_comment_bounds(e);
                            wrap_in_parens(leading.is_some(), expression(opts, None, e))
                        }
                        _ => expression_with_parens(opts, precedence, ctxt, e),
                    }
                };
                let left = logical_expression_with_parens(precedence, &ctxt, opts, &inner.left);
                let operator = match inner.operator {
                    LogicalOperator::Or => atom("||"),
                    LogicalOperator::And => atom("&&"),
                    LogicalOperator::NullishCoalesce => atom("??"),
                };
                let right_separator = comment_aware_separator(
                    Some(pretty_space()),
                    pretty_line(),
                    &comment_attachment::expression_comment_bounds(&inner.right),
                );
                let right =
                    logical_expression_with_parens(precedence + 1, &ctxt, opts, &inner.right);
                // if we need to wrap, the op stays on the first line, with the RHS on a
                // new line and indented by 2 spaces
                layout_node_with_comments_opt(
                    loc,
                    inner.comments.as_ref(),
                    LayoutNode::group(vec![
                        left,
                        pretty_space(),
                        operator,
                        LayoutNode::indent(fuse(vec![right_separator, right])),
                    ]),
                )
            }
            ExpressionInner::Member { loc, inner } => member(
                OptionalMemberKind::NonOptional,
                precedence,
                &ctxt,
                opts,
                inner,
                loc,
            ),
            ExpressionInner::OptionalMember { loc, inner } => member(
                inner.optional.dupe(),
                precedence,
                &ctxt,
                opts,
                &inner.member,
                loc,
            ),
            ExpressionInner::New { loc, inner } => {
                let callee_layout = if definitely_needs_parens(precedence, ctxt, &inner.callee)
                    || contains_call_expression(&inner.callee)
                {
                    let CommentsBounds {
                        first_leading: leading,
                        ..
                    } = comment_attachment::expression_comment_bounds(&inner.callee);
                    wrap_in_parens(
                        leading.is_some(),
                        expression(opts, Some(&ctxt), &inner.callee),
                    )
                } else {
                    expression(opts, Some(&ctxt), &inner.callee)
                };
                layout_node_with_comments_opt(
                    loc,
                    inner.comments.as_ref(),
                    group(vec![
                        fuse_with_space(vec![atom("new"), callee_layout]),
                        option_layout(
                            |targs| call_type_args(opts, "<", targs),
                            inner.targs.as_ref(),
                        ),
                        option_layout(
                            |args| call_args(opts, true, "(", args),
                            inner.arguments.as_ref(),
                        ),
                    ]),
                )
            }
            ExpressionInner::Unary { loc, inner } => {
                let (s_operator, needs_space, prefix) = match inner.operator {
                    UnaryOperator::Minus => (atom("-"), false, true),
                    UnaryOperator::Plus => (atom("+"), false, true),
                    UnaryOperator::Not => (atom("!"), false, true),
                    UnaryOperator::BitNot => (atom("~"), false, true),
                    UnaryOperator::Typeof => (atom("typeof"), true, true),
                    UnaryOperator::Void => (atom("void"), true, true),
                    UnaryOperator::Delete => (atom("delete"), true, true),
                    UnaryOperator::Await => (atom("await"), true, true),
                    UnaryOperator::Nonnull => (atom("!"), false, false),
                };
                let expr_layout = {
                    let ctxt = ExpressionContext {
                        left: match inner.operator {
                            UnaryOperator::Minus => ExpressionContextLeft::InMinusOp,
                            UnaryOperator::Plus => ExpressionContextLeft::InPlusOp,
                            _ => ExpressionContextLeft::NormalLeft,
                        },
                        ..ctxt
                    };
                    expression_with_parens(opts, precedence, &ctxt, &inner.argument)
                };
                let (fst, snd) = if prefix {
                    (s_operator, expr_layout)
                } else {
                    (expr_layout, s_operator)
                };
                layout_node_with_comments_opt(
                    loc,
                    inner.comments.as_ref(),
                    fuse(vec![
                        fst,
                        if needs_space {
                            match inner.argument.deref() {
                                ExpressionInner::Sequence { .. } => LayoutNode::empty(),
                                _ => space(),
                            }
                        } else {
                            LayoutNode::empty()
                        },
                        snd,
                    ]),
                )
            }
            ExpressionInner::Update { loc, inner } => {
                // we never need to wrap `argument` in parens because it must be a valid
                // left-hand side expression
                layout_node_with_comments_opt(loc, inner.comments.as_ref(), {
                    let s_operator = match inner.operator {
                        UpdateOperator::Increment => atom("++"),
                        UpdateOperator::Decrement => atom("--"),
                    };
                    if inner.prefix {
                        fuse(vec![
                            s_operator,
                            expression(opts, Some(&ctxt), &inner.argument),
                        ])
                    } else {
                        fuse(vec![
                            expression(opts, Some(&ctxt), &inner.argument),
                            s_operator,
                        ])
                    }
                })
            }
            ExpressionInner::Class { loc, inner } => class_base(opts, loc, inner),
            ExpressionInner::Yield { loc, inner } => layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                fuse(vec![
                    atom("yield"),
                    if inner.delegate {
                        atom("*")
                    } else {
                        LayoutNode::empty()
                    },
                    match &inner.argument {
                        Some(arg) => fuse(vec![space(), expression(opts, Some(&ctxt), arg)]),
                        None => LayoutNode::empty(),
                    },
                ]),
            ),
            ExpressionInner::Match { loc, inner } => {
                let cases: Vec<(Loc, CommentsBounds, LayoutNode)> = inner
                    .cases
                    .iter()
                    .map(|case| {
                        (
                            case.loc.dupe(),
                            comment_attachment::match_expression_case_comment_bounds(case),
                            match_case(opts, case, &|opts: &Opts,
                                                     body: &ast::expression::Expression<
                                Loc,
                                Loc,
                            >|
                             -> LayoutNode {
                                expression(opts, None, body)
                            }),
                        )
                    })
                    .collect();
                let mut cases_nodes =
                    list_with_newlines_with_sep(atom(","), pretty_hardline(), false, &cases);
                cases_nodes.push(if_pretty(atom(","), LayoutNode::empty()));
                let cases_node = wrap_and_indent(
                    (atom("{"), atom("}")),
                    Some(pretty_hardline()),
                    vec![fuse(cases_nodes)],
                );
                layout_node_with_comments_opt(
                    loc,
                    inner.comments.as_ref(),
                    fuse(vec![
                        group(vec![
                            atom("match"),
                            pretty_space(),
                            wrap_and_indent(
                                (atom("("), atom(")")),
                                None,
                                vec![expression(opts, None, &inner.arg)],
                            ),
                        ]),
                        pretty_space(),
                        cases_node,
                    ]),
                )
            }
            ExpressionInner::MetaProperty { loc, inner } => layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                fuse(vec![
                    identifier(&inner.meta),
                    atom("."),
                    identifier(&inner.property),
                ]),
            ),
            ExpressionInner::TaggedTemplate { loc, inner } => {
                let ctxt = ExpressionContext {
                    left: ExpressionContextLeft::InTaggedTemplate,
                    ..NORMAL_CONTEXT
                };
                let (template_loc, template) = &inner.quasi;
                layout_node_with_comments_opt(
                    loc,
                    inner.comments.as_ref(),
                    fuse(vec![
                        expression_with_parens(opts, precedence, &ctxt, &inner.tag),
                        option_layout(
                            |targs| call_type_args(opts, "<", targs),
                            inner.targs.as_ref(),
                        ),
                        source_location_with_comments(
                            template_loc,
                            template.comments.as_ref(),
                            template_literal(opts, template),
                        ),
                    ]),
                )
            }
            ExpressionInner::TemplateLiteral { loc, inner } => layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                template_literal(opts, inner),
            ),
            ExpressionInner::JSXElement { loc, inner } => jsx_element(opts, loc, inner),
            ExpressionInner::JSXFragment { loc, inner } => jsx_fragment(opts, loc, inner),
            ExpressionInner::TypeCast { loc, inner } => type_cast(opts, loc, inner),
            ExpressionInner::AsConstExpression { loc, inner } => {
                as_const_expression(&ctxt, opts, precedence, loc, inner)
            }
            ExpressionInner::AsExpression { loc, inner } => {
                as_expression(&ctxt, opts, precedence, loc, inner)
            }
            ExpressionInner::TSSatisfies { loc, inner } => ts_satisfies(opts, loc, inner),
            ExpressionInner::Import { loc, inner } => layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                fuse(vec![
                    atom("import"),
                    match inner.phase {
                        Some(ast::statement::ImportPhase::Defer) => atom(".defer"),
                        None => LayoutNode::empty(),
                    },
                    wrap_in_parens(
                        false,
                        match &inner.options {
                            Some(options_arg) => fuse(vec![
                                expression(opts, None, &inner.argument),
                                atom(","),
                                atom(" "),
                                expression(opts, None, options_arg),
                            ]),
                            None => expression(opts, None, &inner.argument),
                        },
                    ),
                ]),
            ),
            ExpressionInner::Record { loc, inner } => {
                let constructor_layout = expression(opts, Some(&ctxt), &inner.constructor);
                let (props_loc, props) = &inner.properties;
                layout_node_with_comments_opt(
                    loc,
                    inner.comments.as_ref(),
                    group(vec![
                        constructor_layout,
                        option_layout(
                            |targs| call_type_args(opts, "<", targs),
                            inner.targs.as_ref(),
                        ),
                        pretty_space(),
                        object_(opts, props_loc, props),
                    ]),
                )
            }
        },
    )
}

fn expression_with_parens(
    opts: &Opts,
    precedence: i32,
    ctxt: &ExpressionContext,
    expr: &ast::expression::Expression<Loc, Loc>,
) -> LayoutNode {
    if definitely_needs_parens(precedence, *ctxt, expr) {
        let CommentsBounds {
            first_leading: leading,
            ..
        } = comment_attachment::expression_comment_bounds(expr);
        wrap_in_parens(
            leading.is_some(),
            expression(opts, Some(&NORMAL_CONTEXT), expr),
        )
    } else {
        expression(opts, Some(ctxt), expr)
    }
}

pub(crate) fn pattern(
    opts: &Opts,
    ctxt: Option<&ExpressionContext>,
    pat: &ast::pattern::Pattern<Loc, Loc>,
) -> LayoutNode {
    let ctxt = ctxt.copied().unwrap_or(NORMAL_CONTEXT);
    let loc = pat.loc();
    source_location_with_comments(
        loc,
        None::<&ast::Syntax<Loc, ()>>,
        match pat {
            ast::pattern::Pattern::Object { inner, .. } => {
                let props: Vec<(Loc, CommentsBounds, LayoutNode)> = inner
                    .properties
                    .iter()
                    .map(|property| match property {
                        ast::pattern::object::Property::NormalProperty(prop) => {
                            let loc = &prop.loc;
                            let prop_layout = pattern_object_property_key(opts, &prop.key);
                            let prop_layout = if !prop.shorthand {
                                fuse(vec![
                                    prop_layout,
                                    atom(":"),
                                    pretty_space(),
                                    pattern(opts, None, &prop.pattern),
                                ])
                            } else {
                                prop_layout
                            };
                            let prop_layout = match &prop.default {
                                Some(expr) => fuse_with_default(opts, None, prop_layout, expr),
                                None => prop_layout,
                            };
                            let prop_layout = source_location_with_comments(
                                loc,
                                None::<&ast::Syntax<Loc, ()>>,
                                prop_layout,
                            );
                            let comment_bounds =
                                comment_attachment::object_pattern_property_comment_bounds(
                                    loc, property,
                                );
                            (loc.dupe(), comment_bounds, prop_layout)
                        }
                        ast::pattern::object::Property::RestElement(el) => {
                            let loc = &el.loc;
                            let prop_layout = rest_element(opts, el);
                            let comment_bounds =
                                comment_attachment::object_pattern_property_comment_bounds(
                                    loc, property,
                                );
                            (loc.dupe(), comment_bounds, prop_layout)
                        }
                    })
                    .collect();
                let props_layout =
                    list_with_newlines_with_sep(atom(","), pretty_line(), true, &props);
                // Add internal comments
                let props_layout =
                    list_add_internal_comments(&props, props_layout, inner.comments.as_ref());
                layout_node_with_comments_opt(
                    loc,
                    inner.comments.as_ref(),
                    group(vec![
                        wrap_and_indent((atom("{"), atom("}")), None, props_layout),
                        if inner.optional {
                            atom("?")
                        } else {
                            LayoutNode::empty()
                        },
                        hint(|a| type_annotation(opts, false, a), &inner.annot),
                    ]),
                )
            }
            ast::pattern::Pattern::Array { inner, .. } => {
                fn element_loc(element: &ast::pattern::array::Element<Loc, Loc>) -> Loc {
                    match element {
                        ast::pattern::array::Element::Hole(loc) => loc.dupe(),
                        ast::pattern::array::Element::NormalElement(el) => el.loc.dupe(),
                        ast::pattern::array::Element::RestElement(el) => el.loc.dupe(),
                    }
                }
                let elements: Vec<(Loc, CommentsBounds, LayoutNode)> = inner
                    .elements
                    .iter()
                    .map(|element| {
                        let loc = element_loc(element);
                        let bounds =
                            comment_attachment::array_pattern_element_comment_bounds(&loc, element);
                        let layout = pattern_array_element(opts, element);
                        (loc, bounds, layout)
                    })
                    .collect();
                let elements_layout =
                    list_with_newlines_with_sep(atom(","), pretty_line(), false, &elements);
                let elements_layout =
                    list_add_internal_comments(&elements, elements_layout, inner.comments.as_ref());
                layout_node_with_comments_opt(
                    loc,
                    inner.comments.as_ref(),
                    group(vec![
                        wrap_and_indent((atom("["), atom("]")), None, elements_layout),
                        if inner.optional {
                            atom("?")
                        } else {
                            LayoutNode::empty()
                        },
                        hint(|a| type_annotation(opts, false, a), &inner.annot),
                    ]),
                )
            }
            ast::pattern::Pattern::Identifier { inner, .. } => fuse(vec![
                identifier(&inner.name),
                if inner.optional {
                    atom("?")
                } else {
                    LayoutNode::empty()
                },
                hint(|a| type_annotation(opts, false, a), &inner.annot),
            ]),
            ast::pattern::Pattern::Expression { inner, .. } => expression(opts, Some(&ctxt), inner),
        },
    )
}

fn array_element(opts: &Opts, element: &ast::expression::ArrayElement<Loc, Loc>) -> LayoutNode {
    let precedence = MIN_PRECEDENCE;
    let ctxt = NORMAL_CONTEXT;
    match element {
        ast::expression::ArrayElement::Hole(_) => LayoutNode::empty(),
        ast::expression::ArrayElement::Expression(expr) => {
            expression_with_parens(opts, precedence, &ctxt, expr)
        }
        ast::expression::ArrayElement::Spread(spread) => {
            spread_element(opts, precedence, &ctxt, spread)
        }
    }
}

fn pattern_object_property_key(
    opts: &Opts,
    key: &ast::pattern::object::Key<Loc, Loc>,
) -> LayoutNode {
    match key {
        ast::pattern::object::Key::StringLiteral((loc, lit)) => string_literal(opts, loc, lit),
        ast::pattern::object::Key::NumberLiteral((loc, lit)) => number_literal(opts, loc, lit),
        ast::pattern::object::Key::BigIntLiteral((loc, lit)) => bigint_literal(loc, lit),
        ast::pattern::object::Key::Identifier(ident) => identifier(ident),
        ast::pattern::object::Key::Computed(computed) => layout_node_with_comments_opt(
            &computed.loc,
            computed.comments.as_ref(),
            fuse(vec![
                atom("["),
                LayoutNode::sequence(
                    layout::ListConfig::seq(),
                    vec![expression(opts, None, &computed.expression)],
                ),
                atom("]"),
            ]),
        ),
    }
}

fn rest_element(opts: &Opts, rest: &ast::pattern::RestElement<Loc, Loc>) -> LayoutNode {
    source_location_with_comments(
        &rest.loc,
        rest.comments.as_ref(),
        fuse(vec![atom("..."), pattern(opts, None, &rest.argument)]),
    )
}

fn pattern_array_element(
    opts: &Opts,
    element: &ast::pattern::array::Element<Loc, Loc>,
) -> LayoutNode {
    match element {
        ast::pattern::array::Element::Hole(_) => LayoutNode::empty(),
        ast::pattern::array::Element::NormalElement(el) => {
            let elem = pattern(opts, None, &el.argument);
            let elem = match &el.default {
                Some(expr) => fuse_with_default(opts, None, elem, expr),
                None => elem,
            };
            source_location_with_comments(&el.loc, None::<&ast::Syntax<Loc, ()>>, elem)
        }
        ast::pattern::array::Element::RestElement(rest) => rest_element(opts, rest),
    }
}

fn object_(opts: &Opts, loc: &Loc, obj: &ast::expression::Object<Loc, Loc>) -> LayoutNode {
    use ast::expression::object::*;
    fn prop_loc(prop: &Property<Loc, Loc>) -> &Loc {
        match prop {
            Property::NormalProperty(np) => np.loc(),
            Property::SpreadProperty(sp) => &sp.loc,
        }
    }
    let num_props = obj.properties.len();
    let internal_comments_list: Vec<(Loc, CommentsBounds, LayoutNode)> =
        match internal_comments(obj.comments.as_ref()) {
            None => vec![],
            Some(comments) => vec![comments],
        };
    // Add trailing comma to last property
    let props: Vec<(Loc, CommentsBounds, LayoutNode)> = obj
        .properties
        .iter()
        .enumerate()
        .map(|(i, prop)| {
            let prop_layout = if i == num_props - 1
                && internal_comments_list.is_empty()
                && TrailingCommas::enabled_for_objects(opts.trailing_commas)
            {
                fuse(vec![
                    object_property(opts, prop),
                    if_break(atom(","), LayoutNode::empty()),
                ])
            } else {
                object_property(opts, prop)
            };
            (
                prop_loc(prop).dupe(),
                comment_attachment::object_property_comment_bounds(prop),
                prop_layout,
            )
        })
        .collect();
    let mut props = props;
    props.extend(internal_comments_list);
    let props = list_with_newlines_with_sep(atom(","), pretty_line(), true, &props);
    // If first prop is on a different line then pretty print with line breaks
    let break_opt = match obj.properties.first() {
        None => None,
        Some(first_prop) => {
            if loc.start.line < prop_loc(first_prop).start.line {
                Some(pretty_hardline())
            } else if opts.bracket_spacing {
                Some(pretty_line())
            } else {
                Some(softline())
            }
        }
    };
    let props_layout = wrap_and_indent((atom("{"), atom("}")), break_opt, props);
    layout_node_with_comments_opt(loc, obj.comments.as_ref(), group(vec![props_layout]))
}

fn call_expr(
    optional: ast::expression::OptionalCallKind<Loc>,
    precedence: i32,
    ctxt: &ExpressionContext,
    opts: &Opts,
    call_node: &ast::expression::Call<Loc, Loc>,
    loc: &Loc,
) -> LayoutNode {
    let callee = &call_node.callee;
    let targs = call_node.targs.as_ref();
    let arguments = &call_node.arguments;
    let comments = call_node.comments.as_ref();
    let (targs_layout, lparen) = match targs {
        None => {
            let lparen = match optional {
                ast::expression::OptionalCallKind::NonOptional => "(",
                ast::expression::OptionalCallKind::Optional => "?.(",
                ast::expression::OptionalCallKind::AssertNonnull { .. } => "!(",
            };
            (LayoutNode::empty(), lparen)
        }
        Some(targs) => {
            let less_than = match optional {
                ast::expression::OptionalCallKind::NonOptional => "<",
                ast::expression::OptionalCallKind::Optional => "?.<",
                ast::expression::OptionalCallKind::AssertNonnull { .. } => "!<",
            };
            (call_type_args(opts, less_than, targs), "(")
        }
    };
    layout_node_with_comments_opt(
        loc,
        comments,
        fuse(vec![
            expression_with_parens(opts, precedence, ctxt, callee),
            targs_layout,
            call_args(opts, false, lparen, arguments),
        ]),
    )
}

fn member(
    optional: ast::expression::OptionalMemberKind<Loc>,
    precedence: i32,
    ctxt: &ExpressionContext,
    opts: &Opts,
    m: &ast::expression::Member<Loc, Loc>,
    loc: &Loc,
) -> LayoutNode {
    let _object = &m.object;
    let property = &m.property;
    let comments = m.comments.as_ref();
    let (computed, property_loc) = match property {
        ast::expression::member::Property::PropertyExpression(expr) => (true, expr.loc().dupe()),
        ast::expression::member::Property::PropertyIdentifier(ident) => (false, ident.loc.dupe()),
        ast::expression::member::Property::PropertyPrivateName(pn) => (false, pn.loc.dupe()),
    };
    let (ldelim, rdelim) = match (computed, optional) {
        (false, ast::expression::OptionalMemberKind::NonOptional) => {
            (atom("."), LayoutNode::empty())
        }
        (false, ast::expression::OptionalMemberKind::Optional) => (atom("?."), LayoutNode::empty()),
        (false, ast::expression::OptionalMemberKind::AssertNonnull { .. }) => {
            (atom("!."), LayoutNode::empty())
        }
        (true, ast::expression::OptionalMemberKind::NonOptional) => (atom("["), atom("]")),
        (true, ast::expression::OptionalMemberKind::Optional) => (atom("?.["), atom("]")),
        (true, ast::expression::OptionalMemberKind::AssertNonnull { .. }) => {
            (atom("!["), atom("]"))
        }
    };
    let property_layout = match property {
        ast::expression::member::Property::PropertyIdentifier(ident) => {
            source_location_with_comments(&ident.loc, ident.comments.as_ref(), atom(&ident.name))
        }
        ast::expression::member::Property::PropertyPrivateName(pn) => {
            source_location_with_comments(
                &pn.loc,
                pn.comments.as_ref(),
                atom(&format!("#{}", pn.name)),
            )
        }
        ast::expression::member::Property::PropertyExpression(expr) => {
            expression(opts, Some(ctxt), expr)
        }
    };
    // If this is a computed property with leading comments we must break and indent so that
    // comments begin on the line after the left delimiter.
    let CommentsBounds {
        first_leading: leading_property,
        ..
    } = comment_attachment::member_property_comment_bounds(&property_loc, property);
    let property_layout_with_delims = if computed && leading_property.is_some() {
        group(vec![wrap_and_indent(
            (ldelim, rdelim),
            Some(pretty_hardline()),
            vec![property_layout],
        )])
    } else {
        fuse(vec![ldelim, property_layout, rdelim])
    };
    layout_node_with_comments_opt(
        loc,
        comments,
        fuse(vec![
            match _object.deref() {
                ExpressionInner::Call { .. } => expression(opts, Some(ctxt), _object),
                ExpressionInner::NumberLiteral { loc, inner } if !computed => {
                    // 1.foo would be confused with a decimal point, so it needs parens
                    number_literal_member(opts, loc, inner)
                }
                _ => expression_with_parens(opts, precedence, ctxt, _object),
            },
            // If this is a non-computed member expression where the object has a trailing block
            //            comment on a line before the property, insert a line break after the comment so that
            // the dot and property are printed on the line below the comment.
            {
                let CommentsBounds { last_trailing, .. } =
                    comment_attachment::expression_comment_bounds(_object);
                match last_trailing {
                    Some((comment_loc, ast::CommentKind::Block))
                        if !computed && comment_loc.end.line < property_loc.start.line =>
                    {
                        pretty_hardline()
                    }
                    _ => LayoutNode::empty(),
                }
            },
            property_layout_with_delims,
        ]),
    )
}

// arrows don't have ids and can't be generators
fn arrow_function(
    ctxt: &ExpressionContext,
    opts: &Opts,
    loc: &Loc,
    func: &ast::function::Function<Loc, Loc>,
) -> LayoutNode {
    let params = &func.params;
    let params_loc = &params.loc;
    let params_comments = params.comments.as_ref();
    // We must wrap the single param in parentheses if there are internal comments
    let is_single_simple_param = if params.params.len() == 1
        && params.rest.is_none()
        && params.this_.is_none()
        && match &params.comments {
            None => true,
            Some(c) => c.internal.is_empty(),
        } {
        if let ast::function::Param::RegularParam {
            argument: ast::pattern::Pattern::Identifier { inner: pat_id, .. },
            default: None,
            ..
        } = &params.params[0]
        {
            if !pat_id.optional && matches!(pat_id.annot, ast::types::AnnotationOrHint::Missing(_))
            {
                let id_loc = &pat_id.name.loc;
                let id_comments = &pat_id.name.comments;
                // We must wrap the single param in parentheses if it has any attached comments on
                // the same line as the param.
                match id_comments {
                    None => true,
                    Some(syntax) => {
                        let on_same_line = |comments: &[ast::Comment<Loc>]| -> bool {
                            comments.iter().any(|c| c.loc.lines_intersect(id_loc))
                        };
                        !on_same_line(&syntax.leading) && !on_same_line(&syntax.trailing)
                    }
                }
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    };
    let params_and_stuff = match (
        is_single_simple_param,
        &func.return_,
        &func.predicate,
        &func.tparams,
    ) {
        (true, ast::function::ReturnAnnot::Missing(_), None, None) => {
            layout_node_with_comments_opt(
                params_loc,
                params_comments,
                function_param(ctxt, opts, &params.params[0]),
            )
        }
        _ => {
            let params_layout = layout_node_with_comments_opt(
                params_loc,
                params_comments,
                function_params_inner(ctxt, opts, params),
            );
            fuse(vec![
                option_layout(
                    |tp| {
                        type_parameter(
                            opts,
                            flow_parser::ast_visitor::TypeParamsContext::Function,
                            tp,
                        )
                    },
                    func.tparams.as_ref(),
                ),
                group(vec![
                    params_layout,
                    function_return(opts, true, &func.return_, func.predicate.as_ref()),
                ]),
            ])
        }
    };
    let body_separator = comment_aware_separator(
        None,
        pretty_space(),
        &comment_attachment::function_body_comment_bounds(&func.body),
    );
    layout_node_with_comments_opt(
        loc,
        func.comments.as_ref(),
        fuse(vec![
            fuse_with_space(vec![
                if func.async_ {
                    atom("async")
                } else {
                    LayoutNode::empty()
                },
                params_and_stuff,
            ]),
            // Babylon does not parse ():*=>{}` because it thinks the `*=` is an
            //             unexpected multiply-and-assign operator. Thus, we format this with a
            match &func.return_ {
                ast::function::ReturnAnnot::Available(annot)
                    if matches!(&*annot.annotation, ast::types::TypeInner::Exists { .. }) =>
                {
                    space()
                }
                _ => pretty_space(),
            },
            atom("=>"),
            body_separator,
            match &func.body {
                ast::function::Body::BodyBlock((block_loc, b)) => block(opts, block_loc, b),
                // BodyExpression is an AssignmentExpression
                ast::function::Body::BodyExpression(expr) => {
                    let ctxt = ExpressionContext {
                        group: ExpressionContextGroup::InArrowFunc,
                        ..NORMAL_CONTEXT
                    };
                    let precedence = PRECEDENCE_OF_ASSIGNMENT;
                    expression_with_parens(opts, precedence, &ctxt, expr)
                }
            },
        ]),
    )
}

fn type_cast(opts: &Opts, loc: &Loc, cast: &ast::expression::TypeCast<Loc, Loc>) -> LayoutNode {
    let expr = &cast.expression;
    let annot = &cast.annot;
    let comments = cast.comments.as_ref();
    let expr_layout = expression(opts, None, expr);
    // Work around Babel bug present in current version of Babel (7.10) where arrow functions with
    // type params in type casts are a parse error. Wrapping the arrow function in parens fixes this.
    // Babel issue: https://github.com/babel/babel/issues/11716
    let expr_layout = match expr.deref() {
        ExpressionInner::ArrowFunction { inner, .. } if inner.tparams.is_some() => {
            wrap_in_parens(false, expr_layout)
        }
        _ => expr_layout,
    };
    layout_node_with_comments_opt(
        loc,
        comments,
        wrap_in_parens(
            false,
            fuse(vec![expr_layout, type_annotation(opts, false, annot)]),
        ),
    )
}

fn as_const_expression(
    ctxt: &ExpressionContext,
    opts: &Opts,
    precedence: i32,
    loc: &Loc,
    cast: &ast::expression::AsConstExpression<Loc, Loc>,
) -> LayoutNode {
    let expr = &cast.expression;
    let comments = cast.comments.as_ref();
    let expr_layout = expression_with_parens(opts, precedence, ctxt, expr);
    let rhs = vec![atom("as"), space(), atom("const")];
    layout_node_with_comments_opt(loc, comments, fuse(vec![expr_layout, space(), fuse(rhs)]))
}

fn as_expression(
    ctxt: &ExpressionContext,
    opts: &Opts,
    precedence: i32,
    loc: &Loc,
    cast: &ast::expression::AsExpression<Loc, Loc>,
) -> LayoutNode {
    let expr = &cast.expression;
    let annot = &cast.annot;
    let comments = cast.comments.as_ref();
    layout_node_with_comments_opt(
        loc,
        comments,
        fuse(vec![
            expression_with_parens(opts, precedence, ctxt, expr),
            space(),
            atom("as"),
            space(),
            type_(opts, &annot.annotation),
        ]),
    )
}

fn ts_satisfies(
    opts: &Opts,
    loc: &Loc,
    cast: &ast::expression::TSSatisfies<Loc, Loc>,
) -> LayoutNode {
    let expr = &cast.expression;
    let annot = &cast.annot;
    let comments = cast.comments.as_ref();
    let expr_layout = expression(opts, None, expr);
    let rhs = vec![atom("satisfies"), space(), type_(opts, &annot.annotation)];
    layout_node_with_comments_opt(loc, comments, fuse(vec![expr_layout, space(), fuse(rhs)]))
}

fn variable_kind(kind: ast::VariableKind) -> LayoutNode {
    atom(flow_parser::ast_utils::string_of_variable_kind(kind))
}

fn variable_declaration(
    opts: &Opts,
    semicolon: Option<LayoutNode>,
    ctxt: Option<&ExpressionContext>,
    loc: &Loc,
    decl: &ast::statement::VariableDeclaration<Loc, Loc>,
) -> LayoutNode {
    let ctxt = ctxt.copied().unwrap_or(NORMAL_CONTEXT);
    let semicolon = semicolon.unwrap_or(LayoutNode::empty());
    let has_init = decl.declarations.iter().any(|var| var.init.is_some());
    let sep = if has_init {
        pretty_hardline()
    } else {
        pretty_line()
    };
    let decls_layout = match decl.declarations.as_ref() {
        [] => {
            // impossible
            LayoutNode::empty()
        }
        [single_decl] => variable_declarator(&ctxt, opts, single_decl),
        [hd, tl @ ..] => {
            let hd = variable_declarator(&ctxt, opts, hd);
            let tl: Vec<LayoutNode> = tl
                .iter()
                .map(|d| variable_declarator(&ctxt, opts, d))
                .collect();
            group(vec![
                hd,
                atom(","),
                LayoutNode::indent(fuse(vec![sep.dupe(), join(fuse(vec![atom(","), sep]), tl)])),
            ])
        }
    };
    source_location_with_comments(
        loc,
        decl.comments.as_ref(),
        fuse(vec![
            fuse_with_space(vec![variable_kind(decl.kind), decls_layout]),
            semicolon,
        ]),
    )
}

fn variable_declarator(
    ctxt: &ExpressionContext,
    opts: &Opts,
    decl: &ast::statement::variable::Declarator<Loc, Loc>,
) -> LayoutNode {
    let id = match &decl.id {
        ast::pattern::Pattern::Identifier { loc, inner } if decl.definite => {
            source_location_with_comments(
                loc,
                None::<&ast::Syntax<Loc, ()>>,
                fuse(vec![
                    identifier(&inner.name),
                    atom("!"),
                    hint(|a| type_annotation(opts, false, a), &inner.annot),
                ]),
            )
        }
        id => pattern(opts, Some(ctxt), id),
    };
    source_location_with_comments(
        &decl.loc,
        None::<&ast::Syntax<Loc, ()>>,
        match &decl.init {
            Some(expr) => {
                let init_separator = comment_aware_separator(
                    None,
                    pretty_space(),
                    &comment_attachment::expression_comment_bounds(expr),
                );
                fuse(vec![
                    id,
                    pretty_space(),
                    atom("="),
                    init_separator,
                    expression_with_parens(opts, PRECEDENCE_OF_ASSIGNMENT, ctxt, expr),
                ])
            }
            None => id,
        },
    )
}

fn fuse_with_default(
    opts: &Opts,
    ctxt: Option<&ExpressionContext>,
    node: LayoutNode,
    expr: &ast::expression::Expression<Loc, Loc>,
) -> LayoutNode {
    let ctxt = ctxt.copied().unwrap_or(NORMAL_CONTEXT);
    fuse(vec![
        node,
        pretty_space(),
        atom("="),
        pretty_space(),
        expression_with_parens(
            opts,
            PRECEDENCE_OF_ASSIGNMENT,
            &context_after_token(&ctxt),
            expr,
        ),
    ])
}

fn function_(opts: &Opts, loc: &Loc, func: &ast::function::Function<Loc, Loc>) -> LayoutNode {
    let prefix = {
        let s_func = fuse(vec![
            if func.effect_ == ast::function::Effect::Hook {
                atom("hook")
            } else {
                atom("function")
            },
            if func.generator {
                atom("*")
            } else {
                LayoutNode::empty()
            },
        ]);
        let id_layout = match &func.id {
            Some(id) => fuse(vec![s_func, space(), identifier(id)]),
            None => s_func,
        };
        if func.async_ {
            fuse(vec![atom("async"), space(), id_layout])
        } else {
            id_layout
        }
    };
    function_base(
        opts,
        &prefix,
        &func.params,
        Some(&func.body),
        func.predicate.as_ref(),
        &func.return_,
        func.tparams.as_ref(),
        loc,
        func.comments.as_ref(),
    )
}

fn function_base(
    opts: &Opts,
    prefix: &LayoutNode,
    params: &ast::function::Params<Loc, Loc>,
    body: Option<&ast::function::Body<Loc, Loc>>,
    predicate: Option<&ast::types::Predicate<Loc, Loc>>,
    return_: &ast::function::ReturnAnnot<Loc, Loc>,
    tparams: Option<&ast::types::TypeParams<Loc, Loc>>,
    loc: &Loc,
    comments: Option<&ast::Syntax<Loc, ()>>,
) -> LayoutNode {
    let params_loc = &params.loc;
    let params_comments = params.comments.as_ref();
    layout_node_with_comments_opt(
        loc,
        comments,
        fuse(vec![
            prefix.dupe(),
            option_layout(
                |tp| {
                    type_parameter(
                        opts,
                        flow_parser::ast_visitor::TypeParamsContext::Function,
                        tp,
                    )
                },
                tparams,
            ),
            group(vec![
                layout_node_with_comments_opt(
                    params_loc,
                    params_comments,
                    function_params(opts, params),
                ),
                function_return(opts, false, return_, predicate),
            ]),
            pretty_space(),
            option_layout(
                |b| match b {
                    ast::function::Body::BodyBlock((blk_loc, blk)) => block(opts, blk_loc, blk),
                    ast::function::Body::BodyExpression(_) => {
                        panic!("Only arrows should have BodyExpressions")
                    }
                },
                body,
            ),
        ]),
    )
}

fn function_return(
    opts: &Opts,
    arrow: bool,
    return_: &ast::function::ReturnAnnot<Loc, Loc>,
    predicate: Option<&ast::types::Predicate<Loc, Loc>>,
) -> LayoutNode {
    // Function return types in arrow functions must be wrapped in parens or else arrow
    // from arrow function is interpreted as part of the function type.
    let needs_parens = |annot: &ast::types::Annotation<Loc, Loc>| -> bool {
        if !arrow {
            return false;
        }
        match annot.annotation.deref() {
            ast::types::TypeInner::Function { inner, .. } => !inner.params.params.is_empty(),
            _ => false,
        }
    };
    match (return_, predicate) {
        (ast::function::ReturnAnnot::Missing(_), None) => LayoutNode::empty(),
        (ast::function::ReturnAnnot::Missing(_), Some(pred)) => {
            fuse(vec![atom(":"), pretty_space(), type_predicate(opts, pred)])
        }
        (ast::function::ReturnAnnot::Available(ret), Some(pred)) => fuse(vec![
            type_annotation(opts, needs_parens(ret), ret),
            pretty_space(),
            type_predicate(opts, pred),
        ]),
        (ast::function::ReturnAnnot::Available(ret), None) => {
            type_annotation(opts, needs_parens(ret), ret)
        }
        (ast::function::ReturnAnnot::TypeGuard(guard_annot), _) => {
            let needs_parens_val = if !arrow {
                false
            } else {
                match &guard_annot.guard.guard.1 {
                    Some(t) => match t.deref() {
                        ast::types::TypeInner::Function { inner, .. } => {
                            !inner.params.params.is_empty()
                        }
                        _ => false,
                    },
                    None => false,
                }
            };
            type_guard_annotation(opts, needs_parens_val, guard_annot)
        }
    }
}

fn type_predicate(opts: &Opts, pred: &ast::types::Predicate<Loc, Loc>) -> LayoutNode {
    source_location_with_comments(
        &pred.loc,
        pred.comments.as_ref(),
        fuse(vec![
            atom("%checks"),
            match &pred.kind {
                ast::types::PredicateKind::Declared(expr) => {
                    wrap_in_parens(false, expression(opts, None, expr))
                }
                ast::types::PredicateKind::Inferred => LayoutNode::empty(),
            },
        ]),
    )
}

fn component_declaration(
    opts: &Opts,
    loc: &Loc,
    comp: &ast::statement::ComponentDeclaration<Loc, Loc>,
) -> LayoutNode {
    let prefix = {
        let s_component = fuse(vec![atom("component"), space(), identifier(&comp.id)]);
        if comp.async_ {
            fuse(vec![atom("async"), space(), s_component])
        } else {
            s_component
        }
    };
    component_base(
        opts,
        prefix,
        &comp.params,
        comp.body.as_ref(),
        &comp.renders,
        comp.tparams.as_ref(),
        loc,
        comp.comments.as_ref(),
    )
}

fn class_base(opts: &Opts, loc: &Loc, class: &ast::class::Class<Loc, Loc>) -> LayoutNode {
    let decorator_parts = decorators_list(opts, &class.class_decorators);
    let class_parts: Vec<LayoutNode> = vec![
        if class.abstract_ {
            fuse(vec![atom("abstract"), space()])
        } else {
            LayoutNode::empty()
        },
        atom("class"),
        match &class.id {
            Some(ident) => fuse(vec![
                space(),
                identifier(ident),
                option_layout(
                    |tp| {
                        type_parameter(opts, flow_parser::ast_visitor::TypeParamsContext::Class, tp)
                    },
                    class.tparams.as_ref(),
                ),
            ]),
            None => LayoutNode::empty(),
        },
    ];
    let class_extends: Vec<Option<LayoutNode>> = vec![
        class.extends.as_ref().map(|ext| {
            source_location_with_comments(
                &ext.loc,
                ext.comments.as_ref(),
                fuse(vec![
                    atom("extends"),
                    space(),
                    source_location_with_comments(
                        &ext.loc,
                        None::<&ast::Syntax<Loc, ()>>,
                        fuse(vec![
                            expression(opts, None, &ext.expr),
                            option_layout(|targs| type_args(opts, targs), ext.targs.as_ref()),
                        ]),
                    ),
                ]),
            )
        }),
        class_implements(opts, class.implements.as_ref()),
    ];
    let extends_parts: Vec<LayoutNode> = match deoptionalize(class_extends).as_slice() {
        [] => vec![],
        items => vec![LayoutNode::indent(fuse(vec![
            line(),
            join(line(), items.to_vec()),
        ]))],
    };
    let mut parts: Vec<LayoutNode> = Vec::new();
    parts.extend(class_parts);
    parts.extend(extends_parts);
    parts.push(pretty_space());
    parts.push(class_body(opts, &class.body));
    group(vec![
        decorator_parts,
        source_location_with_comments(loc, class.comments.as_ref(), group(parts)),
    ])
}

fn record_declaration(
    opts: &Opts,
    loc: &Loc,
    record: &ast::statement::RecordDeclaration<Loc, Loc>,
) -> LayoutNode {
    let implements_layout = class_implements(opts, record.implements.as_ref())
        .map(|impl_layout| fuse(vec![pretty_space(), impl_layout]))
        .unwrap_or(LayoutNode::empty());
    source_location_with_comments(
        loc,
        record.comments.as_ref(),
        fuse(vec![
            atom("record"),
            space(),
            identifier(&record.id),
            option_layout(
                |tp| {
                    type_parameter(
                        opts,
                        flow_parser::ast_visitor::TypeParamsContext::Record,
                        tp,
                    )
                },
                record.tparams.as_ref(),
            ),
            implements_layout,
            pretty_space(),
            record_body(opts, &record.body),
        ]),
    )
}

fn record_property(
    opts: &Opts,
    prop: &ast::statement::record_declaration::Property<Loc, Loc>,
) -> LayoutNode {
    let default_value = match &prop.default_value {
        Some(expr) => fuse(vec![
            pretty_space(),
            atom("="),
            pretty_space(),
            expression(opts, None, expr),
        ]),
        None => LayoutNode::empty(),
    };
    source_location_with_comments(
        &prop.loc,
        prop.comments.as_ref(),
        group(vec![
            object_property_key(opts, &prop.key),
            type_annotation(opts, false, &prop.annot),
            default_value,
            atom(","),
        ]),
    )
}

fn record_static_property(
    opts: &Opts,
    prop: &ast::statement::record_declaration::StaticProperty<Loc, Loc>,
) -> LayoutNode {
    source_location_with_comments(
        &prop.loc,
        prop.comments.as_ref(),
        group(vec![
            atom("static"),
            space(),
            object_property_key(opts, &prop.key),
            type_annotation(opts, false, &prop.annot),
            pretty_space(),
            atom("="),
            pretty_space(),
            expression(opts, None, &prop.value),
            atom(","),
        ]),
    )
}

fn record_body(
    opts: &Opts,
    body: &ast::statement::record_declaration::Body<Loc, Loc>,
) -> LayoutNode {
    let elements: Vec<LayoutNode> = body
        .body
        .iter()
        .map(|elem| match elem {
            ast::statement::record_declaration::BodyElement::Property(prop) => {
                record_property(opts, prop)
            }
            ast::statement::record_declaration::BodyElement::StaticProperty(prop) => {
                record_static_property(opts, prop)
            }
            ast::statement::record_declaration::BodyElement::Method(meth) => {
                class_method(opts, meth)
            }
        })
        .collect();
    if !body.body.is_empty() {
        source_location_with_comments(
            &body.loc,
            body.comments.as_ref(),
            group(vec![wrap_and_indent(
                (atom("{"), atom("}")),
                Some(pretty_hardline()),
                vec![join(pretty_hardline(), elements)],
            )]),
        )
    } else {
        source_location_with_comments(&body.loc, body.comments.as_ref(), atom("{}"))
    }
}

fn enum_declaration(
    def: LayoutNode,
    loc: &Loc,
    enum_: &ast::statement::EnumDeclaration<Loc, Loc>,
) -> LayoutNode {
    let def = if enum_.const_ {
        fuse(vec![atom("const"), space(), def])
    } else {
        def
    };
    let wrap_body = |members: Vec<LayoutNode>| -> LayoutNode {
        wrap_and_indent(
            (atom("{"), atom("}")),
            Some(pretty_hardline()),
            vec![join(pretty_hardline(), members)],
        )
    };
    let enum_member_name_layout =
        |id: &ast::statement::enum_declaration::MemberName<Loc>| -> LayoutNode {
            match id {
                ast::statement::enum_declaration::MemberName::Identifier(ident) => {
                    identifier(ident)
                }
                ast::statement::enum_declaration::MemberName::StringLiteral(loc, str_lit) => {
                    layout_node_with_comments_opt(
                        loc,
                        str_lit.comments.as_ref(),
                        atom(&str_lit.raw),
                    )
                }
            }
        };
    let initialized_member =
        |id: &ast::statement::enum_declaration::MemberName<Loc>, value: LayoutNode| -> LayoutNode {
            fuse(vec![
                enum_member_name_layout(id),
                pretty_space(),
                atom("="),
                pretty_space(),
                value,
                atom(","),
            ])
        };
    let unknown_members = |has: Option<&Loc>| -> Vec<LayoutNode> {
        match has {
            Some(_) => vec![atom("...")],
            None => vec![],
        }
    };
    let enum_internal_comments =
        |comments: Option<&ast::Syntax<Loc, Arc<[ast::Comment<Loc>]>>>| -> Vec<LayoutNode> {
            match internal_comments(comments) {
                None => vec![],
                Some((_, _, layout)) => vec![layout],
            }
        };
    let body_layout = {
        let ast::statement::enum_declaration::Body {
            loc: ref bloc,
            ref members,
            ref explicit_type,
            ref has_unknown_members,
            ref comments,
        } = enum_.body;
        let rep_type = match explicit_type {
            Some((_loc, et)) => fuse(vec![space(), atom("of"), space(), atom(et.as_str())]),
            None => LayoutNode::empty(),
        };
        let member_layout = |m: &ast::statement::enum_declaration::Member<Loc>| -> LayoutNode {
            use ast::statement::enum_declaration::Member;
            match m {
                Member::BooleanMember(m) => initialized_member(
                    &m.id,
                    layout_node_with_comments_opt(
                        &m.init.0,
                        m.init.1.comments.as_ref(),
                        if m.init.1.value {
                            atom("true")
                        } else {
                            atom("false")
                        },
                    ),
                ),
                Member::NumberMember(m) => initialized_member(
                    &m.id,
                    layout_node_with_comments_opt(
                        &m.init.0,
                        m.init.1.comments.as_ref(),
                        atom(&m.init.1.raw),
                    ),
                ),
                Member::StringMember(m) => initialized_member(
                    &m.id,
                    layout_node_with_comments_opt(
                        &m.init.0,
                        m.init.1.comments.as_ref(),
                        atom(&m.init.1.raw),
                    ),
                ),
                Member::BigIntMember(m) => initialized_member(
                    &m.id,
                    layout_node_with_comments_opt(
                        &m.init.0,
                        m.init.1.comments.as_ref(),
                        atom(&m.init.1.raw),
                    ),
                ),
                Member::DefaultedMember(m) => fuse(vec![enum_member_name_layout(&m.id), atom(",")]),
            }
        };
        fuse(vec![
            rep_type,
            pretty_space(),
            layout_node_with_comments_opt(
                bloc,
                comments.as_ref(),
                wrap_body(
                    members
                        .iter()
                        .map(member_layout)
                        .chain(enum_internal_comments(comments.as_ref()))
                        .chain(unknown_members(has_unknown_members.as_ref()))
                        .collect(),
                ),
            ),
        ])
    };
    layout_node_with_comments_opt(
        loc,
        enum_.comments.as_ref(),
        fuse(vec![def, space(), identifier(&enum_.id), body_layout]),
    )
}

fn import_attributes(
    opts: &Opts,
    attributes: &Option<(
        Loc,
        Vec<ast::statement::import_declaration::ImportAttribute<Loc, Loc>>,
    )>,
) -> LayoutNode {
    match attributes {
        None => LayoutNode::empty(),
        Some((_, attrs)) => fuse(vec![
            pretty_space(),
            atom("with"),
            pretty_space(),
            group(vec![new_list(
                Some((atom("{"), atom("}"))),
                Some(atom(",")),
                opts.bracket_spacing,
                true,
                attrs
                    .iter()
                    .map(|attr| {
                        let key_layout = match &attr.key {
                            ast::statement::import_declaration::ImportAttributeKey::Identifier(id) => {
                                identifier(id)
                            }
                            ast::statement::import_declaration::ImportAttributeKey::StringLiteral(
                                aloc,
                                lit,
                            ) => string_literal(opts, aloc, lit),
                        };
                        fuse(vec![
                            key_layout,
                            atom(":"),
                            pretty_space(),
                            string_literal(opts, &attr.value.0, &attr.value.1),
                        ])
                    })
                    .collect(),
            )]),
        ]),
    }
}

fn import_declaration(
    opts: &Opts,
    loc: &Loc,
    import: &ast::statement::ImportDeclaration<Loc, Loc>,
) -> LayoutNode {
    let s_from = fuse(vec![atom("from"), pretty_space()]);
    layout_node_with_comments_opt(
        loc,
        import.comments.as_ref(),
        with_semicolon(fuse(vec![
            atom("import"),
            match import.phase {
                Some(ast::statement::ImportPhase::Defer) => fuse(vec![space(), atom("defer")]),
                None => LayoutNode::empty(),
            },
            match import.import_kind {
                ast::statement::ImportKind::ImportType => fuse(vec![space(), atom("type")]),
                ast::statement::ImportKind::ImportTypeof => fuse(vec![space(), atom("typeof")]),
                ast::statement::ImportKind::ImportValue => LayoutNode::empty(),
            },
            {
                let (special, named) =
                    partition_specifiers(opts, import.default.as_ref(), import.specifiers.as_ref());
                match (special.as_slice(), &named, &import.import_kind) {
                    // No export specifiers
                    // `import 'module-name';`
                    ([], None, ast::statement::ImportKind::ImportValue) => pretty_space(),
                    // `import type {} from 'module-name';`
                    ([], None, _) => fuse(vec![
                        pretty_space(),
                        atom("{}"),
                        pretty_space(),
                        s_from.dupe(),
                    ]),
                    // Only has named specifiers
                    ([], Some(named_layout), _) => fuse(vec![
                        pretty_space(),
                        named_layout.dupe(),
                        pretty_space(),
                        s_from.dupe(),
                    ]),
                    // Only has default or namedspaced specifiers
                    (special_items, None, _) => fuse(vec![
                        space(),
                        layout::fuse_list(Some(atom(",")), None, special_items.to_vec()),
                        space(),
                        s_from.dupe(),
                    ]),
                    // Has both default or namedspaced specifiers and named specifiers
                    (special_items, Some(named_layout), _) => {
                        let mut all = special_items.to_vec();
                        all.push(named_layout.dupe());
                        fuse(vec![
                            space(),
                            layout::fuse_list(Some(atom(",")), None, all),
                            pretty_space(),
                            s_from.dupe(),
                        ])
                    }
                }
            },
            string_literal(opts, &import.source.0, &import.source.1),
            import_attributes(opts, &import.attributes),
        ])),
    )
}

fn import_equals_declaration(
    opts: &Opts,
    loc: &Loc,
    import: &ast::statement::ImportEqualsDeclaration<Loc, Loc>,
) -> LayoutNode {
    let module_ref_layout = match &import.module_reference {
        ast::statement::import_equals_declaration::ModuleReference::ExternalModuleReference(
            ref_loc,
            lit,
        ) => fuse(vec![
            atom("require"),
            atom("("),
            string_literal(opts, ref_loc, lit),
            atom(")"),
        ]),
        ast::statement::import_equals_declaration::ModuleReference::Identifier(git) => {
            generic_identifier(opts, git)
        }
    };
    layout_node_with_comments_opt(
        loc,
        import.comments.as_ref(),
        with_semicolon(fuse(vec![
            if import.is_export {
                fuse(vec![atom("export"), space()])
            } else {
                LayoutNode::empty()
            },
            atom("import"),
            match import.import_kind {
                ast::statement::ImportKind::ImportType => fuse(vec![space(), atom("type")]),
                ast::statement::ImportKind::ImportTypeof => fuse(vec![space(), atom("typeof")]),
                ast::statement::ImportKind::ImportValue => LayoutNode::empty(),
            },
            space(),
            identifier(&import.id),
            pretty_space(),
            atom("="),
            pretty_space(),
            module_ref_layout,
        ])),
    )
}

fn export_declaration(
    opts: &Opts,
    loc: &Loc,
    export: &ast::statement::ExportNamedDeclaration<Loc, Loc>,
) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        export.comments.as_ref(),
        fuse(vec![
            atom("export"),
            match (&export.declaration, &export.specifiers) {
                (Some(decl), None) => fuse(vec![space(), statement(opts, false, decl)]),
                (None, Some(specifier)) => with_semicolon(fuse(vec![
                    match export.export_kind {
                        ast::statement::ExportKind::ExportType => fuse(vec![space(), atom("type")]),
                        ast::statement::ExportKind::ExportValue => LayoutNode::empty(),
                    },
                    pretty_space(),
                    export_specifier(opts, export.source.as_ref(), specifier),
                    import_attributes(opts, &export.attributes),
                ])),
                _ => panic!("Invalid export declaration"),
            },
        ]),
    )
}

fn export_default_declaration(
    opts: &Opts,
    loc: &Loc,
    export: &ast::statement::ExportDefaultDeclaration<Loc, Loc>,
) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        export.comments.as_ref(),
        fuse(vec![
            atom("export"),
            space(),
            atom("default"),
            space(),
            match &export.declaration {
                ast::statement::export_default_declaration::Declaration::Declaration(stat) => {
                    statement(opts, false, stat)
                }
                ast::statement::export_default_declaration::Declaration::Expression(expr) => {
                    with_semicolon(expression(opts, None, expr))
                }
            },
        ]),
    )
}

fn export_assignment(
    opts: &Opts,
    loc: &Loc,
    assign: &ast::statement::ExportAssignment<Loc, Loc>,
) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        assign.comments.as_ref(),
        match &assign.rhs {
            ast::statement::ExportAssignmentRhs::Expression(expr) => with_semicolon(fuse(vec![
                atom("export"),
                pretty_space(),
                atom("="),
                pretty_space(),
                expression(opts, None, expr),
            ])),
            ast::statement::ExportAssignmentRhs::DeclareFunction(fn_loc, decl) => {
                with_semicolon(fuse(vec![
                    atom("export"),
                    pretty_space(),
                    atom("="),
                    pretty_space(),
                    source_location_with_comments(
                        fn_loc,
                        decl.comments.as_ref(),
                        declare_function_body(opts, decl),
                    ),
                ]))
            }
        },
    )
}

fn namespace_export_declaration(
    loc: &Loc,
    decl: &ast::statement::NamespaceExportDeclaration<Loc, Loc>,
) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        decl.comments.as_ref(),
        with_semicolon(fuse(vec![
            atom("export"),
            space(),
            atom("as"),
            space(),
            atom("namespace"),
            space(),
            identifier(&decl.id),
        ])),
    )
}

fn type_alias(
    opts: &Opts,
    declare: bool,
    loc: &Loc,
    alias: &ast::statement::TypeAlias<Loc, Loc>,
) -> LayoutNode {
    let right_separator = comment_aware_separator(
        None,
        pretty_space(),
        &comment_attachment::type_comment_bounds(&alias.right),
    );
    layout_node_with_comments_opt(
        loc,
        alias.comments.as_ref(),
        group(vec![with_semicolon(fuse(vec![
            if declare {
                fuse(vec![atom("declare"), space()])
            } else {
                LayoutNode::empty()
            },
            atom("type"),
            space(),
            identifier(&alias.id),
            option_layout(
                |tp| {
                    type_parameter(
                        opts,
                        flow_parser::ast_visitor::TypeParamsContext::TypeAlias,
                        tp,
                    )
                },
                alias.tparams.as_ref(),
            ),
            pretty_space(),
            atom("="),
            right_separator,
            type_(opts, &alias.right),
        ]))]),
    )
}

fn opaque_type(
    opts: &Opts,
    declare: bool,
    loc: &Loc,
    opaque: &ast::statement::OpaqueType<Loc, Loc>,
) -> LayoutNode {
    let upper_bound: Vec<LayoutNode> = match &opaque.upper_bound {
        Some(t) => vec![
            pretty_space(),
            atom("extends"),
            pretty_space(),
            type_(opts, t),
        ],
        None => match &opaque.legacy_upper_bound {
            Some(t) => vec![atom(":"), pretty_space(), type_(opts, t)],
            None => vec![],
        },
    };
    let bounds: Vec<LayoutNode> = match &opaque.lower_bound {
        None => upper_bound,
        Some(lower_bound_t) => {
            let mut b = vec![
                pretty_space(),
                atom("super"),
                pretty_space(),
                type_(opts, lower_bound_t),
            ];
            b.extend(upper_bound);
            b
        }
    };
    let mut parts = vec![
        if declare {
            fuse(vec![atom("declare"), space()])
        } else {
            LayoutNode::empty()
        },
        atom("opaque type"),
        space(),
        identifier(&opaque.id),
        option_layout(
            |tp| {
                type_parameter(
                    opts,
                    flow_parser::ast_visitor::TypeParamsContext::OpaqueType,
                    tp,
                )
            },
            opaque.tparams.as_ref(),
        ),
    ];
    parts.extend(bounds);
    if let Some(impl_type) = &opaque.impl_type {
        parts.extend(vec![
            pretty_space(),
            atom("="),
            pretty_space(),
            type_(opts, impl_type),
        ]);
    }
    layout_node_with_comments_opt(loc, opaque.comments.as_ref(), with_semicolon(fuse(parts)))
}

fn interface_declaration(
    opts: &Opts,
    loc: &Loc,
    interface: &ast::statement::Interface<Loc, Loc>,
) -> LayoutNode {
    interface_declaration_base(opts, fuse(vec![atom("interface"), space()]), loc, interface)
}

fn declare_class(
    opts: &Opts,
    loc: &Loc,
    class: &ast::statement::DeclareClass<Loc, Loc>,
) -> LayoutNode {
    declare_class_with_type(opts, LayoutNode::empty(), loc, class)
}

fn declare_component(
    opts: &Opts,
    loc: &Loc,
    comp: &ast::statement::DeclareComponent<Loc, Loc>,
) -> LayoutNode {
    declare_component_with_type(opts, LayoutNode::empty(), loc, comp)
}

fn declare_enum(loc: &Loc, enum_: &ast::statement::EnumDeclaration<Loc, Loc>) -> LayoutNode {
    enum_declaration(
        fuse(vec![atom("declare"), space(), atom("enum")]),
        loc,
        enum_,
    )
}

fn declare_function(
    opts: &Opts,
    loc: &Loc,
    func: &ast::statement::DeclareFunction<Loc, Loc>,
) -> LayoutNode {
    declare_function_with_type(opts, LayoutNode::empty(), loc, func)
}

fn declare_interface(
    opts: &Opts,
    loc: &Loc,
    interface: &ast::statement::Interface<Loc, Loc>,
) -> LayoutNode {
    interface_declaration_base(
        opts,
        fuse(vec![atom("declare"), space(), atom("interface"), space()]),
        loc,
        interface,
    )
}

fn declare_variable(
    opts: &Opts,
    loc: &Loc,
    var: &ast::statement::DeclareVariable<Loc, Loc>,
) -> LayoutNode {
    declare_variable_with_type(opts, LayoutNode::empty(), loc, var)
}

fn declare_module_exports(
    opts: &Opts,
    loc: &Loc,
    exports: &ast::statement::DeclareModuleExports<Loc, Loc>,
) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        exports.comments.as_ref(),
        with_semicolon(fuse(vec![
            atom("declare"),
            space(),
            atom("module.exports"),
            type_annotation(opts, false, &exports.annot),
        ])),
    )
}

fn declare_module(
    opts: &Opts,
    loc: &Loc,
    module: &ast::statement::DeclareModule<Loc, Loc>,
) -> LayoutNode {
    source_location_with_comments(
        loc,
        module.comments.as_ref(),
        fuse(vec![
            atom("declare"),
            space(),
            atom("module"),
            space(),
            match &module.id {
                ast::statement::declare_module::Id::Identifier(id) => identifier(id),
                ast::statement::declare_module::Id::Literal((lit_loc, lit)) => {
                    string_literal(opts, lit_loc, lit)
                }
            },
            pretty_space(),
            block(opts, &module.body.0, &module.body.1),
        ]),
    )
}

fn declare_namespace(
    opts: &Opts,
    loc: &Loc,
    ns: &ast::statement::DeclareNamespace<Loc, Loc>,
) -> LayoutNode {
    source_location_with_comments(
        loc,
        ns.comments.as_ref(),
        fuse(match &ns.id {
            ast::statement::declare_namespace::Id::Global(id) => vec![
                if ns.implicit_declare {
                    LayoutNode::empty()
                } else {
                    fuse(vec![atom("declare"), space()])
                },
                identifier(id),
                pretty_space(),
                block(opts, &ns.body.0, &ns.body.1),
            ],
            ast::statement::declare_namespace::Id::Local(id) => {
                let keyword_atom = match ns.keyword {
                    ast::statement::declare_namespace::Keyword::Module => atom("module"),
                    ast::statement::declare_namespace::Keyword::Namespace => atom("namespace"),
                };
                vec![
                    if ns.implicit_declare {
                        LayoutNode::empty()
                    } else {
                        fuse(vec![atom("declare"), space()])
                    },
                    keyword_atom,
                    space(),
                    identifier(id),
                    pretty_space(),
                    block(opts, &ns.body.0, &ns.body.1),
                ]
            }
        }),
    )
}

fn declare_export_declaration(
    opts: &Opts,
    loc: &Loc,
    export: &ast::statement::DeclareExportDeclaration<Loc, Loc>,
) -> LayoutNode {
    let s_export = fuse(vec![
        atom("export"),
        space(),
        if export.default.is_some() {
            fuse(vec![atom("default"), space()])
        } else {
            LayoutNode::empty()
        },
    ]);
    match (&export.declaration, &export.specifiers) {
        (Some(decl), None) => match decl {
            // declare export var
            ast::statement::declare_export_declaration::Declaration::Variable {
                loc: dloc,
                declaration: var,
            } => source_location_with_comments(
                loc,
                export.comments.as_ref(),
                declare_variable_with_type(opts, s_export, dloc, var),
            ),
            // declare export function
            ast::statement::declare_export_declaration::Declaration::Function {
                loc: dloc,
                declaration: func,
            } => source_location_with_comments(
                loc,
                export.comments.as_ref(),
                declare_function_with_type(opts, s_export, dloc, func),
            ),
            // declare export class
            ast::statement::declare_export_declaration::Declaration::Class {
                loc: dloc,
                declaration: c,
            } => source_location_with_comments(
                loc,
                export.comments.as_ref(),
                declare_class_with_type(opts, s_export, dloc, c),
            ),
            // declare export component
            ast::statement::declare_export_declaration::Declaration::Component {
                loc: dloc,
                declaration: c,
            } => source_location_with_comments(
                loc,
                export.comments.as_ref(),
                declare_component_with_type(opts, s_export, dloc, c),
            ),
            // declare export default [type]
            // this corresponds to things like `export default 1+1;`
            ast::statement::declare_export_declaration::Declaration::DefaultType { type_: t } => {
                source_location_with_comments(
                    loc,
                    export.comments.as_ref(),
                    with_semicolon(fuse(vec![
                        atom("declare"),
                        space(),
                        s_export,
                        type_(opts, t),
                    ])),
                )
            }
            // declare export type
            ast::statement::declare_export_declaration::Declaration::NamedType {
                loc: dloc,
                declaration: type_alias_inner,
            } => source_location_with_comments(
                loc,
                export.comments.as_ref(),
                fuse(vec![
                    atom("declare"),
                    space(),
                    s_export,
                    type_alias(opts, false, dloc, type_alias_inner),
                ]),
            ),
            // declare export opaque type
            ast::statement::declare_export_declaration::Declaration::NamedOpaqueType {
                loc: dloc,
                declaration: opaque_type_inner,
            } => source_location_with_comments(
                loc,
                export.comments.as_ref(),
                fuse(vec![
                    atom("declare"),
                    space(),
                    s_export,
                    opaque_type(opts, false, dloc, opaque_type_inner),
                ]),
            ),
            // declare export interface
            ast::statement::declare_export_declaration::Declaration::Interface {
                loc: dloc,
                declaration: interface_inner,
            } => source_location_with_comments(
                loc,
                export.comments.as_ref(),
                fuse(vec![
                    atom("declare"),
                    space(),
                    s_export,
                    interface_declaration(opts, dloc, interface_inner),
                ]),
            ),
            // declare export enum
            ast::statement::declare_export_declaration::Declaration::Enum {
                loc: dloc,
                declaration: enum_inner,
            } => source_location_with_comments(
                loc,
                export.comments.as_ref(),
                fuse(vec![
                    atom("declare"),
                    space(),
                    s_export,
                    enum_declaration(atom("enum"), dloc, enum_inner),
                ]),
            ),
            // declare export namespace
            ast::statement::declare_export_declaration::Declaration::Namespace {
                loc: _dloc,
                declaration: ns,
            } => match &ns.id {
                ast::statement::declare_namespace::Id::Global(_) => {
                    panic!("Global namespace cannot be exported")
                }
                ast::statement::declare_namespace::Id::Local(id) => {
                    let keyword_atom = match ns.keyword {
                        ast::statement::declare_namespace::Keyword::Module => atom("module"),
                        ast::statement::declare_namespace::Keyword::Namespace => atom("namespace"),
                    };
                    source_location_with_comments(
                        loc,
                        export.comments.as_ref(),
                        fuse(vec![
                            if ns.implicit_declare {
                                LayoutNode::empty()
                            } else {
                                fuse(vec![atom("declare"), space()])
                            },
                            s_export,
                            keyword_atom,
                            space(),
                            identifier(id),
                            pretty_space(),
                            block(opts, &ns.body.0, &ns.body.1),
                        ]),
                    )
                }
            },
        },
        (None, Some(specifier)) => source_location_with_comments(
            loc,
            export.comments.as_ref(),
            fuse(vec![
                atom("declare"),
                space(),
                atom("export"),
                pretty_space(),
                export_specifier(opts, export.source.as_ref(), specifier),
            ]),
        ),
        _ => panic!("Invalid declare export declaration"),
    }
}

fn switch_case(
    opts: &Opts,
    last: bool,
    case: &ast::statement::switch::Case<Loc, Loc>,
) -> LayoutNode {
    let case_left = layout_node_with_comments_opt(
        &case.loc,
        case.comments.as_ref(),
        match &case.test {
            Some(expr) => fuse_with_space(vec![
                atom("case"),
                fuse(vec![expression(opts, None, expr), atom(":")]),
            ]),
            None => atom("default:"),
        },
    );
    source_location_with_comments(
        &case.loc,
        None::<&ast::Syntax<Loc, ()>>,
        match case.consequent.as_ref() {
            [] => case_left,
            _ => {
                let statements = statement_list(opts, last, &case.consequent);
                fuse(vec![
                    case_left,
                    LayoutNode::indent(fuse(vec![pretty_hardline(), fuse(statements)])),
                ])
            }
        },
    )
}

fn match_case<B, F>(
    opts: &Opts,
    case: &ast::match_::Case<Loc, Loc, B>,
    on_case_body: &F,
) -> LayoutNode
where
    F: Fn(&Opts, &B) -> LayoutNode,
{
    layout_node_with_comments_opt(
        &case.loc,
        case.comments.as_ref(),
        fuse(vec![
            match_pattern(opts, &case.pattern),
            match_case_guard(opts, case.guard.as_ref()),
            pretty_space(),
            atom("=>"),
            pretty_space(),
            on_case_body(opts, &case.body),
        ]),
    )
}

fn component_base(
    opts: &Opts,
    prefix: LayoutNode,
    params: &ast::statement::component_params::Params<Loc, Loc>,
    body: Option<&(Loc, ast::statement::Block<Loc, Loc>)>,
    renders: &ast::types::ComponentRendersAnnotation<Loc, Loc>,
    tparams: Option<&ast::types::TypeParams<Loc, Loc>>,
    loc: &Loc,
    comments: Option<&ast::Syntax<Loc, ()>>,
) -> LayoutNode {
    let params_loc = &params.loc;
    let params_comments = params.comments.as_ref();
    layout_node_with_comments_opt(
        loc,
        comments,
        fuse(vec![
            prefix,
            option_layout(
                |tp| {
                    type_parameter(
                        opts,
                        flow_parser::ast_visitor::TypeParamsContext::ComponentDeclaration,
                        tp,
                    )
                },
                tparams,
            ),
            group(vec![
                layout_node_with_comments_opt(
                    params_loc,
                    params_comments,
                    component_params(opts, params),
                ),
                component_renders(opts, renders),
            ]),
            match body {
                None => LayoutNode::empty(),
                Some((bloc, blk)) => fuse(vec![pretty_space(), block(opts, bloc, blk)]),
            },
        ]),
    )
}

fn component_params(
    opts: &Opts,
    params_node: &ast::statement::component_params::Params<Loc, Loc>,
) -> LayoutNode {
    let mut params: Vec<(Loc, CommentsBounds, LayoutNode)> = params_node
        .params
        .iter()
        .map(|param| {
            (
                param.loc.dupe(),
                comment_attachment::component_param_comment_bounds(param),
                component_param(opts, param),
            )
        })
        .collect();
    // Add rest param
    if let Some(rest) = &params_node.rest {
        let rest_layout = source_location_with_comments(
            &rest.loc,
            rest.comments.as_ref(),
            fuse(vec![atom("..."), pattern(opts, None, &rest.argument)]),
        );
        let rest_param = (
            rest.loc.dupe(),
            comment_attachment::component_rest_param_comment_bounds(rest),
            rest_layout,
        );
        params.push(rest_param);
    }
    let params_layout = list_with_newlines_impl(atom(","), pretty_line(), false, &params);
    // Add trailing comma
    let mut params_layout = if !params.is_empty()
        && params_node.rest.is_none()
        && TrailingCommas::enabled_for_function_params(opts.trailing_commas)
    {
        let mut pl = params_layout;
        pl.push(if_break(atom(","), LayoutNode::empty()));
        pl
    } else {
        params_layout
    };
    params_layout =
        list_add_internal_comments(&params, params_layout, params_node.comments.as_ref());
    wrap_and_indent((atom("("), atom(")")), None, params_layout)
}

fn component_param(
    opts: &Opts,
    param: &ast::statement::component_params::Param<Loc, Loc>,
) -> LayoutNode {
    let local_node = pattern(opts, None, &param.local);
    let node = match param.shorthand {
        true => local_node,
        false => {
            let name_node = component_param_name(opts, &param.name);
            fuse(vec![name_node, space(), atom("as"), space(), local_node])
        }
    };
    let node = match &param.default {
        Some(expr) => fuse_with_default(opts, None, node, expr),
        None => node,
    };
    source_location_with_comments(&param.loc, None::<&ast::Syntax<Loc, ()>>, node)
}

fn component_param_name(
    opts: &Opts,
    name: &ast::statement::component_params::ParamName<Loc, Loc>,
) -> LayoutNode {
    match name {
        ast::statement::component_params::ParamName::Identifier(name) => identifier(name),
        ast::statement::component_params::ParamName::StringLiteral((loc, lit)) => {
            string_literal(opts, loc, lit)
        }
    }
}

fn component_renders(
    opts: &Opts,
    renders: &ast::types::ComponentRendersAnnotation<Loc, Loc>,
) -> LayoutNode {
    match renders {
        ast::types::ComponentRendersAnnotation::MissingRenders(_) => LayoutNode::empty(),
        ast::types::ComponentRendersAnnotation::AvailableRenders(loc, renders_inner) => {
            source_location_with_comments(
                loc,
                None::<&ast::Syntax<Loc, ()>>,
                fuse(vec![space(), render_type(opts, loc, renders_inner)]),
            )
        }
    }
}

fn decorators_list(opts: &Opts, decorators: &[ast::class::Decorator<Loc, Loc>]) -> LayoutNode {
    if !decorators.is_empty() {
        let decorators: Vec<LayoutNode> = decorators
            .iter()
            .map(|dec| {
                // Magic number, after `Call` but before `Update`
                source_location_with_comments(
                    &dec.loc,
                    dec.comments.as_ref(),
                    fuse(vec![atom("@"), {
                        let precedence = 18;
                        expression_with_parens(opts, precedence, &NORMAL_CONTEXT, &dec.expression)
                    }]),
                )
            })
            .collect();
        group(vec![
            join(pretty_line(), decorators),
            if_pretty(hardline(), space()),
        ])
    } else {
        LayoutNode::empty()
    }
}

fn class_body(opts: &Opts, body: &ast::class::Body<Loc, Loc>) -> LayoutNode {
    let elements: Vec<(Loc, CommentsBounds, LayoutNode)> = body
        .body
        .iter()
        .map(|elem| match elem {
            ast::class::BodyElement::Method(meth) => {
                let loc = meth.loc.dupe();
                let bounds = comment_attachment::comment_bounds(&meth.loc, |collector| {
                    collector.class_method(meth)
                });
                (loc, bounds, class_method(opts, meth))
            }
            ast::class::BodyElement::Property(prop) => {
                let loc = prop.loc.dupe();
                let bounds = comment_attachment::comment_bounds(&prop.loc, |collector| {
                    collector.class_property(prop)
                });
                (loc, bounds, class_property(opts, prop))
            }
            ast::class::BodyElement::PrivateField(field) => {
                let loc = field.loc.dupe();
                let bounds = comment_attachment::comment_bounds(&field.loc, |collector| {
                    collector.class_private_field(field)
                });
                (loc, bounds, class_private_field(opts, field))
            }
            ast::class::BodyElement::StaticBlock(sblock) => {
                let loc = sblock.loc.dupe();
                let bounds = comment_attachment::comment_bounds(&sblock.loc, |collector| {
                    collector.class_static_block(sblock)
                });
                (loc, bounds, class_static_block(opts, sblock))
            }
            ast::class::BodyElement::DeclareMethod(decl_meth) => {
                let loc = decl_meth.loc.dupe();
                let bounds = comment_attachment::comment_bounds(&decl_meth.loc, |collector| {
                    collector.class_declare_method(decl_meth)
                });
                (loc, bounds, class_declare_method(opts, decl_meth))
            }
            ast::class::BodyElement::AbstractMethod(abs_meth) => {
                let loc = abs_meth.loc.dupe();
                let bounds = comment_attachment::comment_bounds(&abs_meth.loc, |collector| {
                    collector.class_abstract_method(abs_meth)
                });
                (loc, bounds, class_abstract_method(opts, abs_meth))
            }
            ast::class::BodyElement::AbstractProperty(abs_prop) => {
                let loc = abs_prop.loc.dupe();
                let bounds = comment_attachment::comment_bounds(&abs_prop.loc, |collector| {
                    collector.class_abstract_property(abs_prop)
                });
                (loc, bounds, class_abstract_property(opts, abs_prop))
            }
            ast::class::BodyElement::IndexSignature(indexer) => {
                let loc = indexer.loc.dupe();
                let bounds = comment_attachment::comment_bounds(&indexer.loc, |collector| {
                    collector.object_indexer_property_type(indexer)
                });
                (loc, bounds, class_index_signature(opts, indexer))
            }
        })
        .collect();
    let layout = list_with_newlines(&elements);
    if !body.body.is_empty() {
        source_location_with_comments(
            &body.loc,
            body.comments.as_ref(),
            group(vec![wrap_and_indent(
                (atom("{"), atom("}")),
                Some(pretty_hardline()),
                vec![fuse(layout)],
            )]),
        )
    } else {
        source_location_with_comments(&body.loc, body.comments.as_ref(), atom("{}"))
    }
}

fn class_static_block(opts: &Opts, sblock: &ast::class::StaticBlock<Loc, Loc>) -> LayoutNode {
    let statements = statement_list(opts, true, &sblock.body);
    source_location_with_comments(
        &sblock.loc,
        sblock.comments.as_ref(),
        if !statements.is_empty() {
            let body = group(vec![wrap_and_indent(
                (atom("{"), atom("}")),
                Some(pretty_hardline()),
                vec![fuse(statements)],
            )]);
            fuse(vec![atom("static"), pretty_space(), body])
        } else {
            match internal_comments(sblock.comments.as_ref()) {
                None => fuse(vec![atom("static"), pretty_space(), atom("{}")]),
                Some((_, _, comments_layout)) => fuse(vec![
                    atom("static"),
                    pretty_space(),
                    atom("{"),
                    comments_layout,
                    atom("}"),
                ]),
            }
        },
    )
}

fn class_implements(
    opts: &Opts,
    implements: Option<&ast::class::Implements<Loc, Loc>>,
) -> Option<LayoutNode> {
    implements.map(|impl_| {
        source_location_with_comments(
            &impl_.loc,
            impl_.comments.as_ref(),
            fuse(vec![
                atom("implements"),
                space(),
                layout::fuse_list(
                    Some(atom(",")),
                    None,
                    impl_
                        .interfaces
                        .iter()
                        .map(|iface| {
                            source_location_with_comments(
                                &iface.loc,
                                None::<&ast::Syntax<Loc, ()>>,
                                fuse(vec![
                                    generic_identifier(opts, &iface.id),
                                    option_layout(
                                        |targs| type_args(opts, targs),
                                        iface.targs.as_ref(),
                                    ),
                                ]),
                            )
                        })
                        .collect(),
                ),
            ]),
        )
    })
}

fn interface_declaration_base(
    opts: &Opts,
    def: LayoutNode,
    loc: &Loc,
    interface: &ast::statement::Interface<Loc, Loc>,
) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        interface.comments.as_ref(),
        fuse(vec![
            def,
            identifier(&interface.id),
            option_layout(
                |tp| {
                    type_parameter(
                        opts,
                        flow_parser::ast_visitor::TypeParamsContext::Interface,
                        tp,
                    )
                },
                interface.tparams.as_ref(),
            ),
            interface_extends(opts, &interface.extends),
            pretty_space(),
            source_location_with_comments(
                &interface.body.0,
                None::<&ast::Syntax<Loc, ()>>,
                type_object(opts, atom(","), &interface.body.0, &interface.body.1),
            ),
        ]),
    )
}

fn interface_extends(opts: &Opts, extends: &[(Loc, ast::types::Generic<Loc, Loc>)]) -> LayoutNode {
    if extends.is_empty() {
        LayoutNode::empty()
    } else {
        fuse(vec![
            space(),
            atom("extends"),
            space(),
            layout::fuse_list(
                Some(atom(",")),
                None,
                extends
                    .iter()
                    .map(|(loc, generic)| {
                        source_location_with_comments(
                            loc,
                            None::<&ast::Syntax<Loc, ()>>,
                            type_generic(opts, loc, generic),
                        )
                    })
                    .collect(),
            ),
        ])
    }
}

fn declare_class_with_type(
    opts: &Opts,
    s_type: LayoutNode,
    loc: &Loc,
    class: &ast::statement::DeclareClass<Loc, Loc>,
) -> LayoutNode {
    let mut class_parts: Vec<LayoutNode> = vec![atom("declare"), space(), s_type];
    if class.abstract_ {
        class_parts.push(atom("abstract"));
        class_parts.push(space());
    }
    class_parts.push(atom("class"));
    class_parts.push(space());
    class_parts.push(identifier(&class.id));
    class_parts.push(option_layout(
        |tp| {
            type_parameter(
                opts,
                flow_parser::ast_visitor::TypeParamsContext::DeclareClass,
                tp,
            )
        },
        class.tparams.as_ref(),
    ));
    fn print_declare_class_extends(
        opts: &Opts,
        loc: &Loc,
        ext: &ast::statement::DeclareClassExtends<Loc, Loc>,
    ) -> LayoutNode {
        match ext {
            ast::statement::DeclareClassExtends::ExtendsIdent(generic) => {
                source_location_with_comments(
                    loc,
                    None::<&ast::Syntax<Loc, ()>>,
                    type_generic(opts, loc, generic),
                )
            }
            ast::statement::DeclareClassExtends::ExtendsCall {
                callee: (_callee_loc, callee),
                arg,
            } => {
                let ast::types::Generic {
                    id,
                    targs,
                    comments: _,
                } = callee;
                source_location_with_comments(
                    loc,
                    None::<&ast::Syntax<Loc, ()>>,
                    fuse(vec![
                        generic_identifier(opts, id),
                        option_layout(|ta| type_args(opts, ta), targs.as_ref()),
                        atom("("),
                        print_declare_class_extends(opts, &arg.0, &arg.1),
                        atom(")"),
                    ]),
                )
            }
        }
    }
    let class_extends: Vec<Option<LayoutNode>> = vec![
        class.extends.as_ref().map(|(ext_loc, ext)| {
            fuse(vec![
                atom("extends"),
                space(),
                print_declare_class_extends(opts, ext_loc, ext),
            ])
        }),
        if class.mixins.is_empty() {
            None
        } else {
            Some(fuse(vec![
                atom("mixins"),
                space(),
                layout::fuse_list(
                    Some(atom(",")),
                    None,
                    class
                        .mixins
                        .iter()
                        .map(|(mloc, generic)| {
                            source_location_with_comments(
                                mloc,
                                None::<&ast::Syntax<Loc, ()>>,
                                type_generic(opts, mloc, generic),
                            )
                        })
                        .collect(),
                ),
            ]))
        },
        class_implements(opts, class.implements.as_ref()),
    ];
    let extends_parts = match deoptionalize(class_extends).as_slice() {
        [] => LayoutNode::empty(),
        items => LayoutNode::indent(fuse(vec![line(), join(line(), items.to_vec())])),
    };
    let body_layout = source_location_with_comments(
        &class.body.0,
        None::<&ast::Syntax<Loc, ()>>,
        type_object(opts, atom(","), &class.body.0, &class.body.1),
    );
    let mut parts: Vec<LayoutNode> = Vec::new();
    parts.extend(class_parts);
    parts.push(extends_parts);
    parts.push(pretty_space());
    parts.push(body_layout);
    source_location_with_comments(loc, class.comments.as_ref(), group(parts))
}

fn declare_component_with_type(
    opts: &Opts,
    s_type: LayoutNode,
    loc: &Loc,
    comp: &ast::statement::DeclareComponent<Loc, Loc>,
) -> LayoutNode {
    let params_loc = &comp.params.loc;
    let params_comments = comp.params.comments.as_ref();
    layout_node_with_comments_opt(
        loc,
        comp.comments.as_ref(),
        with_semicolon(fuse(vec![
            atom("declare"),
            space(),
            s_type,
            atom("component"),
            space(),
            identifier(&comp.id),
            option_layout(
                |tp| {
                    type_parameter(
                        opts,
                        flow_parser::ast_visitor::TypeParamsContext::DeclareComponent,
                        tp,
                    )
                },
                comp.tparams.as_ref(),
            ),
            group(vec![
                layout_node_with_comments_opt(
                    params_loc,
                    params_comments,
                    component_params(opts, &comp.params),
                ),
                component_renders(opts, &comp.renders),
            ]),
        ])),
    )
}

fn declare_function_body(
    opts: &Opts,
    func: &ast::statement::DeclareFunction<Loc, Loc>,
) -> LayoutNode {
    fuse(vec![
        atom("function"),
        match &func.id {
            Some(id) => fuse(vec![space(), identifier(id)]),
            None => LayoutNode::empty(),
        },
        source_location_with_comments(
            &func.annot.loc,
            None::<&ast::Syntax<Loc, ()>>,
            match &*func.annot.annotation {
                ast::types::TypeInner::Function { inner: f, .. } => source_location_with_comments(
                    func.annot.annotation.loc(),
                    None::<&ast::Syntax<Loc, ()>>,
                    type_function(opts, atom(":"), func.annot.annotation.loc(), f),
                ),
                _ => panic!("Invalid DeclareFunction"),
            },
        ),
        match &func.predicate {
            Some(pred) => fuse(vec![pretty_space(), type_predicate(opts, pred)]),
            None => LayoutNode::empty(),
        },
    ])
}

fn declare_function_with_type(
    opts: &Opts,
    s_type: LayoutNode,
    loc: &Loc,
    func: &ast::statement::DeclareFunction<Loc, Loc>,
) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        func.comments.as_ref(),
        with_semicolon(fuse(vec![
            if func.implicit_declare {
                LayoutNode::empty()
            } else {
                fuse(vec![atom("declare"), space()])
            },
            s_type,
            declare_function_body(opts, func),
        ])),
    )
}

fn declare_variable_with_type(
    opts: &Opts,
    s_type: LayoutNode,
    loc: &Loc,
    var: &ast::statement::DeclareVariable<Loc, Loc>,
) -> LayoutNode {
    let decls_layout = match var.declarations.as_ref() {
        [] => LayoutNode::empty(),
        [single_decl] => variable_declarator(&NORMAL_CONTEXT, opts, single_decl),
        [hd, tl @ ..] => {
            let hd_layout = variable_declarator(&NORMAL_CONTEXT, opts, hd);
            let tl_layouts: Vec<LayoutNode> = tl
                .iter()
                .map(|d| variable_declarator(&NORMAL_CONTEXT, opts, d))
                .collect();
            group(vec![
                hd_layout,
                atom(","),
                LayoutNode::indent(fuse(vec![
                    pretty_line(),
                    join(fuse(vec![atom(","), pretty_line()]), tl_layouts),
                ])),
            ])
        }
    };
    layout_node_with_comments_opt(
        loc,
        var.comments.as_ref(),
        with_semicolon(fuse(vec![
            atom("declare"),
            space(),
            s_type,
            variable_kind(var.kind),
            space(),
            decls_layout,
        ])),
    )
}

fn export_source(
    opts: &Opts,
    prefix: LayoutNode,
    source: Option<&(Loc, ast::StringLiteral<Loc>)>,
) -> LayoutNode {
    match source {
        Some((source_loc, lit)) => fuse(vec![
            prefix,
            atom("from"),
            pretty_space(),
            string_literal(opts, source_loc, lit),
        ]),
        None => LayoutNode::empty(),
    }
}

fn export_specifier(
    opts: &Opts,
    source: Option<&(Loc, ast::StringLiteral<Loc>)>,
    specifier: &ast::statement::export_named_declaration::Specifier<Loc, Loc>,
) -> LayoutNode {
    match specifier {
        ast::statement::export_named_declaration::Specifier::ExportSpecifiers(specifiers) => {
            fuse(vec![
                group(vec![new_list(
                    Some((atom("{"), atom("}"))),
                    Some(atom(",")),
                    opts.bracket_spacing,
                    true,
                    specifiers
                        .iter()
                        .map(|spec| {
                            source_location_with_comments(
                                &spec.loc,
                                None::<&ast::Syntax<Loc, ()>>,
                                fuse(vec![
                                    match spec.export_kind {
                                        ast::statement::ExportKind::ExportType => {
                                            fuse(vec![atom("type"), space()])
                                        }
                                        ast::statement::ExportKind::ExportValue => {
                                            LayoutNode::empty()
                                        }
                                    },
                                    identifier(&spec.local),
                                    match &spec.exported {
                                        Some(exported) => fuse(vec![
                                            space(),
                                            atom("as"),
                                            space(),
                                            identifier(exported),
                                        ]),
                                        None => LayoutNode::empty(),
                                    },
                                ]),
                            )
                        })
                        .collect(),
                )]),
                export_source(opts, pretty_space(), source),
            ])
        }
        ast::statement::export_named_declaration::Specifier::ExportBatchSpecifier(batch) => {
            match &batch.specifier {
                Some(ident) => fuse(vec![
                    source_location_with_comments(
                        &batch.loc,
                        None::<&ast::Syntax<Loc, ()>>,
                        fuse(vec![
                            atom("*"),
                            pretty_space(),
                            atom("as"),
                            space(),
                            identifier(ident),
                        ]),
                    ),
                    export_source(opts, space(), source),
                ]),
                None => fuse(vec![
                    source_location_with_comments(
                        &batch.loc,
                        None::<&ast::Syntax<Loc, ()>>,
                        atom("*"),
                    ),
                    export_source(opts, pretty_space(), source),
                ]),
            }
        }
    }
}

fn match_case_guard(
    opts: &Opts,
    guard: Option<&ast::expression::Expression<Loc, Loc>>,
) -> LayoutNode {
    match guard {
        None => LayoutNode::empty(),
        Some(expr) => fuse(vec![
            space(),
            atom("if"),
            pretty_space(),
            wrap_in_parens(false, expression(opts, None, expr)),
        ]),
    }
}

fn partition_specifiers(
    opts: &Opts,
    default: Option<&ast::statement::import_declaration::DefaultIdentifier<Loc, Loc>>,
    specifiers: Option<&ast::statement::import_declaration::Specifier<Loc, Loc>>,
) -> (Vec<LayoutNode>, Option<LayoutNode>) {
    let (special, named) = match specifiers {
        Some(ast::statement::import_declaration::Specifier::ImportNamespaceSpecifier(ns)) => {
            (vec![import_namespace_specifier(ns)], None)
        }
        Some(ast::statement::import_declaration::Specifier::ImportNamedSpecifiers(
            named_specifiers,
        )) => {
            // special case for imports: single specifiers don't break no matter how long
            let named = match (named_specifiers.as_slice(), default) {
                ([_], Some(_)) => import_named_specifiers(opts, named_specifiers), // one specifier and a default
                ([_, _, ..], _) => import_named_specifiers(opts, named_specifiers), // more than one specifier
                ([named_specifier], None) => {
                    // one specifier but no default
                    let bracket_space = if opts.bracket_spacing {
                        pretty_space()
                    } else {
                        LayoutNode::empty()
                    };
                    fuse(vec![
                        atom("{"),
                        bracket_space.dupe(),
                        import_named_specifier(opts, named_specifier),
                        bracket_space,
                        atom("}"),
                    ])
                }
                ([], _) => atom("{}"),
            };
            (vec![], Some(named))
        }
        None => (vec![], None),
    };
    let special = match default {
        Some(def) => {
            let mut s = vec![identifier(&def.identifier)];
            s.extend(special);
            s
        }
        None => special,
    };
    (special, named)
}

fn import_namespace_specifier(ns: &(Loc, ast::Identifier<Loc, Loc>)) -> LayoutNode {
    source_location_with_comments(
        &ns.0,
        None::<&ast::Syntax<Loc, ()>>,
        fuse(vec![
            atom("*"),
            pretty_space(),
            atom("as"),
            space(),
            identifier(&ns.1),
        ]),
    )
}

fn import_named_specifier(
    _opts: &Opts,
    spec: &ast::statement::import_declaration::NamedSpecifier<Loc, Loc>,
) -> LayoutNode {
    fuse(vec![
        match spec.kind {
            Some(ast::statement::ImportKind::ImportType) => fuse(vec![atom("type"), space()]),
            Some(ast::statement::ImportKind::ImportTypeof) => fuse(vec![atom("typeof"), space()]),
            Some(ast::statement::ImportKind::ImportValue) | None => LayoutNode::empty(),
        },
        identifier(&spec.remote),
        match &spec.local {
            Some(id) => fuse(vec![space(), atom("as"), space(), identifier(id)]),
            None => LayoutNode::empty(),
        },
    ])
}

fn import_named_specifiers(
    opts: &Opts,
    named_specifiers: &[ast::statement::import_declaration::NamedSpecifier<Loc, Loc>],
) -> LayoutNode {
    group(vec![new_list(
        Some((atom("{"), atom("}"))),
        Some(atom(",")),
        opts.bracket_spacing,
        true,
        named_specifiers
            .iter()
            .map(|spec| import_named_specifier(opts, spec))
            .collect(),
    )])
}

fn null_literal(loc: &Loc, comments: Option<&ast::Syntax<Loc, ()>>) -> LayoutNode {
    layout_node_with_comments_opt(loc, comments, atom("null"))
}

fn expression_or_spread(
    opts: &Opts,
    expr_or_spread: &ast::expression::ExpressionOrSpread<Loc, Loc>,
) -> LayoutNode {
    // min_precedence causes operators that should always be parenthesized
    // (they have precedence = 0) to be parenthesized. one notable example is
    // the comma operator, which would be confused with additional arguments if
    // not parenthesized.
    let precedence = MIN_PRECEDENCE;
    match expr_or_spread {
        ast::expression::ExpressionOrSpread::Expression(expr) => {
            expression_with_parens(opts, precedence, &NORMAL_CONTEXT, expr)
        }
        ast::expression::ExpressionOrSpread::Spread(spread) => {
            spread_element(opts, precedence, &NORMAL_CONTEXT, spread)
        }
    }
}

fn spread_element(
    opts: &Opts,
    precedence: i32,
    ctxt: &ExpressionContext,
    spread: &ast::expression::SpreadElement<Loc, Loc>,
) -> LayoutNode {
    source_location_with_comments(
        &spread.loc,
        spread.comments.as_ref(),
        fuse(vec![
            atom("..."),
            expression_with_parens(opts, precedence, ctxt, &spread.argument),
        ]),
    )
}

fn object_property_key(opts: &Opts, key: &ast::expression::object::Key<Loc, Loc>) -> LayoutNode {
    match key {
        ast::expression::object::Key::StringLiteral((loc, lit)) => string_literal(opts, loc, lit),
        ast::expression::object::Key::NumberLiteral((loc, lit)) => number_literal(opts, loc, lit),
        ast::expression::object::Key::BigIntLiteral((loc, lit)) => bigint_literal(loc, lit),
        ast::expression::object::Key::Identifier(ident) => identifier(ident),
        ast::expression::object::Key::Computed(computed) => layout_node_with_comments_opt(
            &computed.loc,
            computed.comments.as_ref(),
            fuse(vec![
                atom("["),
                LayoutNode::indent(fuse(vec![
                    pretty_line(),
                    expression(opts, None, &computed.expression),
                ])),
                pretty_line(),
                atom("]"),
            ]),
        ),
        ast::expression::object::Key::PrivateName(_) => {
            panic!("Internal Error: Found object prop with private name")
        }
    }
}

fn ts_accessibility_layout(
    accessibility: Option<&ast::class::ts_accessibility::TSAccessibility<Loc>>,
) -> LayoutNode {
    match accessibility {
        Some(a) => match a.kind {
            ast::class::ts_accessibility::Kind::Private => fuse(vec![atom("private"), space()]),
            ast::class::ts_accessibility::Kind::Protected => fuse(vec![atom("protected"), space()]),
            ast::class::ts_accessibility::Kind::Public => fuse(vec![atom("public"), space()]),
        },
        None => LayoutNode::empty(),
    }
}

fn class_property_helper(
    opts: &Opts,
    loc: &Loc,
    key: LayoutNode,
    value: &ast::class::property::Value<Loc, Loc>,
    static_: bool,
    override_: bool,
    annot: &ast::types::AnnotationOrHint<Loc, Loc>,
    variance_: Option<&ast::Variance<Loc>>,
    ts_accessibility: Option<&ast::class::ts_accessibility::TSAccessibility<Loc>>,
    decorators: &[ast::class::Decorator<Loc, Loc>],
    comments: Option<&ast::Syntax<Loc, ()>>,
    optional: bool,
    definite: bool,
) -> LayoutNode {
    let (declare, value_expr) = match value {
        ast::class::property::Value::Declared => (true, None),
        ast::class::property::Value::Uninitialized => (false, None),
        ast::class::property::Value::Initialized(expr) => (false, Some(expr)),
    };
    let ts_acc = ts_accessibility_layout(ts_accessibility);
    source_location_with_comments(
        loc,
        comments,
        fuse(vec![
            decorators_list(opts, decorators),
            if declare {
                fuse(vec![atom("declare"), space()])
            } else {
                LayoutNode::empty()
            },
            ts_acc,
            if static_ {
                fuse(vec![atom("static"), space()])
            } else {
                LayoutNode::empty()
            },
            if override_ {
                fuse(vec![atom("override"), space()])
            } else {
                LayoutNode::empty()
            },
            option_layout(variance, variance_),
            key,
            if optional {
                atom("?")
            } else {
                LayoutNode::empty()
            },
            if definite {
                atom("!")
            } else {
                LayoutNode::empty()
            },
            hint(|a| type_annotation(opts, false, a), annot),
            match value_expr {
                Some(v) => fuse(vec![
                    pretty_space(),
                    atom("="),
                    pretty_space(),
                    expression_with_parens(opts, MIN_PRECEDENCE, &NORMAL_CONTEXT, v),
                ]),
                None => LayoutNode::empty(),
            },
        ]),
    )
}

fn function_param(
    ctxt: &ExpressionContext,
    opts: &Opts,
    param: &ast::function::Param<Loc, Loc>,
) -> LayoutNode {
    match param {
        ast::function::Param::RegularParam {
            loc,
            argument,
            default,
        } => {
            let node = pattern(opts, Some(ctxt), argument);
            let node = match default {
                Some(expr) => fuse_with_default(opts, None, node, expr),
                None => node,
            };
            source_location_with_comments(loc, None::<&ast::Syntax<Loc, ()>>, node)
        }
        ast::function::Param::ParamProperty { loc, property } => class_property_helper(
            opts,
            loc,
            object_property_key(opts, &property.key),
            &property.value,
            property.static_,
            false,
            &property.annot,
            property.variance.as_ref(),
            property.ts_accessibility.as_ref(),
            &property.decorators,
            property.comments.as_ref(),
            property.optional,
            property.definite,
        ),
    }
}

fn jsx_namespaced_name(ns: &ast::jsx::NamespacedName<Loc, Loc>) -> LayoutNode {
    source_location_with_comments(
        &ns.loc,
        None::<&ast::Syntax<Loc, ()>>,
        fuse(vec![
            jsx_identifier(&ns.namespace),
            atom(":"),
            jsx_identifier(&ns.name),
        ]),
    )
}

fn jsx_member_expression(mem: &ast::jsx::MemberExpression<Loc, Loc>) -> LayoutNode {
    source_location_with_comments(
        &mem.loc,
        None::<&ast::Syntax<Loc, ()>>,
        fuse(vec![
            match &mem.object {
                ast::jsx::member_expression::Object::Identifier(ident) => jsx_identifier(ident),
                ast::jsx::member_expression::Object::MemberExpression(member) => {
                    jsx_member_expression(member)
                }
            },
            atom("."),
            jsx_identifier(&mem.property),
        ]),
    )
}

fn jsx_expression_container(
    opts: &Opts,
    loc: &Loc,
    container: &ast::jsx::ExpressionContainer<Loc, Loc>,
) -> LayoutNode {
    let internal_comments_layout = match internal_comments(container.comments.as_ref()) {
        None => LayoutNode::empty(),
        Some((_, _, comments)) => comments,
    };
    layout_node_with_comments_opt(
        loc,
        container.comments.as_ref(),
        fuse(vec![
            atom("{"),
            match &container.expression {
                ast::jsx::expression_container::Expression::Expression(expr) => {
                    expression(opts, None, expr)
                }
                ast::jsx::expression_container::Expression::EmptyExpression => LayoutNode::empty(),
            },
            internal_comments_layout,
            atom("}"),
        ]),
    )
}

fn jsx_spread_child(
    opts: &Opts,
    loc: &Loc,
    spread: &ast::jsx::SpreadChild<Loc, Loc>,
) -> LayoutNode {
    fuse(vec![
        atom("{"),
        layout_node_with_comments_opt(loc, spread.comments.as_ref(), atom("...")),
        expression(opts, None, &spread.expression),
        atom("}"),
    ])
}

fn jsx_attribute(opts: &Opts, attr: &ast::jsx::Attribute<Loc, Loc>) -> LayoutNode {
    source_location_with_comments(
        &attr.loc,
        None::<&ast::Syntax<Loc, ()>>,
        fuse(vec![
            match &attr.name {
                ast::jsx::attribute::Name::Identifier(ident) => jsx_identifier(ident),
                ast::jsx::attribute::Name::NamespacedName(ns) => jsx_namespaced_name(ns),
            },
            match &attr.value {
                Some(v) => fuse(vec![
                    atom("="),
                    match v {
                        ast::jsx::attribute::Value::StringLiteral((loc, lit)) => {
                            source_location_with_comments(
                                loc,
                                None::<&ast::Syntax<Loc, ()>>,
                                string_literal(opts, loc, lit),
                            )
                        }
                        ast::jsx::attribute::Value::ExpressionContainer((loc, express)) => {
                            source_location_with_comments(
                                loc,
                                None::<&ast::Syntax<Loc, ()>>,
                                jsx_expression_container(opts, loc, express),
                            )
                        }
                    },
                ]),
                None => flat_ugly_space(), // TODO we shouldn't do this for the last attr
            },
        ]),
    )
}

fn jsx_spread_attribute(opts: &Opts, attr: &ast::jsx::SpreadAttribute<Loc, Loc>) -> LayoutNode {
    layout_node_with_comments_opt(
        &attr.loc,
        attr.comments.as_ref(),
        fuse(vec![
            atom("{"),
            atom("..."),
            expression(opts, None, &attr.argument),
            atom("}"),
        ]),
    )
}

fn jsx_element_name(name: &ast::jsx::Name<Loc, Loc>) -> LayoutNode {
    match name {
        ast::jsx::Name::Identifier(ident) => jsx_identifier(ident),
        ast::jsx::Name::NamespacedName(ns) => jsx_namespaced_name(ns),
        ast::jsx::Name::MemberExpression(mem) => jsx_member_expression(mem),
    }
}

fn jsx_opening(opts: &Opts, opening: &ast::jsx::Opening<Loc, Loc>) -> LayoutNode {
    jsx_opening_helper(
        opts,
        &opening.loc,
        Some(&opening.name),
        opening.targs.as_ref(),
        &opening.attributes,
    )
}

fn jsx_fragment_opening(opts: &Opts, loc: &Loc) -> LayoutNode {
    jsx_opening_helper(opts, loc, None, None, &[])
}

fn jsx_opening_helper(
    opts: &Opts,
    loc: &Loc,
    name_opt: Option<&ast::jsx::Name<Loc, Loc>>,
    targs: Option<&ast::expression::CallTypeArgs<Loc, Loc>>,
    attributes: &[ast::jsx::OpeningAttribute<Loc, Loc>],
) -> LayoutNode {
    source_location_with_comments(
        loc,
        None::<&ast::Syntax<Loc, ()>>,
        group(vec![
            atom("<"),
            match name_opt {
                Some(name) => jsx_element_name(name),
                None => LayoutNode::empty(),
            },
            option_layout(|targs| call_type_args(opts, "<", targs), targs),
            if !attributes.is_empty() {
                LayoutNode::indent(fuse(vec![
                    line(),
                    join(
                        pretty_line(),
                        attributes
                            .iter()
                            .map(|attr| jsx_opening_attr(opts, attr))
                            .collect(),
                    ),
                ]))
            } else {
                LayoutNode::empty()
            },
            atom(">"),
        ]),
    )
}

fn jsx_self_closing(opts: &Opts, opening: &ast::jsx::Opening<Loc, Loc>) -> LayoutNode {
    let attributes: Vec<LayoutNode> = opening
        .attributes
        .iter()
        .map(|attr| jsx_opening_attr(opts, attr))
        .collect();
    source_location_with_comments(
        &opening.loc,
        None::<&ast::Syntax<Loc, ()>>,
        group(vec![
            atom("<"),
            jsx_element_name(&opening.name),
            option_layout(
                |targs| call_type_args(opts, "<", targs),
                opening.targs.as_ref(),
            ),
            if !attributes.is_empty() {
                fuse(vec![
                    LayoutNode::indent(fuse(vec![line(), join(pretty_line(), attributes)])),
                    pretty_line(),
                ])
            } else {
                pretty_space()
            },
            atom("/>"),
        ]),
    )
}

pub fn jsx_closing(closing: &ast::jsx::Closing<Loc, Loc>) -> LayoutNode {
    source_location_with_comments(
        &closing.loc,
        None::<&ast::Syntax<Loc, ()>>,
        fuse(vec![atom("</"), jsx_element_name(&closing.name), atom(">")]),
    )
}

fn jsx_closing_fragment(loc: &Loc) -> LayoutNode {
    source_location_with_comments(loc, None::<&ast::Syntax<Loc, ()>>, fuse(vec![atom("</>")]))
}

fn jsx_children(
    opts: &Opts,
    loc: &Loc,
    children: &(Loc, Vec<ast::jsx::Child<Loc, Loc>>),
) -> LayoutNode {
    let processed_children: Vec<(Loc, LayoutNode)> = deoptionalize(
        children
            .1
            .iter()
            .map(|child| jsx_child(opts, child))
            .collect(),
    );
    // Check for empty children
    if processed_children.is_empty() {
        LayoutNode::empty()
    // If start and end lines don't match check inner breaks
    } else if loc.end.line > loc.start.line {
        // If there is at least one blank line between the current and previous child,
        // output a single blank line separating the children.
        // If the current child and the previous child line positions are offset match
        // this via forcing a newline
        // TODO: Remove the `Newline` hack, this forces newlines to exist
        // when using the compact printer
        // Must be on the same line as the previous child
        let (children_n, _) = processed_children.iter().fold(
            (Vec::<LayoutNode>::new(), None::<i32>),
            |(mut children_n, last_line), (child_loc, child)| {
                let child_n = LayoutNode::source_location(
                    flow_aloc::ALoc::of_loc(child_loc.dupe()),
                    child.dupe(),
                );
                let formatted_child_n = match last_line {
                    None => child_n, // First child, newlines will always be forced via the `pretty_hardline` below
                    Some(last_line) if child_loc.start.line > last_line + 1 => {
                        fuse(vec![pretty_hardline(), LayoutNode::newline(), child_n])
                    }
                    Some(last_line) if child_loc.start.line > last_line => {
                        fuse(vec![LayoutNode::newline(), child_n])
                    }
                    Some(_) => child_n,
                };
                children_n.push(formatted_child_n);
                (children_n, Some(child_loc.end.line))
            },
        );
        let mut indent_children = vec![pretty_hardline()];
        indent_children.extend(children_n);
        fuse(vec![
            LayoutNode::indent(fuse(indent_children)),
            pretty_hardline(),
        ])
    // Single line
    } else {
        fuse(
            processed_children
                .into_iter()
                .map(|(child_loc, child)| {
                    LayoutNode::source_location(flow_aloc::ALoc::of_loc(child_loc), child)
                })
                .collect(),
        )
    }
}

fn jsx_element(opts: &Opts, loc: &Loc, elem: &ast::jsx::Element<Loc, Loc>) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        elem.comments.as_ref(),
        fuse(vec![
            if !elem.opening_element.self_closing {
                jsx_opening(opts, &elem.opening_element)
            } else {
                jsx_self_closing(opts, &elem.opening_element)
            },
            jsx_children(opts, loc, &elem.children),
            match &elem.closing_element {
                Some(closing) => jsx_closing(closing),
                None => LayoutNode::empty(),
            },
        ]),
    )
}

fn jsx_fragment(opts: &Opts, loc: &Loc, frag: &ast::jsx::Fragment<Loc, Loc>) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        frag.frag_comments.as_ref(),
        fuse(vec![
            jsx_fragment_opening(opts, &frag.frag_opening_element),
            jsx_children(opts, loc, &frag.frag_children),
            jsx_closing_fragment(&frag.frag_closing_element),
        ]),
    )
}

fn call_type_arg(opts: &Opts, arg: &ast::expression::CallTypeArg<Loc, Loc>) -> LayoutNode {
    match arg {
        ast::expression::CallTypeArg::Implicit(implicit) => {
            layout_node_with_comments_opt(&implicit.loc, implicit.comments.as_ref(), atom("_"))
        }
        ast::expression::CallTypeArg::Explicit(t) => type_(opts, t),
    }
}

fn type_args(opts: &Opts, targs: &ast::types::TypeArgs<Loc, Loc>) -> LayoutNode {
    let num_args = targs.arguments.len();
    let internal_comments_list: Vec<(Loc, CommentsBounds, LayoutNode)> =
        match internal_comments(targs.comments.as_ref()) {
            None => vec![],
            Some(comments) => vec![comments],
        };
    let args: Vec<(Loc, CommentsBounds, LayoutNode)> = targs
        .arguments
        .iter()
        .enumerate()
        .map(|(i, arg)| {
            let comment_bounds = comment_attachment::comment_bounds_without_trailing_line_comment(
                comment_attachment::type_comment_bounds(arg),
            );
            // Add trailing comma to last argument
            let mut arg_layout = type_(opts, arg);
            if i == num_args - 1
                && internal_comments_list.is_empty()
                && TrailingCommas::enabled_for_types(opts.trailing_commas)
            {
                arg_layout = fuse(vec![arg_layout, if_break(atom(","), LayoutNode::empty())]);
            }
            (arg.loc().dupe(), comment_bounds, arg_layout)
        })
        .collect();
    // Add internal comments
    let mut args = args;
    args.extend(internal_comments_list);
    let args_layout = list_with_newlines_with_sep(atom(","), pretty_line(), false, &args);
    source_location_with_comments(
        &targs.loc,
        targs.comments.as_ref(),
        group(vec![wrap_and_indent(
            (atom("<"), atom(">")),
            None,
            args_layout,
        )]),
    )
}

pub(crate) fn match_pattern(
    opts: &Opts,
    pat: &ast::match_pattern::MatchPattern<Loc, Loc>,
) -> LayoutNode {
    match pat {
        ast::match_pattern::MatchPattern::WildcardPattern { loc, inner } => {
            layout_node_with_comments_opt(loc, inner.comments.as_ref(), atom("_"))
        }
        ast::match_pattern::MatchPattern::NumberPattern { loc, inner } => {
            number_literal(opts, loc, inner)
        }
        ast::match_pattern::MatchPattern::BigIntPattern { loc, inner } => {
            bigint_literal(loc, inner)
        }
        ast::match_pattern::MatchPattern::StringPattern { loc, inner } => {
            string_literal(opts, loc, inner)
        }
        ast::match_pattern::MatchPattern::BooleanPattern { loc, inner } => {
            boolean_literal(loc, inner)
        }
        ast::match_pattern::MatchPattern::NullPattern { loc, inner } => {
            null_literal(loc, (**inner).as_ref())
        }
        ast::match_pattern::MatchPattern::IdentifierPattern { inner, .. } => identifier(inner),
        ast::match_pattern::MatchPattern::MemberPattern { inner, .. } => {
            match_member_pattern(opts, inner)
        }
        ast::match_pattern::MatchPattern::UnaryPattern { loc, inner } => {
            let operator = match inner.operator {
                ast::match_pattern::unary_pattern::Operator::Minus => "-",
                ast::match_pattern::unary_pattern::Operator::Plus => "+",
            };
            let argument = match &inner.argument {
                (arg_loc, ast::match_pattern::unary_pattern::Argument::NumberLiteral(lit)) => {
                    number_literal(opts, arg_loc, lit)
                }
                (arg_loc, ast::match_pattern::unary_pattern::Argument::BigIntLiteral(lit)) => {
                    bigint_literal(arg_loc, lit)
                }
            };
            layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                fuse(vec![atom(operator), argument]),
            )
        }
        ast::match_pattern::MatchPattern::BindingPattern { loc, inner } => {
            match_binding_pattern(loc, inner)
        }
        ast::match_pattern::MatchPattern::ObjectPattern { loc, inner } => {
            match_object_pattern(opts, loc, inner)
        }
        ast::match_pattern::MatchPattern::ArrayPattern { loc, inner } => {
            match_array_pattern(opts, loc, inner)
        }
        ast::match_pattern::MatchPattern::InstancePattern { loc, inner } => {
            match_instance_pattern(opts, loc, inner)
        }
        ast::match_pattern::MatchPattern::OrPattern { loc, inner } => {
            let patterns: Vec<LayoutNode> = inner
                .patterns
                .iter()
                .map(|p| match_pattern(opts, p))
                .collect();
            layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                join(
                    fuse(vec![pretty_space(), atom("|"), pretty_space()]),
                    patterns,
                ),
            )
        }
        ast::match_pattern::MatchPattern::AsPattern { loc, inner } => {
            let pattern_layout = match &inner.pattern {
                ast::match_pattern::MatchPattern::OrPattern { .. } => fuse(vec![
                    atom("("),
                    match_pattern(opts, &inner.pattern),
                    atom(")"),
                ]),
                _ => match_pattern(opts, &inner.pattern),
            };
            let target = match &inner.target {
                ast::match_pattern::as_pattern::Target::Binding {
                    loc,
                    pattern: binding,
                } => match_binding_pattern(loc, binding),
                ast::match_pattern::as_pattern::Target::Identifier(ident) => identifier(ident),
            };
            layout_node_with_comments_opt(
                loc,
                inner.comments.as_ref(),
                fuse(vec![pattern_layout, space(), atom("as"), space(), target]),
            )
        }
    }
}

fn match_member_pattern(
    opts: &Opts,
    mem: &ast::match_pattern::MemberPattern<Loc, Loc>,
) -> LayoutNode {
    let base = match &mem.base {
        ast::match_pattern::member_pattern::Base::BaseIdentifier(id) => identifier(id),
        ast::match_pattern::member_pattern::Base::BaseMember(m) => match_member_pattern(opts, m),
    };
    layout_node_with_comments_opt(
        &mem.loc,
        mem.comments.as_ref(),
        match &mem.property {
            ast::match_pattern::member_pattern::Property::PropertyString {
                loc, literal, ..
            } => fuse(vec![
                base,
                atom("["),
                string_literal(opts, loc, literal),
                atom("]"),
            ]),
            ast::match_pattern::member_pattern::Property::PropertyNumber {
                loc, literal, ..
            } => fuse(vec![
                base,
                atom("["),
                number_literal(opts, loc, literal),
                atom("]"),
            ]),
            ast::match_pattern::member_pattern::Property::PropertyBigInt {
                loc, literal, ..
            } => fuse(vec![
                base,
                atom("["),
                bigint_literal(loc, literal),
                atom("]"),
            ]),
            ast::match_pattern::member_pattern::Property::PropertyIdentifier(ident) => {
                fuse(vec![base, atom("."), identifier(ident)])
            }
        },
    )
}

fn match_binding_pattern(
    loc: &Loc,
    binding: &ast::match_pattern::BindingPattern<Loc, Loc>,
) -> LayoutNode {
    source_location_with_comments(
        loc,
        binding.comments.as_ref(),
        fuse_with_space(vec![variable_kind(binding.kind), identifier(&binding.id)]),
    )
}

pub(crate) fn match_object_pattern(
    opts: &Opts,
    loc: &Loc,
    obj: &ast::match_pattern::ObjectPattern<Loc, Loc>,
) -> LayoutNode {
    let mut props: Vec<LayoutNode> = obj
        .properties
        .iter()
        .map(|p| match_object_pattern_property(opts, p))
        .collect();
    if let Some(rest) = &obj.rest {
        props.push(match_rest_pattern(rest));
    }
    layout_node_with_comments_opt(
        loc,
        obj.comments.as_ref(),
        group(vec![new_list(
            Some((atom("{"), atom("}"))),
            Some(fuse(vec![atom(",")])),
            false,
            true,
            props,
        )]),
    )
}

pub(crate) fn match_object_pattern_property(
    opts: &Opts,
    prop: &ast::match_pattern::object_pattern::Property<Loc, Loc>,
) -> LayoutNode {
    match prop {
        ast::match_pattern::object_pattern::Property::Valid { loc, property } => {
            layout_node_with_comments_opt(
                loc,
                property.comments.as_ref(),
                if property.shorthand {
                    match_pattern(opts, &property.pattern)
                } else {
                    let key_layout = match &property.key {
                        ast::match_pattern::object_pattern::Key::StringLiteral((loc, lit)) => {
                            string_literal(opts, loc, lit)
                        }
                        ast::match_pattern::object_pattern::Key::NumberLiteral((loc, lit)) => {
                            number_literal(opts, loc, lit)
                        }
                        ast::match_pattern::object_pattern::Key::BigIntLiteral((loc, lit)) => {
                            bigint_literal(loc, lit)
                        }
                        ast::match_pattern::object_pattern::Key::Identifier(ident) => {
                            identifier(ident)
                        }
                    };
                    fuse(vec![
                        key_layout,
                        atom(":"),
                        pretty_space(),
                        match_pattern(opts, &property.pattern),
                    ])
                },
            )
        }
        ast::match_pattern::object_pattern::Property::InvalidShorthand {
            identifier: ident,
            ..
        } => identifier(ident),
    }
}

fn match_array_pattern(
    opts: &Opts,
    loc: &Loc,
    arr: &ast::match_pattern::ArrayPattern<Loc, Loc>,
) -> LayoutNode {
    let mut elements: Vec<LayoutNode> = arr
        .elements
        .iter()
        .map(|elem| match_pattern(opts, &elem.pattern))
        .collect();
    if let Some(rest) = &arr.rest {
        elements.push(match_rest_pattern(rest));
    }
    layout_node_with_comments_opt(
        loc,
        arr.comments.as_ref(),
        group(vec![new_list(
            Some((atom("["), atom("]"))),
            Some(atom(",")),
            false,
            true,
            elements,
        )]),
    )
}

fn match_instance_pattern(
    opts: &Opts,
    loc: &Loc,
    ins: &ast::match_pattern::InstancePattern<Loc, Loc>,
) -> LayoutNode {
    let constructor = match &ins.constructor {
        ast::match_pattern::InstancePatternConstructor::IdentifierConstructor(id) => identifier(id),
        ast::match_pattern::InstancePatternConstructor::MemberConstructor(mem) => {
            match_member_pattern(opts, mem)
        }
    };
    let properties = match_object_pattern(opts, &ins.properties.0, &ins.properties.1);
    layout_node_with_comments_opt(
        loc,
        ins.comments.as_ref(),
        fuse(vec![constructor, pretty_space(), properties]),
    )
}

fn match_rest_pattern(rest: &ast::match_pattern::RestPattern<Loc, Loc>) -> LayoutNode {
    match &rest.argument {
        Some((arg_loc, arg)) => layout_node_with_comments_opt(
            &rest.loc,
            rest.comments.as_ref(),
            fuse(vec![atom("..."), match_binding_pattern(arg_loc, arg)]),
        ),
        None => layout_node_with_comments_opt(&rest.loc, rest.comments.as_ref(), atom("...")),
    }
}

fn const_layout(const_: Option<&ast::types::type_param::ConstModifier<Loc>>) -> LayoutNode {
    if const_.is_some() {
        fuse(vec![atom("const"), space()])
    } else {
        LayoutNode::empty()
    }
}

pub(crate) fn type_param(opts: &Opts, tparam: &ast::types::TypeParam<Loc, Loc>) -> LayoutNode {
    fuse(vec![
        const_layout(tparam.const_.as_ref()),
        option_layout(variance, tparam.variance.as_ref()),
        source_location_with_comments(
            &tparam.name.loc,
            tparam.name.comments.as_ref(),
            atom(tparam.name.name.as_str()),
        ),
        hint(
            |annot| match tparam.bound_kind {
                ast::types::type_param::BoundKind::Colon => type_annotation(opts, false, annot),
                ast::types::type_param::BoundKind::Extends => fuse(vec![
                    space(),
                    atom("extends"),
                    space(),
                    type_(opts, &annot.annotation),
                ]),
            },
            &tparam.bound,
        ),
        match &tparam.default {
            Some(t) => fuse(vec![
                pretty_space(),
                atom("="),
                pretty_space(),
                type_(opts, t),
            ]),
            None => LayoutNode::empty(),
        },
    ])
}

pub(crate) fn type_parameter(
    opts: &Opts,
    kind: flow_parser::ast_visitor::TypeParamsContext,
    tparams: &ast::types::TypeParams<Loc, Loc>,
) -> LayoutNode {
    let num_params = tparams.params.len();
    let internal_comments_list: Vec<(Loc, CommentsBounds, LayoutNode)> =
        match internal_comments(tparams.comments.as_ref()) {
            None => vec![],
            Some(comments) => vec![comments],
        };
    let mut params: Vec<(Loc, CommentsBounds, LayoutNode)> = tparams
        .params
        .iter()
        .enumerate()
        .map(|(i, param)| {
            let param_layout = type_param(opts, param);
            // Add trailing comma to last parameter
            let param_layout = if i == num_params - 1
                && internal_comments_list.is_empty()
                && TrailingCommas::enabled_for_types(opts.trailing_commas)
            {
                fuse(vec![param_layout, if_break(atom(","), LayoutNode::empty())])
            } else {
                param_layout
            };
            let comment_bounds = comment_attachment::type_param_comment_bounds(kind, param);
            (param.loc.dupe(), comment_bounds, param_layout)
        })
        .collect();
    params.extend(internal_comments_list);
    let params_layout = list_with_newlines_with_sep(atom(","), pretty_line(), false, &params);
    source_location_with_comments(
        &tparams.loc,
        tparams.comments.as_ref(),
        group(vec![wrap_and_indent(
            (atom("<"), atom(">")),
            None,
            params_layout,
        )]),
    )
}

fn component_type_params(
    opts: &Opts,
    params: &ast::types::component_params::Params<Loc, Loc>,
) -> LayoutNode {
    let mut param_list: Vec<(Loc, CommentsBounds, LayoutNode)> = params
        .params
        .iter()
        .map(|param| {
            (
                param.loc.dupe(),
                comment_attachment::component_type_param_comment_bounds(param),
                component_type_param(
                    opts,
                    param.optional,
                    &param.loc,
                    &param.name,
                    &param.annot.annotation,
                ),
            )
        })
        .collect();
    // Add rest param
    if let Some(rest) = &params.rest {
        let rest_layout = source_location_with_comments(
            &rest.loc,
            rest.comments.as_ref(),
            fuse(vec![
                atom("..."),
                match &rest.argument {
                    Some(id) => component_type_param(
                        opts,
                        rest.optional,
                        &rest.loc,
                        &ast::statement::component_params::ParamName::Identifier(id.clone()),
                        &rest.annot,
                    ),
                    None => type_(opts, &rest.annot),
                },
            ]),
        );
        let rest_param = (
            rest.loc.dupe(),
            comment_attachment::component_type_rest_param_comment_bounds(rest),
            rest_layout,
        );
        param_list.push(rest_param);
    }
    let params_layout = list_with_newlines_with_sep(atom(","), pretty_line(), false, &param_list);
    // Add trailing comma
    let mut params_layout = params_layout;
    if !param_list.is_empty()
        && params.rest.is_none()
        && TrailingCommas::enabled_for_function_params(opts.trailing_commas)
    {
        params_layout.push(if_break(atom(","), LayoutNode::empty()));
    }
    let params_layout =
        list_add_internal_comments(&param_list, params_layout, params.comments.as_ref());
    wrap_and_indent((atom("("), atom(")")), None, params_layout)
}

fn component_type_param(
    opts: &Opts,
    optional: bool,
    loc: &Loc,
    name: &ast::statement::component_params::ParamName<Loc, Loc>,
    annot: &ast::types::Type<Loc, Loc>,
) -> LayoutNode {
    let optional_layout = if optional {
        atom("?")
    } else {
        LayoutNode::empty()
    };
    let name_layout = fuse(vec![
        component_param_name(opts, name),
        optional_layout,
        atom(":"),
        pretty_space(),
    ]);
    source_location_with_comments(
        loc,
        None::<&ast::Syntax<Loc, ()>>,
        fuse(vec![name_layout, type_(opts, annot)]),
    )
}

pub fn type_(opts: &Opts, t: &ast::types::Type<Loc, Loc>) -> LayoutNode {
    let loc = t.loc();
    source_location_with_comments(
        loc,
        None::<&ast::Syntax<Loc, ()>>,
        match &**t {
            ast::types::TypeInner::Any { comments, .. } => {
                layout_node_with_comments_opt(loc, comments.as_ref(), atom("any"))
            }
            ast::types::TypeInner::Mixed { comments, .. } => {
                layout_node_with_comments_opt(loc, comments.as_ref(), atom("mixed"))
            }
            ast::types::TypeInner::Empty { comments, .. } => {
                layout_node_with_comments_opt(loc, comments.as_ref(), atom("empty"))
            }
            ast::types::TypeInner::Void { comments, .. } => {
                layout_node_with_comments_opt(loc, comments.as_ref(), atom("void"))
            }
            ast::types::TypeInner::Null { comments, .. } => null_literal(loc, comments.as_ref()),
            ast::types::TypeInner::Symbol { comments, .. } => {
                layout_node_with_comments_opt(loc, comments.as_ref(), atom("symbol"))
            }
            ast::types::TypeInner::Number { comments, .. } => {
                layout_node_with_comments_opt(loc, comments.as_ref(), atom("number"))
            }
            ast::types::TypeInner::BigInt { comments, .. } => {
                layout_node_with_comments_opt(loc, comments.as_ref(), atom("bigint"))
            }
            ast::types::TypeInner::String { comments, .. } => {
                layout_node_with_comments_opt(loc, comments.as_ref(), atom("string"))
            }
            ast::types::TypeInner::Boolean { raw, comments, .. } => {
                let raw = match raw {
                    ast::types::BooleanRaw::Boolean => "boolean",
                    ast::types::BooleanRaw::Bool => "bool",
                };
                layout_node_with_comments_opt(loc, comments.as_ref(), atom(raw))
            }
            ast::types::TypeInner::Unknown { comments, .. } => {
                layout_node_with_comments_opt(loc, comments.as_ref(), atom("unknown"))
            }
            ast::types::TypeInner::Never { comments, .. } => {
                layout_node_with_comments_opt(loc, comments.as_ref(), atom("never"))
            }
            ast::types::TypeInner::Undefined { comments, .. } => {
                layout_node_with_comments_opt(loc, comments.as_ref(), atom("undefined"))
            }
            ast::types::TypeInner::UniqueSymbol { comments, .. } => {
                layout_node_with_comments_opt(loc, comments.as_ref(), atom("unique symbol"))
            }
            ast::types::TypeInner::Nullable { inner: n, .. } => type_nullable(opts, loc, n),
            ast::types::TypeInner::Function { inner: func, .. } => {
                type_function(opts, fuse(vec![pretty_space(), atom("=>")]), loc, func)
            }
            ast::types::TypeInner::ConstructorType {
                abstract_,
                inner: func,
                ..
            } => {
                let prefix = if *abstract_ {
                    fuse(vec![atom("abstract"), pretty_space(), atom("new")])
                } else {
                    atom("new")
                };
                fuse(vec![
                    prefix,
                    pretty_space(),
                    type_function(opts, fuse(vec![pretty_space(), atom("=>")]), loc, func),
                ])
            }
            ast::types::TypeInner::Component { inner: comp, .. } => type_component(opts, loc, comp),
            ast::types::TypeInner::Object { inner: obj, .. } => {
                type_object(opts, atom(","), loc, obj)
            }
            ast::types::TypeInner::Interface { inner: i, .. } => type_interface(opts, loc, i),
            ast::types::TypeInner::Array { inner: a, .. } => type_array(opts, loc, a),
            ast::types::TypeInner::Conditional { inner: c, .. } => type_conditional(opts, loc, c),
            ast::types::TypeInner::Infer { inner: i, .. } => type_infer(opts, loc, i),
            ast::types::TypeInner::Generic { inner: g, .. } => type_generic(opts, loc, g),
            ast::types::TypeInner::IndexedAccess { inner: ia, .. } => {
                type_indexed_access(opts, false, loc, ia)
            }
            ast::types::TypeInner::OptionalIndexedAccess { inner: oia, .. } => {
                type_indexed_access(opts, oia.optional, loc, &oia.indexed_access)
            }
            ast::types::TypeInner::Union { inner: u, .. } => type_union(opts, loc, u),
            ast::types::TypeInner::Intersection { inner: i, .. } => type_intersection(opts, loc, i),
            ast::types::TypeInner::Typeof { inner: t, .. } => type_typeof(opts, loc, t),
            ast::types::TypeInner::Keyof { inner: k, .. } => type_keyof(opts, loc, k),
            ast::types::TypeInner::Renders { inner: r, .. } => render_type(opts, loc, r),
            ast::types::TypeInner::ReadOnly { inner: r, .. } => type_readonly(opts, loc, r),
            ast::types::TypeInner::Tuple { inner: t, .. } => type_tuple(opts, loc, t),
            ast::types::TypeInner::StringLiteral { literal: lit, .. } => {
                string_literal(opts, loc, lit)
            }
            ast::types::TypeInner::NumberLiteral { literal: lit, .. } => {
                number_literal(opts, loc, lit)
            }
            ast::types::TypeInner::BigIntLiteral { literal: lit, .. } => bigint_literal(loc, lit),
            ast::types::TypeInner::BooleanLiteral { literal: lit, .. } => boolean_literal(loc, lit),
            ast::types::TypeInner::TemplateLiteral { inner: t, .. } => {
                template_literal_type(opts, t)
            }
            ast::types::TypeInner::Exists { comments, .. } => {
                layout_node_with_comments_opt(loc, comments.as_ref(), atom("*"))
            }
        },
    )
}

fn type_with_parens(opts: &Opts, t: &ast::types::Type<Loc, Loc>) -> LayoutNode {
    match &**t {
        ast::types::TypeInner::Function { .. }
        | ast::types::TypeInner::Union { .. }
        | ast::types::TypeInner::Intersection { .. }
        | ast::types::TypeInner::Conditional { .. } => wrap_in_parens(false, type_(opts, t)),
        _ => type_(opts, t),
    }
}

fn type_nullable(opts: &Opts, loc: &Loc, n: &ast::types::Nullable<Loc, Loc>) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        n.comments.as_ref(),
        fuse(vec![atom("?"), type_with_parens(opts, &n.argument)]),
    )
}

fn type_function(
    opts: &Opts,
    sep: LayoutNode,
    loc: &Loc,
    func: &ast::types::Function<Loc, Loc>,
) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        func.comments.as_ref(),
        group(vec![
            if func.effect == ast::function::Effect::Hook {
                atom("hook")
            } else {
                LayoutNode::empty()
            },
            option_layout(
                |tp| {
                    type_parameter(
                        opts,
                        flow_parser::ast_visitor::TypeParamsContext::FunctionType,
                        tp,
                    )
                },
                func.tparams.as_ref(),
            ),
            type_function_params(opts, &func.params),
            sep,
            pretty_space(),
            type_function_return(opts, &func.return_),
        ]),
    )
}

fn type_function_return(
    opts: &Opts,
    ret: &ast::types::function::ReturnAnnotation<Loc, Loc>,
) -> LayoutNode {
    match ret {
        ast::types::function::ReturnAnnotation::Available(annot) => type_(opts, &annot.annotation),
        ast::types::function::ReturnAnnotation::TypeGuard(guard) => type_guard(opts, false, guard),
        ast::types::function::ReturnAnnotation::Missing(_) => LayoutNode::empty(),
    }
}

fn type_function_param(opts: &Opts, param: &ast::types::function::Param<Loc, Loc>) -> LayoutNode {
    use ast::types::function::ParamKind;
    source_location_with_comments(
        &param.loc,
        None::<&ast::Syntax<Loc, ()>>,
        match &param.param {
            ParamKind::Anonymous(annot) => type_(opts, annot),
            ParamKind::Labeled {
                name,
                annot,
                optional,
            } => fuse(vec![
                identifier(name),
                if *optional {
                    atom("?")
                } else {
                    LayoutNode::empty()
                },
                atom(":"),
                pretty_space(),
                type_(opts, annot),
            ]),
            ParamKind::Destructuring(patt) => pattern(opts, None, patt),
        },
    )
}

fn type_function_params(
    opts: &Opts,
    params: &ast::types::function::Params<Loc, Loc>,
) -> LayoutNode {
    let mut param_list: Vec<(Loc, CommentsBounds, LayoutNode)> = params
        .params
        .iter()
        .map(|param| {
            (
                param.loc.dupe(),
                comment_attachment::function_type_param_comment_bounds(param),
                type_function_param(opts, param),
            )
        })
        .collect();
    // Add rest param
    if let Some(rest) = &params.rest {
        let rest_layout = source_location_with_comments(
            &rest.loc,
            rest.comments.as_ref(),
            fuse(vec![atom("..."), type_function_param(opts, &rest.argument)]),
        );
        param_list.push((
            rest.loc.dupe(),
            comment_attachment::function_type_rest_param_comment_bounds(rest),
            rest_layout,
        ));
    }
    // Add this constraint
    if let Some(this_param) = &params.this {
        let this_layout = source_location_with_comments(
            &this_param.loc,
            None::<&ast::Syntax<Loc, ()>>,
            fuse(vec![
                atom("this"),
                type_annotation(opts, false, &this_param.annot),
            ]),
        );
        param_list.insert(
            0,
            (
                this_param.loc.dupe(),
                comment_attachment::function_type_this_param_comment_bounds(this_param),
                this_layout,
            ),
        );
    }
    let params_layout = list_with_newlines_with_sep(atom(","), pretty_line(), false, &param_list);
    let params_layout =
        list_add_internal_comments(&param_list, params_layout, params.comments.as_ref());
    layout_node_with_comments_opt(
        &params.loc,
        params.comments.as_ref(),
        fuse(vec![wrap_and_indent(
            (atom("("), atom(")")),
            None,
            params_layout,
        )]),
    )
}

fn type_union_or_intersection(
    opts: &Opts,
    sep: LayoutNode,
    loc: &Loc,
    ts: Vec<&ast::types::Type<Loc, Loc>>,
    comments: Option<&ast::Syntax<Loc, ()>>,
) -> LayoutNode {
    // Do not break at the start if the last leading comment is on an earlier line,
    // as a line break will already be inserted after the comment
    let inline_start = match comments {
        None => false,
        Some(syntax) => match syntax.leading.last() {
            Some(last_leading) if last_leading.loc.end.line < loc.start.line => true,
            _ => false,
        },
    };
    let items: Vec<LayoutNode> = ts
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let type_separator = comment_aware_separator(
                None,
                pretty_space(),
                &comment_attachment::type_comment_bounds(t),
            );
            let full_sep = fuse(vec![sep.dupe(), type_separator]);
            fuse(vec![
                if i == 0 {
                    if_break(full_sep, LayoutNode::empty())
                } else {
                    full_sep
                },
                type_with_parens(opts, t),
            ])
        })
        .collect();
    layout::list(
        None,
        None,
        None,
        false,
        Some((inline_start, true)),
        None,
        items,
    )
}

fn type_union(opts: &Opts, loc: &Loc, u: &ast::types::Union<Loc, Loc>) -> LayoutNode {
    let mut ts = vec![&u.types.0, &u.types.1];
    ts.extend(u.types.2.iter());
    layout_node_with_comments_opt(
        loc,
        u.comments.as_ref(),
        type_union_or_intersection(opts, atom("|"), loc, ts, u.comments.as_ref()),
    )
}

fn type_intersection(opts: &Opts, loc: &Loc, i: &ast::types::Intersection<Loc, Loc>) -> LayoutNode {
    let mut ts = vec![&i.types.0, &i.types.1];
    ts.extend(i.types.2.iter());
    layout_node_with_comments_opt(
        loc,
        i.comments.as_ref(),
        type_union_or_intersection(opts, atom("&"), loc, ts, i.comments.as_ref()),
    )
}

fn type_typeof(opts: &Opts, loc: &Loc, t: &ast::types::Typeof<Loc, Loc>) -> LayoutNode {
    fn typeof_generic_identifier(
        opts: &Opts,
        target: &ast::types::typeof_::Target<Loc, Loc>,
    ) -> LayoutNode {
        match target {
            ast::types::typeof_::Target::Unqualified(id) => identifier(id),
            ast::types::typeof_::Target::Qualified(q) => source_location_with_comments(
                &q.loc,
                None::<&ast::Syntax<Loc, ()>>,
                fuse(vec![
                    typeof_generic_identifier(opts, &q.qualification),
                    atom("."),
                    identifier(&q.id),
                ]),
            ),
            ast::types::typeof_::Target::Import(imp) => fuse(vec![
                atom("import"),
                atom("("),
                string_literal(opts, &imp.source.0, &imp.source.1),
                atom(")"),
            ]),
        }
    }
    layout_node_with_comments_opt(
        loc,
        t.comments.as_ref(),
        fuse(vec![
            atom("typeof"),
            space(),
            typeof_generic_identifier(opts, &t.argument),
            option_layout(|targs| type_args(opts, targs), t.targs.as_ref()),
        ]),
    )
}

fn type_keyof(opts: &Opts, loc: &Loc, k: &ast::types::Keyof<Loc, Loc>) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        k.comments.as_ref(),
        fuse(vec![atom("keyof"), space(), type_(opts, &k.argument)]),
    )
}

fn render_type(opts: &Opts, loc: &Loc, r: &ast::types::Renders<Loc, Loc>) -> LayoutNode {
    let operator = match r.variant {
        ast::types::RendersVariant::Normal => "renders",
        ast::types::RendersVariant::Maybe => "renders?",
        ast::types::RendersVariant::Star => "renders*",
    };
    flow_type_operator(opts, loc, r.comments.as_ref(), operator, &r.argument)
}

fn flow_type_operator(
    opts: &Opts,
    loc: &Loc,
    comments: Option<&ast::Syntax<Loc, ()>>,
    operator: &str,
    operand: &ast::types::Type<Loc, Loc>,
) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        comments,
        fuse(vec![
            atom(operator),
            space(),
            type_with_parens(opts, operand),
        ]),
    )
}

fn type_readonly(opts: &Opts, loc: &Loc, r: &ast::types::ReadOnly<Loc, Loc>) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        r.comments.as_ref(),
        fuse(vec![atom("readonly"), space(), type_(opts, &r.argument)]),
    )
}

fn type_array(opts: &Opts, loc: &Loc, a: &ast::types::Array<Loc, Loc>) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        a.comments.as_ref(),
        fuse(vec![atom("Array<"), type_(opts, &a.argument), atom(">")]),
    )
}

fn type_conditional(opts: &Opts, loc: &Loc, c: &ast::types::Conditional<Loc, Loc>) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        c.comments.as_ref(),
        group(vec![
            fuse(vec![
                type_with_parens(opts, &c.check_type),
                space(),
                atom("extends"),
                space(),
                type_with_parens(opts, &c.extends_type),
            ]),
            LayoutNode::indent(fuse(vec![
                pretty_line(),
                atom("?"),
                pretty_space(),
                type_(opts, &c.true_type),
                pretty_line(),
                atom(":"),
                pretty_space(),
                type_(opts, &c.false_type),
            ])),
        ]),
    )
}

fn type_infer(opts: &Opts, loc: &Loc, i: &ast::types::Infer<Loc, Loc>) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        i.comments.as_ref(),
        fuse(vec![atom("infer"), space(), type_param(opts, &i.tparam)]),
    )
}

fn type_generic(opts: &Opts, loc: &Loc, g: &ast::types::Generic<Loc, Loc>) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        g.comments.as_ref(),
        fuse(vec![
            generic_identifier(opts, &g.id),
            option_layout(|targs| type_args(opts, targs), g.targs.as_ref()),
        ]),
    )
}

fn generic_identifier(opts: &Opts, id: &ast::types::generic::Identifier<Loc, Loc>) -> LayoutNode {
    match id {
        ast::types::generic::Identifier::Unqualified(id) => identifier(id),
        ast::types::generic::Identifier::Qualified(q) => source_location_with_comments(
            &q.loc,
            None::<&ast::Syntax<Loc, ()>>,
            fuse(vec![
                generic_identifier(opts, &q.qualification),
                atom("."),
                identifier(&q.id),
            ]),
        ),
        ast::types::generic::Identifier::ImportTypeAnnot(imp) => layout_node_with_comments_opt(
            &imp.loc,
            imp.comments.as_ref(),
            fuse(vec![
                atom("import"),
                atom("("),
                string_literal(opts, &imp.source.0, &imp.source.1),
                atom(")"),
            ]),
        ),
    }
}

fn type_indexed_access(
    opts: &Opts,
    optional: bool,
    loc: &Loc,
    ia: &ast::types::IndexedAccess<Loc, Loc>,
) -> LayoutNode {
    let left_delim = if optional { atom("?.[") } else { atom("[") };
    layout_node_with_comments_opt(
        loc,
        ia.comments.as_ref(),
        fuse(vec![
            type_(opts, &ia.object),
            left_delim,
            type_(opts, &ia.index),
            atom("]"),
        ]),
    )
}

fn type_tuple(opts: &Opts, loc: &Loc, t: &ast::types::Tuple<Loc, Loc>) -> LayoutNode {
    let mut elements: Vec<LayoutNode> = t
        .elements
        .iter()
        .map(|elem| match elem {
            ast::types::tuple::Element::UnlabeledElement {
                annot, optional, ..
            } => fuse(vec![
                type_(opts, annot),
                if *optional {
                    atom("?")
                } else {
                    LayoutNode::empty()
                },
            ]),
            ast::types::tuple::Element::LabeledElement {
                element: e, loc, ..
            } => type_tuple_labeled_element(opts, loc, e),
            ast::types::tuple::Element::SpreadElement {
                element: e, loc, ..
            } => type_tuple_spread_element(opts, loc, e),
        })
        .collect();
    if t.inexact {
        elements.push(atom("..."));
    }
    layout_node_with_comments_opt(
        loc,
        t.comments.as_ref(),
        group(vec![new_list(
            Some((atom("["), atom("]"))),
            Some(atom(",")),
            false,
            true,
            elements,
        )]),
    )
}

fn type_tuple_labeled_element(
    opts: &Opts,
    loc: &Loc,
    e: &ast::types::tuple::LabeledElement<Loc, Loc>,
) -> LayoutNode {
    source_location_with_comments(
        loc,
        None::<&ast::Syntax<Loc, ()>>,
        fuse(vec![
            option_layout(variance, e.variance.as_ref()),
            identifier(&e.name),
            if e.optional {
                atom("?")
            } else {
                LayoutNode::empty()
            },
            atom(":"),
            pretty_space(),
            type_(opts, &e.annot),
        ]),
    )
}

fn type_tuple_spread_element(
    opts: &Opts,
    loc: &Loc,
    e: &ast::types::tuple::SpreadElement<Loc, Loc>,
) -> LayoutNode {
    source_location_with_comments(
        loc,
        None::<&ast::Syntax<Loc, ()>>,
        match &e.name {
            Some(name) => fuse(vec![
                atom("..."),
                identifier(name),
                atom(":"),
                pretty_space(),
                type_(opts, &e.annot),
            ]),
            None => fuse(vec![atom("..."), type_(opts, &e.annot)]),
        },
    )
}

fn template_literal_type(opts: &Opts, t: &ast::types::TypeTemplateLiteral<Loc, Loc>) -> LayoutNode {
    let template_element = |i: usize, elem: &ast::types::type_template_literal::Element<Loc>| {
        fuse(vec![
            source_location_with_comments(
                &elem.loc,
                None::<&ast::Syntax<Loc, ()>>,
                fuse(vec![
                    if i > 0 {
                        atom("}")
                    } else {
                        LayoutNode::empty()
                    },
                    atom(&elem.value.raw),
                    if !elem.tail {
                        atom("${")
                    } else {
                        LayoutNode::empty()
                    },
                ]),
            ),
            if !elem.tail {
                type_(opts, &t.types[i])
            } else {
                LayoutNode::empty()
            },
        ])
    };
    fuse(vec![
        atom("`"),
        fuse(
            t.quasis
                .iter()
                .enumerate()
                .map(|(i, elem)| template_element(i, elem))
                .collect(),
        ),
        atom("`"),
    ])
}

fn type_object(
    opts: &Opts,
    sep: LayoutNode,
    loc: &Loc,
    obj: &ast::types::Object<Loc, Loc>,
) -> LayoutNode {
    let s_exact = if obj.exact {
        atom("|")
    } else {
        LayoutNode::empty()
    };
    let sep_linebreak = pretty_line();
    fn prop_loc(property: &ast::types::object::Property<Loc, Loc>) -> &Loc {
        match property {
            ast::types::object::Property::NormalProperty(p) => &p.loc,
            ast::types::object::Property::SpreadProperty(p) => &p.loc,
            ast::types::object::Property::Indexer(p) => &p.loc,
            ast::types::object::Property::CallProperty(p) => &p.loc,
            ast::types::object::Property::InternalSlot(p) => &p.loc,
            ast::types::object::Property::MappedType(p) => &p.loc,
            ast::types::object::Property::PrivateField(p) => &p.loc,
        }
    }
    let num_props = obj.properties.len();
    let int_comments = internal_comments(obj.comments.as_ref());
    let has_internal_comments = int_comments.is_some();
    let mut props: Vec<(Loc, CommentsBounds, LayoutNode)> = obj
        .properties
        .iter()
        .enumerate()
        .map(|(i, property)| {
            // Add trailing comma to last property
            let prop_layout = if i == num_props - 1
                && !(has_internal_comments || obj.inexact)
                && TrailingCommas::enabled_for_types(opts.trailing_commas)
            {
                let trailing_sep = if_break(sep.dupe(), LayoutNode::empty());
                fuse(vec![type_object_property(opts, property), trailing_sep])
            } else {
                type_object_property(opts, property)
            };
            (
                prop_loc(property).dupe(),
                comment_attachment::object_type_property_comment_bounds(property),
                prop_layout,
            )
        })
        .collect();
    // Add internal comments and the inexact ellipsis
    match (int_comments, obj.inexact) {
        (Some((c_loc, c_junk, c_layout)), true) => {
            // line comments have their own linebreak, so if the comment
            // right before the ellipsis is a line comment, don't add another linebreak
            let has_trailing_line_comment = match &obj.comments {
                Some(syntax) => {
                    matches!(syntax.internal.last(), Some(c) if matches!(c.kind, ast::CommentKind::Line))
                }
                None => false,
            };
            let linebreak = if has_trailing_line_comment {
                LayoutNode::empty()
            } else {
                sep_linebreak.dupe()
            };
            props.push((c_loc, c_junk, fuse(vec![c_layout, linebreak, atom("...")])));
        }
        (Some(comments), false) => {
            props.push(comments);
        }
        (None, true) => {
            props.push((
                loc.dupe(),
                CommentsBounds {
                    first_leading: None,
                    last_trailing: None,
                },
                atom("..."),
            ));
        }
        (None, false) => {}
    }
    let break_opt = match props.first() {
        None => None,
        Some((first_loc, _, _)) => {
            if loc.start.line < first_loc.start.line {
                Some(pretty_hardline())
            } else if opts.bracket_spacing {
                Some(pretty_line())
            } else {
                None
            }
        }
    };
    let props_list = list_with_newlines_with_sep(sep, sep_linebreak, false, &props);
    let props_layout = wrap_and_indent(
        (
            fuse(vec![atom("{"), s_exact.clone()]),
            fuse(vec![s_exact, atom("}")]),
        ),
        break_opt,
        props_list,
    );
    layout_node_with_comments_opt(loc, obj.comments.as_ref(), group(vec![props_layout]))
}

fn type_object_property(
    opts: &Opts,
    property: &ast::types::object::Property<Loc, Loc>,
) -> LayoutNode {
    match property {
        ast::types::object::Property::NormalProperty(prop) => {
            let loc = &prop.loc;
            let s_static = if prop.static_ {
                fuse(vec![atom("static"), space()])
            } else {
                LayoutNode::empty()
            };
            let s_proto = if prop.proto {
                fuse(vec![atom("proto"), space()])
            } else {
                LayoutNode::empty()
            };
            let s_abstract = if prop.abstract_ {
                fuse(vec![atom("abstract"), space()])
            } else {
                LayoutNode::empty()
            };
            let s_override = if prop.override_ {
                fuse(vec![atom("override"), space()])
            } else {
                LayoutNode::empty()
            };
            let s_accessibility = ts_accessibility_layout(prop.ts_accessibility.as_ref());
            let init_ = &prop.init;
            let s_init = match init_ {
                Some(expr) => fuse(vec![
                    space(),
                    atom("="),
                    space(),
                    expression(opts, None, expr),
                ]),
                None => LayoutNode::empty(),
            };
            source_location_with_comments(
                loc,
                prop.comments.as_ref(),
                match (&prop.value, prop.method, prop.proto, prop.optional) {
                    // Functions with no special properties can be rendered as methods
                    (ast::types::object::PropertyValue::Init(Some(init_t)), true, false, false)
                        if matches!(&**init_t, ast::types::TypeInner::Function { .. }) =>
                    {
                        let func = match &**init_t {
                            ast::types::TypeInner::Function { inner: f, .. } => f,
                            _ => unreachable!(),
                        };
                        source_location_with_comments(
                            init_t.loc(),
                            None::<&ast::Syntax<Loc, ()>>,
                            fuse(vec![
                                s_accessibility,
                                s_static,
                                s_override,
                                s_abstract,
                                object_property_key(opts, &prop.key),
                                type_function(opts, atom(":"), init_t.loc(), func),
                            ]),
                        )
                    }
                    // Optional methods: key?(): Type
                    (ast::types::object::PropertyValue::Init(Some(init_t)), true, false, true)
                        if matches!(&**init_t, ast::types::TypeInner::Function { .. }) =>
                    {
                        let func = match &**init_t {
                            ast::types::TypeInner::Function { inner: f, .. } => f,
                            _ => unreachable!(),
                        };
                        source_location_with_comments(
                            init_t.loc(),
                            None::<&ast::Syntax<Loc, ()>>,
                            fuse(vec![
                                s_accessibility,
                                s_static,
                                s_override,
                                s_abstract,
                                object_property_key(opts, &prop.key),
                                atom("?"),
                                type_function(opts, atom(":"), init_t.loc(), func),
                            ]),
                        )
                    }
                    // Property with accessibility modifier but no annotation, e.g. `private x`
                    (ast::types::object::PropertyValue::Init(Some(init_t)), _, _, _)
                        if prop.ts_accessibility.is_some()
                            && matches!(
                                &**init_t,
                                ast::types::TypeInner::Any { comments: None, .. }
                            ) =>
                    {
                        fuse(vec![
                            s_accessibility,
                            s_static,
                            s_override,
                            s_abstract,
                            s_proto,
                            option_layout(variance, prop.variance.as_ref()),
                            object_property_key(opts, &prop.key),
                        ])
                    }
                    // Property with init but no type annotation
                    (ast::types::object::PropertyValue::Init(None), _, _, _) => fuse(vec![
                        s_accessibility,
                        s_static,
                        s_override,
                        s_abstract,
                        s_proto,
                        option_layout(variance, prop.variance.as_ref()),
                        object_property_key(opts, &prop.key),
                        if prop.optional {
                            atom("?")
                        } else {
                            LayoutNode::empty()
                        },
                        s_init,
                    ]),
                    // Normal properties
                    (ast::types::object::PropertyValue::Init(Some(t)), _, _, _) => fuse(vec![
                        s_accessibility,
                        s_static,
                        s_override,
                        s_abstract,
                        s_proto,
                        option_layout(variance, prop.variance.as_ref()),
                        object_property_key(opts, &prop.key),
                        if prop.optional {
                            atom("?")
                        } else {
                            LayoutNode::empty()
                        },
                        atom(":"),
                        pretty_space(),
                        type_(opts, t),
                        s_init,
                    ]),
                    // Getters/Setters
                    (ast::types::object::PropertyValue::Get(get_loc, get_func), _, _, _) => {
                        source_location_with_comments(
                            get_loc,
                            None::<&ast::Syntax<Loc, ()>>,
                            fuse(vec![
                                s_override,
                                atom("get"),
                                space(),
                                object_property_key(opts, &prop.key),
                                type_function(opts, atom(":"), get_loc, get_func),
                            ]),
                        )
                    }
                    (ast::types::object::PropertyValue::Set(set_loc, set_func), _, _, _) => {
                        source_location_with_comments(
                            set_loc,
                            None::<&ast::Syntax<Loc, ()>>,
                            fuse(vec![
                                s_override,
                                atom("set"),
                                space(),
                                object_property_key(opts, &prop.key),
                                type_function(opts, atom(":"), set_loc, set_func),
                            ]),
                        )
                    }
                },
            )
        }
        ast::types::object::Property::SpreadProperty(spread) => source_location_with_comments(
            &spread.loc,
            spread.comments.as_ref(),
            fuse(vec![atom("..."), type_(opts, &spread.argument)]),
        ),
        ast::types::object::Property::Indexer(indexer) => source_location_with_comments(
            &indexer.loc,
            indexer.comments.as_ref(),
            indexer_property_layout(opts, indexer),
        ),
        ast::types::object::Property::MappedType(mapped) => {
            let optional_token = match mapped.optional {
                ast::types::object::MappedTypeOptionalFlag::PlusOptional => atom("+?"),
                ast::types::object::MappedTypeOptionalFlag::MinusOptional => atom("-?"),
                ast::types::object::MappedTypeOptionalFlag::Optional => atom("?"),
                ast::types::object::MappedTypeOptionalFlag::NoOptionalFlag => LayoutNode::empty(),
            };
            use ast::types::object::MappedTypeVarianceOp;
            let variance_token = match (&mapped.variance_op, &mapped.variance) {
                (Some(MappedTypeVarianceOp::Add), Some(v)) => fuse(vec![atom("+"), variance(v)]),
                (Some(MappedTypeVarianceOp::Remove), Some(v)) => fuse(vec![atom("-"), variance(v)]),
                (_, _) => option_layout(variance, mapped.variance.as_ref()),
            };
            source_location_with_comments(
                &mapped.loc,
                mapped.comments.as_ref(),
                fuse(vec![
                    variance_token,
                    atom("["),
                    identifier(&mapped.key_tparam.name),
                    space(),
                    atom("in"),
                    space(),
                    type_(opts, &mapped.source_type),
                    match &mapped.name_type {
                        Some(t) => fuse(vec![space(), atom("as"), space(), type_(opts, t)]),
                        None => LayoutNode::empty(),
                    },
                    atom("]"),
                    optional_token,
                    atom(":"),
                    pretty_space(),
                    type_(opts, &mapped.prop_type),
                ]),
            )
        }
        ast::types::object::Property::PrivateField(pf) => source_location_with_comments(
            &pf.loc,
            pf.comments.as_ref(),
            source_location_with_comments(
                &pf.key.loc,
                pf.key.comments.as_ref(),
                atom(&format!("#{}", pf.key.name)),
            ),
        ),
        ast::types::object::Property::CallProperty(call) => source_location_with_comments(
            &call.loc,
            call.comments.as_ref(),
            fuse(vec![
                if call.static_ {
                    fuse(vec![atom("static"), space()])
                } else {
                    LayoutNode::empty()
                },
                source_location_with_comments(
                    &call.value.0,
                    None::<&ast::Syntax<Loc, ()>>,
                    type_function(opts, atom(":"), &call.value.0, &call.value.1),
                ),
            ]),
        ),
        ast::types::object::Property::InternalSlot(slot) => source_location_with_comments(
            &slot.loc,
            slot.comments.as_ref(),
            fuse(vec![
                if slot.static_ {
                    fuse(vec![atom("static"), space()])
                } else {
                    LayoutNode::empty()
                },
                atom("[["),
                identifier(&slot.id),
                atom("]]"),
                if slot.optional {
                    atom("?")
                } else {
                    LayoutNode::empty()
                },
                atom(":"),
                pretty_space(),
                type_(opts, &slot.value),
            ]),
        ),
    }
}

fn type_interface(opts: &Opts, loc: &Loc, i: &ast::types::Interface<Loc, Loc>) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        i.comments.as_ref(),
        fuse(vec![
            atom("interface"),
            interface_extends(opts, &i.extends),
            pretty_space(),
            source_location_with_comments(
                &i.body.0,
                None::<&ast::Syntax<Loc, ()>>,
                type_object(opts, atom(","), &i.body.0, &i.body.1),
            ),
        ]),
    )
}

fn type_component(opts: &Opts, loc: &Loc, comp: &ast::types::Component<Loc, Loc>) -> LayoutNode {
    layout_node_with_comments_opt(
        loc,
        comp.comments.as_ref(),
        fuse(vec![
            atom("component"),
            option_layout(
                |tp| {
                    type_parameter(
                        opts,
                        flow_parser::ast_visitor::TypeParamsContext::ComponentType,
                        tp,
                    )
                },
                comp.tparams.as_ref(),
            ),
            group(vec![
                layout_node_with_comments_opt(
                    &comp.params.loc,
                    comp.params.comments.as_ref(),
                    component_type_params(opts, &comp.params),
                ),
                component_renders(opts, &comp.renders),
            ]),
        ]),
    )
}

fn call_args(
    opts: &Opts,
    is_new: bool,
    lparen: &str,
    args: &ast::expression::ArgList<Loc, Loc>,
) -> LayoutNode {
    fn arg_loc(arg: &ast::expression::ExpressionOrSpread<Loc, Loc>) -> &Loc {
        match arg {
            ast::expression::ExpressionOrSpread::Expression(expr) => expr.loc(),
            ast::expression::ExpressionOrSpread::Spread(spread) => &spread.loc,
        }
    }
    let num_args = args.arguments.len();
    let internal_comments_list: Vec<(Loc, CommentsBounds, LayoutNode)> =
        match internal_comments(args.comments.as_ref()) {
            None => vec![],
            Some(comments) => vec![comments],
        };
    // Multiline JSX expression in new expressions are wrapped in parentheses by prettier. If we
    // encounter a multiline JSX expression in a new expression, expand its loc by one line in both
    // directions when determining adjacent whitespace to account for where the parentheses would be.
    let expand_arg_loc = |loc: Loc, arg: &ast::expression::ExpressionOrSpread<Loc, Loc>| -> Loc {
        match arg {
            ast::expression::ExpressionOrSpread::Expression(expr)
                if is_new
                    && matches!(
                        &**expr,
                        ast::expression::ExpressionInner::JSXElement { .. }
                            | ast::expression::ExpressionInner::JSXFragment { .. }
                    )
                    && expr.loc().start.line < expr.loc().end.line =>
            {
                let mut new_loc = loc;
                new_loc.start.line -= 1;
                new_loc.end.line += 1;
                new_loc
            }
            _ => loc,
        }
    };
    let call_args_list: Vec<(Loc, CommentsBounds, LayoutNode)> = args
        .arguments
        .iter()
        .enumerate()
        .map(|(i, arg)| {
            let loc = arg_loc(arg).dupe();
            let comment_bounds = comment_attachment::expression_or_spread_comment_bounds(&loc, arg);
            // Add trailing comma to last argument
            let mut arg_layout = expression_or_spread(opts, arg);
            if i == num_args - 1
                && internal_comments_list.is_empty()
                && TrailingCommas::enabled_for_function_args(opts.trailing_commas)
            {
                arg_layout = fuse(vec![arg_layout, if_break(atom(","), LayoutNode::empty())]);
            }
            (expand_arg_loc(loc, arg), comment_bounds, arg_layout)
        })
        .collect();
    let has_internal_comments = !internal_comments_list.is_empty();
    // Add internal comments
    let mut call_args_list = call_args_list;
    call_args_list.extend(internal_comments_list);
    let args_layout = list_with_newlines_with_sep(atom(","), pretty_line(), false, &call_args_list);
    // Match prettier's behavior by preserving whether a single template argument begins on new line
    let break_opt = match args.arguments.as_ref() {
        [ast::expression::ExpressionOrSpread::Expression(expr)]
            if !has_internal_comments
                && matches!(
                    &**expr,
                    ast::expression::ExpressionInner::TemplateLiteral { .. }
                        | ast::expression::ExpressionInner::TaggedTemplate { .. }
                ) =>
        {
            if args.loc.start.line == expr.loc().start.line {
                Some(LayoutNode::empty())
            } else {
                Some(pretty_hardline())
            }
        }
        _ => Some(softline()),
    };
    source_location_with_comments(
        &args.loc,
        args.comments.as_ref(),
        group(vec![wrap_and_indent(
            (atom(lparen), atom(")")),
            break_opt,
            args_layout,
        )]),
    )
}

fn call_type_args(
    opts: &Opts,
    less_than: &str,
    targs: &ast::expression::CallTypeArgs<Loc, Loc>,
) -> LayoutNode {
    fn arg_loc(arg: &ast::expression::CallTypeArg<Loc, Loc>) -> &Loc {
        match arg {
            ast::expression::CallTypeArg::Explicit(t) => t.loc(),
            ast::expression::CallTypeArg::Implicit(imp) => &imp.loc,
        }
    }
    let num_args = targs.arguments.len();
    let internal_comments_list: Vec<(Loc, CommentsBounds, LayoutNode)> =
        match internal_comments(targs.comments.as_ref()) {
            None => vec![],
            Some(comments) => vec![comments],
        };
    let type_args_list: Vec<(Loc, CommentsBounds, LayoutNode)> = targs
        .arguments
        .iter()
        .enumerate()
        .map(|(i, arg)| {
            let loc = arg_loc(arg).dupe();
            let comment_bounds = comment_attachment::call_type_arg_comment_bounds(&loc, arg);
            // Add trailing comma to last argument
            let mut arg_layout = call_type_arg(opts, arg);
            if i == num_args - 1
                && internal_comments_list.is_empty()
                && TrailingCommas::enabled_for_types(opts.trailing_commas)
            {
                arg_layout = fuse(vec![arg_layout, if_break(atom(","), LayoutNode::empty())]);
            }
            (loc, comment_bounds, arg_layout)
        })
        .collect();
    // Add internal comments
    let mut type_args_list = type_args_list;
    type_args_list.extend(internal_comments_list);
    let args_layout = list_with_newlines_with_sep(atom(","), pretty_line(), false, &type_args_list);
    source_location_with_comments(
        &targs.loc,
        targs.comments.as_ref(),
        group(vec![wrap_and_indent(
            (atom(less_than), atom(">")),
            None,
            args_layout,
        )]),
    )
}

pub(crate) fn comment(replacement_for_same_type: bool, c: &ast::Comment<Loc>) -> LayoutNode {
    comment_layout(replacement_for_same_type, c)
}

pub fn string_literal(opts: &Opts, loc: &Loc, lit: &ast::StringLiteral<Loc>) -> LayoutNode {
    let node = if opts.preserve_formatting {
        atom(&lit.raw)
    } else {
        let quote = better_quote(opts.single_quotes, &lit.value);
        fuse(vec![
            atom(quote),
            atom(&utf8_escape(quote, &lit.value)),
            atom(quote),
        ])
    };
    layout_node_with_comments_opt(loc, lit.comments.as_ref(), node)
}

pub(crate) fn number_literal(opts: &Opts, loc: &Loc, lit: &ast::NumberLiteral<Loc>) -> LayoutNode {
    let node = if opts.preserve_formatting {
        atom(&lit.raw)
    } else {
        let s = shortest_string_of_float(lit.value);
        if lit.raw == s {
            atom(&lit.raw)
        } else {
            if_pretty(atom(&lit.raw), atom(&s))
        }
    };
    layout_node_with_comments_opt(loc, lit.comments.as_ref(), node)
}

fn number_literal_member(opts: &Opts, loc: &Loc, lit: &ast::NumberLiteral<Loc>) -> LayoutNode {
    let node = if opts.preserve_formatting {
        atom(&lit.raw)
    } else {
        let s = shortest_string_of_float(lit.value);
        // `1.foo` is a syntax error, but `1.0.foo`, `1e0.foo` and even `1..foo` are all ok.
        let is_int = |x: &str| !x.contains('.') && !x.contains('e');
        let if_pretty_node = if is_int(&lit.raw) {
            wrap_in_parens(false, atom(&lit.raw))
        } else {
            atom(&lit.raw)
        };
        let if_ugly = if is_int(&s) {
            fuse(vec![atom(&s), atom(".")])
        } else {
            atom(&s)
        };
        if if_pretty_node == if_ugly {
            if_pretty_node
        } else {
            if_pretty(if_pretty_node, if_ugly)
        }
    };
    layout_node_with_comments_opt(loc, lit.comments.as_ref(), node)
}

pub(crate) fn bigint_literal(loc: &Loc, lit: &ast::BigIntLiteral<Loc>) -> LayoutNode {
    layout_node_with_comments_opt(loc, lit.comments.as_ref(), atom(&lit.raw))
}

pub(crate) fn boolean_literal(loc: &Loc, lit: &ast::BooleanLiteral<Loc>) -> LayoutNode {
    let value = if lit.value { "true" } else { "false" };
    layout_node_with_comments_opt(loc, lit.comments.as_ref(), atom(value))
}

pub(crate) fn regexp_literal(opts: &Opts, loc: &Loc, lit: &ast::RegExpLiteral<Loc>) -> LayoutNode {
    let node = if opts.preserve_formatting {
        atom(&lit.raw)
    } else {
        let mut flags_chars: Vec<char> = lit.flags.chars().collect();
        flags_chars.sort();
        let flags: String = flags_chars.into_iter().collect();
        fuse(vec![
            atom("/"),
            atom(lit.pattern.as_str()),
            atom("/"),
            atom(&flags),
        ])
    };
    layout_node_with_comments_opt(loc, lit.comments.as_ref(), node)
}

pub(crate) fn module_ref_literal(loc: &Loc, lit: &ast::ModuleRefLiteral<Loc>) -> LayoutNode {
    layout_node_with_comments_opt(loc, lit.comments.as_ref(), atom(lit.raw.as_str()))
}

pub(crate) fn variance(var: &ast::Variance<Loc>) -> LayoutNode {
    source_location_with_comments(
        &var.loc,
        var.comments.as_ref(),
        match var.kind {
            ast::VarianceKind::Plus => atom("+"),
            ast::VarianceKind::Minus => atom("-"),
            ast::VarianceKind::Readonly => fuse(vec![atom("readonly"), space()]),
            ast::VarianceKind::Writeonly => fuse(vec![atom("writeonly"), space()]),
            ast::VarianceKind::In => fuse(vec![atom("in"), space()]),
            ast::VarianceKind::Out => fuse(vec![atom("out"), space()]),
            ast::VarianceKind::InOut => fuse(vec![atom("in out"), space()]),
        },
    )
}

pub(crate) fn type_annotation(
    opts: &Opts,
    parens: bool,
    annot: &ast::types::Annotation<Loc, Loc>,
) -> LayoutNode {
    source_location_with_comments(
        &annot.loc,
        None::<&ast::Syntax<Loc, ()>>,
        fuse(vec![
            atom(":"),
            pretty_space(),
            if parens {
                wrap_in_parens(false, type_(opts, &annot.annotation))
            } else {
                type_(opts, &annot.annotation)
            },
        ]),
    )
}

pub(crate) fn type_guard(
    opts: &Opts,
    needs_parens: bool,
    guard: &ast::types::TypeGuard<Loc, Loc>,
) -> LayoutNode {
    let (ref x, ref t) = guard.guard;
    let kind_part: Vec<LayoutNode> = match guard.kind {
        ast::types::TypeGuardKind::Default => vec![],
        ast::types::TypeGuardKind::Asserts => vec![atom("asserts")],
        ast::types::TypeGuardKind::Implies => vec![atom("implies")],
    };
    let id_part = identifier(x);
    let type_part: Vec<LayoutNode> = match t {
        None => vec![],
        Some(t) if needs_parens => vec![atom("is"), wrap_in_parens(false, type_(opts, t))],
        Some(t) => vec![atom("is"), type_(opts, t)],
    };
    let mut parts = kind_part;
    parts.push(id_part);
    parts.extend(type_part);
    join(space(), parts)
}

pub(crate) fn type_guard_annotation(
    opts: &Opts,
    needs_parens: bool,
    annot: &ast::types::TypeGuardAnnotation<Loc, Loc>,
) -> LayoutNode {
    source_location_with_comments(
        &annot.loc,
        None::<&ast::Syntax<Loc, ()>>,
        fuse(vec![
            atom(":"),
            pretty_space(),
            type_guard(opts, needs_parens, &annot.guard),
        ]),
    )
}
pub(crate) fn function_params(opts: &Opts, params: &ast::function::Params<Loc, Loc>) -> LayoutNode {
    function_params_inner(&NORMAL_CONTEXT, opts, params)
}

fn function_params_inner(
    ctxt: &ExpressionContext,
    opts: &Opts,
    params_ast: &ast::function::Params<Loc, Loc>,
) -> LayoutNode {
    let mut params_list: Vec<(Loc, CommentsBounds, LayoutNode)> = params_ast
        .params
        .iter()
        .map(|param| {
            let loc = match param {
                ast::function::Param::RegularParam { loc, .. } => loc,
                ast::function::Param::ParamProperty { loc, .. } => loc,
            };
            (
                loc.dupe(),
                comment_attachment::function_param_comment_bounds(param),
                function_param(ctxt, opts, param),
            )
        })
        .collect();
    // Add rest param
    if let Some(rest) = &params_ast.rest {
        let rest_layout = source_location_with_comments(
            &rest.loc,
            rest.comments.as_ref(),
            fuse(vec![atom("..."), pattern(opts, Some(ctxt), &rest.argument)]),
        );
        let rest_param = (
            rest.loc.dupe(),
            comment_attachment::function_rest_param_comment_bounds(rest),
            rest_layout,
        );
        params_list.push(rest_param);
    }
    // Add this param
    if let Some(this_param) = &params_ast.this_ {
        let this_layout = source_location_with_comments(
            &this_param.loc,
            this_param.comments.as_ref(),
            fuse(vec![
                atom("this"),
                type_annotation(opts, false, &this_param.annot),
            ]),
        );
        let this_p = (
            this_param.loc.dupe(),
            comment_attachment::function_this_param_comment_bounds(this_param),
            this_layout,
        );
        params_list.insert(0, this_p);
    }
    let mut params_layout =
        list_with_newlines_with_sep(atom(","), pretty_line(), false, &params_list);
    // Add trailing comma
    if !params_list.is_empty()
        && params_ast.rest.is_none()
        && TrailingCommas::enabled_for_function_params(opts.trailing_commas)
    {
        params_layout.push(if_break(atom(","), LayoutNode::empty()));
    }
    let params_layout =
        list_add_internal_comments(&params_list, params_layout, params_ast.comments.as_ref());
    wrap_and_indent((atom("("), atom(")")), None, params_layout)
}

#[allow(dead_code)]
pub fn function_params_and_return(
    opts: &Opts,
    func: &ast::function::Function<Loc, Loc>,
) -> LayoutNode {
    source_location_with_comments(
        &func.sig_loc,
        func.comments.as_ref(),
        function_base(
            opts,
            &LayoutNode::empty(),
            &func.params,
            None,
            func.predicate.as_ref(),
            &func.return_,
            func.tparams.as_ref(),
            &func.sig_loc,
            func.comments.as_ref(),
        ),
    )
}

pub fn class_method(opts: &Opts, method: &ast::class::Method<Loc, Loc>) -> LayoutNode {
    let func = &method.value.1;
    let func_loc = &method.value.0;
    // methods don't use id; see `key`
    source_location_with_comments(&method.loc, method.comments.as_ref(), {
        let s_key = match &method.key {
            ast::expression::object::Key::PrivateName(priv_name) => layout_node_with_comments_opt(
                &priv_name.loc,
                priv_name.comments.as_ref(),
                identifier(&ident_of_source(
                    None,
                    priv_name.loc.dupe(),
                    FlowSmolStr::from(format!("#{}", priv_name.name)),
                )),
            ),
            _ => object_property_key(opts, &method.key),
        };
        let s_key = if func.generator {
            fuse(vec![atom("*"), s_key])
        } else {
            s_key
        };
        let s_kind = match method.kind {
            ast::class::MethodKind::Constructor | ast::class::MethodKind::Method => {
                LayoutNode::empty()
            }
            ast::class::MethodKind::Get => atom("get"),
            ast::class::MethodKind::Set => atom("set"),
        };
        // TODO: getters/setters/constructors will never be async
        let s_async = if func.async_ {
            atom("async")
        } else {
            LayoutNode::empty()
        };
        let ts_access = ts_accessibility_layout(method.ts_accessibility.as_ref());
        let prefix = fuse_with_space(vec![s_async, s_kind, s_key]);
        fuse(vec![
            decorators_list(opts, &method.decorators),
            ts_access,
            if method.static_ {
                fuse(vec![atom("static"), space()])
            } else {
                LayoutNode::empty()
            },
            if method.override_ {
                fuse(vec![atom("override"), space()])
            } else {
                LayoutNode::empty()
            },
            source_location_with_comments(
                func_loc,
                None::<&ast::Syntax<Loc, ()>>,
                function_base(
                    opts,
                    &prefix,
                    &func.params,
                    Some(&func.body),
                    func.predicate.as_ref(),
                    &func.return_,
                    func.tparams.as_ref(),
                    func_loc,
                    func.comments.as_ref(),
                ),
            ),
        ])
    })
}

fn class_declare_method(
    opts: &Opts,
    decl_meth: &ast::class::DeclareMethod<Loc, Loc>,
) -> LayoutNode {
    let s_key = match &decl_meth.key {
        ast::expression::object::Key::PrivateName(priv_name) => layout_node_with_comments_opt(
            &priv_name.loc,
            priv_name.comments.as_ref(),
            identifier(&ident_of_source(
                None,
                priv_name.loc.dupe(),
                FlowSmolStr::from(format!("#{}", priv_name.name)),
            )),
        ),
        _ => object_property_key(opts, &decl_meth.key),
    };
    let s_static = if decl_meth.static_ {
        fuse(vec![atom("static"), space()])
    } else {
        LayoutNode::empty()
    };
    let s_override = if decl_meth.override_ {
        fuse(vec![atom("override"), space()])
    } else {
        LayoutNode::empty()
    };
    let s_annot = match decl_meth.kind {
        ast::class::MethodKind::Get | ast::class::MethodKind::Set => {
            match decl_meth.annot.annotation.deref() {
                ast::types::TypeInner::Function {
                    loc: func_loc,
                    inner: func,
                } => type_function(opts, atom(":"), func_loc, func),
                _ => type_annotation(opts, false, &decl_meth.annot),
            }
        }
        _ => type_annotation(opts, false, &decl_meth.annot),
    };
    let s_kind = match decl_meth.kind {
        ast::class::MethodKind::Get => fuse(vec![atom("get"), space()]),
        ast::class::MethodKind::Set => fuse(vec![atom("set"), space()]),
        ast::class::MethodKind::Method | ast::class::MethodKind::Constructor => LayoutNode::empty(),
    };
    let s_optional = if decl_meth.optional {
        atom("?")
    } else {
        LayoutNode::empty()
    };
    source_location_with_comments(
        &decl_meth.loc,
        decl_meth.comments.as_ref(),
        with_semicolon(fuse(vec![
            ts_accessibility_layout(decl_meth.ts_accessibility.as_ref()),
            s_static,
            s_override,
            s_kind,
            s_key,
            s_optional,
            s_annot,
        ])),
    )
}

fn class_abstract_method(
    opts: &Opts,
    abs_meth: &ast::class::AbstractMethod<Loc, Loc>,
) -> LayoutNode {
    let s_key = match &abs_meth.key {
        ast::expression::object::Key::PrivateName(priv_name) => layout_node_with_comments_opt(
            &priv_name.loc,
            priv_name.comments.as_ref(),
            identifier(&ident_of_source(
                None,
                priv_name.loc.dupe(),
                FlowSmolStr::from(format!("#{}", priv_name.name)),
            )),
        ),
        _ => object_property_key(opts, &abs_meth.key),
    };
    let ts_accessibility = ts_accessibility_layout(abs_meth.ts_accessibility.as_ref());
    source_location_with_comments(
        &abs_meth.loc,
        abs_meth.comments.as_ref(),
        with_semicolon(fuse(vec![
            ts_accessibility,
            atom("abstract"),
            space(),
            if abs_meth.override_ {
                fuse(vec![atom("override"), space()])
            } else {
                LayoutNode::empty()
            },
            s_key,
            type_function(opts, atom(":"), &abs_meth.annot.0, &abs_meth.annot.1),
        ])),
    )
}

fn class_abstract_property(
    opts: &Opts,
    abs_prop: &ast::class::AbstractProperty<Loc, Loc>,
) -> LayoutNode {
    let s_key = match &abs_prop.key {
        ast::expression::object::Key::PrivateName(priv_name) => layout_node_with_comments_opt(
            &priv_name.loc,
            priv_name.comments.as_ref(),
            identifier(&ident_of_source(
                None,
                priv_name.loc.dupe(),
                FlowSmolStr::from(format!("#{}", priv_name.name)),
            )),
        ),
        _ => object_property_key(opts, &abs_prop.key),
    };
    let ts_accessibility = ts_accessibility_layout(abs_prop.ts_accessibility.as_ref());
    source_location_with_comments(
        &abs_prop.loc,
        abs_prop.comments.as_ref(),
        with_semicolon(fuse(vec![
            ts_accessibility,
            atom("abstract"),
            space(),
            if abs_prop.override_ {
                fuse(vec![atom("override"), space()])
            } else {
                LayoutNode::empty()
            },
            option_layout(variance, abs_prop.variance.as_ref()),
            s_key,
            hint(|a| type_annotation(opts, false, a), &abs_prop.annot),
        ])),
    )
}

fn indexer_property_layout(
    opts: &Opts,
    indexer: &ast::types::object::Indexer<Loc, Loc>,
) -> LayoutNode {
    fuse(vec![
        if indexer.static_ {
            fuse(vec![atom("static"), space()])
        } else {
            LayoutNode::empty()
        },
        option_layout(variance, indexer.variance.as_ref()),
        atom("["),
        match &indexer.id {
            Some(id) => fuse(vec![identifier(id), atom(":"), pretty_space()]),
            None => LayoutNode::empty(),
        },
        type_(opts, &indexer.key),
        atom("]"),
        if indexer.optional {
            atom("?")
        } else {
            LayoutNode::empty()
        },
        atom(":"),
        pretty_space(),
        type_(opts, &indexer.value),
    ])
}

fn class_index_signature(
    opts: &Opts,
    indexer: &ast::types::object::Indexer<Loc, Loc>,
) -> LayoutNode {
    source_location_with_comments(
        &indexer.loc,
        indexer.comments.as_ref(),
        with_semicolon(indexer_property_layout(opts, indexer)),
    )
}

pub(crate) fn class_property(opts: &Opts, prop: &ast::class::Property<Loc, Loc>) -> LayoutNode {
    with_semicolon(class_property_helper(
        opts,
        &prop.loc,
        object_property_key(opts, &prop.key),
        &prop.value,
        prop.static_,
        prop.override_,
        &prop.annot,
        prop.variance.as_ref(),
        prop.ts_accessibility.as_ref(),
        &prop.decorators,
        prop.comments.as_ref(),
        prop.optional,
        prop.definite,
    ))
}

pub(crate) fn class_private_field(
    opts: &Opts,
    field: &ast::class::PrivateField<Loc, Loc>,
) -> LayoutNode {
    let key = layout_node_with_comments_opt(
        &field.key.loc,
        field.key.comments.as_ref(),
        identifier(&flow_parser::ast_utils::ident_of_source(
            None,
            field.key.loc.dupe(),
            format!("#{}", field.key.name).into(),
        )),
    );
    with_semicolon(class_property_helper(
        opts,
        &field.loc,
        key,
        &field.value,
        field.static_,
        field.override_,
        &field.annot,
        field.variance.as_ref(),
        field.ts_accessibility.as_ref(),
        &field.decorators,
        field.comments.as_ref(),
        field.optional,
        field.definite,
    ))
}

pub(crate) fn object_property(
    opts: &Opts,
    property: &ast::expression::object::Property<Loc, Loc>,
) -> LayoutNode {
    match property {
        ast::expression::object::Property::NormalProperty(
            ast::expression::object::NormalProperty::Init {
                loc,
                key,
                value,
                shorthand,
            },
        ) => {
            let value_separator = comment_aware_separator(
                None,
                pretty_space(),
                &comment_attachment::expression_comment_bounds(value),
            );
            source_location_with_comments(loc, None::<&ast::Syntax<Loc, ()>>, {
                let s_key = object_property_key(opts, key);
                if *shorthand {
                    s_key
                } else {
                    group(vec![
                        s_key,
                        atom(":"),
                        value_separator,
                        expression_with_parens(opts, MIN_PRECEDENCE, &NORMAL_CONTEXT, value),
                    ])
                }
            })
        }
        ast::expression::object::Property::NormalProperty(
            ast::expression::object::NormalProperty::Method { loc, key, value },
        ) => {
            let fn_loc = &value.0;
            let func = &value.1;
            let s_key = object_property_key(opts, key);
            // methods don't have ids, see `key`
            debug_assert!(func.id.is_none());
            let prefix = fuse(vec![
                if func.async_ {
                    fuse(vec![atom("async"), space()])
                } else {
                    LayoutNode::empty()
                },
                if func.generator {
                    atom("*")
                } else {
                    LayoutNode::empty()
                },
                s_key,
            ]);
            source_location_with_comments(
                loc,
                None::<&ast::Syntax<Loc, ()>>,
                source_location_with_comments(
                    fn_loc,
                    None::<&ast::Syntax<Loc, ()>>,
                    function_base(
                        opts,
                        &prefix,
                        &func.params,
                        Some(&func.body),
                        func.predicate.as_ref(),
                        &func.return_,
                        func.tparams.as_ref(),
                        fn_loc,
                        func.comments.as_ref(),
                    ),
                ),
            )
        }
        ast::expression::object::Property::NormalProperty(
            ast::expression::object::NormalProperty::Get {
                loc,
                key,
                value,
                comments,
            },
        ) => {
            let fn_loc = &value.0;
            let func = &value.1;
            // getters don't have ids, see `key`
            debug_assert!(func.id.is_none());
            // getters can't be async
            debug_assert!(!func.async_);
            // getters can't be generators
            debug_assert!(!func.generator);
            let prefix = fuse(vec![atom("get"), space(), object_property_key(opts, key)]);
            source_location_with_comments(
                loc,
                comments.as_ref(),
                source_location_with_comments(
                    fn_loc,
                    None::<&ast::Syntax<Loc, ()>>,
                    function_base(
                        opts,
                        &prefix,
                        &func.params,
                        Some(&func.body),
                        func.predicate.as_ref(),
                        &func.return_,
                        func.tparams.as_ref(),
                        fn_loc,
                        func.comments.as_ref(),
                    ),
                ),
            )
        }
        ast::expression::object::Property::NormalProperty(
            ast::expression::object::NormalProperty::Set {
                loc,
                key,
                value,
                comments,
            },
        ) => {
            let fn_loc = &value.0;
            let func = &value.1;
            debug_assert!(func.id.is_none());
            // setters don't have ids, see `key`
            debug_assert!(!func.async_);
            // setters can't be async
            debug_assert!(!func.generator);
            // setters can't be generators
            let prefix = fuse(vec![atom("set"), space(), object_property_key(opts, key)]);
            source_location_with_comments(
                loc,
                comments.as_ref(),
                source_location_with_comments(
                    fn_loc,
                    None::<&ast::Syntax<Loc, ()>>,
                    function_base(
                        opts,
                        &prefix,
                        &func.params,
                        Some(&func.body),
                        func.predicate.as_ref(),
                        &func.return_,
                        func.tparams.as_ref(),
                        fn_loc,
                        func.comments.as_ref(),
                    ),
                ),
            )
        }
        ast::expression::object::Property::SpreadProperty(spread) => source_location_with_comments(
            &spread.loc,
            spread.comments.as_ref(),
            fuse(vec![atom("..."), expression(opts, None, &spread.argument)]),
        ),
    }
}

pub(crate) fn template_literal(
    opts: &Opts,
    t_lit: &ast::expression::TemplateLiteral<Loc, Loc>,
) -> LayoutNode {
    let template_element = |i: usize, elem: &ast::expression::template_literal::Element<Loc>| {
        let raw = &elem.value.raw;
        let tail = elem.tail;
        fuse(vec![
            source_location_with_comments(
                &elem.loc,
                None::<&ast::Syntax<Loc, ()>>,
                fuse(vec![
                    if i > 0 {
                        atom("}")
                    } else {
                        LayoutNode::empty()
                    },
                    atom(raw),
                    if !tail {
                        atom("${")
                    } else {
                        LayoutNode::empty()
                    },
                ]),
            ),
            if !tail {
                expression(opts, None, &t_lit.expressions[i])
            } else {
                LayoutNode::empty()
            },
        ])
    };
    fuse(vec![
        atom("`"),
        fuse(
            t_lit
                .quasis
                .iter()
                .enumerate()
                .map(|(i, elem)| template_element(i, elem))
                .collect(),
        ),
        atom("`"),
    ])
}

pub(crate) fn jsx_child(
    opts: &Opts,
    child: &ast::jsx::Child<Loc, Loc>,
) -> Option<(Loc, LayoutNode)> {
    match child {
        ast::jsx::Child::Element { loc, inner: elem } => {
            Some((loc.dupe(), jsx_element(opts, loc, elem)))
        }
        ast::jsx::Child::Fragment { loc, inner: frag } => {
            Some((loc.dupe(), jsx_fragment(opts, loc, frag)))
        }
        ast::jsx::Child::ExpressionContainer { loc, inner: expr } => {
            Some((loc.dupe(), jsx_expression_container(opts, loc, expr)))
        }
        ast::jsx::Child::SpreadChild { loc, inner: spread } => {
            Some((loc.dupe(), jsx_spread_child(opts, loc, spread)))
        }
        ast::jsx::Child::Text { loc, inner: text } => {
            flow_common_utils::utils_jsx::trim_jsx_text(loc.dupe(), &text.raw)
                .map(|(loc, txt)| (loc, atom(&txt)))
        }
    }
}

pub(crate) fn jsx_identifier(id: &ast::jsx::Identifier<Loc, Loc>) -> LayoutNode {
    identifier_with_comments(&id.loc, id.name.as_str(), id.comments.as_ref())
}

pub fn jsx_opening_attr(opts: &Opts, attr: &ast::jsx::OpeningAttribute<Loc, Loc>) -> LayoutNode {
    match attr {
        ast::jsx::OpeningAttribute::Attribute(attr) => jsx_attribute(opts, attr),
        ast::jsx::OpeningAttribute::SpreadAttribute(attr) => jsx_spread_attribute(opts, attr),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_better_quote_prefers_single() {
        assert_eq!(better_quote(true, "hello"), "'");
        assert_eq!(better_quote(false, "hello"), "\"");
    }

    #[test]
    fn test_better_quote_switches_when_preferred_has_more() {
        assert_eq!(better_quote(true, "it's a test's"), "\"");
        assert_eq!(better_quote(false, r#"a "b" "c""#), "'");
    }

    #[test]
    fn test_utf8_escape_basic() {
        assert_eq!(utf8_escape("'", "hello"), "hello");
        assert_eq!(utf8_escape("'", "it's"), "it\\'s");
        assert_eq!(utf8_escape("\"", "say \"hi\""), "say \\\"hi\\\"");
    }

    #[test]
    fn test_utf8_escape_control_chars() {
        assert_eq!(utf8_escape("'", "\t"), "\\t");
        assert_eq!(utf8_escape("'", "\n"), "\\n");
        assert_eq!(utf8_escape("'", "\r"), "\\r");
        assert_eq!(utf8_escape("'", "\\"), "\\\\");
    }

    #[test]
    fn test_utf8_escape_null_lookahead() {
        assert_eq!(utf8_escape("'", "\x001"), "\\x001");
        assert_eq!(utf8_escape("'", "\x00a"), "\\0a");
        assert_eq!(utf8_escape("'", "\x00"), "\\0");
    }

    #[test]
    fn test_utf8_escape_non_printable_ascii() {
        assert_eq!(utf8_escape("'", "\x01"), "\\x01");
        assert_eq!(utf8_escape("'", "\x1f"), "\\x1f");
    }

    #[test]
    fn test_utf8_escape_bmp() {
        assert_eq!(utf8_escape("'", "\u{00e9}"), "\\xe9");
        assert_eq!(utf8_escape("'", "\u{4e16}"), "\\u4e16");
    }

    #[test]
    fn test_utf8_escape_supplementary() {
        assert_eq!(utf8_escape("'", "\u{1f4a9}"), "\\ud83d\\udca9");
    }

    #[test]
    fn test_quote_string() {
        assert_eq!(quote_string(true, "hello"), "'hello'");
        assert_eq!(quote_string(false, "hello"), "\"hello\"");
        assert_eq!(quote_string(true, "it's"), "\"it's\"");
    }
}
