/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::sync::Arc;

use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_type::type_util::ConstraintNodeId;
pub(super) use flow_typing_type::type_util::constraint_node_id;

use super::helpers::*;
use super::*;

pub(super) fn frozen_implicit_instantiation_tvar(
    cx: &mut Context<'_>,
    env: &FlowJsEnv,
    t: &Type,
) -> Option<Type> {
    let node = constraint_node_id(t)?;
    frozen_constraint_node(cx, env, node).map(|frozen| frozen.tvar)
}

struct FrozenConstraintNode {
    root_id: i32,
    tvar: Type,
}

struct ActiveConstraintNode {
    root_id: i32,
    constraints: constraint::Constraints<Cx>,
}

enum ConstraintNodeState {
    Active(ActiveConstraintNode),
    Frozen(FrozenConstraintNode),
}

fn frozen_constraint_node(
    cx: &mut Context<'_>,
    env: &FlowJsEnv,
    node: ConstraintNodeId,
) -> Option<FrozenConstraintNode> {
    let root_id = cx.find_constraints(node.id()).0;
    let tvar = env.frozen_implicit_instantiation_tvar(|id| cx.find_constraints(id).0 == root_id)?;
    Some(FrozenConstraintNode { root_id, tvar })
}

pub(super) fn active_constraint_node(
    cx: &mut Context<'_>,
    env: &FlowJsEnv,
    t: &Type,
) -> Option<ConstraintNodeId> {
    let node = constraint_node_id(t)?;
    frozen_constraint_node(cx, env, node)
        .is_none()
        .then_some(node)
}

pub(super) fn constraint_node_constraints<'cx>(
    cx: &mut Context<'cx>,
    node: ConstraintNodeId,
) -> (i32, constraint::Constraints<Cx>) {
    cx.find_constraints(node.id())
}

fn constraint_node_state<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    node: ConstraintNodeId,
) -> ConstraintNodeState {
    let (root_id, constraints) = cx.find_constraints(node.id());
    match frozen_constraint_node(cx, env, node) {
        Some(frozen) => ConstraintNodeState::Frozen(frozen),
        None => ConstraintNodeState::Active(ActiveConstraintNode {
            root_id,
            constraints,
        }),
    }
}

fn constraint_node_root<'cx>(
    cx: &mut Context<'cx>,
    node: ConstraintNodeId,
) -> (ConstraintNodeId, Root<constraint::Constraints<Cx>>) {
    let (root_id, root) = match node {
        ConstraintNodeId::OpenT(id) => cx.find_root(id),
        ConstraintNodeId::Inference(id) => cx.find_inference_root(id),
    };
    let root_node = if cx.is_inference_node(root_id) {
        ConstraintNodeId::Inference(root_id)
    } else {
        ConstraintNodeId::OpenT(root_id)
    };
    (root_node, root)
}

fn set_constraint_node_constraints<'cx>(
    cx: &mut Context<'cx>,
    node: ConstraintNodeId,
    constraints: constraint::Constraints<Cx>,
) {
    match node {
        ConstraintNodeId::OpenT(id) => cx.set_root_constraints(id, constraints),
        ConstraintNodeId::Inference(id) => cx.set_inference_root_constraints(id, constraints),
    }
}

fn set_constraint_node_rank(cx: &mut Context<'_>, node: ConstraintNodeId, rank: i32) {
    match node {
        ConstraintNodeId::OpenT(id) => cx.set_root_rank(id, rank),
        ConstraintNodeId::Inference(id) => cx.set_inference_root_rank(id, rank),
    }
}

fn set_constraint_node_goto(
    cx: &mut Context<'_>,
    node: ConstraintNodeId,
    parent: ConstraintNodeId,
) {
    match node {
        ConstraintNodeId::OpenT(id) => match parent {
            ConstraintNodeId::OpenT(parent_id) => {
                cx.add_tvar(id, Node::create_goto(parent_id));
            }
            ConstraintNodeId::Inference(parent_id) => {
                // Preserve the chosen root while keeping production nodes out of the side store.
                let (parent_id, parent_root) = cx.find_inference_root(parent_id);
                cx.set_root_constraints(id, parent_root.constraints);
                cx.set_root_rank(id, parent_root.rank);
                cx.set_inference_goto(parent_id, id);
            }
        },
        ConstraintNodeId::Inference(id) => cx.set_inference_goto(id, parent.id()),
    }
}

// Bounds Manipulation
//
// The following general considerations apply when manipulating bounds.
//
// 1. All type variables start out as roots, but some of them eventually become
// goto nodes. As such, bounds of roots may contain goto nodes. However, we
// never perform operations directly on goto nodes; instead, we perform those
// operations on their roots. It is tempting to replace goto nodes proactively
// with their roots to avoid this issue, but doing so may be expensive, whereas
// the union-find data structure amortizes the cost of looking up roots.
//
// 2. Another issue is that while the bounds of a type variable start out
// empty, and in particular do not contain the type variable itself, eventually
// other type variables in the bounds may be unified with the type variable. We
// do not remove these type variables proactively, but instead filter them out
// when considering the bounds. In the future we might consider amortizing the
// cost of this filtering.
//
// 3. When roots are resolved, they act like the corresponding concrete
// types. We maintain the invariant that whenever lower bounds or upper bounds
// contain resolved roots, they also contain the corresponding concrete types.
//
// 4. When roots are unresolved (they have lower bounds and upper bounds,
// possibly consisting of concrete types as well as type variables), we
// maintain the invarant that every lower bound has already been propagated to
// every upper bound. We also maintain the invariant that the bounds are
// transitively closed modulo equivalence: for every type variable in the
// bounds, all the bounds of its root are also included.

