/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;
use std::rc::Rc;

use dupe::Dupe;
use flow_typing_context::Context;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_type::type_::NominalType;
use flow_typing_type::type_::NominalTypeInner;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::constraint::Constraints;
use flow_typing_type::type_::nominal;
use flow_typing_type::type_util;
use flow_typing_type::type_util::mod_reason_of_t;
use flow_typing_type::type_util::reason_of_t;
use flow_typing_type::type_util::reasonless_cmp_inner_fast;

pub fn swap_reason(t2: &Type, t1: &Type) -> Type {
    match (t2.deref(), t1.deref()) {
        // In reposition we also recurse and reposition some nested types. We need
        // to make sure we swap the types for these reasons as well. Otherwise our
        // optimized union ~> union check will not pass.
        (TypeInner::MaybeT(_, inner_t2), TypeInner::MaybeT(r, inner_t1)) => {
            Type::new(TypeInner::MaybeT(r.dupe(), swap_reason(inner_t2, inner_t1)))
        }
        (
            TypeInner::NominalT {
                nominal_type: nt2, ..
            },
            TypeInner::NominalT {
                reason: r,
                nominal_type: nt1,
            },
        ) => {
            let repr1 = &nt1.underlying_t;
            let repr2 = &nt2.underlying_t;
            let l1 = &nt1.lower_t;
            let l2 = &nt2.lower_t;
            let u1 = &nt1.upper_t;
            let u2 = &nt2.upper_t;

            let underlying_t = match (repr1, repr2) {
                (
                    nominal::UnderlyingT::CustomError(deref!(nominal::CustomErrorData {
                        t: t1_inner,
                        ..
                    })),
                    nominal::UnderlyingT::CustomError(deref!(nominal::CustomErrorData {
                        t: t2_inner,
                        custom_error_loc,
                    })),
                ) => nominal::UnderlyingT::CustomError(Box::new(nominal::CustomErrorData {
                    custom_error_loc: custom_error_loc.dupe(),
                    t: swap_reason(t2_inner, t1_inner),
                })),
                (
                    nominal::UnderlyingT::OpaqueWithLocal { t: t1_inner },
                    nominal::UnderlyingT::OpaqueWithLocal { t: t2_inner },
                ) => nominal::UnderlyingT::OpaqueWithLocal {
                    t: swap_reason(t2_inner, t1_inner),
                },
                _ => repr2.clone(),
            };
            let lower_t = match (l1, l2) {
                (Some(t1_inner), Some(t2_inner)) => Some(swap_reason(t2_inner, t1_inner)),
                _ => l2.clone(),
            };
            let upper_t = match (u1, u2) {
                (Some(t1_inner), Some(t2_inner)) => Some(swap_reason(t2_inner, t1_inner)),
                _ => u2.clone(),
            };
            Type::new(TypeInner::NominalT {
                reason: r.clone(),
                nominal_type: Rc::new(NominalType::new(NominalTypeInner {
                    underlying_t,
                    lower_t,
                    upper_t,
                    nominal_id: nt1.nominal_id.clone(),
                    nominal_type_args: nt1.nominal_type_args.dupe(),
                })),
            })
        }
        _ => {
            let r1 = reason_of_t(t1);
            mod_reason_of_t(&|_| r1.dupe(), t2)
        }
    }
}

/// Equivalent of `t1 == &swap_reason(t2, t1)` but without allocating the
/// swapped temporary. Shares the same fast-path strategy as
/// `type_util::reasonless_compare` via the common
/// `type_util::reasonless_cmp_inner_fast` helper: discriminants drive the
/// cross-variant answer; same-variant pairs whose `swap_reason` only
/// differs in the top-level reason fall through to that helper. Variants
/// whose nested data also carries reasons that `swap_reason` recursively
/// swaps (`MaybeT`, `NominalT`) recurse here.
fn eq_swap_reason(t1: &Type, t2: &Type) -> bool {
    if t1.ptr_eq(t2) {
        return true;
    }
    let inner1 = t1.deref();
    let inner2 = t2.deref();
    if std::mem::discriminant(inner1) != std::mem::discriminant(inner2) {
        return false;
    }
    if let Some(ord) = reasonless_cmp_inner_fast(inner1, inner2) {
        return ord == std::cmp::Ordering::Equal;
    }
    match (inner1, inner2) {
        (TypeInner::MaybeT(_, i1), TypeInner::MaybeT(_, i2)) => eq_swap_reason(i1, i2),
        (
            TypeInner::NominalT {
                nominal_type: nt1, ..
            },
            TypeInner::NominalT {
                nominal_type: nt2, ..
            },
        ) => {
            // Mirror the swap_reason recursion: compare lower_t/upper_t/
            // CustomError/OpaqueWithLocal nested types via eq_swap_reason,
            // and the rest of the NominalType data via derived Eq.
            if nt1.nominal_id != nt2.nominal_id || nt1.nominal_type_args != nt2.nominal_type_args {
                return false;
            }
            let lower_eq = match (&nt1.lower_t, &nt2.lower_t) {
                (Some(l1), Some(l2)) => eq_swap_reason(l1, l2),
                (None, None) => true,
                _ => false,
            };
            if !lower_eq {
                return false;
            }
            let upper_eq = match (&nt1.upper_t, &nt2.upper_t) {
                (Some(u1), Some(u2)) => eq_swap_reason(u1, u2),
                (None, None) => true,
                _ => false,
            };
            if !upper_eq {
                return false;
            }
            match (&nt1.underlying_t, &nt2.underlying_t) {
                (
                    nominal::UnderlyingT::CustomError(deref!(nominal::CustomErrorData {
                        t: t1_inner,
                        ..
                    })),
                    nominal::UnderlyingT::CustomError(deref!(nominal::CustomErrorData {
                        t: t2_inner,
                        ..
                    })),
                ) => eq_swap_reason(t1_inner, t2_inner),
                (
                    nominal::UnderlyingT::OpaqueWithLocal { t: t1_inner },
                    nominal::UnderlyingT::OpaqueWithLocal { t: t2_inner },
                ) => eq_swap_reason(t1_inner, t2_inner),
                (a, b) => a == b,
            }
        }
        // Fallback: build the swapped Type and compare. Hit only by variants
        // not specially handled above (none in the hot path).
        _ => t1 == &swap_reason(t2, t1),
    }
}

