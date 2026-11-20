/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_parser::ast;
use flow_parser::ast::expression;
use flow_parser::ast::expression::ExpressionInner;
use flow_parser::ast::statement;
use flow_parser::ast::statement::StatementInner;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::ast_visitor::TypeParamsContext;
use flow_parser::comment_attachment;
use flow_parser::comment_attachment::CommentsBoundsCollector;
use flow_parser::loc::Loc;

#[derive(Debug, Clone, PartialEq)]
pub enum Change<A> {
    Replace(A, A),
    Insert {
        items: Vec<A>,
        // separator. Defaults to \n
        separator: Option<String>,
        leading_separator: bool,
    },
    Delete(A),
}

pub type NodeChange = (Loc, Change<Node>);

pub type NodeChanges = Vec<NodeChange>;

// Position in the list is necessary to figure out what Loc.t to assign to insertions.
pub type DiffResult<A> = (i32, Change<A>);

// Compares changes based on location.
fn change_compare(a: &NodeChange, b: &NodeChange) -> std::cmp::Ordering {
    let (pos1, chg1) = a;
    let (pos2, chg2) = b;
    let loc_cmp = pos1.partial_cmp(pos2);
    if loc_cmp != Some(std::cmp::Ordering::Equal) {
        return loc_cmp.unwrap_or(std::cmp::Ordering::Equal);
    }
    match (chg1, chg2) {
        (Change::Insert { .. }, Change::Delete(_))
        | (Change::Delete(_), Change::Replace(_, _))
        | (Change::Insert { .. }, Change::Replace(_, _)) => std::cmp::Ordering::Less,
        (Change::Delete(_), Change::Insert { .. })
        | (Change::Replace(_, _), Change::Delete(_))
        | (Change::Replace(_, _), Change::Insert { .. }) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    }
}

// diffs based on http://www.xmailserver.org/diff2.pdf on page 6
pub fn list_diff<'a, A: PartialEq>(
    old_list: &'a [A],
    new_list: &'a [A],
) -> Option<Vec<DiffResult<&'a A>>> {
    // Lots of acccesses in this algorithm so arrays are faster
    let old_arr = old_list;
    let new_arr = new_list;
    let n = old_arr.len() as i32;
    let m = new_arr.len() as i32;

    // The shortest edit sequence problem is equivalent to finding the longest
    // common subsequence, or equivalently the longest trace
    let longest_trace = |max_distance: i32| -> Option<Vec<(i32, i32)>> {
        // adds the match points in this snake to the trace and produces the endpoint along with the
        // new trace
        fn follow_snake<A: PartialEq>(
            old_arr: &[A],
            new_arr: &[A],
            n: i32,
            m: i32,
            mut x: i32,
            mut y: i32,
            mut trace: Vec<(i32, i32)>,
        ) -> (i32, i32, Vec<(i32, i32)>) {
            loop {
                if x >= n || y >= m {
                    return (x, y, trace);
                } else if old_arr[x as usize] == new_arr[y as usize] {
                    trace.push((x, y));
                    x += 1;
                    y += 1;
                } else {
                    return (x, y, trace);
                }
            }
        }

        // Keep track of all visited string locations so we don't duplicate work
        let mut visited: HashMap<(i32, i32), Vec<(i32, i32)>> = HashMap::new();
        let mut frontier: VecDeque<(i32, i32)> = VecDeque::new();
        // Start with the basic trace, but follow a starting snake to a non-match point
        let (x, y, trace) = follow_snake(old_arr, new_arr, n, m, 0, 0, vec![]);
        frontier.push_back((x, y));
        visited.insert((x, y), trace);

        let mut dist = 0;
        loop {
            if visited.contains_key(&(n, m)) {
                break;
            }
            if dist > max_distance {
                break;
            }
            let mut new_frontier: VecDeque<(i32, i32)> = VecDeque::new();
            while let Some((x, y)) = frontier.pop_front() {
                let trace = visited.get(&(x, y)).cloned().unwrap_or_default();
                let (x_old, y_old, advance_in_old_list) =
                    follow_snake(old_arr, new_arr, n, m, x + 1, y, trace.clone());
                let (x_new, y_new, advance_in_new_list) =
                    follow_snake(old_arr, new_arr, n, m, x, y + 1, trace);
                // if we have already visited this location, there is a shorter path to it, so we don't
                // store this trace
                visited.entry((x_old, y_old)).or_insert_with(|| {
                    new_frontier.push_back((x_old, y_old));
                    advance_in_old_list
                });
                visited.entry((x_new, y_new)).or_insert_with(|| {
                    new_frontier.push_back((x_new, y_new));
                    advance_in_new_list
                });
            }
            frontier = new_frontier;
            dist += 1;
        }
        visited.remove(&(n, m))
    };

    // Produces an edit script from a trace via the procedure described on page 4
    // of the paper. Assumes the trace is ordered by the x coordinate
    let build_script_from_trace = |trace: Vec<(i32, i32)>| -> Vec<DiffResult<&A>> {
        let trace_len = trace.len() as i32;

        // adds inserts at position x_k for values in new_list from
        // y_k + 1 to y_(k + 1) - 1 for k such that y_k + 1 < y_(k + 1)
        let mut script: Vec<DiffResult<&A>> = Vec::new();

        // Deletes are added for every element of old_list that does not have a
        // match point with new_list
        let matched: BTreeSet<i32> = trace.iter().map(|(x, _)| *x).collect();
        let all_indices: BTreeSet<i32> = (0..n).collect();
        let deletes: Vec<DiffResult<&A>> = all_indices
            .difference(&matched)
            .map(|&pos| (pos, Change::Delete(&old_arr[pos as usize])))
            .collect();
        script.extend(deletes);

        let mut k: i32 = -1;
        while k < trace_len {
            // The algorithm treats the trace as though (-1,-1) were the (-1)th match point
            let first = if k == -1 { 0 } else { trace[k as usize].1 + 1 };
            let last = if k == trace_len - 1 {
                m
            } else {
                trace[(k + 1) as usize].1
            };
            if first < last {
                let start = if k == -1 { -1 } else { trace[k as usize].0 };
                let items: Vec<&A> = (first..last).map(|i| &new_arr[i as usize]).collect();
                script.push((
                    start,
                    Change::Insert {
                        items,
                        separator: None,
                        leading_separator: false,
                    },
                ));
            }
            k += 1;
        }

        script.sort_by(|a, b| {
            let (pos1, chg1) = a;
            let (pos2, chg2) = b;
            if pos1 != pos2 {
                pos1.cmp(pos2)
            } else {
                // Orders the change types alphabetically. This puts same-indexed inserts before deletes
                fn change_ord<T>(c: &Change<T>) -> i32 {
                    match c {
                        Change::Insert { .. } => 0,
                        Change::Delete(_) => 1,
                        Change::Replace(_, _) => 2,
                    }
                }
                change_ord(chg1).cmp(&change_ord(chg2))
            }
        });

        // Convert like-indexed deletes and inserts into a replacement. This relies
        // on the fact that sorting the script with our change_compare function will order all
        // Insert nodes before Deletes
        let mut result: Vec<DiffResult<&A>> = Vec::new();
        let mut i = 0;
        while i < script.len() {
            if i + 1 < script.len() {
                let i1 = script[i].0;
                let i2 = script[i + 1].0;
                if i1 == i2 - 1 {
                    if let Change::Insert {
                        ref items,
                        ref separator,
                        ..
                    } = script[i].1
                    {
                        if let Change::Delete(y) = &script[i + 1].1 {
                            if items.len() == 1 {
                                result.push((i2, Change::Replace(y, items[0])));
                                i += 2;
                                continue;
                            } else if !items.is_empty() {
                                // We are only removing the first element of the insertion. We make sure to indicate
                                // that the rest of the insert should have a leading separator between it and the replace.
                                let x = items[0];
                                let rst: Vec<&A> = items[1..].to_vec();
                                result.push((i2, Change::Replace(y, x)));
                                script[i + 1] = (
                                    i2,
                                    Change::Insert {
                                        items: rst,
                                        separator: separator.clone(),
                                        leading_separator: true,
                                    },
                                );
                                i += 1;
                                continue;
                            }
                        }
                    }
                }
            }
            result.push(script[i].clone());
            i += 1;
        }

        result
    };

    longest_trace(n + m).map(build_script_from_trace)
}

#[derive(Debug, Clone)]
pub enum ExpressionNodeParent {
    StatementParentOfExpression(statement::Statement<Loc, Loc>),
    ExpressionParentOfExpression(expression::Expression<Loc, Loc>),
    ClassExtends,
    // Any slot that does not require expression to be parenthesized.
    SlotParentOfExpression,
    SpreadParentOfExpression,
    MatchExpressionCaseBodyParentOfExpression,
}

#[derive(Debug, Clone)]
pub enum StatementNodeParent {
    StatementBlockParentOfStatement(Loc),
    ExportParentOfStatement(Loc),
    IfParentOfStatement(Loc),
    LabeledStatementParentOfStatement(Loc),
    LoopParentOfStatement(Loc),
    WithStatementParentOfStatement(Loc),
    TopLevelParentOfStatement,
    SwitchCaseParentOfStatement(Loc),
    MatchCaseParentOfStatement(Loc),
}

// We need a variant here for every node that we want to be able to store a diff for. The more we
// have here, the more granularly we can diff.
#[derive(Debug, Clone)]
pub enum Node {
    Raw(String),
    Comment(ast::Comment<Loc>),
    StringLiteral(Loc, ast::StringLiteral<Loc>),
    NumberLiteral(Loc, ast::NumberLiteral<Loc>),
    BigIntLiteral(Loc, ast::BigIntLiteral<Loc>),
    BooleanLiteral(Loc, ast::BooleanLiteral<Loc>),
    RegExpLiteral(Loc, ast::RegExpLiteral<Loc>),
    ModuleRefLiteral(Loc, ast::ModuleRefLiteral<Loc>),
    Statement(statement::Statement<Loc, Loc>, StatementNodeParent),
    Program(ast::Program<Loc, Loc>),
    Expression(expression::Expression<Loc, Loc>, ExpressionNodeParent),
    Pattern(ast::pattern::Pattern<Loc, Loc>),
    Params(ast::function::Params<Loc, Loc>),
    Variance(ast::Variance<Loc>),
    Type(ast::types::Type<Loc, Loc>),
    TypeParam(ast::types::TypeParam<Loc, Loc>),
    TypeAnnotation(ast::types::Annotation<Loc, Loc>),
    TypeGuard(ast::types::TypeGuard<Loc, Loc>),
    TypeGuardAnnotation(ast::types::TypeGuardAnnotation<Loc, Loc>),
    FunctionTypeAnnotation(ast::types::Annotation<Loc, Loc>),
    ClassProperty(ast::class::Property<Loc, Loc>),
    ClassPrivateField(ast::class::PrivateField<Loc, Loc>),
    ObjectProperty(ast::expression::object::Property<Loc, Loc>),
    TemplateLiteral(Loc, ast::expression::TemplateLiteral<Loc, Loc>),
    JSXChild(ast::jsx::Child<Loc, Loc>),
    JSXIdentifier(ast::jsx::Identifier<Loc, Loc>),
    MatchPattern(ast::match_pattern::MatchPattern<Loc, Loc>),
    MatchObjectPatternProperty(ast::match_pattern::object_pattern::Property<Loc, Loc>),
}

fn expand_loc_with_comments(loc: &Loc, node: &Node) -> Loc {
    let bounds = |loc: &Loc, f: &dyn Fn(&mut CommentsBoundsCollector) -> Result<(), !>| {
        let mut collector = CommentsBoundsCollector::new(loc);
        let Ok(()) = f(&mut collector);
        collector.comment_bounds()
    };
    let comment_bounds = match node {
        Node::StringLiteral(l, lit) => bounds(l, &|c| c.string_literal(lit)),
        Node::NumberLiteral(l, lit) => bounds(l, &|c| c.number_literal(lit)),
        Node::BigIntLiteral(l, lit) => bounds(l, &|c| c.bigint_literal(lit)),
        Node::BooleanLiteral(l, lit) => bounds(l, &|c| c.boolean_literal(lit)),
        Node::RegExpLiteral(l, lit) => bounds(l, &|c| c.regexp_literal(lit)),
        Node::ModuleRefLiteral(l, lit) => bounds(l, &|c| c.module_ref_literal(lit)),
        Node::Statement(stmt, _) => bounds(stmt.loc(), &|c| c.statement(stmt)),
        Node::Expression(expr, _) => bounds(expr.loc(), &|c| c.expression(expr)),
        Node::Pattern(pat) => bounds(pat.loc(), &|c| c.pattern(None, pat)),
        Node::Params(params) => bounds(&params.loc, &|c| c.function_params(params)),
        Node::Variance(var) => bounds(&var.loc, &|c| c.variance(var)),
        Node::Type(ty) => bounds(ty.loc(), &|c| c.type_(ty)),
        Node::TypeParam(tparam) => bounds(&tparam.loc, &|c| {
            c.type_param(&TypeParamsContext::Function, tparam)
        }),
        Node::TypeAnnotation(annot) | Node::FunctionTypeAnnotation(annot) => {
            bounds(&annot.loc, &|c| c.type_annotation(annot))
        }
        Node::TypeGuard(guard) => bounds(&guard.loc, &|c| c.type_guard(guard)),
        Node::TypeGuardAnnotation(guard) => bounds(&guard.loc, &|c| c.type_guard_annotation(guard)),
        Node::ClassProperty(prop) => bounds(&prop.loc, &|c| c.class_property(prop)),
        Node::ClassPrivateField(f) => bounds(&f.loc, &|c| c.class_private_field(f)),
        Node::ObjectProperty(prop) => comment_attachment::object_property_comment_bounds(prop),
        Node::TemplateLiteral(l, lit) => bounds(l, &|c| c.template_literal(l, lit)),
        Node::JSXIdentifier(id) => bounds(&id.loc, &|c| c.jsx_identifier(id)),
        Node::MatchPattern(pat) => bounds(pat.loc(), &|c| c.match_pattern(pat)),
        Node::MatchObjectPatternProperty(prop) => {
            bounds(prop.loc(), &|c| c.match_object_pattern_property(prop))
        }
        // Nodes that do have attached comments
        Node::Raw(_) | Node::Comment(_) | Node::Program(_) | Node::JSXChild(_) => {
            return loc.dupe();
        }
    };
    comment_attachment::expand_loc_with_comment_bounds(loc, &comment_bounds)
}

pub fn expand_statement_comment_bounds(stmt: &statement::Statement<Loc, Loc>) -> Loc {
    let comment_bounds = comment_attachment::statement_comment_bounds(stmt);
    comment_attachment::expand_loc_with_comment_bounds(stmt.loc(), &comment_bounds)
}

fn replace(loc: &Loc, old_node: Node, new_node: Node) -> NodeChange {
    (
        expand_loc_with_comments(loc, &old_node),
        Change::Replace(old_node, new_node),
    )
}

fn delete(loc: &Loc, node: Node) -> NodeChange {
    (expand_loc_with_comments(loc, &node), Change::Delete(node))
}

fn insert(sep: Option<&str>, nodes: Vec<Node>) -> Change<Node> {
    Change::Insert {
        items: nodes,
        separator: sep.map(|s| s.to_string()),
        leading_separator: false,
    }
}

/// This is needed because all of the functions assume that if they are called, there is some
/// difference between their arguments and they will often report that even if no difference actually
/// exists. This allows us to easily avoid calling the diffing function if there is no difference.
fn diff_if_changed<A: PartialEq, F: FnOnce(&A, &A) -> Vec<NodeChange>>(
    f: F,
    x1: &A,
    x2: &A,
) -> Vec<NodeChange> {
    if x1 == x2 { vec![] } else { f(x1, x2) }
}

fn diff_if_changed_ret_opt<A: PartialEq, F: FnOnce(&A, &A) -> Option<Vec<NodeChange>>>(
    f: F,
    x1: &A,
    x2: &A,
) -> Option<Vec<NodeChange>> {
    if x1 == x2 { Some(vec![]) } else { f(x1, x2) }
}

fn diff_if_changed_opt<A: PartialEq, F: FnOnce(&A, &A) -> Option<Vec<NodeChange>>>(
    f: F,
    opt1: &Option<A>,
    opt2: &Option<A>,
) -> Option<Vec<NodeChange>> {
    match (opt1, opt2) {
        (Some(x1), Some(x2)) => {
            if x1 == x2 {
                Some(vec![])
            } else {
                f(x1, x2)
            }
        }
        (None, None) => Some(vec![]),
        _ => None,
    }
}

fn diff_or_add_opt<A: PartialEq, F, G>(
    f: F,
    add: G,
    opt1: &Option<A>,
    opt2: &Option<A>,
) -> Option<Vec<NodeChange>>
where
    F: FnOnce(&A, &A) -> Option<Vec<NodeChange>>,
    G: FnOnce(&A) -> Vec<NodeChange>,
{
    match (opt1, opt2) {
        (Some(x1), Some(x2)) => {
            if x1 == x2 {
                Some(vec![])
            } else {
                f(x1, x2)
            }
        }
        (None, None) => Some(vec![]),
        (None, Some(x2)) => Some(add(x2)),
        _ => None,
    }
}

/// This is needed if the function for the given node returns a node change
/// list instead of a node change list option (for instance, expression)
fn diff_if_changed_nonopt_fn<A: PartialEq, F: FnOnce(&A, &A) -> Vec<NodeChange>>(
    f: F,
    opt1: &Option<A>,
    opt2: &Option<A>,
) -> Option<Vec<NodeChange>> {
    match (opt1, opt2) {
        (Some(x1), Some(x2)) => {
            if x1 == x2 {
                Some(vec![])
            } else {
                Some(f(x1, x2))
            }
        }
        (None, None) => Some(vec![]),
        _ => None,
    }
}