/// for each l in ls: l => u
pub(super) fn flows_to_t<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    ls: &BTreeMap<Type, (DepthTrace, UseOp)>,
    u: &UseT<Cx>,
) -> Result<(), FlowJsException> {
    for (l, (trace_l, use_op)) in ls.iter() {
        let u = flow_use_op(env, use_op.dupe(), u.dupe());
        join_flow(cx, env, &[*trace_l, trace], (l, &u))?;
    }
    Ok(())
}

/// for each u in us: l => u
pub(super) fn flows_from_t<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    new_use_op: &UseOp,
    l: &Type,
    us: &BTreeMap<constraint::UseTypeKey<Cx>, DepthTrace>,
) -> Result<(), FlowJsException> {
    for (use_type_key, trace_u) in us.iter() {
        let u = flow_use_op(env, new_use_op.dupe(), use_type_key.use_t.dupe());
        join_flow(cx, env, &[trace, *trace_u], (l, &u))?;
    }
    Ok(())
}

// for each l in ls, u in us: l => u
pub(super) fn flows_across<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    ls: &BTreeMap<Type, (DepthTrace, UseOp)>,
    us: &BTreeMap<constraint::UseTypeKey<Cx>, DepthTrace>,
) -> Result<(), FlowJsException> {
    for (l, (trace_l, use_op_prime)) in ls.iter() {
        for (use_type_key, trace_u) in us.iter() {
            let u = flow_use_op(
                env,
                use_op_prime.dupe(),
                flow_use_op(env, use_op.dupe(), use_type_key.use_t.dupe()),
            );
            join_flow(cx, env, &[*trace_l, trace, *trace_u], (l, &u))?;
        }
    }
    Ok(())
}

fn flow_active_constraint_nodes<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    (id1, constraints1): (i32, constraint::Constraints<Cx>),
    (id2, constraints2): (i32, constraint::Constraints<Cx>),
) -> Result<(), FlowJsException> {
    match (constraints1, constraints2) {
        (
            constraint::Constraints::Unresolved(bounds1),
            constraint::Constraints::Unresolved(bounds2),
        ) => {
            if not_linked((id1, &bounds1), (id2, &bounds2)) {
                let (lower, upper) = {
                    let bounds1 = bounds1.borrow();
                    let bounds2 = bounds2.borrow();
                    (bounds1.lower.clone(), bounds2.upper.clone())
                };
                add_upper_edges(
                    cx,
                    env,
                    trace,
                    use_op.dupe(),
                    false,
                    (id1, &bounds1),
                    (id2, &bounds2),
                );
                add_lower_edges(
                    cx,
                    env,
                    trace,
                    use_op.dupe(),
                    false,
                    (id1, &bounds1),
                    (id2, &bounds2),
                );
                flows_across(cx, env, trace, use_op, &lower, &upper)?;
            }
        }
        (constraint::Constraints::Unresolved(bounds1), constraint::Constraints::Resolved(t2)) => {
            let t2_use = flow_use_op(
                env,
                unknown_use(),
                UseT::new(UseTInner::UseT(use_op, t2.dupe())),
            );
            edges_and_flows_to_t(cx, env, trace, false, (id1, &bounds1), &t2_use)?;
        }
        (
            constraint::Constraints::Unresolved(bounds1),
            constraint::Constraints::FullyResolved(s2),
        ) => {
            let t2_use = flow_use_op(
                env,
                unknown_use(),
                UseT::new(UseTInner::UseT(use_op, cx.force_fully_resolved_tvar(&s2))),
            );
            edges_and_flows_to_t(cx, env, trace, false, (id1, &bounds1), &t2_use)?;
        }
        (constraint::Constraints::Resolved(t1), constraint::Constraints::Unresolved(bounds2)) => {
            edges_and_flows_from_t(cx, env, trace, use_op, false, &t1, (id2, &bounds2))?;
        }
        (
            constraint::Constraints::FullyResolved(s1),
            constraint::Constraints::Unresolved(bounds2),
        ) => {
            let t1 = cx.force_fully_resolved_tvar(&s1);
            edges_and_flows_from_t(cx, env, trace, use_op, false, &t1, (id2, &bounds2))?;
        }
        (constraint::Constraints::Resolved(t1), constraint::Constraints::Resolved(t2)) => {
            let t2_use = flow_use_op(
                env,
                unknown_use(),
                UseT::new(UseTInner::UseT(use_op, t2.dupe())),
            );
            rec_flow(cx, env, trace, (&t1, &t2_use))?;
        }
        (constraint::Constraints::Resolved(t1), constraint::Constraints::FullyResolved(s2)) => {
            let t2_use = flow_use_op(
                env,
                unknown_use(),
                UseT::new(UseTInner::UseT(use_op, cx.force_fully_resolved_tvar(&s2))),
            );
            rec_flow(cx, env, trace, (&t1, &t2_use))?;
        }
        (constraint::Constraints::FullyResolved(s1), constraint::Constraints::Resolved(t2)) => {
            let t2_use = flow_use_op(
                env,
                unknown_use(),
                UseT::new(UseTInner::UseT(use_op, t2.dupe())),
            );
            let t1 = cx.force_fully_resolved_tvar(&s1);
            rec_flow(cx, env, trace, (&t1, &t2_use))?;
        }
        (
            constraint::Constraints::FullyResolved(s1),
            constraint::Constraints::FullyResolved(s2),
        ) => {
            let t2_use = flow_use_op(
                env,
                unknown_use(),
                UseT::new(UseTInner::UseT(use_op, cx.force_fully_resolved_tvar(&s2))),
            );
            let t1 = cx.force_fully_resolved_tvar(&s1);
            rec_flow(cx, env, trace, (&t1, &t2_use))?;
        }
    }
    Ok(())
}