pub fn eq<'cx>(cx: &mut Context<'cx>, t1: &Type, t2: &Type) -> bool {
    eq_with_env(cx, &FlowJsEnv::entry(), t1, t2)
}

/// This predicate attempts to flatten out OpenTs and AnnotTs before performing a
/// structural reasonless equality check of two types.
pub fn eq_with_env<'cx>(cx: &mut Context<'cx>, env: &FlowJsEnv, t1: &Type, t2: &Type) -> bool {
    if t1.ptr_eq(t2) {
        return true;
    }

    match (
        type_util::constraint_node_id(t1),
        type_util::constraint_node_id(t2),
    ) {
        (Some(node1), Some(node2))
            if cx.find_constraints(node1.id()).0 == cx.find_constraints(node2.id()).0 =>
        {
            true
        }
        (Some(node1), _) => match cx.find_constraints(node1.id()).1 {
            Constraints::Resolved(resolved_t1) => eq_with_env(cx, env, &resolved_t1, t2),
            Constraints::FullyResolved(s1) => {
                let resolved_t1 = cx.force_fully_resolved_tvar(&s1);
                eq_with_env(cx, env, &resolved_t1, t2)
            }
            Constraints::Unresolved(_) => eq_swap_reason(t1, t2),
        },
        (_, Some(node2)) => match cx.find_constraints(node2.id()).1 {
            Constraints::Resolved(resolved_t2) => eq_with_env(cx, env, t1, &resolved_t2),
            Constraints::FullyResolved(s2) => {
                let resolved_t2 = cx.force_fully_resolved_tvar(&s2);
                eq_with_env(cx, env, t1, &resolved_t2)
            }
            Constraints::Unresolved(_) => eq_swap_reason(t1, t2),
        },
        (None, None) => match (t1.deref(), t2.deref()) {
            (TypeInner::AnnotT(_, inner_t1, _), _) => eq_with_env(cx, env, inner_t1, t2),
            (_, TypeInner::AnnotT(_, inner_t2, _)) => eq_with_env(cx, env, t1, inner_t2),
            (TypeInner::UnionT(_, rep1), TypeInner::UnionT(_, rep2)) => {
                let members1: Vec<_> = rep1.members_iter().collect();
                let members2: Vec<_> = rep2.members_iter().collect();
                if members1.len() != members2.len() {
                    return false;
                }
                members1
                    .iter()
                    .zip(members2.iter())
                    .all(|(m1, m2)| eq_with_env(cx, env, m1, m2))
            }
            (TypeInner::EvalT { id, .. }, _) => {
                let evaluated = cx.evaluated();
                match evaluated.get(id) {
                    Some(t) => eq_with_env(cx, env, t, t2),
                    None => eq_swap_reason(t1, t2),
                }
            }
            (_, TypeInner::EvalT { id, .. }) => {
                let evaluated = cx.evaluated();
                match evaluated.get(id) {
                    Some(t) => eq_with_env(cx, env, t1, t),
                    None => eq_swap_reason(t1, t2),
                }
            }
            (
                TypeInner::TypeAppT(deref!(TypeAppTData {
                    type_: inner_t1,
                    targs: targs1,
                    from_value: fv1,
                    ..
                })),
                TypeInner::TypeAppT(deref!(TypeAppTData {
                    type_: inner_t2,
                    targs: targs2,
                    from_value: fv2,
                    ..
                })),
            ) => {
                eq_with_env(cx, env, inner_t1, inner_t2)
                    && fv1 == fv2
                    && eq_targs(cx, env, targs1, targs2)
            }
            _ => eq_swap_reason(t1, t2),
        },
    }
}

pub fn eq_targs<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    targs1: &[Type],
    targs2: &[Type],
) -> bool {
    if targs1.len() != targs2.len() {
        return false;
    }
    targs1
        .iter()
        .zip(targs2.iter())
        .all(|(t1, t2)| eq_with_env(cx, env, t1, t2))
}