/// Is an RHS expression an import expression?
fn is_import_expr(expr: &expression::Expression<Loc, Loc>) -> bool {
    match &**expr {
        ExpressionInner::Import { .. } => true,
        ExpressionInner::Call { inner, .. } => {
            if let ExpressionInner::Identifier { inner: id, .. } = &*inner.callee {
                id.name.as_str() == "require"
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Guess whether a statement is an import or not
fn is_directive_stmt(stmt: &statement::Statement<Loc, Loc>) -> bool {
    match &**stmt {
        StatementInner::Expression { inner, .. } => inner.directive.is_some(),
        _ => false,
    }
}

fn is_import_stmt(stmt: &statement::Statement<Loc, Loc>) -> bool {
    match &**stmt {
        StatementInner::ImportDeclaration { .. } => true,
        StatementInner::ImportEqualsDeclaration { .. } => true,
        StatementInner::Expression { inner, .. } => is_import_expr(&inner.expression),
        StatementInner::VariableDeclaration { inner, .. } => inner
            .declarations
            .iter()
            .any(|dec| dec.init.as_ref().is_some_and(is_import_expr)),
        _ => false,
    }
}

pub struct PartitionResult {
    pub directives: Vec<statement::Statement<Loc, Loc>>,
    pub imports: Vec<statement::Statement<Loc, Loc>>,
    pub body: Vec<statement::Statement<Loc, Loc>>,
}

pub fn partition_imports(stmts: &[statement::Statement<Loc, Loc>]) -> PartitionResult {
    let mut directives = Vec::new();
    let mut imports = Vec::new();
    let mut i = 0;
    while i < stmts.len() {
        if is_directive_stmt(&stmts[i]) {
            directives.push(stmts[i].dupe());
            i += 1;
        } else if is_import_stmt(&stmts[i]) {
            imports.push(stmts[i].dupe());
            i += 1;
        } else {
            break;
        }
    }
    let body = stmts[i..].iter().map(|s| s.dupe()).collect();
    PartitionResult {
        directives,
        imports,
        body,
    }
}

// Outline:
// - There is a function for every AST node that we want to be able to recurse into.
// - Each function for an AST node represented in the `node` type above should return a list of
//   changes.
//   - If it cannot compute a more granular diff, it should return a list with a single element,
//     which records the replacement of `old_node` with `new_node` (where `old_node` and
//     `new_node` are the arguments passed to that function)
// - Every other function should do the same, except if it is unable to return a granular diff, it
//   should return `None` to indicate that its parent must be recorded as a replacement. This is
//   because there is no way to record a replacement for a node which does not appear in the
//   `node` type above.
// - We can add additional functions as needed to improve the granularity of the diffs.
// - We could eventually reach a point where no function would ever fail to generate a diff. That
//   would require us to implement a function here for every AST node, and add a variant to the
//   `node` type for every AST node as well. It would also likely require some tweaks to the AST.
//   For example, a function return type is optional. If it is None, it has no location attached.
//   What would we do if the original tree had no annotation, but the new tree did have one? We
//   would not know what Loc.t to give to the insertion.

// Entry point
pub fn program(
    program1: &ast::Program<Loc, Loc>,
    program2: &ast::Program<Loc, Loc>,
) -> NodeChanges {
    let mut result = program_inner(program1, program2);
    result.sort_by(change_compare);
    result
}

fn join_diff_list(diffs: Vec<Option<Vec<NodeChange>>>) -> Option<Vec<NodeChange>> {
    let mut result: Option<Vec<NodeChange>> = Some(vec![]);
    for diff in diffs {
        result = match (result, diff) {
            (Some(mut a), Some(b)) => {
                a.extend(b);
                Some(a)
            }
            _ => None,
        };
    }
    result
}

/// Assuming a diff has already been generated, recurse into it.
/// This function is passed the old_list and index_offset parameters
/// in order to correctly insert new statements WITHOUT assuming that
/// the entire statement list is being processed with a single call
/// to this function. When an Insert diff is detected, we need to find
/// a Loc.t that represents where in the original program they will be inserted.
/// To do so, we find the statement in the old statement list that they will
/// be inserted after, and get its end_loc. The index_offset parameter represents how
/// many statements in the old statement list are NOT represented in this diff--
/// for example, if we separated the statement lists into a list of initial imports
/// and a list of body statements and generated diffs for them separately
/// (cf. toplevel_statement_list), when recursing into the body diffs, the
/// length of the imports in the old statement list should be passed in to
/// index_offset so that insertions into the body section are given the right index.
fn recurse_into_diff<A: PartialEq, B>(
    f: &dyn Fn(&A, &A) -> Option<Vec<(Loc, Change<B>)>>,
    trivial: &dyn Fn(&A) -> Option<(Loc, B)>,
    old_list: &[A],
    index_offset: i32,
    diffs: &[DiffResult<&A>],
) -> Option<Vec<(Loc, Change<B>)>>
where
    B: Clone,
{
    let mut result: Vec<(Loc, Change<B>)> = Vec::new();
    for diff in diffs {
        let change = match diff {
            (_, Change::Replace(x1, x2)) => f(x1, x2),
            (
                index,
                Change::Insert {
                    items,
                    separator,
                    leading_separator,
                },
            ) => {
                let index = index + index_offset;
                let loc = if old_list.is_empty() {
                    None
                } else if index == -1 {
                    // To insert at the start of the list, insert before the first element
                    trivial(&old_list[0]).map(|(l, _)| l.start_loc())
                } else {
                    // Otherwise insert it after the current element
                    trivial(&old_list[index as usize]).map(|(l, _)| l.end_loc())
                };

                let mapped: Option<Vec<B>> = items
                    .iter()
                    .map(|item| trivial(item).map(|(_, b)| b))
                    .collect();

                match (loc, mapped) {
                    (Some(loc), Some(items)) => Some(vec![(
                        loc,
                        Change::Insert {
                            items,
                            separator: separator.clone(),
                            leading_separator: *leading_separator,
                        },
                    )]),
                    _ => None,
                }
            }
            (_, Change::Delete(x)) => trivial(x).map(|(loc, y)| vec![(loc, Change::Delete(y))]),
        };
        result.extend(change?);
    }
    Some(result)
}

/// Runs `list_diff` and then recurses into replacements (using `f`) to get more granular diffs.
/// For inserts and deletes, it uses `trivial` to produce a Loc.t and a b for the change
fn diff_and_recurse<A: PartialEq, B: Clone>(
    f: &dyn Fn(&A, &A) -> Option<Vec<(Loc, Change<B>)>>,
    trivial: &dyn Fn(&A) -> Option<(Loc, B)>,
    old_list: &[A],
    new_list: &[A],
) -> Option<Vec<(Loc, Change<B>)>> {
    list_diff(old_list, new_list)
        .and_then(|diffs| recurse_into_diff(f, trivial, old_list, 0, &diffs))
}

/// Same as diff_and_recurse but takes in a function `f` that doesn't return an option
fn diff_and_recurse_nonopt<A: PartialEq, B: Clone>(
    f: &dyn Fn(&A, &A) -> Vec<(Loc, Change<B>)>,
    trivial: &dyn Fn(&A) -> Option<(Loc, B)>,
    old_list: &[A],
    new_list: &[A],
) -> Option<Vec<(Loc, Change<B>)>> {
    diff_and_recurse(&|x, y| Some(f(x, y)), trivial, old_list, new_list)
}

/// diff_and_recurse for when there is no way to get a trivial transformation from a to b
fn diff_and_recurse_no_trivial<A: PartialEq>(
    f: &dyn Fn(&A, &A) -> Option<Vec<NodeChange>>,
    old_list: &[A],
    new_list: &[A],
) -> Option<Vec<NodeChange>> {
    diff_and_recurse(f, &|_| None, old_list, new_list)
}

fn diff_and_recurse_nonopt_no_trivial<A: PartialEq>(
    f: &dyn Fn(&A, &A) -> Vec<NodeChange>,
    old_list: &[A],
    new_list: &[A],
) -> Option<Vec<NodeChange>> {
    diff_and_recurse_nonopt(f, &|_| None, old_list, new_list)
}

fn syntax_opt<I: Dupe + PartialEq>(
    loc: &Loc,
    s1: &Option<ast::Syntax<Loc, I>>,
    s2: &Option<ast::Syntax<Loc, I>>,
) -> Option<Vec<NodeChange>> {
    let add_comments = |s: &ast::Syntax<Loc, I>| -> Vec<NodeChange> {
        let leading: Vec<Node> = s
            .leading
            .iter()
            .map(|cmt| Node::Comment(cmt.dupe()))
            .collect();
        let leading_inserts = if leading.is_empty() {
            vec![]
        } else {
            vec![(
                Loc {
                    start: loc.start.dupe(),
                    end: loc.start.dupe(),
                    ..loc.dupe()
                },
                insert(None, leading),
            )]
        };
        let trailing: Vec<Node> = s
            .trailing
            .iter()
            .map(|cmt| Node::Comment(cmt.dupe()))
            .collect();
        let trailing_inserts = if trailing.is_empty() {
            vec![]
        } else {
            vec![(
                Loc {
                    start: loc.end.dupe(),
                    end: loc.end.dupe(),
                    ..loc.dupe()
                },
                insert(None, trailing),
            )]
        };
        let mut result = leading_inserts;
        result.extend(trailing_inserts);
        result
    };
    diff_or_add_opt(|x1, x2| syntax(x1, x2), |x2| add_comments(x2), s1, s2)
}

fn syntax<I: Dupe + PartialEq>(
    s1: &ast::Syntax<Loc, I>,
    s2: &ast::Syntax<Loc, I>,
) -> Option<Vec<NodeChange>> {
    let ast::Syntax {
        leading: leading1,
        trailing: trailing1,
        ..
    } = s1;
    let ast::Syntax {
        leading: leading2,
        trailing: trailing2,
        ..
    } = s2;
    let add_comment = |cmt: &ast::Comment<Loc>| -> Option<(Loc, Node)> {
        Some((cmt.loc.dupe(), Node::Comment(cmt.dupe())))
    };
    let leading = diff_and_recurse(&|c1, c2| comment(c1, c2), &add_comment, leading1, leading2);
    let trailing = diff_and_recurse(
        &|c1, c2| comment(c1, c2),
        &add_comment,
        trailing1,
        trailing2,
    );
    match (leading, trailing) {
        (Some(mut l), Some(t)) => {
            l.extend(t);
            Some(l)
        }
        (Some(l), None) => Some(l),
        (None, Some(t)) => Some(t),
        (None, None) => None,
    }
}

fn comment(cmt1: &ast::Comment<Loc>, cmt2: &ast::Comment<Loc>) -> Option<Vec<NodeChange>> {
    match (&cmt1.kind, &cmt2.kind) {
        (ast::CommentKind::Line, ast::CommentKind::Block) => Some(vec![replace(
            &cmt1.loc,
            Node::Comment(cmt1.dupe()),
            Node::Comment(cmt2.dupe()),
        )]),
        (ast::CommentKind::Block, ast::CommentKind::Line) => Some(vec![replace(
            &cmt1.loc,
            Node::Comment(cmt1.dupe()),
            Node::Comment(cmt2.dupe()),
        )]),
        _ if cmt1.text != cmt2.text => Some(vec![replace(
            &cmt1.loc,
            Node::Comment(cmt1.dupe()),
            Node::Comment(cmt2.dupe()),
        )]),
        _ => None,
    }
}

fn program_inner(
    program1: &ast::Program<Loc, Loc>,
    program2: &ast::Program<Loc, Loc>,
) -> Vec<NodeChange> {
    let program_loc = &program1.loc;
    let statements1 = &program1.statements;
    let statements2 = &program2.statements;
    toplevel_statement_list(statements1, statements2).unwrap_or_else(|| {
        vec![replace(
            program_loc,
            Node::Program(program1.clone()),
            Node::Program(program2.clone()),
        )]
    })
}

fn toplevel_statement_list(
    stmts1: &Arc<[statement::Statement<Loc, Loc>]>,
    stmts2: &Arc<[statement::Statement<Loc, Loc>]>,
) -> Option<Vec<NodeChange>> {
    let part1 = partition_imports(stmts1);
    let mut imports1: Vec<_> = part1.directives;
    imports1.extend(part1.imports);
    let body1 = part1.body;

    let part2 = partition_imports(stmts2);
    let mut imports2: Vec<_> = part2.directives;
    imports2.extend(part2.imports);
    let body2 = part2.body;

    let imports_diff = list_diff(&imports1, &imports2);
    let body_diff = list_diff(&body1, &body2);
    let whole_stmts1: Vec<_> = stmts1.iter().cloned().collect();
    let whole_stmts2: Vec<_> = stmts2.iter().cloned().collect();
    let whole_program_diff = list_diff(&whole_stmts1, &whole_stmts2);

    let split_len = match (&imports_diff, &body_diff) {
        (Some(id), Some(bd)) => id.len() + bd.len(),
        _ => usize::MAX,
    };

    let whole_len = whole_program_diff.as_ref().map_or(usize::MAX, |d| d.len());

    let stmt_trivial = |s: &statement::Statement<Loc, Loc>| -> Option<(Loc, Node)> {
        Some((
            expand_statement_comment_bounds(s),
            Node::Statement(s.dupe(), StatementNodeParent::TopLevelParentOfStatement),
        ))
    };

    let stmt_f = |x: &statement::Statement<Loc, Loc>,
                  y: &statement::Statement<Loc, Loc>|
     -> Option<Vec<NodeChange>> {
        Some(statement(
            &StatementNodeParent::TopLevelParentOfStatement,
            x,
            y,
        ))
    };

    if split_len > whole_len {
        whole_program_diff
            .and_then(|diffs| recurse_into_diff(&stmt_f, &stmt_trivial, &whole_stmts1, 0, &diffs))
    } else {
        let import_recurse = imports_diff
            .and_then(|diffs| recurse_into_diff(&stmt_f, &stmt_trivial, &whole_stmts1, 0, &diffs));

        import_recurse.and_then(|mut import_changes| {
            body_diff
                .and_then(|diffs| {
                    recurse_into_diff(
                        &stmt_f,
                        &stmt_trivial,
                        &whole_stmts1,
                        imports1.len() as i32,
                        &diffs,
                    )
                })
                .map(|body_changes| {
                    import_changes.extend(body_changes);
                    import_changes
                })
        })
    }
}

fn statement_list(
    parent: &StatementNodeParent,
    stmts1: &[statement::Statement<Loc, Loc>],
    stmts2: &[statement::Statement<Loc, Loc>],
) -> Option<Vec<NodeChange>> {
    let parent = parent.clone();
    let stmt_trivial = |s: &statement::Statement<Loc, Loc>| -> Option<(Loc, Node)> {
        Some((
            expand_statement_comment_bounds(s),
            Node::Statement(s.dupe(), parent.clone()),
        ))
    };
    let stmt_f = |x: &statement::Statement<Loc, Loc>,
                  y: &statement::Statement<Loc, Loc>|
     -> Option<Vec<NodeChange>> { Some(statement(&parent, x, y)) };
    diff_and_recurse(&stmt_f, &stmt_trivial, stmts1, stmts2)
}

fn statement(
    parent: &StatementNodeParent,
    stmt1: &statement::Statement<Loc, Loc>,
    stmt2: &statement::Statement<Loc, Loc>,
) -> Vec<NodeChange> {
    let changes: Option<Vec<NodeChange>> = match (&**stmt1, &**stmt2) {
        (
            StatementInner::Block {
                loc, inner: block1, ..
            },
            StatementInner::Block { inner: block2, .. },
        ) => block(loc, block1, block2),
        (
            StatementInner::Expression {
                loc,
                inner: expr_stmt1,
                ..
            },
            StatementInner::Expression {
                inner: expr_stmt2, ..
            },
        ) => expression_statement(loc, expr_stmt1, expr_stmt2),
        (
            StatementInner::If {
                loc, inner: if1, ..
            },
            StatementInner::If { inner: if2, .. },
        ) => if_statement(loc, stmt2, if1, if2),
        (
            StatementInner::While {
                loc, inner: while1, ..
            },
            StatementInner::While { inner: while2, .. },
        ) => Some(while_statement(loc, stmt2, while1, while2)),
        (
            StatementInner::DoWhile {
                loc, inner: dw1, ..
            },
            StatementInner::DoWhile { inner: dw2, .. },
        ) => Some(do_while_statement(loc, stmt2, dw1, dw2)),
        (
            StatementInner::For {
                loc, inner: for1, ..
            },
            StatementInner::For { inner: for2, .. },
        ) => for_statement(loc, stmt2, for1, for2),
        (
            StatementInner::ForIn {
                loc,
                inner: for_in1,
                ..
            },
            StatementInner::ForIn { inner: for_in2, .. },
        ) => for_in_statement(loc, stmt2, for_in1, for_in2),
        (
            StatementInner::ForOf {
                loc,
                inner: for_of1,
                ..
            },
            StatementInner::ForOf { inner: for_of2, .. },
        ) => for_of_statement(loc, stmt2, for_of1, for_of2),
        (
            StatementInner::Try {
                loc, inner: try1, ..
            },
            StatementInner::Try { inner: try2, .. },
        ) => try_(loc, try1, try2),
        (
            StatementInner::Return {
                loc, inner: ret1, ..
            },
            StatementInner::Return { inner: ret2, .. },
        ) => return_statement(loc, stmt2, ret1, ret2),
        (
            StatementInner::Throw {
                loc, inner: throw1, ..
            },
            StatementInner::Throw { inner: throw2, .. },
        ) => Some(throw_statement(loc, stmt2, throw1, throw2)),
        (
            StatementInner::VariableDeclaration {
                loc, inner: var1, ..
            },
            StatementInner::VariableDeclaration { inner: var2, .. },
        ) => variable_declaration(loc, var1, var2),
        (
            StatementInner::FunctionDeclaration {
                loc, inner: func1, ..
            },
            StatementInner::FunctionDeclaration { inner: func2, .. },
        ) => function_(false, loc, func1, func2),
        (
            StatementInner::ComponentDeclaration {
                loc, inner: comp1, ..
            },
            StatementInner::ComponentDeclaration { inner: comp2, .. },
        ) => component_declaration(loc, comp1, comp2),
        (
            StatementInner::ClassDeclaration {
                loc, inner: class1, ..
            },
            StatementInner::ClassDeclaration { inner: class2, .. },
        ) => class_(loc, class1, class2),
        (
            StatementInner::InterfaceDeclaration {
                loc, inner: intf1, ..
            },
            StatementInner::InterfaceDeclaration { inner: intf2, .. },
        ) => interface(loc, intf1, intf2),
        (
            StatementInner::Labeled {
                loc,
                inner: labeled1,
                ..
            },
            StatementInner::Labeled {
                inner: labeled2, ..
            },
        ) => Some(labeled_statement(loc, labeled1, labeled2)),
        (
            StatementInner::Switch {
                loc,
                inner: switch1,
                ..
            },
            StatementInner::Switch { inner: switch2, .. },
        ) => switch_statement(loc, stmt2, switch1, switch2),
        (
            StatementInner::TypeAlias {
                loc, inner: ta1, ..
            },
            StatementInner::TypeAlias { inner: ta2, .. },
        ) => type_alias(loc, ta1, ta2),
        (
            StatementInner::OpaqueType {
                loc, inner: ot1, ..
            },
            StatementInner::OpaqueType { inner: ot2, .. },
        ) => opaque_type(loc, ot1, ot2),
        (
            StatementInner::EnumDeclaration {
                loc, inner: enum1, ..
            },
            StatementInner::EnumDeclaration { inner: enum2, .. },
        ) => enum_declaration(loc, enum1, enum2),
        (
            StatementInner::With {
                loc, inner: with1, ..
            },
            StatementInner::With { inner: with2, .. },
        ) => Some(with_statement(loc, stmt2, with1, with2)),
        (
            StatementInner::DeclareClass {
                loc, inner: dc1, ..
            },
            StatementInner::DeclareClass { inner: dc2, .. },
        ) => declare_class(loc, dc1, dc2),
        (
            StatementInner::DeclareFunction {
                loc, inner: df1, ..
            },
            StatementInner::DeclareFunction { inner: df2, .. },
        ) => declare_function(loc, df1, df2),
        (
            StatementInner::DeclareVariable {
                loc, inner: dv1, ..
            },
            StatementInner::DeclareVariable { inner: dv2, .. },
        ) => declare_variable(loc, dv1, dv2),
        (
            StatementInner::Debugger {
                loc, inner: dbg1, ..
            },
            StatementInner::Debugger { inner: dbg2, .. },
        ) => debugger_statement(loc, dbg1, dbg2),
        (
            StatementInner::Continue {
                loc, inner: cont1, ..
            },
            StatementInner::Continue { inner: cont2, .. },
        ) => continue_statement(loc, cont1, cont2),
        (
            StatementInner::Empty {
                loc, inner: empty1, ..
            },
            StatementInner::Empty { inner: empty2, .. },
        ) => empty_statement(loc, empty1, empty2),
        (StatementInner::Match { loc, inner: m1, .. }, StatementInner::Match { inner: m2, .. }) => {
            match_statement(loc, m1, m2)
        }
        (
            StatementInner::ExportAssignment { loc, inner: a1, .. },
            StatementInner::ExportAssignment { inner: a2, .. },
        ) => Some(export_assignment(loc, stmt2, a1, a2)),
        (
            StatementInner::NamespaceExportDeclaration { loc, inner: d1, .. },
            StatementInner::NamespaceExportDeclaration { inner: d2, .. },
        ) => Some(namespace_export_declaration(loc, d1, d2)),
        (
            StatementInner::ImportEqualsDeclaration { loc, inner: d1, .. },
            StatementInner::ImportEqualsDeclaration { inner: d2, .. },
        ) => import_equals_declaration(loc, d1, d2),
        (
            StatementInner::ExportDefaultDeclaration {
                loc,
                inner: export1,
                ..
            },
            StatementInner::ExportDefaultDeclaration { inner: export2, .. },
        ) => export_default_declaration(loc, parent, export1, export2),
        (
            StatementInner::ExportNamedDeclaration {
                loc,
                inner: export1,
                ..
            },
            StatementInner::ExportNamedDeclaration { inner: export2, .. },
        ) => export_named_declaration(loc, parent, export1, export2),
        (
            StatementInner::DeclareExportDeclaration {
                loc,
                inner: export1,
                ..
            },
            StatementInner::DeclareExportDeclaration { inner: export2, .. },
        ) => declare_export(loc, export1, export2),
        (
            StatementInner::ImportDeclaration {
                loc,
                inner: import1,
                ..
            },
            StatementInner::ImportDeclaration { inner: import2, .. },
        ) => import_declaration(loc, import1, import2),
        (
            StatementInner::DeclareTypeAlias {
                loc, inner: dta1, ..
            },
            StatementInner::DeclareTypeAlias { inner: dta2, .. },
        ) => type_alias(loc, dta1, dta2),
        _ => None,
    };
    let old_loc = stmt1.loc();
    changes.unwrap_or_else(|| {
        vec![replace(
            old_loc,
            Node::Statement(stmt1.dupe(), parent.clone()),
            Node::Statement(stmt2.dupe(), parent.clone()),
        )]
    })
}

fn export_named_declaration(
    loc: &Loc,
    _parent: &StatementNodeParent,
    export1: &ast::statement::ExportNamedDeclaration<Loc, Loc>,
    export2: &ast::statement::ExportNamedDeclaration<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::statement::ExportNamedDeclaration {
        declaration: decl1,
        specifiers: specs1,
        source: src1,
        export_kind: kind1,
        comments: comments1,
        ..
    } = export1;
    let ast::statement::ExportNamedDeclaration {
        declaration: decl2,
        specifiers: specs2,
        source: src2,
        export_kind: kind2,
        comments: comments2,
        ..
    } = export2;
    if src1 != src2 || kind1 != kind2 || export1.attributes != export2.attributes {
        return None;
    }
    let decls = diff_if_changed_nonopt_fn(
        |d1, d2| {
            statement(
                &StatementNodeParent::ExportParentOfStatement(loc.dupe()),
                d1,
                d2,
            )
        },
        decl1,
        decl2,
    );
    let specs = diff_if_changed_opt(export_named_declaration_specifier, specs1, specs2);
    let comments = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![decls, specs, comments])
}

fn export_default_declaration(
    loc: &Loc,
    _parent: &StatementNodeParent,
    export1: &ast::statement::ExportDefaultDeclaration<Loc, Loc>,
    export2: &ast::statement::ExportDefaultDeclaration<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::statement::ExportDefaultDeclaration {
        declaration: declaration1,
        default: default1,
        comments: comments1,
        ..
    } = export1;
    let ast::statement::ExportDefaultDeclaration {
        declaration: declaration2,
        default: default2,
        comments: comments2,
        ..
    } = export2;
    if default1 != default2 {
        return None;
    }
    use ast::statement::export_default_declaration::Declaration;
    let declaration_diff = match (declaration1, declaration2) {
        (Declaration::Declaration(s1), Declaration::Declaration(s2)) => Some(statement(
            &StatementNodeParent::ExportParentOfStatement(loc.dupe()),
            s1,
            s2,
        )),
        (Declaration::Expression(e1), Declaration::Expression(e2)) => {
            let stmt2 = statement::Statement::new(StatementInner::ExportDefaultDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(export2.clone()),
            });
            Some(expression(
                &ExpressionNodeParent::StatementParentOfExpression(stmt2),
                e1,
                e2,
            ))
        }
        _ => None,
    };
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![declaration_diff, comments_diff])
}

fn export_specifier(
    spec1: &ast::statement::export_named_declaration::ExportSpecifier<Loc, Loc>,
    spec2: &ast::statement::export_named_declaration::ExportSpecifier<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    if spec1.export_kind != spec2.export_kind {
        return None;
    }
    let locals = Some(diff_if_changed(identifier, &spec1.local, &spec2.local));
    let exporteds = diff_if_changed_nonopt_fn(identifier, &spec1.exported, &spec2.exported);
    join_diff_list(vec![locals, exporteds])
}

fn export_named_declaration_specifier(
    specs1: &ast::statement::export_named_declaration::Specifier<Loc, Loc>,
    specs2: &ast::statement::export_named_declaration::Specifier<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use ast::statement::export_named_declaration::Specifier::*;
    match (specs1, specs2) {
        (ExportSpecifiers(es1), ExportSpecifiers(es2)) => {
            diff_and_recurse_no_trivial(&|s1, s2| export_specifier(s1, s2), es1, es2)
        }
        (ExportBatchSpecifier(ebs1), ExportBatchSpecifier(ebs2)) => {
            diff_if_changed_nonopt_fn(identifier, &ebs1.specifier, &ebs2.specifier)
        }
        _ => None,
    }
}

fn declare_export(
    loc: &Loc,
    export1: &ast::statement::DeclareExportDeclaration<Loc, Loc>,
    export2: &ast::statement::DeclareExportDeclaration<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::statement::DeclareExportDeclaration {
        default: default1,
        declaration: decl1,
        specifiers: specs1,
        source: src1,
        comments: comments1,
        ..
    } = export1;
    let ast::statement::DeclareExportDeclaration {
        default: default2,
        declaration: decl2,
        specifiers: specs2,
        source: src2,
        comments: comments2,
        ..
    } = export2;
    if default1 != default2 || src1 != src2 {
        return None;
    }
    let decl_diff = match (decl1, decl2) {
        (None, None) => Some(vec![]),
        (Some(d1), Some(d2)) if d1 == d2 => Some(vec![]),
        (Some(d1), Some(d2)) => declare_export_declaration(d1, d2),
        _ => None,
    };
    let specs_diff = diff_if_changed_opt(export_named_declaration_specifier, specs1, specs2);
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![decl_diff, specs_diff, comments_diff])
}

fn declare_export_declaration(
    decl1: &ast::statement::declare_export_declaration::Declaration<Loc, Loc>,
    decl2: &ast::statement::declare_export_declaration::Declaration<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use ast::statement::declare_export_declaration::Declaration::*;
    match (decl1, decl2) {
        (
            Variable {
                loc,
                declaration: v1,
            },
            Variable {
                declaration: v2, ..
            },
        ) => declare_variable(loc, v1, v2),
        (
            Function {
                loc,
                declaration: f1,
            },
            Function {
                declaration: f2, ..
            },
        ) => declare_function(loc, f1, f2),
        (
            Class {
                loc,
                declaration: c1,
            },
            Class {
                declaration: c2, ..
            },
        ) => declare_class(loc, c1, c2),
        (
            NamedType {
                loc,
                declaration: t1,
            },
            NamedType {
                declaration: t2, ..
            },
        ) => type_alias(loc, t1, t2),
        (
            NamedOpaqueType {
                loc,
                declaration: t1,
            },
            NamedOpaqueType {
                declaration: t2, ..
            },
        ) => opaque_type(loc, t1, t2),
        (DefaultType { type_: t1 }, DefaultType { type_: t2 }) => Some(type_(t1, t2)),
        _ => None,
    }
}

fn import_default_specifier(
    ds1: &Option<ast::statement::import_declaration::DefaultIdentifier<Loc, Loc>>,
    ds2: &Option<ast::statement::import_declaration::DefaultIdentifier<Loc, Loc>>,
) -> Option<Vec<NodeChange>> {
    diff_if_changed_nonopt_fn(
        |ds1, ds2| identifier(&ds1.identifier, &ds2.identifier),
        ds1,
        ds2,
    )
}

fn import_namespace_specifier(
    ident1: &ast::Identifier<Loc, Loc>,
    ident2: &ast::Identifier<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    Some(diff_if_changed(identifier, ident1, ident2))
}

fn import_named_specifier(
    nm_spec1: &ast::statement::import_declaration::NamedSpecifier<Loc, Loc>,
    nm_spec2: &ast::statement::import_declaration::NamedSpecifier<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::statement::import_declaration::NamedSpecifier {
        kind: kind1,
        local: local1,
        remote: remote1,
        ..
    } = nm_spec1;
    let ast::statement::import_declaration::NamedSpecifier {
        kind: kind2,
        local: local2,
        remote: remote2,
        ..
    } = nm_spec2;
    if kind1 != kind2 {
        None
    } else {
        let locals = diff_if_changed_nonopt_fn(identifier, local1, local2);
        let remotes = Some(diff_if_changed(identifier, remote1, remote2));
        join_diff_list(vec![locals, remotes])
    }
}

fn import_specifier(
    spec1: &ast::statement::import_declaration::Specifier<Loc, Loc>,
    spec2: &ast::statement::import_declaration::Specifier<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use ast::statement::import_declaration::Specifier::*;
    match (spec1, spec2) {
        (ImportNamedSpecifiers(nm_specs1), ImportNamedSpecifiers(nm_specs2)) => {
            diff_and_recurse_no_trivial(
                &|s1, s2| import_named_specifier(s1, s2),
                nm_specs1,
                nm_specs2,
            )
        }
        (ImportNamespaceSpecifier((_, ident1)), ImportNamespaceSpecifier((_, ident2))) => {
            diff_if_changed_ret_opt(import_namespace_specifier, ident1, ident2)
        }
        _ => None,
    }
}

fn import_declaration(
    loc: &Loc,
    import1: &ast::statement::ImportDeclaration<Loc, Loc>,
    import2: &ast::statement::ImportDeclaration<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::statement::ImportDeclaration {
        import_kind: imprt_knd1,
        phase: phase1,
        source: src1,
        default: dflt1,
        specifiers: spec1,
        comments: comments1,
        ..
    } = import1;
    let ast::statement::ImportDeclaration {
        import_kind: imprt_knd2,
        phase: phase2,
        source: src2,
        default: dflt2,
        specifiers: spec2,
        comments: comments2,
        ..
    } = import2;
    if imprt_knd1 != imprt_knd2 || phase1 != phase2 || src1 != src2 {
        None
    } else {
        let dflt_diff = import_default_specifier(dflt1, dflt2);
        let spec_diff = diff_if_changed_opt(import_specifier, spec1, spec2);
        let comments_diff = syntax_opt(loc, comments1, comments2);
        join_diff_list(vec![dflt_diff, spec_diff, comments_diff])
    }
}

fn component_declaration(
    loc: &Loc,
    comp1: &ast::statement::ComponentDeclaration<Loc, Loc>,
    comp2: &ast::statement::ComponentDeclaration<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::statement::ComponentDeclaration {
        id: id1,
        params: params1,
        body: body1_opt,
        renders: renders1,
        tparams: tparams1,
        comments: comments1,
        ..
    } = comp1;
    let ast::statement::ComponentDeclaration {
        id: id2,
        params: params2,
        body: body2_opt,
        renders: renders2,
        tparams: tparams2,
        comments: comments2,
        ..
    } = comp2;
    let id = diff_if_changed_nonopt_fn(|i1, i2| identifier(i1, i2), &Some(id1), &Some(id2));
    let tparams = diff_if_changed_opt(type_params, tparams1, tparams2);
    let params = diff_if_changed_ret_opt(component_params, params1, params2);
    let returns = match (renders1, renders2) {
        (
            ast::types::ComponentRendersAnnotation::AvailableRenders(rloc, r1),
            ast::types::ComponentRendersAnnotation::AvailableRenders(_, r2),
        ) => diff_if_changed_ret_opt(|r1, r2| render_type(rloc, r1, r2), r1, r2),
        (
            ast::types::ComponentRendersAnnotation::MissingRenders(_),
            ast::types::ComponentRendersAnnotation::MissingRenders(_),
        ) => Some(vec![]),
        _ => None,
    };
    let body = match (body1_opt, body2_opt) {
        (Some((loc_body, body1)), Some((_, body2))) => {
            diff_if_changed_ret_opt(|b1, b2| block(loc_body, b1, b2), body1, body2)
        }
        (None, None) => Some(vec![]),
        _ => None,
    };
    let comments = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![id, tparams, params, returns, body, comments])
}

fn component_params(
    params1: &ast::statement::component_params::Params<Loc, Loc>,
    params2: &ast::statement::component_params::Params<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let loc = &params1.loc;
    let params_diff = diff_and_recurse_no_trivial(
        &|p1, p2| component_param(p1, p2),
        &params1.params,
        &params2.params,
    );
    let rest_diff = diff_if_changed_opt(component_rest_param, &params1.rest, &params2.rest);
    let comments_diff = syntax_opt(loc, &params1.comments, &params2.comments);
    join_diff_list(vec![params_diff, rest_diff, comments_diff])
}

fn component_param(
    param1: &ast::statement::component_params::Param<Loc, Loc>,
    param2: &ast::statement::component_params::Param<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use ast::statement::component_params::ParamName;
    let ast::statement::component_params::Param {
        name: name1,
        local: local1,
        default: def1,
        shorthand: shorthand1,
        ..
    } = param1;
    let ast::statement::component_params::Param {
        name: name2,
        local: local2,
        default: def2,
        shorthand: shorthand2,
        ..
    } = param2;
    let name_diff = match (name1, name2) {
        (ParamName::Identifier(id1), ParamName::Identifier(id2)) => {
            diff_if_changed_nonopt_fn(|i1, i2| identifier(i1, i2), &Some(id1), &Some(id2))
        }
        (ParamName::StringLiteral((loc1, lit1)), ParamName::StringLiteral((loc2, lit2))) => {
            diff_if_changed_ret_opt(|l1, l2| string_literal(loc1, loc2, l1, l2), lit1, lit2)
        }
        (ParamName::Identifier(id1), ParamName::StringLiteral((loc2, lit2))) => {
            Some(vec![replace(
                &id1.loc,
                Node::Raw(id1.name.to_string()),
                Node::StringLiteral(loc2.dupe(), lit2.clone()),
            )])
        }
        (ParamName::StringLiteral((loc1, lit1)), ParamName::Identifier(id2)) => {
            Some(vec![replace(
                loc1,
                Node::StringLiteral(loc1.dupe(), lit1.clone()),
                Node::Raw(id2.name.to_string()),
            )])
        }
    };
    let local_diff = Some(diff_if_changed(pattern, local1, local2));
    let default_diff = diff_if_changed_nonopt_fn(
        |e1, e2| expression(&ExpressionNodeParent::SlotParentOfExpression, e1, e2),
        def1,
        def2,
    );
    match (shorthand1, shorthand2) {
        (false, false) => join_diff_list(vec![name_diff, local_diff, default_diff]),
        (true, true) => join_diff_list(vec![local_diff, default_diff]),
        (_, _) => None,
    }
}

fn component_rest_param(
    elem1: &ast::statement::component_params::RestParam<Loc, Loc>,
    elem2: &ast::statement::component_params::RestParam<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let loc = &elem1.loc;
    let arg_diff = Some(pattern(&elem1.argument, &elem2.argument));
    let comments_diff = syntax_opt(loc, &elem1.comments, &elem2.comments);
    join_diff_list(vec![arg_diff, comments_diff])
}

fn function_(
    is_arrow: bool,
    loc: &Loc,
    func1: &ast::function::Function<Loc, Loc>,
    func2: &ast::function::Function<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::function::Function {
        id: id1,
        params: params1,
        body: body1,
        async_: async1,
        generator: generator1,
        predicate: predicate1,
        effect_: effect1,
        return_: return1,
        tparams: tparams1,
        comments: comments1,
        ..
    } = func1;
    let ast::function::Function {
        id: id2,
        params: params2,
        body: body2,
        async_: async2,
        generator: generator2,
        predicate: predicate2,
        effect_: effect2,
        return_: return2,
        tparams: tparams2,
        comments: comments2,
        ..
    } = func2;
    if async1 != async2
        || generator1 != generator2
        || predicate1 != predicate2
        || effect1 != effect2
    {
        return None;
    }
    let id = diff_if_changed_nonopt_fn(identifier, id1, id2);
    let tparams = diff_if_changed_opt(type_params, tparams1, tparams2);
    let params = diff_if_changed_ret_opt(function_params, params1, params2);
    let returns = Some(diff_if_changed(function_return_annot, return1, return2));
    // reprint the parameter if it's the single parameter of a lambda written without (), or when
    // return annotation has changed to add () to avoid syntax errors. Such a parameter list is
    // located at its parameter.
    let params = {
        let is_single_param_arrow = is_arrow
            && matches!(
                &*params1.params,
                [ast::function::Param::RegularParam { loc, .. }] if *loc == params1.loc
            )
            && params1.rest.is_none()
            && params1.this_.is_none()
            && params2.params.len() == 1
            && params2.rest.is_none()
            && params2.this_.is_none();
        if is_single_param_arrow {
            match (&params, &returns) {
                (Some(p), _) if p.len() == 1 => {
                    let l = &params1.loc;
                    Some(vec![replace(
                        l,
                        Node::Params(params1.clone()),
                        Node::Params(params2.clone()),
                    )])
                }
                (_, Some(r)) if !r.is_empty() => {
                    let l = &params1.loc;
                    Some(vec![replace(
                        l,
                        Node::Params(params1.clone()),
                        Node::Params(params2.clone()),
                    )])
                }
                _ => params,
            }
        } else {
            params
        }
    };
    let fnbody = diff_if_changed_ret_opt(function_body_any, body1, body2);
    let comments = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![id, tparams, params, returns, fnbody, comments])
}