pub(super) fn flow_active_constraint_node_to_use<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    (id, constraints): (i32, constraint::Constraints<Cx>),
    u: &UseT<Cx>,
) -> Result<Option<(Type, UseT<Cx>)>, FlowJsException> {
    match constraints {
        constraint::Constraints::Unresolved(bounds) => {
            edges_and_flows_to_t(cx, env, trace, false, (id, &bounds), u)?;
            Ok(None)
        }
        constraint::Constraints::Resolved(t) => Ok(Some((t, u.dupe()))),
        constraint::Constraints::FullyResolved(s) => {
            Ok(Some((cx.force_fully_resolved_tvar(&s), u.dupe())))
        }
    }
}

fn flow_type_to_active_constraint_node<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    l: &Type,
    (id, constraints): (i32, constraint::Constraints<Cx>),
) -> Result<Option<(Type, UseT<Cx>)>, FlowJsException> {
    match constraints {
        constraint::Constraints::Unresolved(bounds) => {
            edges_and_flows_from_t(cx, env, trace, use_op, false, l, (id, &bounds))?;
            Ok(None)
        }
        constraint::Constraints::Resolved(t) => {
            Ok(Some((l.dupe(), UseT::new(UseTInner::UseT(use_op, t)))))
        }
        constraint::Constraints::FullyResolved(s) => Ok(Some((
            l.dupe(),
            UseT::new(UseTInner::UseT(use_op, cx.force_fully_resolved_tvar(&s))),
        ))),
    }
}

fn flow_type_to_frozen_constraint_node<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: UseOp,
    l: &Type,
    frozen: &FrozenConstraintNode,
) -> Result<(), FlowJsException> {
    if matches!(
        l.deref(),
        TypeInner::AnyT(..)
            | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
                kind: ImplicitInstantiationTvarKind::Placeholder,
                ..
            })
    ) || matches!(l.deref(), TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::EmptyT))
    {
        return Ok(());
    }

    flow_js_utils::add_output_with_env(
        cx,
        env,
        flow_js_utils::incompatible_types_error(l, &frozen.tvar, use_op, None),
    )
}

pub(super) fn flow_constraint_nodes<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    node1: ConstraintNodeId,
    node2: ConstraintNodeId,
) -> Result<(), FlowJsException> {
    let state1 = constraint_node_state(cx, env, node1);
    let state2 = constraint_node_state(cx, env, node2);
    match (state1, state2) {
        (
            ConstraintNodeState::Active(ActiveConstraintNode {
                root_id: id1,
                constraints: constraints1,
            }),
            ConstraintNodeState::Active(ActiveConstraintNode {
                root_id: id2,
                constraints: constraints2,
            }),
        ) => flow_active_constraint_nodes(
            cx,
            env,
            trace,
            use_op,
            (id1, constraints1),
            (id2, constraints2),
        ),
        (ConstraintNodeState::Frozen(frozen1), ConstraintNodeState::Frozen(frozen2))
            if frozen1.root_id == frozen2.root_id =>
        {
            Ok(())
        }
        (ConstraintNodeState::Frozen(frozen1), ConstraintNodeState::Frozen(frozen2)) => {
            flow_type_to_frozen_constraint_node(cx, env, use_op, &frozen1.tvar, &frozen2)
        }
        (
            ConstraintNodeState::Frozen(frozen),
            ConstraintNodeState::Active(ActiveConstraintNode {
                root_id,
                constraints,
            }),
        ) => {
            if let Some((l, u)) = flow_type_to_active_constraint_node(
                cx,
                env,
                trace,
                use_op,
                &frozen.tvar,
                (root_id, constraints),
            )? {
                rec_flow(cx, env, trace, (&l, &u))?;
            }
            Ok(())
        }
        (
            ConstraintNodeState::Active(ActiveConstraintNode {
                root_id,
                constraints,
            }),
            ConstraintNodeState::Frozen(frozen),
        ) => {
            let u = UseT::new(UseTInner::UseT(use_op, frozen.tvar));
            if let Some((l, u)) =
                flow_active_constraint_node_to_use(cx, env, trace, (root_id, constraints), &u)?
            {
                rec_flow(cx, env, trace, (&l, &u))?;
            }
            Ok(())
        }
    }
}

