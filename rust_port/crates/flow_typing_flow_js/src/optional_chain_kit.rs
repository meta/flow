/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;

use dupe::Dupe;
use flow_common::reason::VirtualReasonDesc;
use flow_typing_context::Context;
use flow_typing_context::Cx;
use flow_typing_errors::intermediate_error_types::ExpressionReferenceData;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_common::flow_js_utils::callee_recorder;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_type::type_::AnySource;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::DepthTrace;
use flow_typing_type::type_::GetPropTData;
use flow_typing_type::type_::ImplicitInstantiationTvarData;
use flow_typing_type::type_::ImplicitInstantiationTvarKind;
use flow_typing_type::type_::TestPropTData;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::UseT;
use flow_typing_type::type_::UseTInner;
use flow_typing_type::type_::type_collector::TypeCollector;

use crate::default_resolve;
use crate::flow_js::FlowJs;
use crate::speculation_kit;

fn run_on_concretized<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    l: &Type,
    reason: &flow_common::reason::Reason,
    lhs_reason: &flow_common::reason::Reason,
    lhs_expression: &ExpressionReferenceData<flow_aloc::ALoc>,
    upper: &UseT<Cx>,
    voided_out_collector: &Option<TypeCollector>,
) -> Result<(), FlowJsException> {
    match l.deref() {
        TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::VoidT) => {
            callee_recorder::add_callee_use(env, callee_recorder::Kind::Tast, l.dupe(), upper);
            cx.mark_optional_chain(reason.loc().dupe(), lhs_expression.dupe(), true);
            if let Some(c) = voided_out_collector {
                c.add(l.dupe());
            }
            return Ok(());
        }
        TypeInner::DefT(r, def_t) if matches!(def_t.deref(), DefTInner::TsVoidT) => {
            callee_recorder::add_callee_use(env, callee_recorder::Kind::Tast, l.dupe(), upper);
            cx.mark_optional_chain(reason.loc().dupe(), lhs_expression.dupe(), true);
            if let Some(c) = voided_out_collector {
                c.add(flow_typing_type::type_::void::why(r.dupe()));
            }
            return Ok(());
        }
        TypeInner::DefT(r, def_t) if matches!(def_t.deref(), DefTInner::NullT) => {
            callee_recorder::add_callee_use(env, callee_recorder::Kind::Tast, l.dupe(), upper);
            let void = {
                match r.desc(true) {
                    Some(VirtualReasonDesc::RNull) => Type::new(TypeInner::DefT(
                        r.dupe().replace_desc(VirtualReasonDesc::RVoidedNull),
                        DefT::new(DefTInner::VoidT),
                    )),
                    _ => Type::new(TypeInner::DefT(r.dupe(), DefT::new(DefTInner::VoidT))),
                }
            };
            cx.mark_optional_chain(reason.loc().dupe(), lhs_expression.dupe(), true);
            if let Some(c) = voided_out_collector {
                c.add(void);
            }
            return Ok(());
        }
        TypeInner::IntersectionT(r, rep) => {
            let upper = match upper.deref() {
                UseTInner::GetPropT(data) if data.id.is_some() => {
                    UseT::new(UseTInner::GetPropT(Box::new(GetPropTData {
                        use_op: data.use_op.dupe(),
                        reason: data.reason.dupe(),
                        id: None,
                        from_annot: data.from_annot,
                        skip_optional: data.skip_optional,
                        propref: data.propref.clone(),
                        tout: data.tout.clone(),
                        hint: data.hint.clone(),
                    })))
                }
                UseTInner::TestPropT(box TestPropTData {
                    use_op,
                    reason: test_reason,
                    id: _,
                    propref,
                    tout,
                    hint,
                }) => UseT::new(UseTInner::GetPropT(Box::new(GetPropTData {
                    use_op: use_op.dupe(),
                    reason: test_reason.dupe(),
                    id: None,
                    from_annot: false,
                    skip_optional: false,
                    propref: propref.clone(),
                    tout: tout.clone(),
                    hint: hint.clone(),
                }))),
                _ => upper.dupe(),
            };
            // We only call CalleeRecorder here for sig-help information. As far as
            // the typed AST is concerned when dealing with intersections we record
            // the specific branch that was selected. Therefore, we do not record
            // intersections when they hit a CallT constraint. The only time when an
            // intersection is allowed is when we have exhausted the branches of a
            // speculation job (this is a Flow error) and fall back to the
            // intersection as the type for the callee node. (This happens in
            // Default_resolver.)
            callee_recorder::add_callee_use(env, callee_recorder::Kind::SigHelp, l.dupe(), &upper);
            let r_loc = r.loc().dupe();
            let upper_for_default = upper.dupe();
            let default_resolve_fn: Box<
                dyn FnOnce(&mut Context<'cx>, &FlowJsEnv) -> Result<(), FlowJsException>,
            > = {
                let r_loc_clone = r_loc.dupe();
                let upper_clone = upper_for_default.dupe();
                Box::new(move |cx, env: &FlowJsEnv| {
                    let flow_fn = |cx: &mut Context<'cx>, t: &Type, u: &UseT<Cx>| {
                        FlowJs::flow_with_env(cx, env, t, u)
                    };
                    default_resolve::default_resolve_touts(
                        cx,
                        &flow_fn,
                        None,
                        env,
                        r_loc_clone,
                        &upper_clone,
                    )?;
                    Ok(())
                })
            };
            let cases: Vec<
                Box<dyn FnOnce(&mut Context<'cx>, &FlowJsEnv) -> Result<(), FlowJsException>>,
            > = rep
                .members_iter()
                .map(|t| {
                    let t = t.dupe();
                    let reason = reason.dupe();
                    let lhs_reason = lhs_reason.dupe();
                    let lhs_expression = lhs_expression.dupe();
                    let upper = upper.dupe();
                    let voided_out_collector_clone = voided_out_collector.clone();
                    let f: Box<
                        dyn FnOnce(&mut Context<'cx>, &FlowJsEnv) -> Result<(), FlowJsException>,
                    > = Box::new(move |cx, env: &FlowJsEnv| {
                        run_with_env(
                            cx,
                            env,
                            trace,
                            &t,
                            &reason,
                            &lhs_reason,
                            &lhs_expression,
                            &upper,
                            &voided_out_collector_clone,
                        )
                    });
                    f
                })
                .collect();
            speculation_kit::try_custom(
                cx,
                env,
                None,
                Some(upper.dupe()),
                Some(default_resolve_fn),
                r_loc,
                cases,
            )?;
            return Ok(());
        }
        _ => {}
    }
    let useful = match l.deref() {
        TypeInner::AnyT(_, AnySource::AnyError(_)) => false,
        TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::MixedT(_)) => true,
        TypeInner::AnyT(_, _)
        | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
            kind: ImplicitInstantiationTvarKind::Placeholder,
            ..
        }) => true,
        _ => false,
    };
    cx.mark_optional_chain(reason.loc().dupe(), lhs_expression.dupe(), useful);
    FlowJs::flow_with_env(cx, env, l, upper)?;
    Ok(())
}