fn function_params(
    params1: &ast::function::Params<Loc, Loc>,
    params2: &ast::function::Params<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let loc = &params1.loc;
    let params_diff = diff_and_recurse_no_trivial(
        &|p1, p2| function_param(p1, p2),
        &params1.params,
        &params2.params,
    );
    let rest_diff = diff_if_changed_opt(function_rest_param, &params1.rest, &params2.rest);
    let this_diff = diff_if_changed_opt(function_this_param, &params1.this_, &params2.this_);
    let comments_diff = syntax_opt(loc, &params1.comments, &params2.comments);
    join_diff_list(vec![params_diff, rest_diff, this_diff, comments_diff])
}

fn function_this_param(
    ftp1: &ast::function::ThisParam<Loc, Loc>,
    ftp2: &ast::function::ThisParam<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let loc = &ftp1.loc;
    let annot_diff = Some(diff_if_changed(type_annotation, &ftp1.annot, &ftp2.annot));
    let comments_diff = syntax_opt(loc, &ftp1.comments, &ftp2.comments);
    join_diff_list(vec![annot_diff, comments_diff])
}

fn function_param(
    param1: &ast::function::Param<Loc, Loc>,
    param2: &ast::function::Param<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use ast::function::Param;
    match (param1, param2) {
        (
            Param::RegularParam {
                argument: arg1,
                default: def1,
                ..
            },
            Param::RegularParam {
                argument: arg2,
                default: def2,
                ..
            },
        ) => {
            let param_diff = Some(diff_if_changed(pattern, arg1, arg2));
            let default_diff = diff_if_changed_nonopt_fn(
                |e1, e2| expression(&ExpressionNodeParent::SlotParentOfExpression, e1, e2),
                def1,
                def2,
            );
            join_diff_list(vec![param_diff, default_diff])
        }
        (
            Param::ParamProperty {
                loc: loc1,
                property: prop1,
            },
            Param::ParamProperty {
                property: prop2, ..
            },
        ) => {
            if prop1.key != prop2.key
                || prop1.static_ != prop2.static_
                || prop1.optional != prop2.optional
                || prop1.variance != prop2.variance
                || prop1.ts_accessibility != prop2.ts_accessibility
            {
                None
            } else {
                let vals =
                    diff_if_changed_ret_opt(class_property_value, &prop1.value, &prop2.value);
                let annots = Some(diff_if_changed(
                    type_annotation_hint,
                    &prop1.annot,
                    &prop2.annot,
                ));
                let decorators = diff_and_recurse_no_trivial(
                    &|d1, d2| class_decorator(d1, d2),
                    &prop1.decorators,
                    &prop2.decorators,
                );
                let comments = syntax_opt(loc1, &prop1.comments, &prop2.comments);
                join_diff_list(vec![vals, annots, decorators, comments])
            }
        }
        _ => None,
    }
}

fn function_return_annot(
    return1: &ast::function::ReturnAnnot<Loc, Loc>,
    return2: &ast::function::ReturnAnnot<Loc, Loc>,
) -> Vec<NodeChange> {
    use ast::function::ReturnAnnot;
    let annot_change = |typ: ast::types::Annotation<Loc, Loc>| -> Node {
        match return2 {
            ReturnAnnot::Available(annot) => match &*annot.annotation {
                ast::types::TypeInner::Function { .. } => Node::FunctionTypeAnnotation(typ),
                _ => Node::TypeAnnotation(typ),
            },
            _ => Node::TypeAnnotation(typ),
        }
    };
    match (return1, return2) {
        (ReturnAnnot::Missing(_), ReturnAnnot::Missing(_)) => vec![],
        (ReturnAnnot::Available(annot1), ReturnAnnot::Missing(_)) => {
            vec![delete(&annot1.loc, Node::TypeAnnotation(annot1.clone()))]
        }
        (ReturnAnnot::TypeGuard(guard1), ReturnAnnot::Missing(_)) => {
            vec![delete(
                &guard1.loc,
                Node::TypeGuardAnnotation(guard1.clone()),
            )]
        }
        (ReturnAnnot::Missing(loc1), ReturnAnnot::Available(annot)) => {
            vec![(loc1.dupe(), insert(None, vec![annot_change(annot.clone())]))]
        }
        (ReturnAnnot::Missing(loc1), ReturnAnnot::TypeGuard(guard)) => {
            vec![(
                loc1.dupe(),
                insert(None, vec![Node::TypeGuardAnnotation(guard.clone())]),
            )]
        }
        (ReturnAnnot::Available(annot1), ReturnAnnot::Available(annot2)) => {
            type_annotation(annot1, annot2)
        }
        (ReturnAnnot::TypeGuard(guard1), ReturnAnnot::TypeGuard(guard2)) => {
            type_guard_annotation(guard1, guard2)
        }
        (ReturnAnnot::Available(annot1), ReturnAnnot::TypeGuard(guard2)) => {
            vec![replace(
                &annot1.loc,
                Node::TypeAnnotation(annot1.clone()),
                Node::TypeGuardAnnotation(guard2.clone()),
            )]
        }
        (ReturnAnnot::TypeGuard(guard1), ReturnAnnot::Available(annot2)) => {
            vec![replace(
                &guard1.loc,
                Node::TypeGuardAnnotation(guard1.clone()),
                Node::TypeAnnotation(annot2.clone()),
            )]
        }
    }
}

fn function_body_any(
    body1: &ast::function::Body<Loc, Loc>,
    body2: &ast::function::Body<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use ast::function::Body;
    match (body1, body2) {
        (Body::BodyExpression(e1), Body::BodyExpression(e2)) => Some(expression(
            &ExpressionNodeParent::SlotParentOfExpression,
            e1,
            e2,
        )),
        (Body::BodyBlock((loc, block1)), Body::BodyBlock((_, block2))) => {
            block(loc, block1, block2)
        }
        _ => None,
    }
}

fn variable_declarator(
    decl1: &ast::statement::variable::Declarator<Loc, Loc>,
    decl2: &ast::statement::variable::Declarator<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let id_diff = Some(diff_if_changed(pattern, &decl1.id, &decl2.id));
    let expr_diff = diff_if_changed_nonopt_fn(
        |e1, e2| expression(&ExpressionNodeParent::SlotParentOfExpression, e1, e2),
        &decl1.init,
        &decl2.init,
    );
    join_diff_list(vec![id_diff, expr_diff])
}

fn variable_declaration(
    loc: &Loc,
    var1: &ast::statement::VariableDeclaration<Loc, Loc>,
    var2: &ast::statement::VariableDeclaration<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::statement::VariableDeclaration {
        declarations: declarations1,
        kind: kind1,
        comments: comments1,
    } = var1;
    let ast::statement::VariableDeclaration {
        declarations: declarations2,
        kind: kind2,
        comments: comments2,
    } = var2;
    if kind1 != kind2 {
        return None;
    }
    let declarations_diff = if declarations1 != declarations2 {
        diff_and_recurse_no_trivial(
            &|d1, d2| variable_declarator(d1, d2),
            declarations1,
            declarations2,
        )
    } else {
        Some(vec![])
    };
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![declarations_diff, comments_diff])
}

fn if_statement(
    loc: &Loc,
    stmt2: &statement::Statement<Loc, Loc>,
    if1: &ast::statement::If<Loc, Loc>,
    if2: &ast::statement::If<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::statement::If {
        test: test1,
        consequent: consequent1,
        alternate: alternate1,
        comments: comments1,
    } = if1;
    let ast::statement::If {
        test: test2,
        consequent: consequent2,
        alternate: alternate2,
        comments: comments2,
    } = if2;
    let expr_diff = Some(expression(
        &ExpressionNodeParent::StatementParentOfExpression(stmt2.dupe()),
        test1,
        test2,
    ));
    let parent = StatementNodeParent::IfParentOfStatement(loc.dupe());
    let cons_diff = Some(diff_if_changed(
        |c1, c2| statement(&parent, c1, c2),
        consequent1,
        consequent2,
    ));
    let alt_diff = match (alternate1, alternate2) {
        (None, None) => Some(vec![]),
        (Some(_), None) | (None, Some(_)) => None,
        (Some(alt1), Some(alt2)) => {
            let body_diff = Some(diff_if_changed(
                |b1, b2| statement(&parent, b1, b2),
                &alt1.body,
                &alt2.body,
            ));
            let comments_diff = syntax_opt(&alt1.loc, &alt1.comments, &alt2.comments);
            join_diff_list(vec![body_diff, comments_diff])
        }
    };
    let comments = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![comments, expr_diff, cons_diff, alt_diff])
}

fn with_statement(
    loc: &Loc,
    stmt2: &statement::Statement<Loc, Loc>,
    with1: &ast::statement::With<Loc, Loc>,
    with2: &ast::statement::With<Loc, Loc>,
) -> Vec<NodeChange> {
    let ast::statement::With {
        object: _object1,
        body: body1,
        comments: comments1,
        ..
    } = with1;
    let ast::statement::With {
        object: _object2,
        body: body2,
        comments: comments2,
        ..
    } = with2;
    let object_diff = diff_if_changed(
        |o1, o2| {
            expression(
                &ExpressionNodeParent::StatementParentOfExpression(stmt2.dupe()),
                o1,
                o2,
            )
        },
        _object1,
        _object2,
    );
    let body_diff = diff_if_changed(
        |b1, b2| {
            statement(
                &StatementNodeParent::WithStatementParentOfStatement(loc.dupe()),
                b1,
                b2,
            )
        },
        body1,
        body2,
    );
    let comments_diff = syntax_opt(loc, comments1, comments2).unwrap_or_default();
    let mut result = object_diff;
    result.extend(body_diff);
    result.extend(comments_diff);
    result
}

fn try_(
    loc: &Loc,
    try1: &ast::statement::Try<Loc, Loc>,
    try2: &ast::statement::Try<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::statement::Try {
        block: (block_loc, block1),
        handler: handler1,
        finalizer: finalizer1,
        comments: comments1,
        ..
    } = try1;
    let ast::statement::Try {
        block: (_, block2),
        handler: handler2,
        finalizer: finalizer2,
        comments: comments2,
        ..
    } = try2;
    let comments = syntax_opt(loc, comments1, comments2);
    let block_diff = diff_if_changed_ret_opt(|b1, b2| block(block_loc, b1, b2), block1, block2);
    let finalizer_diff = match (finalizer1, finalizer2) {
        (Some(f1), Some(f2)) => {
            diff_if_changed_ret_opt(|b1, b2| block(&f1.0, b1, b2), &f1.1, &f2.1)
        }
        (None, None) => Some(vec![]),
        _ => None,
    };
    let handler_diff = diff_if_changed_opt(handler, handler1, handler2);
    join_diff_list(vec![comments, block_diff, finalizer_diff, handler_diff])
}

fn handler(
    hand1: &ast::statement::try_::CatchClause<Loc, Loc>,
    hand2: &ast::statement::try_::CatchClause<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::statement::try_::CatchClause {
        loc: old_loc,
        body: (block_loc, block1),
        param: param1,
        comments: comments1,
    } = hand1;
    let ast::statement::try_::CatchClause {
        body: (_, block2),
        param: param2,
        comments: comments2,
        ..
    } = hand2;
    let comments = syntax_opt(old_loc, comments1, comments2);
    let body_diff = diff_if_changed_ret_opt(|b1, b2| block(block_loc, b1, b2), block1, block2);
    let param_diff = diff_if_changed_nonopt_fn(pattern, param1, param2);
    join_diff_list(vec![comments, body_diff, param_diff])
}

fn class_(
    loc: &Loc,
    class1: &ast::class::Class<Loc, Loc>,
    class2: &ast::class::Class<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::class::Class {
        id: id1,
        body: body1,
        tparams: tparams1,
        extends: extends1,
        implements: implements1,
        class_decorators: class_decorators1,
        abstract_: abstract1,
        comments: comments1,
        ..
    } = class1;
    let ast::class::Class {
        id: id2,
        body: body2,
        tparams: tparams2,
        extends: extends2,
        implements: implements2,
        class_decorators: class_decorators2,
        abstract_: abstract2,
        comments: comments2,
        ..
    } = class2;
    if id1 != id2 || abstract1 != abstract2 {
        return None;
    }
    let tparams_diff = diff_if_changed_opt(type_params, tparams1, tparams2);
    let extends_diff = diff_if_changed_opt(class_extends, extends1, extends2);
    let implements_diff = diff_if_changed_opt(class_implements, implements1, implements2);
    let body_diff = diff_if_changed_ret_opt(class_body, body1, body2);
    let decorators_diff = diff_and_recurse_no_trivial(
        &|d1, d2| class_decorator(d1, d2),
        class_decorators1,
        class_decorators2,
    );
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![
        tparams_diff,
        extends_diff,
        implements_diff,
        body_diff,
        decorators_diff,
        comments_diff,
    ])
}

fn class_extends(
    extends1: &ast::class::Extends<Loc, Loc>,
    extends2: &ast::class::Extends<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::class::Extends {
        loc: extends_loc,
        expr: expr1,
        targs: targs1,
        comments: comments1,
    } = extends1;
    let ast::class::Extends {
        expr: expr2,
        targs: targs2,
        comments: comments2,
        ..
    } = extends2;
    let expr_diff = Some(diff_if_changed(
        |e1, e2| expression(&ExpressionNodeParent::ClassExtends, e1, e2),
        expr1,
        expr2,
    ));
    let targs_diff = diff_if_changed_opt(type_args, targs1, targs2);
    let comments_diff = syntax_opt(extends_loc, comments1, comments2);
    join_diff_list(vec![expr_diff, targs_diff, comments_diff])
}

fn class_implements(
    implements1: &ast::class::Implements<Loc, Loc>,
    implements2: &ast::class::Implements<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::class::Implements {
        loc: impl_loc,
        interfaces: interfaces1,
        comments: comments1,
    } = implements1;
    let ast::class::Implements {
        interfaces: interfaces2,
        comments: comments2,
        ..
    } = implements2;
    let interfaces_diff = diff_and_recurse_no_trivial(
        &|i1, i2| class_implements_interface(i1, i2),
        interfaces1,
        interfaces2,
    );
    let comments_diff = syntax_opt(impl_loc, comments1, comments2);
    join_diff_list(vec![interfaces_diff, comments_diff])
}

fn class_implements_interface(
    interface1: &ast::class::implements::Interface<Loc, Loc>,
    interface2: &ast::class::implements::Interface<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::class::implements::Interface {
        id: id1,
        targs: targs1,
        ..
    } = interface1;
    let ast::class::implements::Interface {
        id: id2,
        targs: targs2,
        ..
    } = interface2;
    let id_diff = diff_if_changed_ret_opt(generic_identifier_type, id1, id2);
    let targs_diff = diff_if_changed_opt(type_args, targs1, targs2);
    join_diff_list(vec![id_diff, targs_diff])
}

fn interface(
    loc: &Loc,
    intf1: &ast::statement::Interface<Loc, Loc>,
    intf2: &ast::statement::Interface<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::statement::Interface {
        id: id1,
        tparams: tparams1,
        extends: extends1,
        body: (body_loc, body1),
        comments: comments1,
    } = intf1;
    let ast::statement::Interface {
        id: id2,
        tparams: tparams2,
        extends: extends2,
        body: (_, body2),
        comments: comments2,
    } = intf2;
    let id_diff = Some(diff_if_changed(identifier, id1, id2));
    let tparams_diff = diff_if_changed_opt(type_params, tparams1, tparams2);
    let extends_diff =
        diff_and_recurse_no_trivial(&|e1, e2| generic_type_with_loc(e1, e2), extends1, extends2);
    let body_diff = diff_if_changed_ret_opt(|b1, b2| object_type(body_loc, b1, b2), body1, body2);
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![
        id_diff,
        tparams_diff,
        extends_diff,
        body_diff,
        comments_diff,
    ])
}

fn class_body(
    body1: &ast::class::Body<Loc, Loc>,
    body2: &ast::class::Body<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let body_diff =
        diff_and_recurse_no_trivial(&|e1, e2| class_element(e1, e2), &body1.body, &body2.body);
    let comments_diff = syntax_opt(&body1.loc, &body1.comments, &body2.comments);
    join_diff_list(vec![body_diff, comments_diff])
}

fn class_decorator(
    dec1: &ast::class::Decorator<Loc, Loc>,
    dec2: &ast::class::Decorator<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let expression_diff = Some(expression(
        &ExpressionNodeParent::SlotParentOfExpression,
        &dec1.expression,
        &dec2.expression,
    ));
    let comments_diff = syntax_opt(&dec1.loc, &dec1.comments, &dec2.comments);
    join_diff_list(vec![expression_diff, comments_diff])
}

fn class_element(
    elem1: &ast::class::BodyElement<Loc, Loc>,
    elem2: &ast::class::BodyElement<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use ast::class::BodyElement::*;
    match (elem1, elem2) {
        (Method(m1), Method(m2)) => class_method(m1, m2),
        (Property(p1), Property(p2)) => Some(class_property(p1, p2)),
        (PrivateField(f1), PrivateField(f2)) => Some(class_private_field(f1, f2)),
        (DeclareMethod(m1), DeclareMethod(m2)) => class_declare_method(m1, m2),
        (Method(_), _) | (_, Method(_)) => None,
        (Property(p1), PrivateField(f2)) => Some(vec![replace(
            &p1.loc,
            Node::ClassProperty(p1.clone()),
            Node::ClassPrivateField(f2.clone()),
        )]),
        (PrivateField(f1), Property(p2)) => Some(vec![replace(
            &f1.loc,
            Node::ClassPrivateField(f1.clone()),
            Node::ClassProperty(p2.clone()),
        )]),
        (StaticBlock(_), _) | (_, StaticBlock(_)) => None,
        (DeclareMethod(_), _) | (_, DeclareMethod(_)) => None,
        (AbstractMethod(_), _) | (_, AbstractMethod(_)) => None,
        (AbstractProperty(_), _) | (_, AbstractProperty(_)) => None,
        (IndexSignature(_), _) | (_, IndexSignature(_)) => None,
    }
}

fn class_private_field(
    field1: &ast::class::PrivateField<Loc, Loc>,
    field2: &ast::class::PrivateField<Loc, Loc>,
) -> Vec<NodeChange> {
    let result = if field1.key != field2.key
        || field1.static_ != field2.static_
        || field1.optional != field2.optional
        || field1.variance != field2.variance
        || field1.ts_accessibility != field2.ts_accessibility
    {
        None
    } else {
        let vals = diff_if_changed_ret_opt(class_property_value, &field1.value, &field2.value);
        let annots = Some(diff_if_changed(
            type_annotation_hint,
            &field1.annot,
            &field2.annot,
        ));
        let decorators = diff_and_recurse_no_trivial(
            &|d1, d2| class_decorator(d1, d2),
            &field1.decorators,
            &field2.decorators,
        );
        let comments = syntax_opt(&field1.loc, &field1.comments, &field2.comments);
        join_diff_list(vec![vals, annots, decorators, comments])
    };
    result.unwrap_or_else(|| {
        vec![replace(
            &field1.loc,
            Node::ClassPrivateField(field1.clone()),
            Node::ClassPrivateField(field2.clone()),
        )]
    })
}

fn class_property(
    prop1: &ast::class::Property<Loc, Loc>,
    prop2: &ast::class::Property<Loc, Loc>,
) -> Vec<NodeChange> {
    let result = if prop1.key != prop2.key
        || prop1.static_ != prop2.static_
        || prop1.optional != prop2.optional
        || prop1.variance != prop2.variance
        || prop1.ts_accessibility != prop2.ts_accessibility
    {
        None
    } else {
        let vals = diff_if_changed_ret_opt(class_property_value, &prop1.value, &prop2.value);
        let annots = Some(diff_if_changed(
            type_annotation_hint,
            &prop1.annot,
            &prop2.annot,
        ));
        let decorators = diff_and_recurse_no_trivial(
            &|d1, d2| class_decorator(d1, d2),
            &prop1.decorators,
            &prop2.decorators,
        );
        let comments = syntax_opt(&prop1.loc, &prop1.comments, &prop2.comments);
        join_diff_list(vec![vals, annots, decorators, comments])
    };
    result.unwrap_or_else(|| {
        vec![replace(
            &prop1.loc,
            Node::ClassProperty(prop1.clone()),
            Node::ClassProperty(prop2.clone()),
        )]
    })
}

fn class_property_value(
    val1: &ast::class::property::Value<Loc, Loc>,
    val2: &ast::class::property::Value<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use ast::class::property::Value::*;
    match (val1, val2) {
        (Declared, Declared) => Some(vec![]),
        (Uninitialized, Uninitialized) => Some(vec![]),
        (Initialized(e1), Initialized(e2)) => Some(diff_if_changed(
            |e1, e2| expression(&ExpressionNodeParent::SlotParentOfExpression, e1, e2),
            e1,
            e2,
        )),
        _ => None,
    }
}

fn class_method(
    m1: &ast::class::Method<Loc, Loc>,
    m2: &ast::class::Method<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::class::Method {
        kind: kind1,
        key: key1,
        value: (value_loc, value1),
        static_: static1,
        ts_accessibility: acc1,
        decorators: decorators1,
        comments: comments1,
        ..
    } = m1;
    let ast::class::Method {
        kind: kind2,
        key: key2,
        value: (_, value2),
        static_: static2,
        ts_accessibility: acc2,
        decorators: decorators2,
        comments: comments2,
        ..
    } = m2;
    // value handled below
    if kind1 != kind2 || key1 != key2 || static1 != static2 || acc1 != acc2 {
        return None;
    }
    let value_diff = function_(false, value_loc, value1, value2);
    let decorators_diff =
        diff_and_recurse_no_trivial(&|d1, d2| class_decorator(d1, d2), decorators1, decorators2);
    let comments_diff = syntax_opt(&m1.loc, comments1, comments2);
    join_diff_list(vec![value_diff, decorators_diff, comments_diff])
}

fn class_declare_method(
    m1: &ast::class::DeclareMethod<Loc, Loc>,
    m2: &ast::class::DeclareMethod<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    if m1.key != m2.key
        || m1.static_ != m2.static_
        || m1.kind != m2.kind
        || m1.optional != m2.optional
        || m1.ts_accessibility != m2.ts_accessibility
    {
        return None;
    }
    let annot_diff = Some(diff_if_changed(type_annotation, &m1.annot, &m2.annot));
    let comments_diff = syntax_opt(&m1.loc, &m1.comments, &m2.comments);
    join_diff_list(vec![annot_diff, comments_diff])
}

fn block(
    loc: &Loc,
    block1: &ast::statement::Block<Loc, Loc>,
    block2: &ast::statement::Block<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::statement::Block {
        body: body1,
        comments: comments1,
        ..
    } = block1;
    let ast::statement::Block {
        body: body2,
        comments: comments2,
        ..
    } = block2;
    let body_diff = statement_list(
        &StatementNodeParent::StatementBlockParentOfStatement(loc.dupe()),
        body1,
        body2,
    );
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![body_diff, comments_diff])
}

fn expression_statement(
    loc: &Loc,
    stmt1: &ast::statement::Expression<Loc, Loc>,
    stmt2: &ast::statement::Expression<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::statement::Expression {
        expression: expr1,
        directive: dir1,
        comments: comments1,
        ..
    } = stmt1;
    let ast::statement::Expression {
        expression: expr2,
        directive: dir2,
        comments: comments2,
        ..
    } = stmt2;
    if dir1 != dir2 {
        return None;
    }
    let stmt2_as_stmt = statement::Statement::new(StatementInner::Expression {
        loc: loc.dupe(),
        inner: Arc::new(stmt2.clone()),
    });
    let expression_diff = Some(expression(
        &ExpressionNodeParent::StatementParentOfExpression(stmt2_as_stmt),
        expr1,
        expr2,
    ));
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![expression_diff, comments_diff])
}

fn expression(
    parent: &ExpressionNodeParent,
    expr1: &expression::Expression<Loc, Loc>,
    expr2: &expression::Expression<Loc, Loc>,
) -> Vec<NodeChange> {
    let changes: Option<Vec<NodeChange>> = match (&**expr1, &**expr2) {
        (
            ExpressionInner::StringLiteral {
                loc: loc1,
                inner: lit1,
            },
            ExpressionInner::StringLiteral {
                loc: loc2,
                inner: lit2,
            },
        ) => diff_if_changed_ret_opt(|l1, l2| string_literal(loc1, loc2, l1, l2), lit1, lit2),
        (
            ExpressionInner::BooleanLiteral {
                loc: loc1,
                inner: lit1,
            },
            ExpressionInner::BooleanLiteral {
                loc: loc2,
                inner: lit2,
            },
        ) => diff_if_changed_ret_opt(|l1, l2| boolean_literal(loc1, loc2, l1, l2), lit1, lit2),
        (
            ExpressionInner::NullLiteral {
                loc: loc1,
                inner: lit1,
            },
            ExpressionInner::NullLiteral { inner: lit2, .. },
        ) => diff_if_changed_ret_opt(|l1, l2| syntax_opt(loc1, l1, l2), lit1, lit2),
        (
            ExpressionInner::NumberLiteral {
                loc: loc1,
                inner: lit1,
            },
            ExpressionInner::NumberLiteral {
                loc: loc2,
                inner: lit2,
            },
        ) => diff_if_changed_ret_opt(|l1, l2| number_literal(loc1, loc2, l1, l2), lit1, lit2),
        (
            ExpressionInner::BigIntLiteral {
                loc: loc1,
                inner: lit1,
            },
            ExpressionInner::BigIntLiteral {
                loc: loc2,
                inner: lit2,
            },
        ) => diff_if_changed_ret_opt(|l1, l2| bigint_literal(loc1, loc2, l1, l2), lit1, lit2),
        (
            ExpressionInner::RegExpLiteral {
                loc: loc1,
                inner: lit1,
            },
            ExpressionInner::RegExpLiteral {
                loc: loc2,
                inner: lit2,
            },
        ) => diff_if_changed_ret_opt(|l1, l2| regexp_literal(loc1, loc2, l1, l2), lit1, lit2),
        (
            ExpressionInner::ModuleRefLiteral {
                loc: loc1,
                inner: lit1,
            },
            ExpressionInner::ModuleRefLiteral {
                loc: loc2,
                inner: lit2,
            },
        ) => diff_if_changed_ret_opt(|l1, l2| module_ref_literal(loc1, loc2, l1, l2), lit1, lit2),
        (
            ExpressionInner::Identifier { inner: id1, .. },
            ExpressionInner::Identifier { inner: id2, .. },
        ) => Some(identifier(id1, id2)),
        (
            ExpressionInner::ArrowFunction { loc, inner: f1 },
            ExpressionInner::ArrowFunction { inner: f2, .. },
        ) => function_(true, loc, f1, f2),
        (
            ExpressionInner::Function { loc, inner: f1 },
            ExpressionInner::Function { inner: f2, .. },
        ) => function_(false, loc, f1, f2),
        (ExpressionInner::Class { loc, inner: c1 }, ExpressionInner::Class { inner: c2, .. }) => {
            class_(loc, c1, c2)
        }
        (ExpressionInner::Binary { loc, inner: b1 }, ExpressionInner::Binary { inner: b2, .. }) => {
            binary(loc, b1, b2)
        }
        (ExpressionInner::Unary { loc, inner: u1 }, ExpressionInner::Unary { inner: u2, .. }) => {
            unary(loc, u1, u2)
        }
        (
            ExpressionInner::Conditional { loc, inner: c1 },
            ExpressionInner::Conditional { inner: c2, .. },
        ) => Some(conditional(loc, c1, c2)),
        (ExpressionInner::New { loc, inner: n1 }, ExpressionInner::New { inner: n2, .. }) => {
            new_(loc, n1, n2)
        }
        (ExpressionInner::Member { loc, inner: m1 }, ExpressionInner::Member { inner: m2, .. }) => {
            member(loc, m1, m2)
        }
        (ExpressionInner::Call { loc, inner: c1 }, ExpressionInner::Call { inner: c2, .. }) => {
            call(loc, c1, c2)
        }
        (
            ExpressionInner::Assignment { loc, inner: a1 },
            ExpressionInner::Assignment { inner: a2, .. },
        ) => assignment(loc, a1, a2),
        (ExpressionInner::Object { loc, inner: o1 }, ExpressionInner::Object { inner: o2, .. }) => {
            object_(loc, o1, o2)
        }
        (
            ExpressionInner::TaggedTemplate { loc, inner: t1 },
            ExpressionInner::TaggedTemplate { inner: t2, .. },
        ) => Some(tagged_template(loc, t1, t2)),
        (
            ExpressionInner::TemplateLiteral {
                loc: loc1,
                inner: t1,
            },
            ExpressionInner::TemplateLiteral {
                loc: loc2,
                inner: t2,
            },
        ) => Some(template_literal(loc1, loc2, t1, t2)),
        (
            ExpressionInner::JSXElement { loc, inner: j1 },
            ExpressionInner::JSXElement { inner: j2, .. },
        ) => jsx_element(loc, j1, j2),
        (
            ExpressionInner::JSXFragment { loc, inner: f1 },
            ExpressionInner::JSXFragment { inner: f2, .. },
        ) => jsx_fragment(loc, f1, f2),
        (
            ExpressionInner::TypeCast { loc, inner: t1 },
            ExpressionInner::TypeCast { inner: t2, .. },
        ) => Some(type_cast(loc, t1, t2)),
        (
            ExpressionInner::Logical { loc, inner: l1 },
            ExpressionInner::Logical { inner: l2, .. },
        ) => logical(loc, l1, l2),
        (ExpressionInner::Array { loc, inner: a1 }, ExpressionInner::Array { inner: a2, .. }) => {
            array(loc, a1, a2)
        }
        (
            ExpressionInner::AsExpression { loc, inner: a1 },
            ExpressionInner::AsExpression { inner: a2, .. },
        ) => Some(as_cast(loc, a1, a2)),
        (
            ExpressionInner::AsExpression { .. } | ExpressionInner::TSSatisfies { .. },
            ExpressionInner::TypeCast { .. },
        ) => None,
        (
            _,
            ExpressionInner::TypeCast {
                loc: loc2,
                inner: t2,
            },
        ) => Some(type_cast_added(parent, expr1, loc2, t2)),
        (ExpressionInner::Update { loc, inner: u1 }, ExpressionInner::Update { inner: u2, .. }) => {
            update(loc, u1, u2)
        }
        (
            ExpressionInner::Sequence { loc, inner: s1 },
            ExpressionInner::Sequence { inner: s2, .. },
        ) => sequence(loc, s1, s2),
        (ExpressionInner::This { loc, inner: t1 }, ExpressionInner::This { inner: t2, .. }) => {
            this_expression(loc, t1, t2)
        }
        (ExpressionInner::Super { loc, inner: s1 }, ExpressionInner::Super { inner: s2, .. }) => {
            super_expression(loc, s1, s2)
        }
        (
            ExpressionInner::MetaProperty { loc, inner: m1 },
            ExpressionInner::MetaProperty { inner: m2, .. },
        ) => meta_property(loc, m1, m2),
        (ExpressionInner::Import { loc, inner: i1 }, ExpressionInner::Import { inner: i2, .. }) => {
            import_expression(loc, i1, i2)
        }
        (ExpressionInner::Record { loc, inner: r1 }, ExpressionInner::Record { inner: r2, .. }) => {
            record(loc, r1, r2)
        }
        (ExpressionInner::Match { loc, inner: m1 }, ExpressionInner::Match { inner: m2, .. }) => {
            match_expression(loc, m1, m2)
        }
        // TODO: handle AsConstExpression, OptionalCall, OptionalMember, TSSatisfies
        // The catch all case where LHS and RHS are different AST nodes.
        // We keep this so that we don't forget to create handlers above when we add new AST nodes.
        _ => None,
    };
    let old_loc = expr1.loc();
    changes.unwrap_or_else(|| {
        vec![replace(
            old_loc,
            Node::Expression(expr1.dupe(), parent.clone()),
            Node::Expression(expr2.dupe(), parent.clone()),
        )]
    })
}