pub(super) fn flow_type_to_constraint_node<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    l: &Type,
    node: ConstraintNodeId,
) -> Result<Option<(Type, UseT<Cx>)>, FlowJsException> {
    match constraint_node_state(cx, env, node) {
        ConstraintNodeState::Active(ActiveConstraintNode {
            root_id,
            constraints,
        }) => {
            flow_type_to_active_constraint_node(cx, env, trace, use_op, l, (root_id, constraints))
        }
        ConstraintNodeState::Frozen(frozen) => {
            flow_type_to_frozen_constraint_node(cx, env, use_op, l, &frozen)?;
            Ok(None)
        }
    }
}

/// bounds.upper += u
fn add_upper_to_bounds(
    env: &FlowJsEnv,
    bounds: &constraint::BoundsRef<Cx>,
    u: &UseT<Cx>,
    trace: DepthTrace,
) {
    let key = constraint::UseTypeKey {
        use_t: u.dupe(),
        assoc: env.speculation_id().map(|s| (s.speculation_id, s.case_id)),
    };
    let mut bounds = bounds.borrow_mut();
    if bounds
        .upper
        .get(&key)
        .is_some_and(|existing| *existing == trace)
    {
        return;
    }
    bounds.upper.insert(key, trace);
}

/// bounds.lower += l
fn add_lower_to_bounds(
    bounds: &constraint::BoundsRef<Cx>,
    l: &Type,
    trace: DepthTrace,
    use_op: UseOp,
) {
    let mut bounds = bounds.borrow_mut();
    if bounds
        .lower
        .get(l)
        .is_some_and(|(existing_trace, existing_use_op)| {
            *existing_trace == trace && existing_use_op == &use_op
        })
    {
        return;
    }
    bounds.lower.insert(l.dupe(), (trace, use_op));
}

/// Given a map of bindings from tvars to traces, a tvar to skip, and an `each`
/// function taking a tvar and its associated trace, apply `each` to all
/// unresolved root constraints reached from the bound tvars, except those of
/// skip_tvar. (Typically skip_tvar is a tvar that will be processed separately,
/// so we don't want to redo that work. We also don't want to consider any tvar
/// that has already been resolved, because the resolved type will be processed
/// separately, too, as part of the bounds of skip_tvar.
pub(super) fn iter_with_filter<'cx, F>(
    cx: &mut Context<'cx>,
    bindings: &BTreeMap<i32, (DepthTrace, UseOp)>,
    skip_id: i32,
    each: F,
) where
    F: Fn(&mut Context<'cx>, i32, constraint::BoundsRef<Cx>, &DepthTrace, &UseOp),
{
    for (id, (trace, use_op)) in bindings.iter() {
        let (root_id, constraints) = cx.find_constraints(*id);
        match constraints {
            constraint::Constraints::Unresolved(bounds) if root_id != skip_id => {
                each(cx, root_id, bounds, trace, use_op);
            }
            _ => {}
        }
    }
}

/// Given [edges_to_t(.., (id1, bounds1), t2)], for each [id] in [id1] + [bounds1.lowertvars],
/// [id.bounds.upper += t2]. When going through [bounds1.lowertvars], filter out [id1].
///
/// As an optimization, skip [id1] when it will become either a resolved root or a
/// goto node (so that updating its bounds is unnecessary).
pub(super) fn edges_to_t<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    opt: bool,
    (id1, bounds1): (i32, &constraint::BoundsRef<Cx>),
    t2: &UseT<Cx>,
) {
    if !opt {
        add_upper_to_bounds(env, bounds1, t2, trace);
    }
    let bounds1 = bounds1.borrow();
    iter_with_filter(
        cx,
        &bounds1.lowertvars,
        id1,
        |_cx, _root_id, bounds, trace_l, use_op| {
            let t2 = flow_use_op(env, use_op.dupe(), t2.dupe());
            add_upper_to_bounds(
                env,
                &bounds,
                &t2,
                DepthTrace::concat_trace(&[*trace_l, trace]),
            );
        },
    );
}

/// Given [edges_from_t(.., t1, (id2, bounds2))], for each [id] in [id2] + [bounds2.uppertvars],
/// [id.bounds.lower += t1]. When going through [bounds2.uppertvars], filter out [id2].
///
/// As an optimization, skip [id2] when it will become either a resolved root or a
/// goto node (so that updating its bounds is unnecessary).
pub(super) fn edges_from_t<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    new_use_op: &UseOp,
    opt: bool,
    t1: &Type,
    (id2, bounds2): (i32, &constraint::BoundsRef<Cx>),
) {
    if !opt {
        add_lower_to_bounds(bounds2, t1, trace, new_use_op.dupe());
    }
    let bounds2 = bounds2.borrow();
    iter_with_filter(
        cx,
        &bounds2.uppertvars,
        id2,
        |_cx, _root_id, bounds, trace_u, use_op| {
            let use_op = pick_use_op(env, new_use_op, use_op);
            add_lower_to_bounds(
                &bounds,
                t1,
                DepthTrace::concat_trace(&[trace, *trace_u]),
                use_op,
            );
        },
    );
}