pub fn run<'cx>(
    cx: &mut Context<'cx>,
    trace: DepthTrace,
    lhs: &Type,
    reason: &flow_common::reason::Reason,
    lhs_reason: &flow_common::reason::Reason,
    lhs_expression: &ExpressionReferenceData<flow_aloc::ALoc>,
    upper: &UseT<Cx>,
    voided_out_collector: &Option<TypeCollector>,
) -> Result<(), FlowJsException> {
    run_with_env(
        cx,
        &FlowJsEnv::entry(),
        trace,
        lhs,
        reason,
        lhs_reason,
        lhs_expression,
        upper,
        voided_out_collector,
    )
}

pub(super) fn run_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    lhs: &Type,
    reason: &flow_common::reason::Reason,
    lhs_reason: &flow_common::reason::Reason,
    lhs_expression: &ExpressionReferenceData<flow_aloc::ALoc>,
    upper: &UseT<Cx>,
    voided_out_collector: &Option<TypeCollector>,
) -> Result<(), FlowJsException> {
    let concrete_types =
        FlowJs::possible_concrete_types_for_optional_chain_with_env(cx, env, lhs_reason, lhs)?;
    for t in &concrete_types {
        run_on_concretized(
            cx,
            env,
            trace,
            t,
            reason,
            lhs_reason,
            lhs_expression,
            upper,
            voided_out_collector,
        )?;
    }
    Ok(())
}