fn string_literal(
    loc1: &Loc,
    loc2: &Loc,
    lit1: &ast::StringLiteral<Loc>,
    lit2: &ast::StringLiteral<Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::StringLiteral {
        value: val1,
        raw: raw1,
        comments: comments1,
    } = lit1;
    let ast::StringLiteral {
        value: val2,
        raw: raw2,
        comments: comments2,
    } = lit2;
    let value_diff = if val1 == val2 && raw1 == raw2 {
        Some(vec![])
    } else {
        Some(vec![replace(
            loc1,
            Node::StringLiteral(loc1.dupe(), lit1.clone()),
            Node::StringLiteral(loc2.dupe(), lit2.clone()),
        )])
    };
    let comments_diff = syntax_opt(loc1, comments1, comments2);
    join_diff_list(vec![value_diff, comments_diff])
}

fn number_literal(
    loc1: &Loc,
    loc2: &Loc,
    lit1: &ast::NumberLiteral<Loc>,
    lit2: &ast::NumberLiteral<Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::NumberLiteral {
        value: value1,
        raw: raw1,
        comments: comments1,
    } = lit1;
    let ast::NumberLiteral {
        value: value2,
        raw: raw2,
        comments: comments2,
    } = lit2;
    let value_diff = if value1 == value2 && raw1 == raw2 {
        Some(vec![])
    } else {
        Some(vec![replace(
            loc1,
            Node::NumberLiteral(loc1.dupe(), lit1.clone()),
            Node::NumberLiteral(loc2.dupe(), lit2.clone()),
        )])
    };
    let comments_diff = syntax_opt(loc1, comments1, comments2);
    join_diff_list(vec![value_diff, comments_diff])
}

fn bigint_literal(
    loc1: &Loc,
    loc2: &Loc,
    lit1: &ast::BigIntLiteral<Loc>,
    lit2: &ast::BigIntLiteral<Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::BigIntLiteral {
        value: value1,
        raw: raw1,
        comments: comments1,
    } = lit1;
    let ast::BigIntLiteral {
        value: value2,
        raw: raw2,
        comments: comments2,
    } = lit2;
    let value_diff = if value1 == value2 && raw1 == raw2 {
        Some(vec![])
    } else {
        Some(vec![replace(
            loc1,
            Node::BigIntLiteral(loc1.dupe(), lit1.clone()),
            Node::BigIntLiteral(loc2.dupe(), lit2.clone()),
        )])
    };
    let comments_diff = syntax_opt(loc1, comments1, comments2);
    join_diff_list(vec![value_diff, comments_diff])
}

fn boolean_literal(
    loc1: &Loc,
    loc2: &Loc,
    lit1: &ast::BooleanLiteral<Loc>,
    lit2: &ast::BooleanLiteral<Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::BooleanLiteral {
        value: value1,
        comments: comments1,
    } = lit1;
    let ast::BooleanLiteral {
        value: value2,
        comments: comments2,
    } = lit2;
    let value_diff = if value1 == value2 {
        Some(vec![])
    } else {
        Some(vec![replace(
            loc1,
            Node::BooleanLiteral(loc1.dupe(), lit1.clone()),
            Node::BooleanLiteral(loc2.dupe(), lit2.clone()),
        )])
    };
    let comments_diff = syntax_opt(loc1, comments1, comments2);
    join_diff_list(vec![value_diff, comments_diff])
}

fn regexp_literal(
    loc1: &Loc,
    loc2: &Loc,
    lit1: &ast::RegExpLiteral<Loc>,
    lit2: &ast::RegExpLiteral<Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::RegExpLiteral {
        pattern: pattern1,
        flags: flags1,
        raw: raw1,
        comments: comments1,
    } = lit1;
    let ast::RegExpLiteral {
        pattern: pattern2,
        flags: flags2,
        raw: raw2,
        comments: comments2,
    } = lit2;
    let value_diff = if pattern1 == pattern2 && flags1 == flags2 && raw1 == raw2 {
        Some(vec![])
    } else {
        Some(vec![replace(
            loc1,
            Node::RegExpLiteral(loc1.dupe(), lit1.clone()),
            Node::RegExpLiteral(loc2.dupe(), lit2.clone()),
        )])
    };
    let comments_diff = syntax_opt(loc1, comments1, comments2);
    join_diff_list(vec![value_diff, comments_diff])
}

fn module_ref_literal(
    loc1: &Loc,
    loc2: &Loc,
    lit1: &ast::ModuleRefLiteral<Loc>,
    lit2: &ast::ModuleRefLiteral<Loc>,
) -> Option<Vec<NodeChange>> {
    Some(vec![replace(
        loc1,
        Node::ModuleRefLiteral(loc1.dupe(), lit1.clone()),
        Node::ModuleRefLiteral(loc2.dupe(), lit2.clone()),
    )])
}

fn tagged_template(
    loc: &Loc,
    t1: &expression::TaggedTemplate<Loc, Loc>,
    t2: &expression::TaggedTemplate<Loc, Loc>,
) -> Vec<NodeChange> {
    let expression::TaggedTemplate {
        tag: tag1,
        targs: targs1,
        quasi: (quasi_loc1, quasi1),
        comments: comments1,
    } = t1;
    let expression::TaggedTemplate {
        tag: tag2,
        targs: targs2,
        quasi: (quasi_loc2, quasi2),
        comments: comments2,
    } = t2;
    let parent = ExpressionNodeParent::ExpressionParentOfExpression(expression::Expression::new(
        ExpressionInner::TaggedTemplate {
            loc: loc.dupe(),
            inner: Arc::new(t2.clone()),
        },
    ));
    let tag_diff = diff_if_changed(|t1, t2| expression(&parent, t1, t2), tag1, tag2);
    let targs_diff = diff_if_changed_opt(call_type_args, targs1, targs2).unwrap_or_default();
    let quasi_diff = diff_if_changed(
        |q1, q2| template_literal(quasi_loc1, quasi_loc2, q1, q2),
        quasi1,
        quasi2,
    );
    let comments_diff = syntax_opt(loc, comments1, comments2).unwrap_or_default();
    let mut result = tag_diff;
    result.extend(targs_diff);
    result.extend(quasi_diff);
    result.extend(comments_diff);
    result
}

// Need to pass in locs because TemplateLiteral doesn't have a loc attached
fn template_literal(
    loc1: &Loc,
    loc2: &Loc,
    t_lit1: &expression::TemplateLiteral<Loc, Loc>,
    t_lit2: &expression::TemplateLiteral<Loc, Loc>,
) -> Vec<NodeChange> {
    let expression::TemplateLiteral {
        quasis: quasis1,
        expressions: exprs1,
        comments: comments1,
    } = t_lit1;
    let expression::TemplateLiteral {
        quasis: quasis2,
        expressions: exprs2,
        comments: comments2,
    } = t_lit2;
    let quasis_diff =
        diff_and_recurse_no_trivial(&|q1, q2| template_literal_element(q1, q2), quasis1, quasis2);
    let parent = ExpressionNodeParent::ExpressionParentOfExpression(expression::Expression::new(
        ExpressionInner::TemplateLiteral {
            loc: loc1.dupe(),
            inner: Arc::new(t_lit2.clone()),
        },
    ));
    let exprs_diff =
        diff_and_recurse_nonopt_no_trivial(&|e1, e2| expression(&parent, e1, e2), exprs1, exprs2);
    let comments_diff = syntax_opt(loc1, comments1, comments2);
    let result = join_diff_list(vec![quasis_diff, exprs_diff, comments_diff]);
    result.unwrap_or_else(|| {
        vec![replace(
            loc1,
            Node::TemplateLiteral(loc1.dupe(), t_lit1.clone()),
            Node::TemplateLiteral(loc2.dupe(), t_lit2.clone()),
        )]
    })
}

fn template_literal_element(
    tl_elem1: &expression::template_literal::Element<Loc>,
    tl_elem2: &expression::template_literal::Element<Loc>,
) -> Option<Vec<NodeChange>> {
    let expression::template_literal::Element {
        value: value1,
        tail: tail1,
        ..
    } = tl_elem1;
    let expression::template_literal::Element {
        value: value2,
        tail: tail2,
        ..
    } = tl_elem2;
    // These are primitives, so structural equality is fine
    if value1.raw != value2.raw || value1.cooked != value2.cooked || tail1 != tail2 {
        None
    } else {
        Some(vec![])
    }
}

fn jsx_element(
    loc: &Loc,
    elem1: &ast::jsx::Element<Loc, Loc>,
    elem2: &ast::jsx::Element<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::jsx::Element {
        opening_element: open_elem1,
        closing_element: close_elem1,
        children: (_, children1),
        comments: comments1,
    } = elem1;
    let ast::jsx::Element {
        opening_element: open_elem2,
        closing_element: close_elem2,
        children: (_, children2),
        comments: comments2,
    } = elem2;
    let opening_diff = diff_if_changed_ret_opt(jsx_opening_element, open_elem1, open_elem2);
    let children_diff =
        diff_and_recurse_nonopt_no_trivial(&|c1, c2| jsx_child(c1, c2), children1, children2);
    let closing_diff = diff_if_changed_opt(jsx_closing_element, close_elem1, close_elem2);
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![
        opening_diff,
        children_diff,
        closing_diff,
        comments_diff,
    ])
}

fn jsx_fragment(
    loc: &Loc,
    frag1: &ast::jsx::Fragment<Loc, Loc>,
    frag2: &ast::jsx::Fragment<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    // Opening and closing elements contain no information besides loc, so we
    // ignore them for the diff
    let ast::jsx::Fragment {
        frag_children: (_, children1),
        frag_comments: frag_comments1,
        ..
    } = frag1;
    let ast::jsx::Fragment {
        frag_children: (_, children2),
        frag_comments: frag_comments2,
        ..
    } = frag2;
    let children_diff =
        diff_and_recurse_nonopt_no_trivial(&|c1, c2| jsx_child(c1, c2), children1, children2);
    let frag_comments_diff = syntax_opt(loc, frag_comments1, frag_comments2);
    join_diff_list(vec![children_diff, frag_comments_diff])
}

fn jsx_opening_element(
    elem1: &ast::jsx::Opening<Loc, Loc>,
    elem2: &ast::jsx::Opening<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::jsx::Opening {
        name: name1,
        targs: targs1,
        self_closing: self_close1,
        attributes: attrs1,
        ..
    } = elem1;
    let ast::jsx::Opening {
        name: name2,
        targs: targs2,
        self_closing: self_close2,
        attributes: attrs2,
        ..
    } = elem2;
    if self_close1 != self_close2 {
        return None;
    }
    let name_diff = diff_if_changed_ret_opt(jsx_element_name, name1, name2);
    let targs_diff = diff_if_changed_ret_opt(
        |t1, t2| diff_if_changed_opt(call_type_args, t1, t2),
        targs1,
        targs2,
    );
    let attrs_diff =
        diff_and_recurse_no_trivial(&|a1, a2| jsx_opening_attribute(a1, a2), attrs1, attrs2);
    join_diff_list(vec![name_diff, targs_diff, attrs_diff])
}

fn jsx_element_name(
    name1: &ast::jsx::Name<Loc, Loc>,
    name2: &ast::jsx::Name<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    match (name1, name2) {
        (ast::jsx::Name::Identifier(id1), ast::jsx::Name::Identifier(id2)) => {
            Some(diff_if_changed(jsx_identifier, id1, id2))
        }
        (ast::jsx::Name::NamespacedName(ns1), ast::jsx::Name::NamespacedName(ns2)) => {
            Some(diff_if_changed(jsx_namespaced_name, ns1, ns2))
        }
        (ast::jsx::Name::MemberExpression(me1), ast::jsx::Name::MemberExpression(me2)) => {
            diff_if_changed_ret_opt(jsx_member_expression, me1, me2)
        }
        _ => None,
    }
}

fn jsx_identifier(
    id1: &ast::jsx::Identifier<Loc, Loc>,
    id2: &ast::jsx::Identifier<Loc, Loc>,
) -> Vec<NodeChange> {
    let ast::jsx::Identifier {
        loc: old_loc,
        name: name1,
        comments: comments1,
    } = id1;
    let ast::jsx::Identifier {
        name: name2,
        comments: comments2,
        ..
    } = id2;
    let name_diff = if name1 == name2 {
        vec![]
    } else {
        vec![replace(
            old_loc,
            Node::JSXIdentifier(id1.clone()),
            Node::JSXIdentifier(id2.clone()),
        )]
    };
    let comments_diff = syntax_opt(old_loc, comments1, comments2).unwrap_or_default();
    let mut result = name_diff;
    result.extend(comments_diff);
    result
}

fn jsx_namespaced_name(
    ns1: &ast::jsx::NamespacedName<Loc, Loc>,
    ns2: &ast::jsx::NamespacedName<Loc, Loc>,
) -> Vec<NodeChange> {
    let ast::jsx::NamespacedName {
        namespace: namespace1,
        name: name1,
        ..
    } = ns1;
    let ast::jsx::NamespacedName {
        namespace: namespace2,
        name: name2,
        ..
    } = ns2;
    let namespace_diff = diff_if_changed(jsx_identifier, namespace1, namespace2);
    let name_diff = diff_if_changed(jsx_identifier, name1, name2);
    let mut result = namespace_diff;
    result.extend(name_diff);
    result
}

fn jsx_member_expression(
    me1: &ast::jsx::MemberExpression<Loc, Loc>,
    me2: &ast::jsx::MemberExpression<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::jsx::MemberExpression {
        object: object1,
        property: prop1,
        ..
    } = me1;
    let ast::jsx::MemberExpression {
        object: object2,
        property: prop2,
        ..
    } = me2;
    let obj_diff = match (object1, object2) {
        (
            ast::jsx::member_expression::Object::Identifier(id1),
            ast::jsx::member_expression::Object::Identifier(id2),
        ) => Some(diff_if_changed(jsx_identifier, id1, id2)),
        (
            ast::jsx::member_expression::Object::MemberExpression(me1),
            ast::jsx::member_expression::Object::MemberExpression(me2),
        ) => diff_if_changed_ret_opt(jsx_member_expression, me1.as_ref(), me2.as_ref()),
        _ => None,
    };
    let prop_diff = Some(diff_if_changed(jsx_identifier, prop1, prop2));
    join_diff_list(vec![obj_diff, prop_diff])
}

fn jsx_closing_element(
    elem1: &ast::jsx::Closing<Loc, Loc>,
    elem2: &ast::jsx::Closing<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::jsx::Closing { name: name1, .. } = elem1;
    let ast::jsx::Closing { name: name2, .. } = elem2;
    diff_if_changed_ret_opt(jsx_element_name, name1, name2)
}

fn jsx_opening_attribute(
    attr1: &ast::jsx::OpeningAttribute<Loc, Loc>,
    attr2: &ast::jsx::OpeningAttribute<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    match (attr1, attr2) {
        (ast::jsx::OpeningAttribute::Attribute(a1), ast::jsx::OpeningAttribute::Attribute(a2)) => {
            diff_if_changed_ret_opt(jsx_attribute, a1, a2)
        }
        (
            ast::jsx::OpeningAttribute::SpreadAttribute(s1),
            ast::jsx::OpeningAttribute::SpreadAttribute(s2),
        ) => {
            let loc = &s1.loc;
            diff_if_changed_ret_opt(|a1, a2| jsx_spread_attribute(loc, a1, a2), s1, s2)
        }
        _ => None,
    }
}

fn jsx_spread_attribute(
    loc: &Loc,
    attr1: &ast::jsx::SpreadAttribute<Loc, Loc>,
    attr2: &ast::jsx::SpreadAttribute<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::jsx::SpreadAttribute {
        argument: arg1,
        comments: comments1,
        ..
    } = attr1;
    let ast::jsx::SpreadAttribute {
        argument: arg2,
        comments: comments2,
        ..
    } = attr2;
    let argument_diff = Some(diff_if_changed(
        |e1, e2| expression(&ExpressionNodeParent::SpreadParentOfExpression, e1, e2),
        arg1,
        arg2,
    ));
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![argument_diff, comments_diff])
}

fn jsx_attribute(
    attr1: &ast::jsx::Attribute<Loc, Loc>,
    attr2: &ast::jsx::Attribute<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::jsx::Attribute {
        name: name1,
        value: value1,
        ..
    } = attr1;
    let ast::jsx::Attribute {
        name: name2,
        value: value2,
        ..
    } = attr2;
    let name_diff = match (name1, name2) {
        (
            ast::jsx::attribute::Name::Identifier(id1),
            ast::jsx::attribute::Name::Identifier(id2),
        ) => Some(diff_if_changed(jsx_identifier, id1, id2)),
        (
            ast::jsx::attribute::Name::NamespacedName(ns1),
            ast::jsx::attribute::Name::NamespacedName(ns2),
        ) => Some(diff_if_changed(jsx_namespaced_name, ns1, ns2)),
        _ => None,
    };
    let value_diff = match (value1, value2) {
        (
            Some(ast::jsx::attribute::Value::StringLiteral((loc1, lit1))),
            Some(ast::jsx::attribute::Value::StringLiteral((loc2, lit2))),
        ) => diff_if_changed_ret_opt(|l1, l2| string_literal(loc1, loc2, l1, l2), lit1, lit2),
        (
            Some(ast::jsx::attribute::Value::ExpressionContainer((loc, expr1))),
            Some(ast::jsx::attribute::Value::ExpressionContainer((_, expr2))),
        ) => diff_if_changed_ret_opt(|e1, e2| jsx_expression(loc, e1, e2), expr1, expr2),
        _ => None,
    };
    join_diff_list(vec![name_diff, value_diff])
}

fn jsx_child(
    child1: &ast::jsx::Child<Loc, Loc>,
    child2: &ast::jsx::Child<Loc, Loc>,
) -> Vec<NodeChange> {
    let old_loc = child1.loc();
    if child1 == child2 {
        return vec![];
    }
    let changes = match (child1, child2) {
        (
            ast::jsx::Child::Element { inner: elem1, .. },
            ast::jsx::Child::Element { inner: elem2, .. },
        ) => diff_if_changed_ret_opt(|e1, e2| jsx_element(old_loc, e1, e2), elem1, elem2),
        (
            ast::jsx::Child::Fragment { inner: frag1, .. },
            ast::jsx::Child::Fragment { inner: frag2, .. },
        ) => diff_if_changed_ret_opt(|f1, f2| jsx_fragment(old_loc, f1, f2), frag1, frag2),
        (
            ast::jsx::Child::ExpressionContainer { inner: expr1, .. },
            ast::jsx::Child::ExpressionContainer { inner: expr2, .. },
        ) => diff_if_changed_ret_opt(|e1, e2| jsx_expression(old_loc, e1, e2), expr1, expr2),
        (
            ast::jsx::Child::SpreadChild { inner: spread1, .. },
            ast::jsx::Child::SpreadChild { inner: spread2, .. },
        ) => diff_if_changed_ret_opt(|s1, s2| jsx_spread_child(old_loc, s1, s2), spread1, spread2),
        (ast::jsx::Child::Text { .. }, ast::jsx::Child::Text { .. }) => None,
        _ => None,
    };
    changes.unwrap_or_else(|| {
        vec![replace(
            old_loc,
            Node::JSXChild(child1.clone()),
            Node::JSXChild(child2.clone()),
        )]
    })
}

fn jsx_expression(
    loc: &Loc,
    expr1: &ast::jsx::ExpressionContainer<Loc, Loc>,
    expr2: &ast::jsx::ExpressionContainer<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::jsx::ExpressionContainer {
        expression: jsx_expr1,
        comments: comments1,
    } = expr1;
    let ast::jsx::ExpressionContainer {
        expression: jsx_expr2,
        comments: comments2,
    } = expr2;
    let comments_diff = syntax_opt(loc, comments1, comments2);
    let expression_diff = match (jsx_expr1, jsx_expr2) {
        (
            ast::jsx::expression_container::Expression::Expression(e1),
            ast::jsx::expression_container::Expression::Expression(e2),
        ) => Some(diff_if_changed(
            |e1, e2| expression(&ExpressionNodeParent::SlotParentOfExpression, e1, e2),
            e1,
            e2,
        )),
        (
            ast::jsx::expression_container::Expression::EmptyExpression,
            ast::jsx::expression_container::Expression::EmptyExpression,
        ) => Some(vec![]),
        _ => None,
    };
    join_diff_list(vec![expression_diff, comments_diff])
}

fn jsx_spread_child(
    loc: &Loc,
    spread1: &ast::jsx::SpreadChild<Loc, Loc>,
    spread2: &ast::jsx::SpreadChild<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::jsx::SpreadChild {
        expression: expr1,
        comments: comments1,
        ..
    } = spread1;
    let ast::jsx::SpreadChild {
        expression: expr2,
        comments: comments2,
        ..
    } = spread2;
    let expression_diff = Some(diff_if_changed(
        |e1, e2| expression(&ExpressionNodeParent::SpreadParentOfExpression, e1, e2),
        expr1,
        expr2,
    ));
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![expression_diff, comments_diff])
}

fn assignment(
    loc: &Loc,
    assn1: &expression::Assignment<Loc, Loc>,
    assn2: &expression::Assignment<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let expression::Assignment {
        operator: op1,
        left: pat1,
        right: exp1,
        comments: comments1,
    } = assn1;
    let expression::Assignment {
        operator: op2,
        left: pat2,
        right: exp2,
        comments: comments2,
    } = assn2;
    if op1 != op2 {
        return None;
    }
    let pat_diff = diff_if_changed(pattern, pat1, pat2);
    let parent = ExpressionNodeParent::ExpressionParentOfExpression(expression::Expression::new(
        ExpressionInner::Assignment {
            loc: loc.dupe(),
            inner: Arc::new(assn2.clone()),
        },
    ));
    let exp_diff = diff_if_changed(|e1, e2| expression(&parent, e1, e2), exp1, exp2);
    let comments_diff = syntax_opt(loc, comments1, comments2).unwrap_or_default();
    let mut result = pat_diff;
    result.extend(exp_diff);
    result.extend(comments_diff);
    Some(result)
}

fn object_spread_property(
    loc: &Loc,
    prop1: &expression::object::SpreadProperty<Loc, Loc>,
    prop2: &expression::object::SpreadProperty<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let expression::object::SpreadProperty {
        argument: arg1,
        comments: comments1,
        ..
    } = prop1;
    let expression::object::SpreadProperty {
        argument: arg2,
        comments: comments2,
        ..
    } = prop2;
    let argument_diff = Some(diff_if_changed(
        |a1, a2| expression(&ExpressionNodeParent::SpreadParentOfExpression, a1, a2),
        arg1,
        arg2,
    ));
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![argument_diff, comments_diff])
}

fn object_key(
    key1: &expression::object::Key<Loc, Loc>,
    key2: &expression::object::Key<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use expression::object::Key;
    match (key1, key2) {
        (Key::StringLiteral((loc1, l1)), Key::StringLiteral((loc2, l2))) => {
            diff_if_changed_ret_opt(|l1, l2| string_literal(loc1, loc2, l1, l2), l1, l2)
        }
        (Key::NumberLiteral((loc1, l1)), Key::NumberLiteral((loc2, l2))) => {
            diff_if_changed_ret_opt(|l1, l2| number_literal(loc1, loc2, l1, l2), l1, l2)
        }
        (Key::BigIntLiteral((loc1, l1)), Key::BigIntLiteral((loc2, l2))) => {
            diff_if_changed_ret_opt(|l1, l2| bigint_literal(loc1, loc2, l1, l2), l1, l2)
        }
        (Key::Identifier(i1), Key::Identifier(i2)) => Some(diff_if_changed(identifier, i1, i2)),
        (Key::Computed(c1), Key::Computed(c2)) => computed_key(&c1.loc, c1, c2),
        _ => None,
    }
}