/// for each [id'] in [id] + [bounds.lowertvars], [id'.bounds.upper += us]
pub(super) fn edges_to_ts<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    new_use_op: &UseOp,
    opt: bool,
    (id, bounds): (i32, &constraint::BoundsRef<Cx>),
    us: &BTreeMap<constraint::UseTypeKey<Cx>, DepthTrace>,
) {
    for (use_type_key, trace_u) in us.iter() {
        let u = flow_use_op(env, new_use_op.dupe(), use_type_key.use_t.dupe());
        edges_to_t(
            cx,
            env,
            DepthTrace::concat_trace(&[trace, *trace_u]),
            opt,
            (id, bounds),
            &u,
        );
    }
}

/// for each [id'] in [id] + [bounds.uppertvars], [id'.bounds.lower += ls]
pub(super) fn edges_from_ts<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    new_use_op: &UseOp,
    opt: bool,
    ls: &BTreeMap<Type, (DepthTrace, UseOp)>,
    (id, bounds): (i32, &constraint::BoundsRef<Cx>),
) {
    for (l, (trace_l, use_op)) in ls.iter() {
        let new_use_op = pick_use_op(env, use_op, new_use_op);
        edges_from_t(
            cx,
            env,
            DepthTrace::concat_trace(&[*trace_l, trace]),
            &new_use_op,
            opt,
            l,
            (id, bounds),
        );
    }
}

/// for each [id] in [id1] + [bounds1.lowertvars]:
///       id.bounds.upper += t2
///       for each l in bounds1.lower: l => t2
///
///   As an invariant, [bounds1.lower] should already contain [id.bounds.lower] for
///   each id in [bounds1.lowertvars].
pub(super) fn edges_and_flows_to_t<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    opt: bool,
    (id1, bounds1): (i32, &constraint::BoundsRef<Cx>),
    t2: &UseT<Cx>,
) -> Result<(), FlowJsException> {
    // Skip iff edge exists as part of the speculation path to the current branch
    let skip = {
        let bounds1 = bounds1.borrow();
        env.speculation_path().any(|(speculation_id, case_id)| {
            bounds1.upper.contains_key(&constraint::UseTypeKey {
                use_t: t2.dupe(),
                assoc: Some((speculation_id, case_id)),
            })
        })
    } || bounds1
        .borrow()
        .upper
        .contains_key(&constraint::UseTypeKey {
            use_t: t2.dupe(),
            assoc: None,
        });
    if !skip {
        let lower = bounds1.borrow().lower.clone();
        edges_to_t(cx, env, trace, opt, (id1, bounds1), t2);
        flows_to_t(cx, env, trace, &lower, t2)?;
    }
    Ok(())
}

/// for each [id] in [id2] + [bounds2.uppertvars]:
///       id.bounds.lower += t1
///       for each u in bounds2.upper: t1 => u
///
/// As an invariant, [bounds2.upper] should already contain [id.bounds.upper] for
/// each id in [bounds2.uppertvars].
pub(super) fn edges_and_flows_from_t<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    new_use_op: UseOp,
    opt: bool,
    t1: &Type,
    (id2, bounds2): (i32, &constraint::BoundsRef<Cx>),
) -> Result<(), FlowJsException> {
    if !bounds2.borrow().lower.contains_key(t1) {
        let upper = bounds2.borrow().upper.clone();
        edges_from_t(cx, env, trace, &new_use_op, opt, t1, (id2, bounds2));
        flows_from_t(cx, env, trace, &new_use_op, t1, &upper)?;
    }
    Ok(())
}

/// bounds.uppertvars += id
fn add_uppertvar_to_bounds(
    bounds: &constraint::BoundsRef<Cx>,
    id: i32,
    trace: DepthTrace,
    use_op: UseOp,
) {
    let mut bounds = bounds.borrow_mut();
    if bounds
        .uppertvars
        .get(&id)
        .is_some_and(|(existing_trace, existing_use_op)| {
            *existing_trace == trace && existing_use_op == &use_op
        })
    {
        return;
    }
    bounds.uppertvars.insert(id, (trace, use_op));
}

/// bounds.lowertvars += id
fn add_lowertvar_to_bounds(
    bounds: &constraint::BoundsRef<Cx>,
    id: i32,
    trace: DepthTrace,
    use_op: UseOp,
) {
    let mut bounds = bounds.borrow_mut();
    if bounds
        .lowertvars
        .get(&id)
        .is_some_and(|(existing_trace, existing_use_op)| {
            *existing_trace == trace && existing_use_op == &use_op
        })
    {
        return;
    }
    bounds.lowertvars.insert(id, (trace, use_op));
}

