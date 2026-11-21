/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_aloc::ALoc;
use flow_common::reason::Reason;
use flow_typing_context::Context;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_util::reason_of_t;

use crate::speculation_kit;

/// For a type t, run the check defined by check_base.
/// This function will break down the unions in t. When it encounters an intersection,
/// the check can pass as long as the check can pass on one member of intersection
pub fn distribute<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: Option<UseOp>,
    break_up_union: &dyn Fn(
        &mut Context<'cx>,
        &Reason,
        &Type,
    ) -> Result<Vec<Type>, FlowJsException>,
    get_no_match_error_loc: &dyn Fn(&Context<'cx>, &Reason) -> ALoc,
    check_base: &dyn Fn(&mut Context<'cx>, &FlowJsEnv, &Type) -> Result<(), FlowJsException>,
    t: &Type,
) -> Result<(), FlowJsException> {
    let ts = break_up_union(cx, reason_of_t(t), t)?;
    for ti in &ts {
        match ti.deref() {
            TypeInner::IntersectionT(r, rep) => {
                let cases: Vec<_> = rep
                    .members_iter()
                    .map(|t| {
                        let t = t.clone();
                        let use_op = use_op.clone();
                        Box::new(move |cx: &mut Context<'cx>, env: &FlowJsEnv| {
                            distribute(
                                cx,
                                env,
                                use_op,
                                break_up_union,
                                get_no_match_error_loc,
                                check_base,
                                &t,
                            )
                        })
                            as Box<
                                dyn FnOnce(
                                        &mut Context<'cx>,
                                        &FlowJsEnv,
                                    )
                                        -> Result<(), FlowJsException>
                                    + '_,
                            >
                    })
                    .collect();
                let no_match_error_loc = get_no_match_error_loc(cx, r);
                speculation_kit::try_custom(
                    cx,
                    env,
                    use_op.clone(),
                    None,
                    None,
                    no_match_error_loc,
                    cases,
                )?;
            }
            _ => {
                check_base(cx, env, ti)?;
            }
        }
    }
    Ok(())
}

/// For a pair of type (t1, t2), run the check defined by check_base.
/// This function will break down the unions in t1 and t2. When it encounters an intersection,
/// the check can pass as long as the check can pass on one member of intersection
pub fn distribute_2<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: Option<UseOp>,
    break_up_union: &dyn Fn(
        &mut Context<'cx>,
        &Reason,
        &Type,
    ) -> Result<Vec<Type>, FlowJsException>,
    get_no_match_error_loc: &dyn Fn(&Context<'cx>, &Reason, &Reason) -> ALoc,
    check_base: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        (&Type, &Type),
    ) -> Result<(), FlowJsException>,
    (t1, t2): (&Type, &Type),
) -> Result<(), FlowJsException> {
    let t1s = break_up_union(cx, reason_of_t(t1), t1)?;
    let t2s = break_up_union(cx, reason_of_t(t2), t2)?;

    for t1i in &t1s {
        for t2i in &t2s {
            match (t1i.deref(), t2i.deref()) {
                (TypeInner::IntersectionT(r1, rep1), TypeInner::IntersectionT(r2, rep2)) => {
                    let members1: Vec<Type> = rep1.members_iter().duped().collect();
                    let members2: Vec<Type> = rep2.members_iter().duped().collect();
                    let cases: Vec<_> = members1
                        .into_iter()
                        .flat_map(|m1| {
                            let use_op = use_op.dupe();
                            members2.iter().map(move |m2| {
                                let m1 = m1.dupe();
                                let m2 = m2.dupe();
                                let use_op = use_op.dupe();
                                Box::new(move |cx: &mut Context<'cx>, env: &FlowJsEnv| {
                                    distribute_2(
                                        cx,
                                        env,
                                        use_op,
                                        break_up_union,
                                        get_no_match_error_loc,
                                        check_base,
                                        (&m1, &m2),
                                    )
                                })
                                    as Box<
                                        dyn FnOnce(
                                                &mut Context<'cx>,
                                                &FlowJsEnv,
                                            )
                                                -> Result<(), FlowJsException>
                                            + '_,
                                    >
                            })
                        })
                        .collect();
                    let no_match_error_loc = get_no_match_error_loc(cx, r1, r2);
                    speculation_kit::try_custom(
                        cx,
                        env,
                        use_op.dupe(),
                        None,
                        None,
                        no_match_error_loc,
                        cases,
                    )?;
                }
                (TypeInner::IntersectionT(r1, rep1), _) => {
                    let t2_clone = t2i.dupe();
                    let cases: Vec<
                        Box<
                            dyn FnOnce(&mut Context<'cx>, &FlowJsEnv) -> Result<(), FlowJsException>
                                + '_,
                        >,
                    > = rep1
                        .members_iter()
                        .map(|t1| {
                            let t1 = t1.dupe();
                            let t2 = t2_clone.dupe();
                            let use_op = use_op.clone();
                            Box::new(move |cx: &mut Context<'cx>, env: &FlowJsEnv| {
                                distribute_2(
                                    cx,
                                    env,
                                    use_op,
                                    break_up_union,
                                    get_no_match_error_loc,
                                    check_base,
                                    (&t1, &t2),
                                )
                            })
                                as Box<
                                    dyn FnOnce(
                                            &mut Context<'cx>,
                                            &FlowJsEnv,
                                        )
                                            -> Result<(), FlowJsException>
                                        + '_,
                                >
                        })
                        .collect();
                    let no_match_error_loc = get_no_match_error_loc(cx, r1, reason_of_t(t2i));
                    speculation_kit::try_custom(
                        cx,
                        env,
                        use_op.clone(),
                        None,
                        None,
                        no_match_error_loc,
                        cases,
                    )?;
                }
                (_, TypeInner::IntersectionT(r2, rep2)) => {
                    let t1_clone = t1i.dupe();
                    let cases: Vec<_> = rep2
                        .members_iter()
                        .map(|t2| {
                            let t2 = t2.dupe();
                            let t1 = t1_clone.dupe();
                            let use_op = use_op.dupe();
                            Box::new(move |cx: &mut Context<'cx>, env: &FlowJsEnv| {
                                distribute_2(
                                    cx,
                                    env,
                                    use_op,
                                    break_up_union,
                                    get_no_match_error_loc,
                                    check_base,
                                    (&t1, &t2),
                                )
                            })
                                as Box<
                                    dyn FnOnce(
                                            &mut Context<'cx>,
                                            &FlowJsEnv,
                                        )
                                            -> Result<(), FlowJsException>
                                        + '_,
                                >
                        })
                        .collect();
                    let no_match_error_loc = get_no_match_error_loc(cx, reason_of_t(t1i), r2);
                    speculation_kit::try_custom(
                        cx,
                        env,
                        use_op.clone(),
                        None,
                        None,
                        no_match_error_loc,
                        cases,
                    )?;
                }
                _ => {
                    check_base(cx, env, (t1i, t2i))?;
                }
            }
        }
    }
    Ok(())
}