fn object_regular_property(
    prop1: &expression::object::NormalProperty<Loc, Loc>,
    prop2: &expression::object::NormalProperty<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use expression::object::NormalProperty;
    match (prop1, prop2) {
        (
            NormalProperty::Init {
                shorthand: sh1,
                value: val1,
                key: key1,
                ..
            },
            NormalProperty::Init {
                shorthand: sh2,
                value: val2,
                key: key2,
                ..
            },
        ) => {
            if sh1 != sh2 {
                return None;
            }
            let values = Some(diff_if_changed(
                |v1, v2| expression(&ExpressionNodeParent::SlotParentOfExpression, v1, v2),
                val1,
                val2,
            ));
            let keys = diff_if_changed_ret_opt(object_key, key1, key2);
            join_diff_list(vec![keys, values])
        }
        (
            NormalProperty::Method {
                value: val1,
                key: key1,
                ..
            },
            NormalProperty::Method {
                value: val2,
                key: key2,
                ..
            },
        ) => {
            let values = diff_if_changed_ret_opt(
                |v1, v2| function_(false, &val1.0, v1, v2),
                &val1.1,
                &val2.1,
            );
            let keys = diff_if_changed_ret_opt(object_key, key1, key2);
            join_diff_list(vec![keys, values])
        }
        (
            NormalProperty::Get {
                loc,
                value: val1,
                key: key1,
                comments: comments1,
            },
            NormalProperty::Get {
                value: val2,
                key: key2,
                comments: comments2,
                ..
            },
        )
        | (
            NormalProperty::Set {
                loc,
                value: val1,
                key: key1,
                comments: comments1,
            },
            NormalProperty::Set {
                value: val2,
                key: key2,
                comments: comments2,
                ..
            },
        ) => {
            let key_diff = diff_if_changed_ret_opt(object_key, key1, key2);
            let value_diff = diff_if_changed_ret_opt(
                |v1, v2| function_(false, &val1.0, v1, v2),
                &val1.1,
                &val2.1,
            );
            let comments_diff = syntax_opt(loc, comments1, comments2);
            join_diff_list(vec![key_diff, value_diff, comments_diff])
        }
        _ => None,
    }
}

fn object_property(
    prop1: &expression::object::Property<Loc, Loc>,
    prop2: &expression::object::Property<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use expression::object::Property;
    match (prop1, prop2) {
        (Property::NormalProperty(p1), Property::NormalProperty(p2)) => {
            let loc = p1.loc();
            Some(object_regular_property(p1, p2).unwrap_or_else(|| {
                vec![replace(
                    loc,
                    Node::ObjectProperty(prop1.clone()),
                    Node::ObjectProperty(prop2.clone()),
                )]
            }))
        }
        (Property::SpreadProperty(s1), Property::SpreadProperty(s2)) => {
            object_spread_property(&s1.loc, s1, s2)
        }
        _ => None,
    }
}

fn object_(
    loc: &Loc,
    obj1: &expression::Object<Loc, Loc>,
    obj2: &expression::Object<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let expression::Object {
        properties: properties1,
        comments: comments1,
    } = obj1;
    let expression::Object {
        properties: properties2,
        comments: comments2,
    } = obj2;
    let comments = syntax_opt(loc, comments1, comments2);
    let properties =
        diff_and_recurse_no_trivial(&|p1, p2| object_property(p1, p2), properties1, properties2);
    join_diff_list(vec![comments, properties])
}

fn record(
    loc: &Loc,
    rec1: &expression::Record<Loc, Loc>,
    rec2: &expression::Record<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let parent = ExpressionNodeParent::ExpressionParentOfExpression(expression::Expression::new(
        ExpressionInner::Record {
            loc: loc.dupe(),
            inner: Arc::new(rec2.clone()),
        },
    ));
    let constructor_diff = Some(diff_if_changed(
        |c1, c2| expression(&parent, c1, c2),
        &rec1.constructor,
        &rec2.constructor,
    ));
    let targs_diff = diff_if_changed_opt(call_type_args, &rec1.targs, &rec2.targs);
    let properties_diff = diff_if_changed_ret_opt(
        |p1, p2| object_(&rec1.properties.0, p1, p2),
        &rec1.properties.1,
        &rec2.properties.1,
    );
    let comments_diff = syntax_opt(loc, &rec1.comments, &rec2.comments);
    join_diff_list(vec![
        constructor_diff,
        targs_diff,
        properties_diff,
        comments_diff,
    ])
}

fn binary(
    loc: &Loc,
    b1: &expression::Binary<Loc, Loc>,
    b2: &expression::Binary<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let expression::Binary {
        operator: op1,
        left: left1,
        right: right1,
        comments: comments1,
    } = b1;
    let expression::Binary {
        operator: op2,
        left: left2,
        right: right2,
        comments: comments2,
    } = b2;
    if op1 != op2 {
        return None;
    }
    let parent = ExpressionNodeParent::ExpressionParentOfExpression(expression::Expression::new(
        ExpressionInner::Binary {
            loc: loc.dupe(),
            inner: Arc::new(b2.clone()),
        },
    ));
    let left_diff = diff_if_changed(|l1, l2| expression(&parent, l1, l2), left1, left2);
    let right_diff = diff_if_changed(|r1, r2| expression(&parent, r1, r2), right1, right2);
    let comments_diff = syntax_opt(loc, comments1, comments2).unwrap_or_default();
    let mut result = left_diff;
    result.extend(right_diff);
    result.extend(comments_diff);
    Some(result)
}

fn unary(
    loc: &Loc,
    u1: &expression::Unary<Loc, Loc>,
    u2: &expression::Unary<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let expression::Unary {
        operator: op1,
        argument: arg1,
        comments: comments1,
    } = u1;
    let expression::Unary {
        operator: op2,
        argument: arg2,
        comments: comments2,
    } = u2;
    let comments = syntax_opt(loc, comments1, comments2).unwrap_or_default();
    if op1 != op2 {
        return None;
    }
    let parent = ExpressionNodeParent::ExpressionParentOfExpression(expression::Expression::new(
        ExpressionInner::Unary {
            loc: loc.dupe(),
            inner: Arc::new(u2.clone()),
        },
    ));
    let arg_diff = expression(&parent, arg1, arg2);
    let mut result = comments;
    result.extend(arg_diff);
    Some(result)
}

fn identifier(id1: &ast::Identifier<Loc, Loc>, id2: &ast::Identifier<Loc, Loc>) -> Vec<NodeChange> {
    let old_loc = &id1.loc;
    let name1 = &id1.name;
    let comments1 = &id1.comments;
    let name2 = &id2.name;
    let comments2 = &id2.comments;
    let name = if name1 == name2 {
        vec![]
    } else {
        vec![replace(
            old_loc,
            Node::Raw(name1.to_string()),
            Node::Raw(name2.to_string()),
        )]
    };
    let comments = syntax_opt(old_loc, comments1, comments2).unwrap_or_default();
    let mut result = comments;
    result.extend(name);
    result
}

fn conditional(
    loc: &Loc,
    c1: &expression::Conditional<Loc, Loc>,
    c2: &expression::Conditional<Loc, Loc>,
) -> Vec<NodeChange> {
    let expression::Conditional {
        test: test1,
        consequent: cons1,
        alternate: alt1,
        comments: comments1,
    } = c1;
    let expression::Conditional {
        test: test2,
        consequent: cons2,
        alternate: alt2,
        comments: comments2,
    } = c2;
    let parent = ExpressionNodeParent::ExpressionParentOfExpression(expression::Expression::new(
        ExpressionInner::Conditional {
            loc: loc.dupe(),
            inner: Arc::new(c2.clone()),
        },
    ));
    let test_diff = diff_if_changed(|t1, t2| expression(&parent, t1, t2), test1, test2);
    let cons_diff = diff_if_changed(|c1, c2| expression(&parent, c1, c2), cons1, cons2);
    let alt_diff = diff_if_changed(|a1, a2| expression(&parent, a1, a2), alt1, alt2);
    let comments_diff = syntax_opt(loc, comments1, comments2).unwrap_or_default();
    let mut result = test_diff;
    result.extend(cons_diff);
    result.extend(alt_diff);
    result.extend(comments_diff);
    result
}

fn new_(
    loc: &Loc,
    new1: &expression::New<Loc, Loc>,
    new2: &expression::New<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let expression::New {
        callee: callee1,
        targs: targs1,
        arguments: arguments1,
        comments: comments1,
    } = new1;
    let expression::New {
        callee: callee2,
        targs: targs2,
        arguments: arguments2,
        comments: comments2,
    } = new2;
    let comments = syntax_opt(loc, comments1, comments2);
    let targs = diff_if_changed_ret_opt(
        |t1, t2| diff_if_changed_opt(call_type_args, t1, t2),
        targs1,
        targs2,
    );
    let args = diff_if_changed_ret_opt(
        |a1, a2| diff_if_changed_opt(call_args, a1, a2),
        arguments1,
        arguments2,
    );
    let parent = ExpressionNodeParent::ExpressionParentOfExpression(expression::Expression::new(
        ExpressionInner::New {
            loc: loc.dupe(),
            inner: Arc::new(new2.clone()),
        },
    ));
    let callee = Some(diff_if_changed(
        |c1, c2| expression(&parent, c1, c2),
        callee1,
        callee2,
    ));
    join_diff_list(vec![comments, targs, args, callee])
}

fn member(
    loc: &Loc,
    member1: &expression::Member<Loc, Loc>,
    member2: &expression::Member<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let expression::Member {
        object: obj1,
        property: prop1,
        comments: comments1,
    } = member1;
    let expression::Member {
        object: obj2,
        property: prop2,
        comments: comments2,
    } = member2;
    let parent = ExpressionNodeParent::ExpressionParentOfExpression(expression::Expression::new(
        ExpressionInner::Member {
            loc: loc.dupe(),
            inner: Arc::new(member2.clone()),
        },
    ));
    let obj = Some(diff_if_changed(
        |o1, o2| expression(&parent, o1, o2),
        obj1,
        obj2,
    ));
    let prop = diff_if_changed_ret_opt(member_property, prop1, prop2);
    let comments = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![obj, prop, comments])
}

fn member_property(
    prop1: &expression::member::Property<Loc, Loc>,
    prop2: &expression::member::Property<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use expression::member::Property;
    match (prop1, prop2) {
        (Property::PropertyExpression(exp1), Property::PropertyExpression(exp2)) => {
            Some(diff_if_changed(
                |e1, e2| expression(&ExpressionNodeParent::SlotParentOfExpression, e1, e2),
                exp1,
                exp2,
            ))
        }
        (Property::PropertyIdentifier(id1), Property::PropertyIdentifier(id2)) => {
            Some(diff_if_changed(identifier, id1, id2))
        }
        (Property::PropertyPrivateName(pn1), Property::PropertyPrivateName(pn2)) => Some(
            diff_if_changed(|n1, n2| private_name(&pn1.loc, n1, n2), pn1, pn2),
        ),
        _ => None,
    }
}

fn call(
    loc: &Loc,
    call1: &expression::Call<Loc, Loc>,
    call2: &expression::Call<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let expression::Call {
        callee: callee1,
        targs: targs1,
        arguments: arguments1,
        comments: comments1,
    } = call1;
    let expression::Call {
        callee: callee2,
        targs: targs2,
        arguments: arguments2,
        comments: comments2,
    } = call2;
    let targs = diff_if_changed_ret_opt(
        |t1, t2| diff_if_changed_opt(call_type_args, t1, t2),
        targs1,
        targs2,
    );
    let args = diff_if_changed_ret_opt(call_args, arguments1, arguments2);
    let parent = ExpressionNodeParent::ExpressionParentOfExpression(expression::Expression::new(
        ExpressionInner::Call {
            loc: loc.dupe(),
            inner: Arc::new(call2.clone()),
        },
    ));
    let callee = Some(diff_if_changed(
        |c1, c2| expression(&parent, c1, c2),
        callee1,
        callee2,
    ));
    let comments = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![targs, args, callee, comments])
}

fn call_type_arg(
    t1: &expression::CallTypeArg<Loc, Loc>,
    t2: &expression::CallTypeArg<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use expression::CallTypeArg;
    match (t1, t2) {
        (CallTypeArg::Explicit(type1), CallTypeArg::Explicit(type2)) => {
            Some(diff_if_changed(type_, type1, type2))
        }
        (CallTypeArg::Implicit(imp1), CallTypeArg::Implicit(imp2)) => {
            syntax_opt(&imp1.loc, &imp1.comments, &imp2.comments)
        }
        _ => None,
    }
}

fn call_type_args(
    pi1: &expression::CallTypeArgs<Loc, Loc>,
    pi2: &expression::CallTypeArgs<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let expression::CallTypeArgs {
        loc,
        arguments: arguments1,
        comments: comments1,
    } = pi1;
    let expression::CallTypeArgs {
        arguments: arguments2,
        comments: comments2,
        ..
    } = pi2;
    let args_diff =
        diff_and_recurse_no_trivial(&|a1, a2| call_type_arg(a1, a2), arguments1, arguments2);
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![args_diff, comments_diff])
}

fn call_args(
    args1: &expression::ArgList<Loc, Loc>,
    args2: &expression::ArgList<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let expression::ArgList {
        loc,
        arguments: arguments1,
        comments: comments1,
    } = args1;
    let expression::ArgList {
        arguments: arguments2,
        comments: comments2,
        ..
    } = args2;
    let args_diff = diff_and_recurse_no_trivial(
        &|a1, a2| expression_or_spread(a1, a2),
        arguments1,
        arguments2,
    );
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![args_diff, comments_diff])
}

fn expression_or_spread(
    expr1: &expression::ExpressionOrSpread<Loc, Loc>,
    expr2: &expression::ExpressionOrSpread<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use expression::ExpressionOrSpread;
    match (expr1, expr2) {
        (ExpressionOrSpread::Expression(e1), ExpressionOrSpread::Expression(e2)) => {
            Some(diff_if_changed(
                |e1, e2| expression(&ExpressionNodeParent::SlotParentOfExpression, e1, e2),
                e1,
                e2,
            ))
        }
        (ExpressionOrSpread::Spread(s1), ExpressionOrSpread::Spread(s2)) => {
            diff_if_changed_ret_opt(spread_element, s1, s2)
        }
        _ => None,
    }
}

fn array_element(
    element1: &expression::ArrayElement<Loc, Loc>,
    element2: &expression::ArrayElement<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use expression::ArrayElement;
    match (element1, element2) {
        (ArrayElement::Hole(_), ArrayElement::Hole(_)) => Some(vec![]),
        (ArrayElement::Expression(e1), ArrayElement::Expression(e2)) => Some(diff_if_changed(
            |e1, e2| expression(&ExpressionNodeParent::SlotParentOfExpression, e1, e2),
            e1,
            e2,
        )),
        (ArrayElement::Spread(s1), ArrayElement::Spread(s2)) => {
            diff_if_changed_ret_opt(spread_element, s1, s2)
        }
        _ => None,
    }
}

fn spread_element(
    spread1: &expression::SpreadElement<Loc, Loc>,
    spread2: &expression::SpreadElement<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let expression::SpreadElement {
        loc,
        argument: arg1,
        comments: comments1,
    } = spread1;
    let expression::SpreadElement {
        argument: arg2,
        comments: comments2,
        ..
    } = spread2;
    let argument_diff = Some(diff_if_changed(
        |a1, a2| expression(&ExpressionNodeParent::SpreadParentOfExpression, a1, a2),
        arg1,
        arg2,
    ));
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![argument_diff, comments_diff])
}

fn logical(
    loc: &Loc,
    l1: &expression::Logical<Loc, Loc>,
    l2: &expression::Logical<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let expression::Logical {
        left: left1,
        right: right1,
        operator: operator1,
        comments: comments1,
    } = l1;
    let expression::Logical {
        left: left2,
        right: right2,
        operator: operator2,
        comments: comments2,
    } = l2;
    if operator1 == operator2 {
        let parent = ExpressionNodeParent::ExpressionParentOfExpression(
            expression::Expression::new(ExpressionInner::Logical {
                loc: loc.dupe(),
                inner: Arc::new(l2.clone()),
            }),
        );
        let left = diff_if_changed(|l1, l2| expression(&parent, l1, l2), left1, left2);
        let right = diff_if_changed(|r1, r2| expression(&parent, r1, r2), right1, right2);
        let comments = syntax_opt(loc, comments1, comments2).unwrap_or_default();
        let mut result = left;
        result.extend(right);
        result.extend(comments);
        Some(result)
    } else {
        None
    }
}

fn array(
    loc: &Loc,
    arr1: &expression::Array<Loc, Loc>,
    arr2: &expression::Array<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let expression::Array {
        elements: elems1,
        trailing_comma: _,
        comments: comments1,
    } = arr1;
    let expression::Array {
        elements: elems2,
        trailing_comma: _,
        comments: comments2,
    } = arr2;
    let comments = syntax_opt(loc, comments1, comments2);
    let elements = diff_and_recurse_no_trivial(&|e1, e2| array_element(e1, e2), elems1, elems2);
    join_diff_list(vec![comments, elements])
}

fn sequence(
    loc: &Loc,
    seq1: &expression::Sequence<Loc, Loc>,
    seq2: &expression::Sequence<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let expression::Sequence {
        expressions: exps1,
        comments: comments1,
    } = seq1;
    let expression::Sequence {
        expressions: exps2,
        comments: comments2,
    } = seq2;
    let parent = ExpressionNodeParent::ExpressionParentOfExpression(expression::Expression::new(
        ExpressionInner::Sequence {
            loc: loc.dupe(),
            inner: Arc::new(seq2.clone()),
        },
    ));
    let expressions_diff =
        diff_and_recurse_nonopt_no_trivial(&|e1, e2| expression(&parent, e1, e2), exps1, exps2);
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![expressions_diff, comments_diff])
}

fn for_statement(
    loc: &Loc,
    stmt2: &statement::Statement<Loc, Loc>,
    for1: &ast::statement::For<Loc, Loc>,
    for2: &ast::statement::For<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let init = diff_if_changed_opt(for_statement_init, &for1.init, &for2.init);
    let parent = ExpressionNodeParent::StatementParentOfExpression(stmt2.dupe());
    let test =
        diff_if_changed_nonopt_fn(|t1, t2| expression(&parent, t1, t2), &for1.test, &for2.test);
    let update = diff_if_changed_nonopt_fn(
        |u1, u2| expression(&parent, u1, u2),
        &for1.update,
        &for2.update,
    );
    let body = Some(diff_if_changed(
        |b1, b2| {
            statement(
                &StatementNodeParent::LoopParentOfStatement(loc.dupe()),
                b1,
                b2,
            )
        },
        &for1.body,
        &for2.body,
    ));
    let comments = syntax_opt(loc, &for1.comments, &for2.comments);
    join_diff_list(vec![init, test, update, body, comments])
}

fn for_statement_init(
    init1: &ast::statement::for_::Init<Loc, Loc>,
    init2: &ast::statement::for_::Init<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use ast::statement::for_::Init;
    match (init1, init2) {
        (Init::InitDeclaration((loc, decl1)), Init::InitDeclaration((_, decl2))) => {
            variable_declaration(loc, decl1, decl2)
        }
        (Init::InitExpression(expr1), Init::InitExpression(expr2)) => Some(diff_if_changed(
            |e1, e2| expression(&ExpressionNodeParent::SlotParentOfExpression, e1, e2),
            expr1,
            expr2,
        )),
        _ => None,
    }
}

fn for_in_statement(
    loc: &Loc,
    stmt2: &statement::Statement<Loc, Loc>,
    for_in1: &ast::statement::ForIn<Loc, Loc>,
    for_in2: &ast::statement::ForIn<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let left = if for_in1.left == for_in2.left {
        Some(vec![])
    } else {
        for_in_statement_lhs(&for_in1.left, &for_in2.left)
    };
    let body = Some(diff_if_changed(
        |b1, b2| {
            statement(
                &StatementNodeParent::LoopParentOfStatement(loc.dupe()),
                b1,
                b2,
            )
        },
        &for_in1.body,
        &for_in2.body,
    ));
    let right = Some(diff_if_changed(
        |r1, r2| {
            expression(
                &ExpressionNodeParent::StatementParentOfExpression(stmt2.dupe()),
                r1,
                r2,
            )
        },
        &for_in1.right,
        &for_in2.right,
    ));
    let comments = syntax_opt(loc, &for_in1.comments, &for_in2.comments);
    join_diff_list(vec![left, right, body, comments])
}

fn for_in_statement_lhs(
    left1: &ast::statement::for_in::Left<Loc, Loc>,
    left2: &ast::statement::for_in::Left<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use ast::statement::for_in::Left;
    match (left1, left2) {
        (Left::LeftDeclaration((loc, decl1)), Left::LeftDeclaration((_, decl2))) => {
            variable_declaration(loc, decl1, decl2)
        }
        (Left::LeftPattern(p1), Left::LeftPattern(p2)) => Some(pattern(p1, p2)),
        _ => None,
    }
}

fn while_statement(
    loc: &Loc,
    stmt2: &statement::Statement<Loc, Loc>,
    while1: &ast::statement::While<Loc, Loc>,
    while2: &ast::statement::While<Loc, Loc>,
) -> Vec<NodeChange> {
    let test = diff_if_changed(
        |t1, t2| {
            expression(
                &ExpressionNodeParent::StatementParentOfExpression(stmt2.dupe()),
                t1,
                t2,
            )
        },
        &while1.test,
        &while2.test,
    );
    let body = diff_if_changed(
        |b1, b2| {
            statement(
                &StatementNodeParent::LoopParentOfStatement(loc.dupe()),
                b1,
                b2,
            )
        },
        &while1.body,
        &while2.body,
    );
    let comments = syntax_opt(loc, &while1.comments, &while2.comments).unwrap_or_default();
    let mut result = test;
    result.extend(body);
    result.extend(comments);
    result
}

fn for_of_statement(
    loc: &Loc,
    stmt2: &statement::Statement<Loc, Loc>,
    for_of1: &ast::statement::ForOf<Loc, Loc>,
    for_of2: &ast::statement::ForOf<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let left = if for_of1.left == for_of2.left {
        Some(vec![])
    } else {
        for_of_statement_lhs(&for_of1.left, &for_of2.left)
    };
    let body = Some(diff_if_changed(
        |b1, b2| {
            statement(
                &StatementNodeParent::LoopParentOfStatement(loc.dupe()),
                b1,
                b2,
            )
        },
        &for_of1.body,
        &for_of2.body,
    ));
    let right = Some(diff_if_changed(
        |r1, r2| {
            expression(
                &ExpressionNodeParent::StatementParentOfExpression(stmt2.dupe()),
                r1,
                r2,
            )
        },
        &for_of1.right,
        &for_of2.right,
    ));
    let await_ = if for_of1.await_ != for_of2.await_ {
        None
    } else {
        Some(vec![])
    };
    let comments = syntax_opt(loc, &for_of1.comments, &for_of2.comments);
    join_diff_list(vec![left, right, body, await_, comments])
}

fn for_of_statement_lhs(
    left1: &ast::statement::for_of::Left<Loc, Loc>,
    left2: &ast::statement::for_of::Left<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use ast::statement::for_of::Left;
    match (left1, left2) {
        (Left::LeftDeclaration((loc, decl1)), Left::LeftDeclaration((_, decl2))) => {
            variable_declaration(loc, decl1, decl2)
        }
        (Left::LeftPattern(p1), Left::LeftPattern(p2)) => Some(pattern(p1, p2)),
        _ => None,
    }
}

fn do_while_statement(
    loc: &Loc,
    stmt2: &statement::Statement<Loc, Loc>,
    dw1: &ast::statement::DoWhile<Loc, Loc>,
    dw2: &ast::statement::DoWhile<Loc, Loc>,
) -> Vec<NodeChange> {
    let body = diff_if_changed(
        |b1, b2| {
            statement(
                &StatementNodeParent::LoopParentOfStatement(loc.dupe()),
                b1,
                b2,
            )
        },
        &dw1.body,
        &dw2.body,
    );
    let test = diff_if_changed(
        |t1, t2| {
            expression(
                &ExpressionNodeParent::StatementParentOfExpression(stmt2.dupe()),
                t1,
                t2,
            )
        },
        &dw1.test,
        &dw2.test,
    );
    let comments = syntax_opt(loc, &dw1.comments, &dw2.comments).unwrap_or_default();
    let mut result = body;
    result.extend(test);
    result.extend(comments);
    result
}

fn debugger_statement(
    loc: &Loc,
    dbg1: &ast::statement::Debugger<Loc>,
    dbg2: &ast::statement::Debugger<Loc>,
) -> Option<Vec<NodeChange>> {
    let comments_diff = syntax_opt(loc, &dbg1.comments, &dbg2.comments);
    join_diff_list(vec![comments_diff])
}

fn continue_statement(
    loc: &Loc,
    cont1: &ast::statement::Continue<Loc>,
    cont2: &ast::statement::Continue<Loc>,
) -> Option<Vec<NodeChange>> {
    let comments_diff = syntax_opt(loc, &cont1.comments, &cont2.comments);
    join_diff_list(vec![comments_diff])
}

fn return_statement(
    loc: &Loc,
    stmt2: &statement::Statement<Loc, Loc>,
    ret1: &ast::statement::Return<Loc, Loc>,
    ret2: &ast::statement::Return<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    if ret1.return_out != ret2.return_out {
        return None;
    }
    let comments = syntax_opt(loc, &ret1.comments, &ret2.comments);
    let arg_diff = diff_if_changed_nonopt_fn(
        |a1, a2| {
            expression(
                &ExpressionNodeParent::StatementParentOfExpression(stmt2.dupe()),
                a1,
                a2,
            )
        },
        &ret1.argument,
        &ret2.argument,
    );
    join_diff_list(vec![comments, arg_diff])
}

fn throw_statement(
    loc: &Loc,
    stmt2: &statement::Statement<Loc, Loc>,
    throw1: &ast::statement::Throw<Loc, Loc>,
    throw2: &ast::statement::Throw<Loc, Loc>,
) -> Vec<NodeChange> {
    let argument = diff_if_changed(
        |a1, a2| {
            expression(
                &ExpressionNodeParent::StatementParentOfExpression(stmt2.dupe()),
                a1,
                a2,
            )
        },
        &throw1.argument,
        &throw2.argument,
    );
    let comments = syntax_opt(loc, &throw1.comments, &throw2.comments).unwrap_or_default();
    let mut result = argument;
    result.extend(comments);
    result
}

fn labeled_statement(
    loc: &Loc,
    labeled1: &ast::statement::Labeled<Loc, Loc>,
    labeled2: &ast::statement::Labeled<Loc, Loc>,
) -> Vec<NodeChange> {
    let label_diff = diff_if_changed(identifier, &labeled1.label, &labeled2.label);
    let body_diff = diff_if_changed(
        |b1, b2| {
            statement(
                &StatementNodeParent::LabeledStatementParentOfStatement(loc.dupe()),
                b1,
                b2,
            )
        },
        &labeled1.body,
        &labeled2.body,
    );
    let comments_diff = syntax_opt(loc, &labeled1.comments, &labeled2.comments).unwrap_or_default();
    let mut result = label_diff;
    result.extend(body_diff);
    result.extend(comments_diff);
    result
}

fn switch_statement(
    loc: &Loc,
    stmt2: &statement::Statement<Loc, Loc>,
    switch1: &ast::statement::Switch<Loc, Loc>,
    switch2: &ast::statement::Switch<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    if switch1.exhaustive_out != switch2.exhaustive_out {
        return None;
    }
    let discriminant = Some(diff_if_changed(
        |d1, d2| {
            expression(
                &ExpressionNodeParent::StatementParentOfExpression(stmt2.dupe()),
                d1,
                d2,
            )
        },
        &switch1.discriminant,
        &switch2.discriminant,
    ));
    let cases = diff_and_recurse_no_trivial(
        &|c1, c2| switch_case(c1, c2),
        &switch1.cases,
        &switch2.cases,
    );
    let comments = syntax_opt(loc, &switch1.comments, &switch2.comments);
    join_diff_list(vec![discriminant, cases, comments])
}

fn switch_case(
    s1: &ast::statement::switch::Case<Loc, Loc>,
    s2: &ast::statement::switch::Case<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let test = diff_if_changed_nonopt_fn(
        |t1, t2| expression(&ExpressionNodeParent::SlotParentOfExpression, t1, t2),
        &s1.test,
        &s2.test,
    );
    let consequent = statement_list(
        &StatementNodeParent::SwitchCaseParentOfStatement(s1.loc.dupe()),
        &s1.consequent,
        &s2.consequent,
    );
    let comments = syntax_opt(&s1.loc, &s1.comments, &s2.comments);
    join_diff_list(vec![test, consequent, comments])
}