/// for each [id] in [id1] + [bounds1.lowertvars]:
///       id.bounds.uppertvars += id2
///
/// When going through [bounds1.lowertvars], filter out [id1].
///
/// As an optimization, skip id1 when it will become either a resolved root or a
/// goto node (so that updating its bounds is unnecessary).
pub(super) fn edges_to_tvar<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    new_use_op: &UseOp,
    opt: bool,
    (id1, bounds1): (i32, &constraint::BoundsRef<Cx>),
    id2: i32,
) {
    if !opt {
        add_uppertvar_to_bounds(bounds1, id2, trace, new_use_op.dupe());
    }
    let bounds1 = bounds1.borrow();
    iter_with_filter(
        cx,
        &bounds1.lowertvars,
        id1,
        |_cx, _root_id, bounds, trace_l, use_op| {
            let use_op = pick_use_op(env, use_op, new_use_op);
            add_uppertvar_to_bounds(
                &bounds,
                id2,
                DepthTrace::concat_trace(&[*trace_l, trace]),
                use_op,
            );
        },
    );
}

/// for each id in id2 + bounds2.uppertvars:
///       id.bounds.lowertvars += id1
///
/// When going through bounds2.uppertvars, filter out id2.
///
/// As an optimization, skip id2 when it will become either a resolved root or a
/// goto node (so that updating its bounds is unnecessary).
pub(super) fn edges_from_tvar<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    new_use_op: &UseOp,
    opt: bool,
    id1: i32,
    (id2, bounds2): (i32, &constraint::BoundsRef<Cx>),
) {
    if !opt {
        add_lowertvar_to_bounds(bounds2, id1, trace, new_use_op.dupe());
    }
    let bounds2 = bounds2.borrow();
    iter_with_filter(
        cx,
        &bounds2.uppertvars,
        id2,
        |_cx, _root_id, bounds, trace_u, use_op| {
            let use_op = pick_use_op(env, new_use_op, use_op);
            add_lowertvar_to_bounds(
                &bounds,
                id1,
                DepthTrace::concat_trace(&[trace, *trace_u]),
                use_op,
            );
        },
    );
}

/// for each id in id1 + bounds1.lowertvars:
///       id.bounds.upper += bounds2.upper
///       id.bounds.uppertvars += id2
///       id.bounds.uppertvars += bounds2.uppertvars
pub(super) fn add_upper_edges<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    new_use_op: UseOp,
    opt: bool,
    (id1, bounds1): (i32, &constraint::BoundsRef<Cx>),
    (id2, bounds2): (i32, &constraint::BoundsRef<Cx>),
) {
    let upper = bounds2.borrow().upper.clone();
    edges_to_ts(cx, env, trace, &new_use_op, opt, (id1, bounds1), &upper);
    edges_to_tvar(cx, env, trace, &new_use_op, opt, (id1, bounds1), id2);
    let uppertvars = bounds2.borrow().uppertvars.clone();
    iter_with_filter(
        cx,
        &uppertvars,
        id2,
        |cx, tvar, _bounds, trace_u, use_op| {
            let new_use_op = pick_use_op(env, &new_use_op, use_op);
            let trace = DepthTrace::concat_trace(&[trace, *trace_u]);
            edges_to_tvar(cx, env, trace, &new_use_op, opt, (id1, bounds1), tvar);
        },
    );
}

/// for each id in id2 + bounds2.uppertvars:
///       id.bounds.lower += bounds1.lower
///       id.bounds.lowertvars += id1
///       id.bounds.lowertvars += bounds1.lowertvars
pub(super) fn add_lower_edges<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    new_use_op: UseOp,
    opt: bool,
    (id1, bounds1): (i32, &constraint::BoundsRef<Cx>),
    (id2, bounds2): (i32, &constraint::BoundsRef<Cx>),
) {
    let lower = bounds1.borrow().lower.clone();
    edges_from_ts(cx, env, trace, &new_use_op, opt, &lower, (id2, bounds2));
    edges_from_tvar(cx, env, trace, &new_use_op, opt, id1, (id2, bounds2));
    let lowertvars = bounds1.borrow().lowertvars.clone();
    iter_with_filter(
        cx,
        &lowertvars,
        id1,
        |cx, tvar, _bounds, trace_l, use_op| {
            let use_op = pick_use_op(env, use_op, &new_use_op);
            let trace = DepthTrace::concat_trace(&[*trace_l, trace]);
            edges_from_tvar(cx, env, trace, &use_op, opt, tvar, (id2, bounds2));
        },
    );
}

// unification

pub(super) fn unify_flip(use_op: UseOp) -> UseOp {
    UseOp::Frame(Arc::new(VirtualFrameUseOp::UnifyFlip), Arc::new(use_op))
}