fn pattern(
    p1: &ast::pattern::Pattern<Loc, Loc>,
    p2: &ast::pattern::Pattern<Loc, Loc>,
) -> Vec<NodeChange> {
    let changes = match (p1, p2) {
        (
            ast::pattern::Pattern::Identifier { inner: i1, .. },
            ast::pattern::Pattern::Identifier { inner: i2, .. },
        ) => pattern_identifier(i1, i2),
        (
            ast::pattern::Pattern::Array { loc, inner: a1 },
            ast::pattern::Pattern::Array { inner: a2, .. },
        ) => pattern_array(loc, a1, a2),
        (
            ast::pattern::Pattern::Object { loc, inner: o1 },
            ast::pattern::Pattern::Object { inner: o2, .. },
        ) => pattern_object(loc, o1, o2),
        (
            ast::pattern::Pattern::Expression { inner: e1, .. },
            ast::pattern::Pattern::Expression { inner: e2, .. },
        ) => Some(expression(
            &ExpressionNodeParent::SlotParentOfExpression,
            e1,
            e2,
        )),
        _ => None,
    };
    let old_loc = p1.loc();
    changes.unwrap_or_else(|| {
        vec![replace(
            old_loc,
            Node::Pattern(p1.clone()),
            Node::Pattern(p2.clone()),
        )]
    })
}

fn pattern_object(
    loc: &Loc,
    o1: &ast::pattern::Object<Loc, Loc>,
    o2: &ast::pattern::Object<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::pattern::Object {
        properties: properties1,
        annot: annot1,
        optional: optional1,
        comments: comments1,
    } = o1;
    let ast::pattern::Object {
        properties: properties2,
        annot: annot2,
        optional: optional2,
        comments: comments2,
    } = o2;
    if optional1 != optional2 {
        return None;
    }
    let properties_diff = diff_and_recurse_no_trivial(
        &|p1, p2| pattern_object_property(p1, p2),
        properties1,
        properties2,
    );
    let annot_diff = Some(diff_if_changed(type_annotation_hint, annot1, annot2));
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![properties_diff, annot_diff, comments_diff])
}

fn pattern_object_property(
    p1: &ast::pattern::object::Property<Loc, Loc>,
    p2: &ast::pattern::object::Property<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    match (p1, p2) {
        (
            ast::pattern::object::Property::NormalProperty(p3),
            ast::pattern::object::Property::NormalProperty(p4),
        ) => {
            let ast::pattern::object::NormalProperty {
                key: key1,
                pattern: pattern1,
                default: default1,
                shorthand: shorthand1,
                ..
            } = p3;
            let ast::pattern::object::NormalProperty {
                key: key2,
                pattern: pattern2,
                default: default2,
                shorthand: shorthand2,
                ..
            } = p4;
            let keys = diff_if_changed_ret_opt(pattern_object_property_key, key1, key2);
            let pats = Some(diff_if_changed(pattern, pattern1, pattern2));
            let defaults = diff_if_changed_nonopt_fn(
                |e1, e2| expression(&ExpressionNodeParent::SlotParentOfExpression, e1, e2),
                default1,
                default2,
            );
            match (shorthand1, shorthand2) {
                (false, false) => join_diff_list(vec![keys, pats, defaults]),
                _ => None,
            }
        }
        (
            ast::pattern::object::Property::RestElement(re1),
            ast::pattern::object::Property::RestElement(re2),
        ) => pattern_rest_element(re1, re2),
        _ => None,
    }
}

fn pattern_object_property_key(
    k1: &ast::pattern::object::Key<Loc, Loc>,
    k2: &ast::pattern::object::Key<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    match (k1, k2) {
        (
            ast::pattern::object::Key::StringLiteral((loc1, l1)),
            ast::pattern::object::Key::StringLiteral((loc2, l2)),
        ) => diff_if_changed_ret_opt(|l1, l2| string_literal(loc1, loc2, l1, l2), l1, l2),
        (
            ast::pattern::object::Key::NumberLiteral((loc1, l1)),
            ast::pattern::object::Key::NumberLiteral((loc2, l2)),
        ) => diff_if_changed_ret_opt(|l1, l2| number_literal(loc1, loc2, l1, l2), l1, l2),
        (
            ast::pattern::object::Key::BigIntLiteral((loc1, l1)),
            ast::pattern::object::Key::BigIntLiteral((loc2, l2)),
        ) => diff_if_changed_ret_opt(|l1, l2| bigint_literal(loc1, loc2, l1, l2), l1, l2),
        (ast::pattern::object::Key::Identifier(i1), ast::pattern::object::Key::Identifier(i2)) => {
            Some(diff_if_changed(identifier, i1, i2))
        }
        (ast::pattern::object::Key::Computed(c1), ast::pattern::object::Key::Computed(c2)) => {
            let loc = &c1.loc;
            diff_if_changed_ret_opt(|c1, c2| computed_key(loc, c1, c2), c1, c2)
        }
        _ => None,
    }
}

fn pattern_array(
    loc: &Loc,
    a1: &ast::pattern::Array<Loc, Loc>,
    a2: &ast::pattern::Array<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::pattern::Array {
        elements: elements1,
        annot: annot1,
        optional: optional1,
        comments: comments1,
    } = a1;
    let ast::pattern::Array {
        elements: elements2,
        annot: annot2,
        optional: optional2,
        comments: comments2,
    } = a2;
    if optional1 != optional2 {
        return None;
    }
    let elements_diff =
        diff_and_recurse_no_trivial(&|e1, e2| pattern_array_e(e1, e2), elements1, elements2);
    let annot_diff = Some(diff_if_changed(type_annotation_hint, annot1, annot2));
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![comments_diff, elements_diff, annot_diff])
}

fn pattern_array_e(
    eo1: &ast::pattern::array::Element<Loc, Loc>,
    eo2: &ast::pattern::array::Element<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    match (eo1, eo2) {
        (
            ast::pattern::array::Element::NormalElement(p1),
            ast::pattern::array::Element::NormalElement(p2),
        ) => pattern_array_element(p1, p2),
        (
            ast::pattern::array::Element::RestElement(re1),
            ast::pattern::array::Element::RestElement(re2),
        ) => pattern_rest_element(re1, re2),
        (ast::pattern::array::Element::Hole(_), ast::pattern::array::Element::Hole(_)) => {
            Some(vec![]) // Both elements elided
        }
        // one element is elided and another is not
        _ => None,
    }
}

fn pattern_array_element(
    e1: &ast::pattern::array::NormalElement<Loc, Loc>,
    e2: &ast::pattern::array::NormalElement<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::pattern::array::NormalElement {
        argument: argument1,
        default: default1,
        ..
    } = e1;
    let ast::pattern::array::NormalElement {
        argument: argument2,
        default: default2,
        ..
    } = e2;
    let args = Some(diff_if_changed(pattern, argument1, argument2));
    let defaults = diff_if_changed_nonopt_fn(
        |e1, e2| expression(&ExpressionNodeParent::SlotParentOfExpression, e1, e2),
        default1,
        default2,
    );
    join_diff_list(vec![args, defaults])
}

fn pattern_rest_element(
    r1: &ast::pattern::RestElement<Loc, Loc>,
    r2: &ast::pattern::RestElement<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::pattern::RestElement {
        loc,
        argument: argument1,
        comments: comments1,
    } = r1;
    let ast::pattern::RestElement {
        argument: argument2,
        comments: comments2,
        ..
    } = r2;
    let argument_diff = Some(pattern(argument1, argument2));
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![argument_diff, comments_diff])
}

fn pattern_identifier(
    i1: &ast::pattern::Identifier<Loc, Loc>,
    i2: &ast::pattern::Identifier<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::pattern::Identifier {
        name: name1,
        annot: annot1,
        optional: optional1,
    } = i1;
    let ast::pattern::Identifier {
        name: name2,
        annot: annot2,
        optional: optional2,
    } = i2;
    if optional1 != optional2 {
        return None;
    }
    let ids = Some(diff_if_changed(identifier, name1, name2));
    let annots = Some(diff_if_changed(type_annotation_hint, annot1, annot2));
    join_diff_list(vec![ids, annots])
}

fn function_rest_param(
    rest1: &ast::function::RestParam<Loc, Loc>,
    rest2: &ast::function::RestParam<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let param_diff = Some(diff_if_changed(pattern, &rest1.argument, &rest2.argument));
    let comments_diff = syntax_opt(&rest1.loc, &rest1.comments, &rest2.comments);
    join_diff_list(vec![param_diff, comments_diff])
}

fn type_(typ1: &ast::types::Type<Loc, Loc>, typ2: &ast::types::Type<Loc, Loc>) -> Vec<NodeChange> {
    use ast::types::TypeInner::*;
    let type_diff = match (&**typ1, &**typ2) {
        (
            Any {
                loc: loc1,
                comments: c1,
            },
            Any { comments: c2, .. },
        )
        | (
            Mixed {
                loc: loc1,
                comments: c1,
            },
            Mixed { comments: c2, .. },
        )
        | (
            Empty {
                loc: loc1,
                comments: c1,
            },
            Empty { comments: c2, .. },
        )
        | (
            Void {
                loc: loc1,
                comments: c1,
            },
            Void { comments: c2, .. },
        )
        | (
            Null {
                loc: loc1,
                comments: c1,
            },
            Null { comments: c2, .. },
        )
        | (
            Symbol {
                loc: loc1,
                comments: c1,
            },
            Symbol { comments: c2, .. },
        )
        | (
            Number {
                loc: loc1,
                comments: c1,
            },
            Number { comments: c2, .. },
        )
        | (
            BigInt {
                loc: loc1,
                comments: c1,
            },
            BigInt { comments: c2, .. },
        )
        | (
            String {
                loc: loc1,
                comments: c1,
            },
            String { comments: c2, .. },
        )
        | (
            Exists {
                loc: loc1,
                comments: c1,
            },
            Exists { comments: c2, .. },
        ) => diff_if_changed_ret_opt(|c1, c2| syntax_opt(loc1, c1, c2), c1, c2),
        (
            Boolean {
                loc: loc1,
                raw: r1,
                comments: c1,
            },
            Boolean {
                raw: r2,
                comments: c2,
                ..
            },
        ) => {
            if r1 == r2 {
                diff_if_changed_ret_opt(|c1, c2| syntax_opt(loc1, c1, c2), c1, c2)
            } else {
                None
            }
        }
        (
            Function {
                loc: loc1,
                inner: fn1,
            },
            Function { inner: fn2, .. },
        ) => diff_if_changed_ret_opt(
            |f1, f2| function_type(loc1, f1, f2),
            fn1.as_ref(),
            fn2.as_ref(),
        ),
        (
            Interface {
                loc: loc1,
                inner: i1,
            },
            Interface { inner: i2, .. },
        ) => diff_if_changed_ret_opt(
            |i1, i2| interface_type(loc1, i1, i2),
            i1.as_ref(),
            i2.as_ref(),
        ),
        (
            Generic {
                loc: loc1,
                inner: g1,
            },
            Generic { inner: g2, .. },
        ) => diff_if_changed_ret_opt(
            |g1, g2| generic_type(loc1, g1, g2),
            g1.as_ref(),
            g2.as_ref(),
        ),
        (
            Intersection {
                loc: loc1,
                inner: t1,
            },
            Intersection { inner: t2, .. },
        ) => diff_if_changed_ret_opt(
            |t1, t2| intersection_type(loc1, t1, t2),
            t1.as_ref(),
            t2.as_ref(),
        ),
        (
            Union {
                loc: loc1,
                inner: t1,
            },
            Union { inner: t2, .. },
        ) => diff_if_changed_ret_opt(|t1, t2| union_type(loc1, t1, t2), t1.as_ref(), t2.as_ref()),
        (
            Nullable {
                loc: loc1,
                inner: t1,
            },
            Nullable { inner: t2, .. },
        ) => diff_if_changed_ret_opt(
            |t1, t2| nullable_type(loc1, t1, t2),
            t1.as_ref(),
            t2.as_ref(),
        ),
        (
            Object {
                loc: loc1,
                inner: obj1,
            },
            Object { inner: obj2, .. },
        ) => diff_if_changed_ret_opt(
            |o1, o2| object_type(loc1, o1, o2),
            obj1.as_ref(),
            obj2.as_ref(),
        ),
        (
            StringLiteral {
                loc: loc1,
                literal: s1,
            },
            StringLiteral {
                loc: loc2,
                literal: s2,
            },
        ) => diff_if_changed_ret_opt(|s1, s2| string_literal(loc1, loc2, s1, s2), s1, s2),
        (
            NumberLiteral {
                loc: loc1,
                literal: n1,
            },
            NumberLiteral {
                loc: loc2,
                literal: n2,
            },
        ) => diff_if_changed_ret_opt(|n1, n2| number_literal(loc1, loc2, n1, n2), n1, n2),
        (
            BigIntLiteral {
                loc: loc1,
                literal: b1,
            },
            BigIntLiteral {
                loc: loc2,
                literal: b2,
            },
        ) => diff_if_changed_ret_opt(|b1, b2| bigint_literal(loc1, loc2, b1, b2), b1, b2),
        (
            BooleanLiteral {
                loc: loc1,
                literal: b1,
            },
            BooleanLiteral {
                loc: loc2,
                literal: b2,
            },
        ) => diff_if_changed_ret_opt(|b1, b2| boolean_literal(loc1, loc2, b1, b2), b1, b2),
        (
            Typeof {
                loc: loc1,
                inner: t1,
            },
            Typeof { inner: t2, .. },
        ) => diff_if_changed_ret_opt(|t1, t2| typeof_type(loc1, t1, t2), t1.as_ref(), t2.as_ref()),
        (
            Tuple {
                loc: loc1,
                inner: t1,
            },
            Tuple { inner: t2, .. },
        ) => diff_if_changed_ret_opt(|t1, t2| tuple_type(loc1, t1, t2), t1.as_ref(), t2.as_ref()),
        (
            Array {
                loc: loc1,
                inner: t1,
            },
            Array { inner: t2, .. },
        ) => diff_if_changed_ret_opt(|t1, t2| array_type(loc1, t1, t2), t1.as_ref(), t2.as_ref()),
        (
            Renders {
                loc: loc1,
                inner: t1,
            },
            Renders { inner: t2, .. },
        ) => diff_if_changed_ret_opt(|t1, t2| render_type(loc1, t1, t2), t1.as_ref(), t2.as_ref()),
        _ => None,
    };
    let loc1 = typ1.loc();
    type_diff.unwrap_or_else(|| {
        // A keyword replaced by a keyword with the same comments keeps the comments where they
        // are in the source, which can be past a separator: only the keyword is replaced.
        if let Some(comments1) = keyword_type_comments(typ1)
            && let Some(comments2) = keyword_type_comments(typ2)
            && comments1 == comments2
        {
            return vec![(
                loc1.dupe(),
                Change::Replace(
                    Node::Type(typ1.dupe()),
                    Node::Type(keyword_type_without_comments(typ2)),
                ),
            )];
        }
        vec![replace(
            loc1,
            Node::Type(typ1.dupe()),
            Node::Type(typ2.dupe()),
        )]
    })
}

/// The comments of a keyword type, which has nothing but a location and comments.
fn keyword_type_comments(t: &ast::types::Type<Loc, Loc>) -> Option<&Option<ast::Syntax<Loc, ()>>> {
    use ast::types::TypeInner::*;
    match &**t {
        Any { comments, .. }
        | Mixed { comments, .. }
        | Empty { comments, .. }
        | Void { comments, .. }
        | Null { comments, .. }
        | Number { comments, .. }
        | BigInt { comments, .. }
        | String { comments, .. }
        | Symbol { comments, .. }
        | Unknown { comments, .. }
        | Never { comments, .. }
        | Undefined { comments, .. } => Some(comments),
        _ => None,
    }
}

fn keyword_type_without_comments(t: &ast::types::Type<Loc, Loc>) -> ast::types::Type<Loc, Loc> {
    use ast::types::TypeInner::*;
    let without_comments = match &**t {
        Any { loc, .. } => Any {
            loc: loc.dupe(),
            comments: None,
        },
        Mixed { loc, .. } => Mixed {
            loc: loc.dupe(),
            comments: None,
        },
        Empty { loc, .. } => Empty {
            loc: loc.dupe(),
            comments: None,
        },
        Void { loc, .. } => Void {
            loc: loc.dupe(),
            comments: None,
        },
        Null { loc, .. } => Null {
            loc: loc.dupe(),
            comments: None,
        },
        Number { loc, .. } => Number {
            loc: loc.dupe(),
            comments: None,
        },
        BigInt { loc, .. } => BigInt {
            loc: loc.dupe(),
            comments: None,
        },
        String { loc, .. } => String {
            loc: loc.dupe(),
            comments: None,
        },
        Symbol { loc, .. } => Symbol {
            loc: loc.dupe(),
            comments: None,
        },
        Unknown { loc, .. } => Unknown {
            loc: loc.dupe(),
            comments: None,
        },
        Never { loc, .. } => Never {
            loc: loc.dupe(),
            comments: None,
        },
        Undefined { loc, .. } => Undefined {
            loc: loc.dupe(),
            comments: None,
        },
        _ => return t.dupe(),
    };
    ast::types::Type::new(without_comments)
}

fn interface_type(
    loc: &Loc,
    it1: &ast::types::Interface<Loc, Loc>,
    it2: &ast::types::Interface<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::types::Interface {
        extends: extends1,
        body: (body_loc, body1),
        comments: comments1,
    } = it1;
    let ast::types::Interface {
        extends: extends2,
        body: (_, body2),
        comments: comments2,
    } = it2;
    let extends_diff =
        diff_and_recurse_no_trivial(&|e1, e2| generic_type_with_loc(e1, e2), extends1, extends2);
    let body_diff = diff_if_changed_ret_opt(|b1, b2| object_type(body_loc, b1, b2), body1, body2);
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![extends_diff, body_diff, comments_diff])
}

fn generic_type(
    loc: &Loc,
    gt1: &ast::types::Generic<Loc, Loc>,
    gt2: &ast::types::Generic<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::types::Generic {
        id: id1,
        targs: targs1,
        comments: comments1,
    } = gt1;
    let ast::types::Generic {
        id: id2,
        targs: targs2,
        comments: comments2,
    } = gt2;
    let id_diff = diff_if_changed_ret_opt(generic_identifier_type, id1, id2);
    let targs_diff = diff_if_changed_opt(type_args, targs1, targs2);
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![id_diff, targs_diff, comments_diff])
}

fn generic_type_with_loc(
    g1: &(Loc, ast::types::Generic<Loc, Loc>),
    g2: &(Loc, ast::types::Generic<Loc, Loc>),
) -> Option<Vec<NodeChange>> {
    let (loc1, gt1) = g1;
    let (_, gt2) = g2;
    generic_type(loc1, gt1, gt2)
}

fn generic_identifier_type(
    git1: &ast::types::generic::Identifier<Loc, Loc>,
    git2: &ast::types::generic::Identifier<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    match (git1, git2) {
        (
            ast::types::generic::Identifier::Unqualified(id1),
            ast::types::generic::Identifier::Unqualified(id2),
        ) => Some(diff_if_changed(identifier, id1, id2)),
        (
            ast::types::generic::Identifier::Qualified(q1),
            ast::types::generic::Identifier::Qualified(q2),
        ) => {
            let qualification_diff = diff_if_changed_ret_opt(
                generic_identifier_type,
                &q1.qualification,
                &q2.qualification,
            );
            let id_diff = Some(diff_if_changed(identifier, &q1.id, &q2.id));
            join_diff_list(vec![qualification_diff, id_diff])
        }
        _ => None,
    }
}

fn object_type(
    loc: &Loc,
    ot1: &ast::types::Object<Loc, Loc>,
    ot2: &ast::types::Object<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::types::Object {
        properties: props1,
        exact: exact1,
        inexact: inexact1,
        comments: comments1,
    } = ot1;
    let ast::types::Object {
        properties: props2,
        exact: exact2,
        inexact: inexact2,
        comments: comments2,
    } = ot2;
    // These are boolean literals, so structural equality is ok
    let exact_diff = if exact1 == exact2 { Some(vec![]) } else { None };
    let inexact_diff = if inexact1 == inexact2 {
        Some(vec![])
    } else {
        None
    };
    let properties_diff =
        diff_and_recurse_no_trivial(&|p1, p2| object_type_property(p1, p2), props1, props2);
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![
        exact_diff,
        inexact_diff,
        properties_diff,
        comments_diff,
    ])
}

fn object_type_property(
    prop1: &ast::types::object::Property<Loc, Loc>,
    prop2: &ast::types::object::Property<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use ast::types::object::Property::*;
    match (prop1, prop2) {
        (NormalProperty(p1), NormalProperty(p2)) => {
            diff_if_changed_ret_opt(object_property_type, p1, p2)
        }
        (SpreadProperty(s1), SpreadProperty(s2)) => {
            let loc = &s1.loc;
            diff_if_changed_ret_opt(|s1, s2| object_spread_property_type(loc, s1, s2), s1, s2)
        }
        (Indexer(i1), Indexer(i2)) => {
            let loc = &i1.loc;
            diff_if_changed_ret_opt(|i1, i2| object_indexer_type(loc, i1, i2), i1, i2)
        }
        (InternalSlot(s1), InternalSlot(s2)) => {
            let loc = &s1.loc;
            diff_if_changed_ret_opt(|s1, s2| object_internal_slot_type(loc, s1, s2), s1, s2)
        }
        (CallProperty(c1), CallProperty(c2)) => {
            let loc = &c1.loc;
            diff_if_changed_ret_opt(|c1, c2| object_call_property_type(loc, c1, c2), c1, c2)
        }
        _ => None,
    }
}

fn object_property_type(
    p1: &ast::types::object::NormalProperty<Loc, Loc>,
    p2: &ast::types::object::NormalProperty<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    if p1.optional != p2.optional
        || p1.static_ != p2.static_
        || p1.proto != p2.proto
        || p1.method != p2.method
        || p1.abstract_ != p2.abstract_
        || p1.ts_accessibility != p2.ts_accessibility
    {
        return None;
    }
    let variance_diff = diff_if_changed_ret_opt(variance, &p1.variance, &p2.variance);
    let key_diff = diff_if_changed_ret_opt(object_key, &p1.key, &p2.key);
    let value_diff = diff_if_changed_ret_opt(object_property_value_type, &p1.value, &p2.value);
    let init_diff = diff_if_changed_nonopt_fn(
        |e1, e2| expression(&ExpressionNodeParent::SlotParentOfExpression, e1, e2),
        &p1.init,
        &p2.init,
    );
    let comments_diff = syntax_opt(&p1.loc, &p1.comments, &p2.comments);
    join_diff_list(vec![
        variance_diff,
        key_diff,
        value_diff,
        init_diff,
        comments_diff,
    ])
}

fn object_property_value_type(
    v1: &ast::types::object::PropertyValue<Loc, Loc>,
    v2: &ast::types::object::PropertyValue<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use ast::types::object::PropertyValue::*;
    match (v1, v2) {
        (Init(Some(t1)), Init(Some(t2))) => Some(diff_if_changed(type_, t1, t2)),
        (Init(None), Init(None)) => Some(vec![]),
        (Get(loc1, ft1), Get(_, ft2)) | (Set(loc1, ft1), Set(_, ft2)) => {
            diff_if_changed_ret_opt(|f1, f2| function_type(loc1, f1, f2), ft1, ft2)
        }
        _ => None,
    }
}

fn object_spread_property_type(
    loc: &Loc,
    s1: &ast::types::object::SpreadProperty<Loc, Loc>,
    s2: &ast::types::object::SpreadProperty<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::types::object::SpreadProperty {
        argument: argument1,
        comments: comments1,
        ..
    } = s1;
    let ast::types::object::SpreadProperty {
        argument: argument2,
        comments: comments2,
        ..
    } = s2;
    let argument_diff = Some(diff_if_changed(type_, argument1, argument2));
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![argument_diff, comments_diff])
}

fn object_indexer_type(
    loc: &Loc,
    i1: &ast::types::object::Indexer<Loc, Loc>,
    i2: &ast::types::object::Indexer<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    if i1.static_ != i2.static_ || i1.optional != i2.optional {
        return None;
    }
    let id_diff = diff_if_changed_nonopt_fn(identifier, &i1.id, &i2.id);
    let key_diff = Some(diff_if_changed(type_, &i1.key, &i2.key));
    let value_diff = Some(diff_if_changed(type_, &i1.value, &i2.value));
    let variance_diff = diff_if_changed_ret_opt(variance, &i1.variance, &i2.variance);
    let comments = syntax_opt(loc, &i1.comments, &i2.comments);
    join_diff_list(vec![id_diff, key_diff, value_diff, variance_diff, comments])
}

fn object_internal_slot_type(
    loc: &Loc,
    s1: &ast::types::object::InternalSlot<Loc, Loc>,
    s2: &ast::types::object::InternalSlot<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    if s1.optional != s2.optional || s1.static_ != s2.static_ || s1.method != s2.method {
        return None;
    }
    let id_diff = Some(diff_if_changed(identifier, &s1.id, &s2.id));
    let value_diff = Some(diff_if_changed(type_, &s1.value, &s2.value));
    let comments_diff = syntax_opt(loc, &s1.comments, &s2.comments);
    join_diff_list(vec![id_diff, value_diff, comments_diff])
}

fn object_call_property_type(
    loc: &Loc,
    c1: &ast::types::object::CallProperty<Loc, Loc>,
    c2: &ast::types::object::CallProperty<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    if c1.static_ != c2.static_ {
        return None;
    }
    let (ref value_loc, ref value1) = c1.value;
    let (_, ref value2) = c2.value;
    let value_diff =
        diff_if_changed_ret_opt(|f1, f2| function_type(value_loc, f1, f2), value1, value2);
    let comments_diff = syntax_opt(loc, &c1.comments, &c2.comments);
    join_diff_list(vec![value_diff, comments_diff])
}

fn tuple_type(
    loc: &Loc,
    t1: &ast::types::Tuple<Loc, Loc>,
    t2: &ast::types::Tuple<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::types::Tuple {
        elements: elements1,
        inexact: inexact1,
        comments: comments1,
    } = t1;
    let ast::types::Tuple {
        elements: elements2,
        inexact: inexact2,
        comments: comments2,
    } = t2;
    let inexact_diff = if inexact1 == inexact2 {
        Some(vec![])
    } else {
        None
    };
    let elements_diff =
        diff_and_recurse_no_trivial(&|e1, e2| tuple_element(e1, e2), elements1, elements2);
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![elements_diff, inexact_diff, comments_diff])
}

fn tuple_element(
    e1: &ast::types::tuple::Element<Loc, Loc>,
    e2: &ast::types::tuple::Element<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use ast::types::tuple::Element::*;
    match (e1, e2) {
        (
            UnlabeledElement {
                annot: a1,
                optional: o1,
                ..
            },
            UnlabeledElement {
                annot: a2,
                optional: o2,
                ..
            },
        ) => {
            if o1 != o2 {
                None
            } else {
                Some(diff_if_changed(type_, a1, a2))
            }
        }
        (LabeledElement { element: l1, .. }, LabeledElement { element: l2, .. }) => {
            tuple_labeled_element(l1, l2)
        }
        (SpreadElement { element: s1, .. }, SpreadElement { element: s2, .. }) => {
            tuple_spread_element(s1, s2)
        }
        _ => None,
    }
}

fn tuple_labeled_element(
    t1: &ast::types::tuple::LabeledElement<Loc, Loc>,
    t2: &ast::types::tuple::LabeledElement<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::types::tuple::LabeledElement {
        name: name1,
        annot: annot1,
        variance: var1,
        optional: opt1,
        ..
    } = t1;
    let ast::types::tuple::LabeledElement {
        name: name2,
        annot: annot2,
        variance: var2,
        optional: opt2,
        ..
    } = t2;
    let name_diff = Some(diff_if_changed(identifier, name1, name2));
    let annot_diff = Some(diff_if_changed(type_, annot1, annot2));
    let variance_diff = diff_if_changed_ret_opt(variance, var1, var2);
    let optional_diff = if opt1 == opt2 { Some(vec![]) } else { None };
    join_diff_list(vec![name_diff, annot_diff, variance_diff, optional_diff])
}

fn tuple_spread_element(
    e1: &ast::types::tuple::SpreadElement<Loc, Loc>,
    e2: &ast::types::tuple::SpreadElement<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::types::tuple::SpreadElement {
        name: name1,
        annot: annot1,
        ..
    } = e1;
    let ast::types::tuple::SpreadElement {
        name: name2,
        annot: annot2,
        ..
    } = e2;
    let name_diff = diff_if_changed_nonopt_fn(identifier, name1, name2);
    let annot_diff = Some(diff_if_changed(type_, annot1, annot2));
    join_diff_list(vec![name_diff, annot_diff])
}

fn type_args(
    pi1: &ast::types::TypeArgs<Loc, Loc>,
    pi2: &ast::types::TypeArgs<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::types::TypeArgs {
        loc,
        arguments: arguments1,
        comments: comments1,
    } = pi1;
    let ast::types::TypeArgs {
        arguments: arguments2,
        comments: comments2,
        ..
    } = pi2;
    let args_diff =
        diff_and_recurse_nonopt_no_trivial(&|t1, t2| type_(t1, t2), arguments1, arguments2);
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![args_diff, comments_diff])
}

fn function_param_type(
    fpt1: &ast::types::function::Param<Loc, Loc>,
    fpt2: &ast::types::function::Param<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    use ast::types::function::ParamKind;
    match (&fpt1.param, &fpt2.param) {
        (ParamKind::Anonymous(annot1), ParamKind::Anonymous(annot2)) => {
            Some(diff_if_changed(type_, annot1, annot2))
        }
        (
            ParamKind::Labeled {
                name: name1,
                annot: annot1,
                optional: optional1,
            },
            ParamKind::Labeled {
                name: name2,
                annot: annot2,
                optional: optional2,
            },
        ) => {
            if optional1 != optional2 {
                None
            } else {
                let name_diff = Some(diff_if_changed(identifier, name1, name2));
                let annot_diff = Some(diff_if_changed(type_, annot1, annot2));
                join_diff_list(vec![name_diff, annot_diff])
            }
        }
        (ParamKind::Destructuring(patt1), ParamKind::Destructuring(patt2)) => {
            Some(diff_if_changed(pattern, patt1, patt2))
        }
        _ => None,
    }
}

fn function_rest_param_type(
    frpt1: &ast::types::function::RestParam<Loc, Loc>,
    frpt2: &ast::types::function::RestParam<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::types::function::RestParam {
        loc,
        argument: arg1,
        comments: comments1,
    } = frpt1;
    let ast::types::function::RestParam {
        argument: arg2,
        comments: comments2,
        ..
    } = frpt2;
    let arg_diff = diff_if_changed_ret_opt(function_param_type, arg1, arg2);
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![arg_diff, comments_diff])
}

fn function_this_constraint_type(
    ftct1: &ast::types::function::ThisParam<Loc, Loc>,
    ftct2: &ast::types::function::ThisParam<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::types::function::ThisParam {
        loc,
        annot: annot1,
        comments: comments1,
    } = ftct1;
    let ast::types::function::ThisParam {
        annot: annot2,
        comments: comments2,
        ..
    } = ftct2;
    let annot_diff = Some(diff_if_changed(type_annotation, annot1, annot2));
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![annot_diff, comments_diff])
}

fn function_type_return_annotation(
    ret1: &ast::types::function::ReturnAnnotation<Loc, Loc>,
    ret2: &ast::types::function::ReturnAnnotation<Loc, Loc>,
) -> Vec<NodeChange> {
    use ast::types::function::ReturnAnnotation::*;
    match (ret1, ret2) {
        (Missing(_), Missing(_)) => vec![],
        (Available(annot1), Missing(_)) => {
            vec![delete(&annot1.loc, Node::Type(annot1.annotation.dupe()))]
        }
        (TypeGuard(grd1), Missing(_)) => {
            vec![delete(&grd1.loc, Node::TypeGuard(grd1.clone()))]
        }
        (Missing(loc1), Available(annot2)) => {
            vec![(
                loc1.dupe(),
                insert(None, vec![Node::Type(annot2.annotation.dupe())]),
            )]
        }
        (Missing(loc1), TypeGuard(grd2)) => {
            vec![(
                loc1.dupe(),
                insert(None, vec![Node::TypeGuard(grd2.clone())]),
            )]
        }
        (Available(annot1), Available(annot2)) => type_(&annot1.annotation, &annot2.annotation),
        (TypeGuard(grd1), TypeGuard(grd2)) => type_guard(grd1, grd2),
        (TypeGuard(grd1), Available(annot2)) => {
            vec![replace(
                &grd1.loc,
                Node::TypeGuard(grd1.clone()),
                Node::Type(annot2.annotation.dupe()),
            )]
        }
        (Available(annot1), TypeGuard(grd2)) => {
            vec![replace(
                &annot1.loc,
                Node::Type(annot1.annotation.dupe()),
                Node::TypeGuard(grd2.clone()),
            )]
        }
    }
}

fn function_type(
    loc: &Loc,
    ft1: &ast::types::Function<Loc, Loc>,
    ft2: &ast::types::Function<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    if ft1.effect != ft2.effect {
        return None;
    }
    let tparams_diff = diff_if_changed_opt(type_params, &ft1.tparams, &ft2.tparams);
    let this_diff = diff_if_changed_opt(
        function_this_constraint_type,
        &ft1.params.this,
        &ft2.params.this,
    );
    let params_diff = diff_and_recurse_no_trivial(
        &|p1, p2| function_param_type(p1, p2),
        &ft1.params.params,
        &ft2.params.params,
    );
    let rest_diff =
        diff_if_changed_opt(function_rest_param_type, &ft1.params.rest, &ft2.params.rest);
    let return_diff = Some(diff_if_changed(
        function_type_return_annotation,
        &ft1.return_,
        &ft2.return_,
    ));
    let func_comments_diff = syntax_opt(loc, &ft1.comments, &ft2.comments);
    let params_comments_diff =
        syntax_opt(&ft1.params.loc, &ft1.params.comments, &ft2.params.comments);
    join_diff_list(vec![
        tparams_diff,
        this_diff,
        params_diff,
        rest_diff,
        return_diff,
        func_comments_diff,
        params_comments_diff,
    ])
}

fn nullable_type(
    loc: &Loc,
    t1: &ast::types::Nullable<Loc, Loc>,
    t2: &ast::types::Nullable<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let argument_diff = Some(diff_if_changed(type_, &t1.argument, &t2.argument));
    let comments_diff = syntax_opt(loc, &t1.comments, &t2.comments);
    join_diff_list(vec![argument_diff, comments_diff])
}

fn render_type(
    loc: &Loc,
    t1: &ast::types::Renders<Loc, Loc>,
    t2: &ast::types::Renders<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let variant_diff = if t1.variant == t2.variant {
        Some(vec![])
    } else {
        let str_of_variant = |v: &ast::types::RendersVariant| match v {
            ast::types::RendersVariant::Normal => "renders",
            ast::types::RendersVariant::Maybe => "renders?",
            ast::types::RendersVariant::Star => "renders*",
        };
        Some(vec![replace(
            &t1.operator_loc,
            Node::Raw(str_of_variant(&t1.variant).to_string()),
            Node::Raw(str_of_variant(&t2.variant).to_string()),
        )])
    };
    let argument_diff = Some(diff_if_changed(type_, &t1.argument, &t2.argument));
    let comments_diff = syntax_opt(loc, &t1.comments, &t2.comments);
    join_diff_list(vec![variant_diff, argument_diff, comments_diff])
}

fn typeof_type(
    loc: &Loc,
    t1: &ast::types::Typeof<Loc, Loc>,
    t2: &ast::types::Typeof<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let argument_diff = diff_if_changed_ret_opt(typeof_expr, &t1.argument, &t2.argument);
    let targs_diff = diff_if_changed_opt(type_args, &t1.targs, &t2.targs);
    let comments_diff = syntax_opt(loc, &t1.comments, &t2.comments);
    join_diff_list(vec![argument_diff, targs_diff, comments_diff])
}

fn typeof_expr(
    git1: &ast::types::typeof_::Target<Loc, Loc>,
    git2: &ast::types::typeof_::Target<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    match (git1, git2) {
        (
            ast::types::typeof_::Target::Unqualified(id1),
            ast::types::typeof_::Target::Unqualified(id2),
        ) => Some(diff_if_changed(identifier, id1, id2)),
        (
            ast::types::typeof_::Target::Qualified(q1),
            ast::types::typeof_::Target::Qualified(q2),
        ) => {
            let qualification_diff =
                diff_if_changed_ret_opt(typeof_expr, &q1.qualification, &q2.qualification);
            let id_diff = Some(diff_if_changed(identifier, &q1.id, &q2.id));
            join_diff_list(vec![qualification_diff, id_diff])
        }
        _ => None,
    }
}

fn array_type(
    loc: &Loc,
    t1: &ast::types::Array<Loc, Loc>,
    t2: &ast::types::Array<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let argument_diff = Some(diff_if_changed(type_, &t1.argument, &t2.argument));
    let comments_diff = syntax_opt(loc, &t1.comments, &t2.comments);
    join_diff_list(vec![argument_diff, comments_diff])
}

fn union_type(
    loc: &Loc,
    t1: &ast::types::Union<Loc, Loc>,
    t2: &ast::types::Union<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let types1: Vec<_> = {
        let (ref t0, ref t1, ref ts) = t1.types;
        std::iter::once(t0.dupe())
            .chain(std::iter::once(t1.dupe()))
            .chain(ts.iter().duped())
            .collect()
    };
    let types2: Vec<_> = {
        let (ref t0, ref t1, ref ts) = t2.types;
        std::iter::once(t0.dupe())
            .chain(std::iter::once(t1.dupe()))
            .chain(ts.iter().duped())
            .collect()
    };
    let types_diff = diff_and_recurse_nonopt_no_trivial(&|t1, t2| type_(t1, t2), &types1, &types2);
    let comments_diff = syntax_opt(loc, &t1.comments, &t2.comments);
    join_diff_list(vec![types_diff, comments_diff])
}

fn intersection_type(
    loc: &Loc,
    t1: &ast::types::Intersection<Loc, Loc>,
    t2: &ast::types::Intersection<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let types1: Vec<_> = {
        let (ref t0, ref t1, ref ts) = t1.types;
        std::iter::once(t0.dupe())
            .chain(std::iter::once(t1.dupe()))
            .chain(ts.iter().duped())
            .collect()
    };
    let types2: Vec<_> = {
        let (ref t0, ref t1, ref ts) = t2.types;
        std::iter::once(t0.dupe())
            .chain(std::iter::once(t1.dupe()))
            .chain(ts.iter().duped())
            .collect()
    };
    let types_diff = diff_and_recurse_nonopt_no_trivial(&|t1, t2| type_(t1, t2), &types1, &types2);
    let comments_diff = syntax_opt(loc, &t1.comments, &t2.comments);
    join_diff_list(vec![types_diff, comments_diff])
}

fn type_alias(
    loc: &Loc,
    ta1: &ast::statement::TypeAlias<Loc, Loc>,
    ta2: &ast::statement::TypeAlias<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let id_diff = Some(diff_if_changed(identifier, &ta1.id, &ta2.id));
    let t_params_diff = diff_if_changed_opt(type_params, &ta1.tparams, &ta2.tparams);
    let right_diff = Some(diff_if_changed(type_, &ta1.right, &ta2.right));
    let comments_diff = syntax_opt(loc, &ta1.comments, &ta2.comments);
    join_diff_list(vec![id_diff, t_params_diff, right_diff, comments_diff])
}

fn opaque_type(
    loc: &Loc,
    ot1: &ast::statement::OpaqueType<Loc, Loc>,
    ot2: &ast::statement::OpaqueType<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let id_diff = Some(diff_if_changed(identifier, &ot1.id, &ot2.id));
    let t_params_diff = diff_if_changed_opt(type_params, &ot1.tparams, &ot2.tparams);
    let lower_bound_diff = diff_if_changed_nonopt_fn(type_, &ot1.lower_bound, &ot2.lower_bound);
    let upper_bound_diff = diff_if_changed_nonopt_fn(type_, &ot1.upper_bound, &ot2.upper_bound);
    let legacy_upper_bound_diff =
        diff_if_changed_nonopt_fn(type_, &ot1.legacy_upper_bound, &ot2.legacy_upper_bound);
    let impl_type_diff = diff_if_changed_nonopt_fn(type_, &ot1.impl_type, &ot2.impl_type);
    let comments_diff = syntax_opt(loc, &ot1.comments, &ot2.comments);
    join_diff_list(vec![
        id_diff,
        t_params_diff,
        lower_bound_diff,
        upper_bound_diff,
        legacy_upper_bound_diff,
        impl_type_diff,
        comments_diff,
    ])
}

fn declare_class(
    loc: &Loc,
    dc1: &ast::statement::DeclareClass<Loc, Loc>,
    dc2: &ast::statement::DeclareClass<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    if dc1.abstract_ != dc2.abstract_ {
        return None;
    }
    let id_diff = Some(diff_if_changed(identifier, &dc1.id, &dc2.id));
    let t_params_diff = diff_if_changed_opt(type_params, &dc1.tparams, &dc2.tparams);
    let (ref body_loc, ref body1) = dc1.body;
    let (_, ref body2) = dc2.body;
    let body_diff = diff_if_changed_ret_opt(|b1, b2| object_type(body_loc, b1, b2), body1, body2);
    let extends_diff = {
        fn declare_class_extends_with_loc(
            (loc1, ext1): &(Loc, ast::statement::DeclareClassExtends<Loc, Loc>),
            (_loc2, ext2): &(Loc, ast::statement::DeclareClassExtends<Loc, Loc>),
        ) -> Option<Vec<NodeChange>> {
            use ast::statement::DeclareClassExtends;
            match (ext1, ext2) {
                (
                    DeclareClassExtends::ExtendsIdent(gt1),
                    DeclareClassExtends::ExtendsIdent(gt2),
                ) => generic_type(loc1, gt1, gt2),
                (
                    DeclareClassExtends::ExtendsCall {
                        callee: callee1,
                        arg: arg1,
                    },
                    DeclareClassExtends::ExtendsCall {
                        callee: callee2,
                        arg: arg2,
                    },
                ) => {
                    let callee_diff =
                        diff_if_changed_ret_opt(generic_type_with_loc, callee1, callee2);
                    let arg_diff = diff_if_changed_ret_opt(
                        declare_class_extends_with_loc,
                        arg1.as_ref(),
                        arg2.as_ref(),
                    );
                    join_diff_list(vec![callee_diff, arg_diff])
                }
                _ => None,
            }
        }
        diff_if_changed_opt(declare_class_extends_with_loc, &dc1.extends, &dc2.extends)
    };
    let implements_diff = diff_if_changed_opt(class_implements, &dc1.implements, &dc2.implements);
    let comments_diff = syntax_opt(loc, &dc1.comments, &dc2.comments);
    if dc1.mixins != dc2.mixins {
        None
    } else {
        join_diff_list(vec![
            id_diff,
            t_params_diff,
            body_diff,
            extends_diff,
            implements_diff,
            comments_diff,
        ])
    }
}

fn declare_function(
    loc: &Loc,
    func1: &ast::statement::DeclareFunction<Loc, Loc>,
    func2: &ast::statement::DeclareFunction<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    if func1.predicate != func2.predicate || func1.implicit_declare != func2.implicit_declare {
        return None;
    }
    let id_diff = match (&func1.id, &func2.id) {
        (Some(id1), Some(id2)) => Some(diff_if_changed(identifier, id1, id2)),
        (None, None) => Some(vec![]),
        _ => None,
    };
    let annot_diff = Some(diff_if_changed(type_annotation, &func1.annot, &func2.annot));
    let comments_diff = syntax_opt(loc, &func1.comments, &func2.comments);
    join_diff_list(vec![id_diff, annot_diff, comments_diff])
}

fn declare_variable(
    loc: &Loc,
    decl1: &ast::statement::DeclareVariable<Loc, Loc>,
    decl2: &ast::statement::DeclareVariable<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    if decl1.kind != decl2.kind {
        return None;
    }
    let declarations_diff = if decl1.declarations != decl2.declarations {
        diff_and_recurse_no_trivial(
            &|d1, d2| variable_declarator(d1, d2),
            &decl1.declarations,
            &decl2.declarations,
        )
    } else {
        Some(vec![])
    };
    let comments_diff = syntax_opt(loc, &decl1.comments, &decl2.comments);
    join_diff_list(vec![declarations_diff, comments_diff])
}

fn enum_declaration(
    loc: &Loc,
    enum1: &ast::statement::EnumDeclaration<Loc, Loc>,
    enum2: &ast::statement::EnumDeclaration<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::statement::EnumDeclaration {
        id: id1,
        body: body1,
        const_: const1,
        comments: comments1,
    } = enum1;
    let ast::statement::EnumDeclaration {
        id: id2,
        body: body2,
        const_: const2,
        comments: comments2,
    } = enum2;
    if const1 != const2 {
        return None;
    }
    let id_diff = Some(diff_if_changed(identifier, id1, id2));
    let body_diff = enum_body(body1, body2);
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![id_diff, body_diff, comments_diff])
}

fn enum_body(
    body1: &ast::statement::enum_declaration::Body<Loc>,
    body2: &ast::statement::enum_declaration::Body<Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::statement::enum_declaration::Body {
        loc,
        members: members1,
        explicit_type: explicit_type1,
        has_unknown_members: has_unknown_members1,
        comments: comments1,
    } = body1;
    let ast::statement::enum_declaration::Body {
        members: members2,
        explicit_type: explicit_type2,
        has_unknown_members: has_unknown_members2,
        comments: comments2,
        ..
    } = body2;
    if explicit_type1 != explicit_type2 || has_unknown_members1 != has_unknown_members2 {
        return None;
    }
    let members_diff =
        diff_and_recurse_no_trivial(&|m1, m2| enum_member(m1, m2), members1, members2);
    let comments_diff = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![members_diff, comments_diff])
}

fn enum_member(
    member1: &ast::statement::enum_declaration::Member<Loc>,
    member2: &ast::statement::enum_declaration::Member<Loc>,
) -> Option<Vec<NodeChange>> {
    use ast::statement::enum_declaration::Member::*;
    match (member1, member2) {
        (DefaultedMember(m1), DefaultedMember(m2)) => enum_defaulted_member(m1, m2),
        (BooleanMember(m1), BooleanMember(m2)) => enum_boolean_member(m1, m2),
        (NumberMember(m1), NumberMember(m2)) => enum_number_member(m1, m2),
        (StringMember(m1), StringMember(m2)) => enum_string_member(m1, m2),
        (BigIntMember(m1), BigIntMember(m2)) => enum_bigint_member(m1, m2),
        _ => None,
    }
}

fn enum_member_name(
    id1: &ast::statement::enum_declaration::MemberName<Loc>,
    id2: &ast::statement::enum_declaration::MemberName<Loc>,
) -> Option<Vec<NodeChange>> {
    match (id1, id2) {
        (
            ast::statement::enum_declaration::MemberName::Identifier(i1),
            ast::statement::enum_declaration::MemberName::Identifier(i2),
        ) => Some(diff_if_changed(identifier, i1, i2)),
        (
            ast::statement::enum_declaration::MemberName::StringLiteral(loc1, lit1),
            ast::statement::enum_declaration::MemberName::StringLiteral(loc2, lit2),
        ) => diff_if_changed_ret_opt(|l1, l2| string_literal(loc1, loc2, l1, l2), lit1, lit2),
        _ => None,
    }
}

fn enum_defaulted_member(
    member1: &ast::statement::enum_declaration::DefaultedMember<Loc>,
    member2: &ast::statement::enum_declaration::DefaultedMember<Loc>,
) -> Option<Vec<NodeChange>> {
    enum_member_name(&member1.id, &member2.id)
}

fn enum_boolean_member(
    member1: &ast::statement::enum_declaration::InitializedMember<ast::BooleanLiteral<Loc>, Loc>,
    member2: &ast::statement::enum_declaration::InitializedMember<ast::BooleanLiteral<Loc>, Loc>,
) -> Option<Vec<NodeChange>> {
    let (ref loc1, ref lit1) = member1.init;
    let (ref loc2, ref lit2) = member2.init;
    let id_diff = enum_member_name(&member1.id, &member2.id);
    let value_diff =
        diff_if_changed_ret_opt(|l1, l2| boolean_literal(loc1, loc2, l1, l2), lit1, lit2);
    join_diff_list(vec![id_diff, value_diff])
}

fn enum_number_member(
    member1: &ast::statement::enum_declaration::InitializedMember<ast::NumberLiteral<Loc>, Loc>,
    member2: &ast::statement::enum_declaration::InitializedMember<ast::NumberLiteral<Loc>, Loc>,
) -> Option<Vec<NodeChange>> {
    let (ref loc1, ref lit1) = member1.init;
    let (ref loc2, ref lit2) = member2.init;
    let id_diff = enum_member_name(&member1.id, &member2.id);
    let value_diff =
        diff_if_changed_ret_opt(|l1, l2| number_literal(loc1, loc2, l1, l2), lit1, lit2);
    join_diff_list(vec![id_diff, value_diff])
}

fn enum_string_member(
    member1: &ast::statement::enum_declaration::InitializedMember<ast::StringLiteral<Loc>, Loc>,
    member2: &ast::statement::enum_declaration::InitializedMember<ast::StringLiteral<Loc>, Loc>,
) -> Option<Vec<NodeChange>> {
    let (ref loc1, ref lit1) = member1.init;
    let (ref loc2, ref lit2) = member2.init;
    let id_diff = enum_member_name(&member1.id, &member2.id);
    let value_diff =
        diff_if_changed_ret_opt(|l1, l2| string_literal(loc1, loc2, l1, l2), lit1, lit2);
    join_diff_list(vec![id_diff, value_diff])
}

fn enum_bigint_member(
    member1: &ast::statement::enum_declaration::InitializedMember<ast::BigIntLiteral<Loc>, Loc>,
    member2: &ast::statement::enum_declaration::InitializedMember<ast::BigIntLiteral<Loc>, Loc>,
) -> Option<Vec<NodeChange>> {
    let (ref loc1, ref lit1) = member1.init;
    let (ref loc2, ref lit2) = member2.init;
    let id_diff = enum_member_name(&member1.id, &member2.id);
    let value_diff =
        diff_if_changed_ret_opt(|l1, l2| bigint_literal(loc1, loc2, l1, l2), lit1, lit2);
    join_diff_list(vec![id_diff, value_diff])
}

fn type_params(
    tparams1: &ast::types::TypeParams<Loc, Loc>,
    tparams2: &ast::types::TypeParams<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let params_diff = diff_and_recurse_nonopt_no_trivial(
        &|p1, p2| type_param(p1, p2),
        &tparams1.params,
        &tparams2.params,
    );
    let comments_diff = syntax_opt(&tparams1.loc, &tparams1.comments, &tparams2.comments);
    join_diff_list(vec![params_diff, comments_diff])
}

fn type_param(
    t_param1: &ast::types::TypeParam<Loc, Loc>,
    t_param2: &ast::types::TypeParam<Loc, Loc>,
) -> Vec<NodeChange> {
    let variance_diff = diff_if_changed_ret_opt(variance, &t_param1.variance, &t_param2.variance);
    let name_diff = Some(diff_if_changed(identifier, &t_param1.name, &t_param2.name));
    let bound_diff = Some(diff_if_changed(
        type_annotation_hint,
        &t_param1.bound,
        &t_param2.bound,
    ));
    let bound_kind_diff = if t_param1.bound_kind == t_param2.bound_kind {
        Some(vec![])
    } else {
        None
    };
    let default_diff = diff_if_changed_nonopt_fn(type_, &t_param1.default, &t_param2.default);
    let const_diff = if t_param1.const_ == t_param2.const_ {
        Some(vec![])
    } else {
        None
    };
    let result = join_diff_list(vec![
        variance_diff,
        name_diff,
        bound_diff,
        bound_kind_diff,
        default_diff,
        const_diff,
    ]);
    result.unwrap_or_else(|| {
        vec![replace(
            &t_param1.loc,
            Node::TypeParam(t_param1.clone()),
            Node::TypeParam(t_param2.clone()),
        )]
    })
}

fn variance(
    var1: &Option<ast::Variance<Loc>>,
    var2: &Option<ast::Variance<Loc>>,
) -> Option<Vec<NodeChange>> {
    use ast::VarianceKind::*;
    match (var1, var2) {
        (
            Some(ast::Variance {
                kind: Readonly | In | Out | InOut,
                ..
            }),
            Some(_),
        ) => None,
        (Some(v1), Some(v2)) => Some(vec![replace(
            &v1.loc,
            Node::Variance(v1.clone()),
            Node::Variance(v2.clone()),
        )]),
        (Some(v1), None) => Some(vec![delete(&v1.loc, Node::Variance(v1.clone()))]),
        (None, None) => Some(vec![]),
        _ => None,
    }
}

fn type_annotation_hint(
    return1: &ast::types::AnnotationOrHint<Loc, Loc>,
    return2: &ast::types::AnnotationOrHint<Loc, Loc>,
) -> Vec<NodeChange> {
    use ast::types::AnnotationOrHint::*;
    let annot_change = |typ: &ast::types::Annotation<Loc, Loc>| -> Node {
        match return2 {
            Available(annot2) => match &*annot2.annotation {
                ast::types::TypeInner::Function { .. } => Node::FunctionTypeAnnotation(typ.clone()),
                _ => Node::TypeAnnotation(typ.clone()),
            },
            _ => Node::TypeAnnotation(typ.clone()),
        }
    };
    match (return1, return2) {
        (Missing(_), Missing(_)) => vec![],
        (Available(annot1), Missing(_)) => {
            vec![delete(&annot1.loc, Node::TypeAnnotation(annot1.clone()))]
        }
        (Missing(loc1), Available(annot)) => {
            vec![(loc1.dupe(), insert(None, vec![annot_change(annot)]))]
        }
        (Available(annot1), Available(annot2)) => type_annotation(annot1, annot2),
    }
}

fn type_annotation(
    annot1: &ast::types::Annotation<Loc, Loc>,
    annot2: &ast::types::Annotation<Loc, Loc>,
) -> Vec<NodeChange> {
    let replace_function = || {
        vec![replace(
            &annot1.loc,
            Node::TypeAnnotation(annot1.clone()),
            Node::FunctionTypeAnnotation(annot2.clone()),
        )]
    };
    match (&*annot1.annotation, &*annot2.annotation) {
        (
            ast::types::TypeInner::Function { loc, inner: ft1 },
            ast::types::TypeInner::Function { inner: ft2, .. },
        ) => function_type(loc, ft1, ft2).unwrap_or_else(replace_function),
        (_, ast::types::TypeInner::Function { .. }) => replace_function(),
        _ => type_(&annot1.annotation, &annot2.annotation),
    }
}

fn type_guard_annotation(
    guard1: &ast::types::TypeGuardAnnotation<Loc, Loc>,
    guard2: &ast::types::TypeGuardAnnotation<Loc, Loc>,
) -> Vec<NodeChange> {
    type_guard(&guard1.guard, &guard2.guard)
}

fn type_guard(
    grd1: &ast::types::TypeGuard<Loc, Loc>,
    grd2: &ast::types::TypeGuard<Loc, Loc>,
) -> Vec<NodeChange> {
    let ast::types::TypeGuard {
        kind: kind1,
        guard: (x1, t1),
        comments: comments1,
        ..
    } = grd1;
    let ast::types::TypeGuard {
        kind: kind2,
        guard: (x2, t2),
        comments: comments2,
        ..
    } = grd2;
    if kind1 != kind2 || t1 != t2 {
        vec![replace(
            &grd1.loc,
            Node::TypeGuard(grd1.clone()),
            Node::TypeGuard(grd2.clone()),
        )]
    } else {
        let id = diff_if_changed(identifier, x1, x2);
        let comments = syntax_opt(&grd1.loc, comments1, comments2).unwrap_or_default();
        let mut result = id;
        result.extend(comments);
        result
    }
}

fn as_cast(
    loc: &Loc,
    ac1: &expression::AsExpression<Loc, Loc>,
    ac2: &expression::AsExpression<Loc, Loc>,
) -> Vec<NodeChange> {
    let expression::AsExpression {
        expression: expr1,
        annot: annot1,
        comments: comments1,
    } = ac1;
    let expression::AsExpression {
        expression: expr2,
        annot: annot2,
        comments: comments2,
    } = ac2;
    let parent = ExpressionNodeParent::ExpressionParentOfExpression(expression::Expression::new(
        ExpressionInner::AsExpression {
            loc: loc.dupe(),
            inner: Arc::new(ac2.clone()),
        },
    ));
    let expr = diff_if_changed(|e1, e2| expression(&parent, e1, e2), expr1, expr2);
    let annot = diff_if_changed(type_annotation, annot1, annot2);
    let comments = syntax_opt(loc, comments1, comments2).unwrap_or_default();
    let mut result = expr;
    result.extend(annot);
    result.extend(comments);
    result
}