/// Chain a root to another root. If both roots are unresolved, this amounts to
/// copying over the bounds of one root to another, and adding all the
/// connections necessary when two non-unifiers flow to each other. If one or
/// both of the roots are resolved, they effectively act like the corresponding
/// concrete types.
pub(super) fn goto<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    (node1, root1): (ConstraintNodeId, Root<constraint::Constraints<Cx>>),
    (node2, root2): (ConstraintNodeId, Root<constraint::Constraints<Cx>>),
) -> Result<(), FlowJsException> {
    let id1 = node1.id();
    let id2 = node2.id();
    match (root1.constraints, root2.constraints) {
        (
            constraint::Constraints::Unresolved(bounds1),
            constraint::Constraints::Unresolved(bounds2),
        ) => {
            let cond1 = not_linked((id1, &bounds1), (id2, &bounds2));
            let cond2 = not_linked((id2, &bounds2), (id1, &bounds1));
            if cond1 {
                let (lower, upper) = {
                    let bounds1 = bounds1.borrow();
                    let bounds2 = bounds2.borrow();
                    (bounds1.lower.clone(), bounds2.upper.clone())
                };
                flows_across(cx, env, trace, use_op.dupe(), &lower, &upper)?;
            }
            let mut flipped_use_op = None;
            if cond2 {
                let flipped = unify_flip(use_op.dupe());
                let (lower, upper) = {
                    let bounds1 = bounds1.borrow();
                    let bounds2 = bounds2.borrow();
                    (bounds2.lower.clone(), bounds1.upper.clone())
                };
                flows_across(cx, env, trace, flipped.dupe(), &lower, &upper)?;
                flipped_use_op = Some(flipped);
            }
            if cond1 {
                add_upper_edges(
                    cx,
                    env,
                    trace,
                    use_op.dupe(),
                    true,
                    (id1, &bounds1),
                    (id2, &bounds2),
                );
                add_lower_edges(
                    cx,
                    env,
                    trace,
                    use_op.dupe(),
                    false,
                    (id1, &bounds1),
                    (id2, &bounds2),
                );
            }
            if let Some(flipped_use_op) = flipped_use_op {
                add_upper_edges(
                    cx,
                    env,
                    trace,
                    flipped_use_op.dupe(),
                    false,
                    (id2, &bounds2),
                    (id1, &bounds1),
                );
                add_lower_edges(
                    cx,
                    env,
                    trace,
                    flipped_use_op,
                    true,
                    (id2, &bounds2),
                    (id1, &bounds1),
                );
            }
            set_constraint_node_goto(cx, node1, node2);
            Ok(())
        }
        (constraint::Constraints::Unresolved(bounds1), constraint::Constraints::Resolved(t2)) => {
            let t2_use = UseT::new(UseTInner::UseT(use_op.dupe(), t2.dupe()));
            edges_and_flows_to_t(cx, env, trace, true, (id1, &bounds1), &t2_use)?;
            edges_and_flows_from_t(
                cx,
                env,
                trace,
                unify_flip(use_op.dupe()),
                true,
                &t2,
                (id1, &bounds1),
            )?;
            set_constraint_node_goto(cx, node1, node2);
            Ok(())
        }
        (
            constraint::Constraints::Unresolved(bounds1),
            constraint::Constraints::FullyResolved(s2),
        ) => {
            let t2 = cx.force_fully_resolved_tvar(&s2);
            let t2_use = UseT::new(UseTInner::UseT(use_op.dupe(), t2.dupe()));
            edges_and_flows_to_t(cx, env, trace, true, (id1, &bounds1), &t2_use)?;
            edges_and_flows_from_t(
                cx,
                env,
                trace,
                unify_flip(use_op.dupe()),
                true,
                &t2,
                (id1, &bounds1),
            )?;
            set_constraint_node_goto(cx, node1, node2);
            Ok(())
        }
        (constraint::Constraints::Resolved(t1), constraint::Constraints::Unresolved(bounds2)) => {
            let t1_use = UseT::new(UseTInner::UseT(unify_flip(use_op.dupe()), t1.dupe()));
            edges_and_flows_to_t(cx, env, trace, true, (id2, &bounds2), &t1_use)?;
            edges_and_flows_from_t(cx, env, trace, use_op.dupe(), true, &t1, (id2, &bounds2))?;
            set_constraint_node_constraints(cx, node2, constraint::Constraints::Resolved(t1));
            set_constraint_node_goto(cx, node1, node2);
            Ok(())
        }
        (
            constraint::Constraints::FullyResolved(s1),
            constraint::Constraints::Unresolved(bounds2),
        ) => {
            let t1 = cx.force_fully_resolved_tvar(&s1);
            let t1_use = UseT::new(UseTInner::UseT(unify_flip(use_op.dupe()), t1.dupe()));
            edges_and_flows_to_t(cx, env, trace, true, (id2, &bounds2), &t1_use)?;
            edges_and_flows_from_t(cx, env, trace, use_op.dupe(), true, &t1, (id2, &bounds2))?;
            set_constraint_node_constraints(cx, node2, constraint::Constraints::FullyResolved(s1));
            set_constraint_node_goto(cx, node1, node2);
            Ok(())
        }
        (constraint::Constraints::Resolved(t1), constraint::Constraints::Resolved(t2)) => {
            // replace node first, in case rec_unify recurses back to these tvars
            set_constraint_node_goto(cx, node1, node2);
            rec_unify(
                cx,
                env,
                trace,
                use_op,
                UnifyCause::Uncategorized,
                None,
                &t1,
                &t2,
            )
        }
        (constraint::Constraints::Resolved(t1), constraint::Constraints::FullyResolved(s2)) => {
            let t2 = cx.force_fully_resolved_tvar(&s2);
            // replace node first, in case rec_unify recurses back to these tvars
            set_constraint_node_goto(cx, node1, node2);
            rec_unify(
                cx,
                env,
                trace,
                use_op,
                UnifyCause::Uncategorized,
                None,
                &t1,
                &t2,
            )
        }
        (
            constraint::Constraints::FullyResolved(s1),
            constraint::Constraints::FullyResolved(s2),
        ) => {
            let t1 = cx.force_fully_resolved_tvar(&s1);
            let t2 = cx.force_fully_resolved_tvar(&s2);
            // replace node first, in case rec_unify recurses back to these tvars
            set_constraint_node_goto(cx, node1, node2);
            rec_unify(
                cx,
                env,
                trace,
                use_op,
                UnifyCause::Uncategorized,
                None,
                &t1,
                &t2,
            )
        }
        (constraint::Constraints::FullyResolved(s1), constraint::Constraints::Resolved(t2)) => {
            let t1 = cx.force_fully_resolved_tvar(&s1);
            // prefer fully resolved roots to resolved roots
            set_constraint_node_constraints(cx, node2, constraint::Constraints::FullyResolved(s1));
            // replace node first, in case rec_unify recurses back to these tvars
            set_constraint_node_goto(cx, node1, node2);
            rec_unify(
                cx,
                env,
                trace,
                use_op,
                UnifyCause::Uncategorized,
                None,
                &t1,
                &t2,
            )
        }
    }
}