fn type_cast(
    loc: &Loc,
    tc1: &expression::TypeCast<Loc, Loc>,
    tc2: &expression::TypeCast<Loc, Loc>,
) -> Vec<NodeChange> {
    let expression::TypeCast {
        expression: expr1,
        annot: annot1,
        comments: comments1,
    } = tc1;
    let expression::TypeCast {
        expression: expr2,
        annot: annot2,
        comments: comments2,
    } = tc2;
    let parent = ExpressionNodeParent::ExpressionParentOfExpression(expression::Expression::new(
        ExpressionInner::TypeCast {
            loc: loc.dupe(),
            inner: Arc::new(tc2.clone()),
        },
    ));
    let expr = diff_if_changed(|e1, e2| expression(&parent, e1, e2), expr1, expr2);
    let annot = diff_if_changed(type_annotation, annot1, annot2);
    let comments = syntax_opt(loc, comments1, comments2).unwrap_or_default();
    let mut result = expr;
    result.extend(annot);
    result.extend(comments);
    result
}

fn type_cast_added(
    parent: &ExpressionNodeParent,
    expr: &expression::Expression<Loc, Loc>,
    loc: &Loc,
    type_cast: &expression::TypeCast<Loc, Loc>,
) -> Vec<NodeChange> {
    let expression::TypeCast {
        expression: expr2,
        annot: annot2,
        ..
    } = type_cast;
    let expr_diff = diff_if_changed(|e1, e2| expression(parent, e1, e2), expr, expr2);
    let leading_loc = Loc {
        end: loc.start.dupe(),
        ..loc.dupe()
    };
    let leading_insert = (
        leading_loc,
        insert(Some(""), vec![Node::Raw("(".to_string())]),
    );
    let trailing_loc = Loc {
        start: loc.end.dupe(),
        ..loc.dupe()
    };
    let trailing_insert = (
        trailing_loc,
        insert(
            Some(""),
            vec![
                Node::TypeAnnotation(annot2.clone()),
                Node::Raw(")".to_string()),
            ],
        ),
    );
    let mut result = vec![leading_insert];
    result.extend(expr_diff);
    result.push(trailing_insert);
    result
}

fn update(
    loc: &Loc,
    u1: &expression::Update<Loc, Loc>,
    u2: &expression::Update<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let expression::Update {
        operator: op1,
        argument: arg1,
        prefix: p1,
        comments: comments1,
    } = u1;
    let expression::Update {
        operator: op2,
        argument: arg2,
        prefix: p2,
        comments: comments2,
    } = u2;
    if op1 != op2 || p1 != p2 {
        return None;
    }
    let parent = ExpressionNodeParent::ExpressionParentOfExpression(expression::Expression::new(
        ExpressionInner::Update {
            loc: loc.dupe(),
            inner: Arc::new(u2.clone()),
        },
    ));
    let argument = expression(&parent, arg1, arg2);
    let comments = syntax_opt(loc, comments1, comments2).unwrap_or_default();
    let mut result = argument;
    result.extend(comments);
    Some(result)
}

fn this_expression(
    loc: &Loc,
    this1: &expression::This<Loc>,
    this2: &expression::This<Loc>,
) -> Option<Vec<NodeChange>> {
    syntax_opt(loc, &this1.comments, &this2.comments)
}

fn super_expression(
    loc: &Loc,
    super1: &expression::Super<Loc>,
    super2: &expression::Super<Loc>,
) -> Option<Vec<NodeChange>> {
    syntax_opt(loc, &super1.comments, &super2.comments)
}

fn meta_property(
    loc: &Loc,
    meta1: &expression::MetaProperty<Loc>,
    meta2: &expression::MetaProperty<Loc>,
) -> Option<Vec<NodeChange>> {
    let meta = Some(diff_if_changed(identifier, &meta1.meta, &meta2.meta));
    let property = Some(diff_if_changed(
        identifier,
        &meta1.property,
        &meta2.property,
    ));
    let comments = syntax_opt(loc, &meta1.comments, &meta2.comments);
    join_diff_list(vec![meta, property, comments])
}

fn import_expression(
    loc: &Loc,
    import1: &expression::Import<Loc, Loc>,
    import2: &expression::Import<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let expression::Import {
        argument: argument1,
        options: options1,
        phase: phase1,
        comments: comments1,
    } = import1;
    let expression::Import {
        argument: argument2,
        options: options2,
        phase: phase2,
        comments: comments2,
    } = import2;
    if phase1 != phase2 {
        // No edit within the node can turn `import(x)` into
        // `import.defer(x)`; the whole node has to be replaced.
        return None;
    }
    let parent = ExpressionNodeParent::ExpressionParentOfExpression(expression::Expression::new(
        ExpressionInner::Import {
            loc: loc.dupe(),
            inner: Arc::new(import2.clone()),
        },
    ));
    let argument = Some(diff_if_changed(
        |a1, a2| expression(&parent, a1, a2),
        argument1,
        argument2,
    ));
    let options = match (options1, options2) {
        (Some(o1), Some(o2)) => Some(diff_if_changed(
            |o1, o2| expression(&parent, o1, o2),
            o1,
            o2,
        )),
        _ => None,
    };
    let comments = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![argument, options, comments])
}

fn match_expression(
    loc: &Loc,
    m1: &expression::MatchExpression<Loc, Loc>,
    m2: &expression::MatchExpression<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let expression::MatchExpression {
        arg: arg1,
        cases: cases1,
        comments: comments1,
        ..
    } = m1;
    let expression::MatchExpression {
        arg: arg2,
        cases: cases2,
        comments: comments2,
        ..
    } = m2;
    let arg = Some(diff_if_changed(
        |e1, e2| expression(&ExpressionNodeParent::SlotParentOfExpression, e1, e2),
        arg1,
        arg2,
    ));
    let cases =
        diff_and_recurse_no_trivial(&|c1, c2| match_expression_case(c1, c2), cases1, cases2);
    let comments = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![arg, cases, comments])
}

fn match_expression_case(
    c1: &ast::match_::Case<Loc, Loc, expression::Expression<Loc, Loc>>,
    c2: &ast::match_::Case<Loc, Loc, expression::Expression<Loc, Loc>>,
) -> Option<Vec<NodeChange>> {
    let ast::match_::Case {
        loc,
        pattern: pattern1,
        body: body1,
        guard: guard1,
        comments: comments1,
        invalid_syntax: invalid_syntax1,
        ..
    } = c1;
    let ast::match_::Case {
        pattern: pattern2,
        body: body2,
        guard: guard2,
        comments: comments2,
        invalid_syntax: invalid_syntax2,
        ..
    } = c2;
    let pattern = Some(diff_if_changed(match_pattern, pattern1, pattern2));
    let body = Some(diff_if_changed(
        |b1, b2| {
            expression(
                &ExpressionNodeParent::MatchExpressionCaseBodyParentOfExpression,
                b1,
                b2,
            )
        },
        body1,
        body2,
    ));
    let guard = diff_if_changed_nonopt_fn(
        |g1, g2| expression(&ExpressionNodeParent::SlotParentOfExpression, g1, g2),
        guard1,
        guard2,
    );
    let comments = syntax_opt(loc, comments1, comments2);
    let invalid_syntax =
        diff_if_changed_ret_opt(match_case_invalid_syntax, invalid_syntax1, invalid_syntax2);
    join_diff_list(vec![pattern, body, guard, comments, invalid_syntax])
}

fn match_statement(
    loc: &Loc,
    m1: &ast::statement::MatchStatement<Loc, Loc>,
    m2: &ast::statement::MatchStatement<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::statement::MatchStatement {
        arg: arg1,
        cases: cases1,
        comments: comments1,
        ..
    } = m1;
    let ast::statement::MatchStatement {
        arg: arg2,
        cases: cases2,
        comments: comments2,
        ..
    } = m2;
    let arg = Some(diff_if_changed(
        |e1, e2| expression(&ExpressionNodeParent::SlotParentOfExpression, e1, e2),
        arg1,
        arg2,
    ));
    let cases = diff_and_recurse_no_trivial(&|c1, c2| match_statement_case(c1, c2), cases1, cases2);
    let comments = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![arg, cases, comments])
}

fn match_statement_case(
    c1: &ast::match_::Case<Loc, Loc, statement::Statement<Loc, Loc>>,
    c2: &ast::match_::Case<Loc, Loc, statement::Statement<Loc, Loc>>,
) -> Option<Vec<NodeChange>> {
    let ast::match_::Case {
        loc,
        pattern: pattern1,
        body: body1,
        guard: guard1,
        comments: comments1,
        invalid_syntax: invalid_syntax1,
        ..
    } = c1;
    let ast::match_::Case {
        pattern: pattern2,
        body: body2,
        guard: guard2,
        comments: comments2,
        invalid_syntax: invalid_syntax2,
        ..
    } = c2;
    let pattern = Some(diff_if_changed(match_pattern, pattern1, pattern2));
    let body = Some(diff_if_changed(
        |b1, b2| {
            statement(
                &StatementNodeParent::MatchCaseParentOfStatement(loc.dupe()),
                b1,
                b2,
            )
        },
        body1,
        body2,
    ));
    let guard = diff_if_changed_nonopt_fn(
        |g1, g2| expression(&ExpressionNodeParent::SlotParentOfExpression, g1, g2),
        guard1,
        guard2,
    );
    let comments = syntax_opt(loc, comments1, comments2);
    let invalid_syntax =
        diff_if_changed_ret_opt(match_case_invalid_syntax, invalid_syntax1, invalid_syntax2);
    join_diff_list(vec![pattern, body, guard, comments, invalid_syntax])
}

fn match_case_invalid_syntax(
    x1: &ast::match_::InvalidSyntax<Loc>,
    x2: &ast::match_::InvalidSyntax<Loc>,
) -> Option<Vec<NodeChange>> {
    match (x1, x2) {
        (
            ast::match_::InvalidSyntax {
                invalid_prefix_case: None,
                invalid_infix_colon: None,
                invalid_suffix_semicolon: None,
            },
            ast::match_::InvalidSyntax {
                invalid_prefix_case: None,
                invalid_infix_colon: None,
                invalid_suffix_semicolon: None,
            },
        ) => Some(vec![]),
        _ => None,
    }
}

fn match_pattern(
    p1: &ast::match_pattern::MatchPattern<Loc, Loc>,
    p2: &ast::match_pattern::MatchPattern<Loc, Loc>,
) -> Vec<NodeChange> {
    use ast::match_pattern::MatchPattern::*;
    let result = match (p1, p2) {
        (WildcardPattern { loc, inner: w1 }, WildcardPattern { inner: w2, .. }) => {
            if w1.invalid_syntax_default_keyword != w2.invalid_syntax_default_keyword {
                None
            } else {
                diff_if_changed_ret_opt(
                    |c1, c2| syntax_opt(loc, c1, c2),
                    &w1.comments,
                    &w2.comments,
                )
            }
        }
        (
            NumberPattern {
                loc: loc1,
                inner: n1,
            },
            NumberPattern {
                loc: loc2,
                inner: n2,
            },
        ) => diff_if_changed_ret_opt(|l1, l2| number_literal(loc1, loc2, l1, l2), n1, n2),
        (
            BigIntPattern {
                loc: loc1,
                inner: b1,
            },
            BigIntPattern {
                loc: loc2,
                inner: b2,
            },
        ) => diff_if_changed_ret_opt(|l1, l2| bigint_literal(loc1, loc2, l1, l2), b1, b2),
        (
            StringPattern {
                loc: loc1,
                inner: s1,
            },
            StringPattern {
                loc: loc2,
                inner: s2,
            },
        ) => diff_if_changed_ret_opt(|l1, l2| string_literal(loc1, loc2, l1, l2), s1, s2),
        (
            BooleanPattern {
                loc: loc1,
                inner: b1,
            },
            BooleanPattern {
                loc: loc2,
                inner: b2,
            },
        ) => diff_if_changed_ret_opt(|l1, l2| boolean_literal(loc1, loc2, l1, l2), b1, b2),
        (NullPattern { loc, inner: n1 }, NullPattern { inner: n2, .. }) => {
            diff_if_changed_ret_opt(|c1, c2| syntax_opt(loc, c1, c2), n1, n2)
        }
        (UnaryPattern { loc, inner: u1 }, UnaryPattern { inner: u2, .. }) => {
            let ast::match_pattern::UnaryPattern {
                operator: op1,
                argument: arg1,
                comments: comments1,
            } = &**u1;
            let ast::match_pattern::UnaryPattern {
                operator: op2,
                argument: arg2,
                comments: comments2,
            } = &**u2;
            if op1 != op2 {
                None
            } else {
                let argument = match (arg1, arg2) {
                    (
                        (loc1, ast::match_pattern::unary_pattern::Argument::NumberLiteral(n1)),
                        (loc2, ast::match_pattern::unary_pattern::Argument::NumberLiteral(n2)),
                    ) => {
                        diff_if_changed_ret_opt(|l1, l2| number_literal(loc1, loc2, l1, l2), n1, n2)
                    }
                    (
                        (loc1, ast::match_pattern::unary_pattern::Argument::BigIntLiteral(b1)),
                        (loc2, ast::match_pattern::unary_pattern::Argument::BigIntLiteral(b2)),
                    ) => {
                        diff_if_changed_ret_opt(|l1, l2| bigint_literal(loc1, loc2, l1, l2), b1, b2)
                    }
                    _ => None,
                };
                let comments = syntax_opt(loc, comments1, comments2);
                join_diff_list(vec![argument, comments])
            }
        }
        (BindingPattern { loc, inner: b1 }, BindingPattern { inner: b2, .. }) => {
            diff_if_changed_ret_opt(
                |b1, b2| match_binding_pattern(loc, b1, b2),
                b1.as_ref(),
                b2.as_ref(),
            )
        }
        (IdentifierPattern { inner: id1, .. }, IdentifierPattern { inner: id2, .. }) => {
            Some(diff_if_changed(identifier, id1, id2))
        }
        (MemberPattern { loc, inner: m1 }, MemberPattern { inner: m2, .. }) => {
            diff_if_changed_ret_opt(
                |m1, m2| match_member_pattern(loc, m1, m2),
                m1.as_ref(),
                m2.as_ref(),
            )
        }
        (OrPattern { loc, inner: o1 }, OrPattern { inner: o2, .. }) => {
            let ast::match_pattern::OrPattern {
                patterns: patterns1,
                comments: comments1,
            } = &**o1;
            let ast::match_pattern::OrPattern {
                patterns: patterns2,
                comments: comments2,
            } = &**o2;
            let patterns = diff_and_recurse_nonopt_no_trivial(
                &|p1, p2| match_pattern(p1, p2),
                patterns1,
                patterns2,
            );
            let comments = syntax_opt(loc, comments1, comments2);
            join_diff_list(vec![patterns, comments])
        }
        (ArrayPattern { loc, inner: a1 }, ArrayPattern { inner: a2, .. }) => {
            let ast::match_pattern::ArrayPattern {
                elements: elements1,
                rest: rest1,
                comments: comments1,
            } = &**a1;
            let ast::match_pattern::ArrayPattern {
                elements: elements2,
                rest: rest2,
                comments: comments2,
            } = &**a2;
            let elements = diff_and_recurse_nonopt_no_trivial(
                &|e1, e2| match_array_pattern_element(e1, e2),
                elements1,
                elements2,
            );
            let rest = diff_if_changed_opt(match_rest_pattern, rest1, rest2);
            let comments = syntax_opt(loc, comments1, comments2);
            join_diff_list(vec![elements, rest, comments])
        }
        (ObjectPattern { loc, inner: o1 }, ObjectPattern { inner: o2, .. }) => {
            let ast::match_pattern::ObjectPattern {
                properties: props1,
                rest: rest1,
                comments: comments1,
            } = &**o1;
            let ast::match_pattern::ObjectPattern {
                properties: props2,
                rest: rest2,
                comments: comments2,
            } = &**o2;
            let properties = diff_and_recurse_nonopt_no_trivial(
                &|p1, p2| match_object_pattern_property(p1, p2),
                props1,
                props2,
            );
            let rest = diff_if_changed_opt(match_rest_pattern, rest1, rest2);
            let comments = syntax_opt(loc, comments1, comments2);
            join_diff_list(vec![properties, rest, comments])
        }
        (AsPattern { loc, inner: a1 }, AsPattern { inner: a2, .. }) => {
            let ast::match_pattern::AsPattern {
                pattern: pattern1,
                target: t1,
                comments: comments1,
            } = &**a1;
            let ast::match_pattern::AsPattern {
                pattern: pattern2,
                target: t2,
                comments: comments2,
            } = &**a2;
            let pattern = Some(diff_if_changed(match_pattern, pattern1, pattern2));
            let target = match (t1, t2) {
                (
                    ast::match_pattern::as_pattern::Target::Identifier(id1),
                    ast::match_pattern::as_pattern::Target::Identifier(id2),
                ) => Some(diff_if_changed(identifier, id1, id2)),
                (
                    ast::match_pattern::as_pattern::Target::Binding { loc, pattern: b1 },
                    ast::match_pattern::as_pattern::Target::Binding { pattern: b2, .. },
                ) => diff_if_changed_ret_opt(|b1, b2| match_binding_pattern(loc, b1, b2), b1, b2),
                _ => None,
            };
            let comments = syntax_opt(loc, comments1, comments2);
            join_diff_list(vec![pattern, target, comments])
        }
        _ => None,
    };
    let loc = p1.loc();
    result.unwrap_or_else(|| {
        vec![replace(
            loc,
            Node::MatchPattern(p1.clone()),
            Node::MatchPattern(p2.clone()),
        )]
    })
}

fn match_member_pattern(
    loc: &Loc,
    p1: &ast::match_pattern::MemberPattern<Loc, Loc>,
    p2: &ast::match_pattern::MemberPattern<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::match_pattern::MemberPattern {
        base: base1,
        property: property1,
        comments: comments1,
        ..
    } = p1;
    let ast::match_pattern::MemberPattern {
        base: base2,
        property: property2,
        comments: comments2,
        ..
    } = p2;
    let comments = syntax_opt(loc, comments1, comments2);
    let base = match (base1, base2) {
        (
            ast::match_pattern::member_pattern::Base::BaseIdentifier(id1),
            ast::match_pattern::member_pattern::Base::BaseIdentifier(id2),
        ) => Some(diff_if_changed(identifier, id1, id2)),
        (
            ast::match_pattern::member_pattern::Base::BaseMember(m1),
            ast::match_pattern::member_pattern::Base::BaseMember(m2),
        ) => {
            let loc = &m1.loc;
            diff_if_changed_ret_opt(
                |m1, m2| match_member_pattern(loc, m1, m2),
                m1.as_ref(),
                m2.as_ref(),
            )
        }
        _ => None,
    };
    let property = match (property1, property2) {
        (
            ast::match_pattern::member_pattern::Property::PropertyNumber {
                loc: loc1,
                literal: lit1,
            },
            ast::match_pattern::member_pattern::Property::PropertyNumber {
                loc: loc2,
                literal: lit2,
            },
        ) => diff_if_changed_ret_opt(|l1, l2| number_literal(loc1, loc2, l1, l2), lit1, lit2),
        (
            ast::match_pattern::member_pattern::Property::PropertyBigInt {
                loc: loc1,
                literal: lit1,
            },
            ast::match_pattern::member_pattern::Property::PropertyBigInt {
                loc: loc2,
                literal: lit2,
            },
        ) => diff_if_changed_ret_opt(|l1, l2| bigint_literal(loc1, loc2, l1, l2), lit1, lit2),
        (
            ast::match_pattern::member_pattern::Property::PropertyString {
                loc: loc1,
                literal: lit1,
            },
            ast::match_pattern::member_pattern::Property::PropertyString {
                loc: loc2,
                literal: lit2,
            },
        ) => diff_if_changed_ret_opt(|l1, l2| string_literal(loc1, loc2, l1, l2), lit1, lit2),
        (
            ast::match_pattern::member_pattern::Property::PropertyIdentifier(id1),
            ast::match_pattern::member_pattern::Property::PropertyIdentifier(id2),
        ) => Some(diff_if_changed(identifier, id1, id2)),
        _ => None,
    };
    join_diff_list(vec![base, property, comments])
}

fn match_binding_pattern(
    loc: &Loc,
    p1: &ast::match_pattern::BindingPattern<Loc, Loc>,
    p2: &ast::match_pattern::BindingPattern<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::match_pattern::BindingPattern {
        kind: kind1,
        id: id1,
        comments: comments1,
    } = p1;
    let ast::match_pattern::BindingPattern {
        kind: kind2,
        id: id2,
        comments: comments2,
    } = p2;
    if kind1 != kind2 {
        return None;
    }
    let id = Some(diff_if_changed(identifier, id1, id2));
    let comments = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![id, comments])
}

fn match_array_pattern_element(
    e1: &ast::match_pattern::array_pattern::Element<Loc, Loc>,
    e2: &ast::match_pattern::array_pattern::Element<Loc, Loc>,
) -> Vec<NodeChange> {
    let ast::match_pattern::array_pattern::Element { pattern: p1, .. } = e1;
    let ast::match_pattern::array_pattern::Element { pattern: p2, .. } = e2;
    diff_if_changed(match_pattern, p1, p2)
}

fn match_object_pattern_property(
    p1: &ast::match_pattern::object_pattern::Property<Loc, Loc>,
    p2: &ast::match_pattern::object_pattern::Property<Loc, Loc>,
) -> Vec<NodeChange> {
    let result = match (p1, p2) {
        (
            ast::match_pattern::object_pattern::Property::Valid {
                loc,
                property: prop1,
            },
            ast::match_pattern::object_pattern::Property::Valid {
                property: prop2, ..
            },
        ) => {
            if prop1.shorthand != prop2.shorthand {
                None
            } else {
                let key = match (&prop1.key, &prop2.key) {
                    (
                        ast::match_pattern::object_pattern::Key::NumberLiteral((loc1, lit1)),
                        ast::match_pattern::object_pattern::Key::NumberLiteral((loc2, lit2)),
                    ) => diff_if_changed_ret_opt(
                        |l1, l2| number_literal(loc1, loc2, l1, l2),
                        lit1,
                        lit2,
                    ),
                    (
                        ast::match_pattern::object_pattern::Key::BigIntLiteral((loc1, lit1)),
                        ast::match_pattern::object_pattern::Key::BigIntLiteral((loc2, lit2)),
                    ) => diff_if_changed_ret_opt(
                        |l1, l2| bigint_literal(loc1, loc2, l1, l2),
                        lit1,
                        lit2,
                    ),
                    (
                        ast::match_pattern::object_pattern::Key::StringLiteral((loc1, lit1)),
                        ast::match_pattern::object_pattern::Key::StringLiteral((loc2, lit2)),
                    ) => diff_if_changed_ret_opt(
                        |l1, l2| string_literal(loc1, loc2, l1, l2),
                        lit1,
                        lit2,
                    ),
                    (
                        ast::match_pattern::object_pattern::Key::Identifier(id1),
                        ast::match_pattern::object_pattern::Key::Identifier(id2),
                    ) => Some(diff_if_changed(identifier, id1, id2)),
                    _ => None,
                };
                let pattern = Some(diff_if_changed(
                    match_pattern,
                    &prop1.pattern,
                    &prop2.pattern,
                ));
                let comments = syntax_opt(loc, &prop1.comments, &prop2.comments);
                join_diff_list(vec![key, pattern, comments])
            }
        }
        _ => None,
    };
    let loc = p1.loc();
    result.unwrap_or_else(|| {
        vec![replace(
            loc,
            Node::MatchObjectPatternProperty(p1.clone()),
            Node::MatchObjectPatternProperty(p2.clone()),
        )]
    })
}

fn match_rest_pattern(
    r1: &ast::match_pattern::RestPattern<Loc, Loc>,
    r2: &ast::match_pattern::RestPattern<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let ast::match_pattern::RestPattern {
        loc,
        argument: arg1,
        comments: comments1,
    } = r1;
    let ast::match_pattern::RestPattern {
        argument: arg2,
        comments: comments2,
        ..
    } = r2;
    let argument = diff_if_changed_opt(
        |a1: &(Loc, ast::match_pattern::BindingPattern<Loc, Loc>),
         a2: &(Loc, ast::match_pattern::BindingPattern<Loc, Loc>)| {
            let (loc, b1) = a1;
            let (_, b2) = a2;
            match_binding_pattern(loc, b1, b2)
        },
        arg1,
        arg2,
    );
    let comments = syntax_opt(loc, comments1, comments2);
    join_diff_list(vec![argument, comments])
}

fn computed_key(
    loc: &Loc,
    computed1: &ast::ComputedKey<Loc, Loc>,
    computed2: &ast::ComputedKey<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    let expression_diff = Some(diff_if_changed(
        |e1, e2| expression(&ExpressionNodeParent::SlotParentOfExpression, e1, e2),
        &computed1.expression,
        &computed2.expression,
    ));
    let comments_diff = syntax_opt(loc, &computed1.comments, &computed2.comments);
    join_diff_list(vec![expression_diff, comments_diff])
}

fn private_name(
    loc: &Loc,
    id1: &ast::PrivateName<Loc>,
    id2: &ast::PrivateName<Loc>,
) -> Vec<NodeChange> {
    let name = if id1.name == id2.name {
        vec![]
    } else {
        vec![replace(
            loc,
            Node::Raw(format!("#{}", id1.name)),
            Node::Raw(format!("#{}", id2.name)),
        )]
    };
    let comments = syntax_opt(loc, &id1.comments, &id2.comments).unwrap_or_default();
    let mut result = comments;
    result.extend(name);
    result
}

fn export_assignment(
    loc: &Loc,
    stmt2: &statement::Statement<Loc, Loc>,
    a1: &ast::statement::ExportAssignment<Loc, Loc>,
    a2: &ast::statement::ExportAssignment<Loc, Loc>,
) -> Vec<NodeChange> {
    let rhs_diff = match (&a1.rhs, &a2.rhs) {
        (
            ast::statement::ExportAssignmentRhs::Expression(expr1),
            ast::statement::ExportAssignmentRhs::Expression(expr2),
        ) => diff_if_changed(
            |e1, e2| {
                expression(
                    &ExpressionNodeParent::StatementParentOfExpression(stmt2.dupe()),
                    e1,
                    e2,
                )
            },
            expr1,
            expr2,
        ),
        _ => {
            let s1 = statement::Statement::new(StatementInner::ExportAssignment {
                loc: loc.dupe(),
                inner: Arc::new(a1.clone()),
            });
            let s2 = statement::Statement::new(StatementInner::ExportAssignment {
                loc: loc.dupe(),
                inner: Arc::new(a2.clone()),
            });
            vec![replace(
                loc,
                Node::Statement(s1, StatementNodeParent::TopLevelParentOfStatement),
                Node::Statement(s2, StatementNodeParent::TopLevelParentOfStatement),
            )]
        }
    };
    let comments_diff = syntax_opt(loc, &a1.comments, &a2.comments).unwrap_or_default();
    let mut result = rhs_diff;
    result.extend(comments_diff);
    result
}

fn namespace_export_declaration(
    loc: &Loc,
    d1: &ast::statement::NamespaceExportDeclaration<Loc, Loc>,
    d2: &ast::statement::NamespaceExportDeclaration<Loc, Loc>,
) -> Vec<NodeChange> {
    let id_diff = diff_if_changed(identifier, &d1.id, &d2.id);
    let comments_diff = syntax_opt(loc, &d1.comments, &d2.comments).unwrap_or_default();
    let mut result = id_diff;
    result.extend(comments_diff);
    result
}

fn import_equals_declaration(
    loc: &Loc,
    d1: &ast::statement::ImportEqualsDeclaration<Loc, Loc>,
    d2: &ast::statement::ImportEqualsDeclaration<Loc, Loc>,
) -> Option<Vec<NodeChange>> {
    if d1.import_kind != d2.import_kind
        || d1.is_export != d2.is_export
        || d1.module_reference != d2.module_reference
    {
        return None;
    }
    let id_diff = diff_if_changed(identifier, &d1.id, &d2.id);
    let comments_diff = syntax_opt(loc, &d1.comments, &d2.comments).unwrap_or_default();
    let mut result = id_diff;
    result.extend(comments_diff);
    Some(result)
}

fn empty_statement(
    loc: &Loc,
    empty1: &ast::statement::Empty<Loc>,
    empty2: &ast::statement::Empty<Loc>,
) -> Option<Vec<NodeChange>> {
    syntax_opt(loc, &empty1.comments, &empty2.comments)
}