/// Unify two constraint nodes. This involves finding their roots, and making one
/// point to the other. Ranks are used to keep chains short. A side root always
/// points to a production root when the stores differ.
pub(super) fn merge_constraint_nodes<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    node1: ConstraintNodeId,
    node2: ConstraintNodeId,
) -> Result<(), FlowJsException> {
    let frozen1 = frozen_constraint_node(cx, env, node1);
    let frozen2 = frozen_constraint_node(cx, env, node2);
    match (frozen1.as_ref(), frozen2.as_ref()) {
        (Some(frozen1), Some(frozen2)) if frozen1.root_id == frozen2.root_id => return Ok(()),
        (Some(frozen1), Some(frozen2)) => {
            return flow_type_to_frozen_constraint_node(cx, env, use_op, &frozen1.tvar, frozen2);
        }
        (Some(frozen), None) => {
            return resolve_constraint_node(
                cx,
                env,
                trace,
                unify_flip(use_op),
                node2,
                &frozen.tvar,
            );
        }
        (None, Some(frozen)) => {
            return resolve_constraint_node(cx, env, trace, use_op, node1, &frozen.tvar);
        }
        (None, None) => {}
    }

    let (node1, root1) = constraint_node_root(cx, node1);
    let (node2, root2) = constraint_node_root(cx, node2);
    if node1 == node2 {
        return Ok(());
    }

    match (root1.rank, root2.rank) {
        (rank1, rank2) if rank1 < rank2 => {
            goto(cx, env, trace, use_op, (node1, root1), (node2, root2))
        }
        (rank1, rank2) if rank2 < rank1 => goto(
            cx,
            env,
            trace,
            unify_flip(use_op),
            (node2, root2),
            (node1, root1),
        ),
        _ => {
            set_constraint_node_rank(cx, node2, root1.rank + 1);
            goto(cx, env, trace, use_op, (node1, root1), (node2, root2))
        }
    }
}

/// Resolve a constraint node to a type. This involves finding its root,
/// and resolving to that type.
pub(super) fn resolve_constraint_node<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    node: ConstraintNodeId,
    t: &Type,
) -> Result<(), FlowJsException> {
    if let Some(frozen) = frozen_constraint_node(cx, env, node) {
        return flow_type_to_frozen_constraint_node(cx, env, use_op, t, &frozen);
    }

    let (node, root) = constraint_node_root(cx, node);
    let id = node.id();
    match root.constraints {
        constraint::Constraints::Unresolved(bounds) => {
            set_constraint_node_constraints(cx, node, constraint::Constraints::Resolved(t.dupe()));
            let use_t = UseT::new(UseTInner::UseT(use_op.dupe(), t.dupe()));
            edges_and_flows_to_t(cx, env, trace, true, (id, &bounds), &use_t)?;
            edges_and_flows_from_t(cx, env, trace, use_op, true, t, (id, &bounds))
        }
        constraint::Constraints::Resolved(t_) => rec_unify(
            cx,
            env,
            trace,
            use_op,
            UnifyCause::Uncategorized,
            None,
            &t_,
            t,
        ),
        constraint::Constraints::FullyResolved(s) => {
            let forced = cx.force_fully_resolved_tvar(&s);
            rec_unify(
                cx,
                env,
                trace,
                use_op,
                UnifyCause::Uncategorized,
                None,
                &forced,
                t,
            )
        }
    }
}

/// Resolve a production type variable to a type.
pub(super) fn resolve_id<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    id: i32,
    t: &Type,
) -> Result<(), FlowJsException> {
    resolve_constraint_node(cx, env, trace, use_op, ConstraintNodeId::OpenT(id), t)
}
