/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc::*;
use flow_common::reason::mk_reason;
use flow_env_builder::env_api;
use flow_env_builder::env_api::AssertionConsistencyInfo;
use flow_env_builder::env_api::AssertionExit;
use flow_env_builder::pattern_helper;
use flow_parser::ast::expression::Expression;
use flow_parser::ast::expression::ExpressionInner;
use flow_parser::ast::function;
use flow_parser::ast::pattern;
use flow_typing_context::Context;
use flow_typing_context::TypingMode;
use flow_typing_errors::error_message::EAssertionFunctionFalsyAtExitData;
use flow_typing_errors::error_message::ENegativeTypeGuardConsistencyData;
use flow_typing_errors::error_message::ETypeGuardFunctionInvalidWritesData;
use flow_typing_errors::error_message::ETypeGuardFunctionParamHavocedData;
use flow_typing_errors::error_message::ETypeGuardInvalidParameterData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::TypeGuardBindingKind;
use flow_typing_errors::error_message::TypeGuardParameterData;
use flow_typing_errors::intermediate_error_types::NamedReferenceData;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_js::flow_js;
use flow_typing_flow_js::flow_js::FlowJs;
use flow_typing_flow_js::tvar_resolver::default_no_lowers;
use flow_typing_type::type_::AssertionFunctionConsistencyData;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::PositiveTypeGuardConsistencyData;
use flow_typing_type::type_::Predicate;
use flow_typing_type::type_::PredicateInner;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeGuard;
use flow_typing_type::type_::TypeGuardInner;
use flow_typing_type::type_::TypeGuardKind;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::UseT;
use flow_typing_type::type_::UseTInner;
use flow_typing_type::type_::VirtualRootUseOp;
use flow_typing_type::type_util::reason_of_t;
use flow_typing_type::type_util::ref_loc_of_t;
use flow_typing_visitors::type_visitor::TypeVisitor;
use flow_typing_visitors::type_visitor::predicate_default;
use flow_utils_concurrency::job_error::JobError;

use crate::abnormal::CheckExprError;
use crate::predicate_kit;
use crate::type_env;

// This check to be performed after the function has been checked to ensure all
// entries have been prepared for type checking.
fn check_type_guard_consistency<'cx>(
    cx: &mut Context<'cx>,
    reason: &Reason,
    one_sided: bool,
    param_loc: &ALoc,
    tg_param: &(ALoc, flow_data_structure_wrapper::smol_str::FlowSmolStr),
    tg_reason: &Reason,
    type_guard: &Type,
) -> Result<(), JobError> {
    let type_guard_consistency_maps = {
        let env = cx.environment();
        env.var_info.type_guard_consistency_maps.dupe()
    };
    let (name_loc, name) = tg_param;
    let param_reason = if name.as_str() == "this" {
        mk_reason(RThis, param_loc.dupe())
    } else {
        mk_reason(RParameter(Some(name.dupe())), param_loc.dupe())
    };
    match type_guard_consistency_maps.get(name_loc) {
        // Entry missing when function does not return. Error raised in Func_sig.
        None => (),
        Some((Some(havoced_loc_set), _)) => {
            flow_js::add_output_non_speculating(
                cx,
                ErrorMessage::ETypeGuardFunctionParamHavoced(Box::new(
                    ETypeGuardFunctionParamHavocedData {
                        type_guard: TypeGuardParameterData {
                            loc: name_loc.dupe(),
                            name: name.dupe(),
                        },
                        param_loc: param_loc.dupe(),
                        is_this: name.as_str() == "this",
                        call_locs: havoced_loc_set.iter().cloned().collect(),
                    },
                )),
            );
        }
        Some((None, reads)) => {
            // Each read corresponds to a return expression.
            for env_api::TypeGuardConsistencyEntry(ret_expr, return_reason, pos_read, neg_refi) in
                reads
            {
                let pos_write_locs = &pos_read.write_locs;
                let is_return_false_statement = matches!(
                    ret_expr.deref(),
                    ExpressionInner::BooleanLiteral { inner, .. } if !inner.value
                );
                let is_return_true_statement = matches!(
                    ret_expr.deref(),
                    ExpressionInner::BooleanLiteral { inner, .. } if inner.value
                );
                let return_loc = return_reason.loc().dupe();
                match type_env::checked_type_guard_at_return(
                    cx,
                    param_reason.dupe(),
                    param_loc.dupe(),
                    return_loc,
                    pos_write_locs,
                    neg_refi,
                )? {
                    Ok((t, neg_pred)) => {
                        // Positive
                        if !is_return_false_statement {
                            let use_op = UseOp::Op(Arc::new(
                                VirtualRootUseOp::PositiveTypeGuardConsistency(Box::new(
                                    PositiveTypeGuardConsistencyData {
                                        reason: reason.dupe(),
                                        param_loc: param_loc.dupe(),
                                        param_name: name.dupe(),
                                        guard_type_reference_loc: ref_loc_of_t(type_guard).dupe(),
                                        guard_type: flow_js_utils::type_or_type_desc_for_error(
                                            type_guard,
                                        ),
                                        return_reason: return_reason.dupe(),
                                        is_return_false_statement,
                                    },
                                )),
                            ));
                            flow_js::flow_non_speculating(
                                cx,
                                (&t, &UseT::new(UseTInner::UseT(use_op, type_guard.dupe()))),
                            )?;
                        }
                        // Negative
                        if !one_sided && !is_return_true_statement {
                            let type_guard_with_neg_pred = match neg_pred {
                                None => type_guard.dupe(),
                                Some(neg_pred) => predicate_kit::collect_predicate_for_filtering(
                                    cx, type_guard, &neg_pred,
                                )?
                                .union_opt(tg_reason.dupe())
                                .unwrap_or_else(|| default_no_lowers(tg_reason)),
                            };
                            let empty_t = flow_typing_type::type_::empty_t::at(ALoc::default());
                            if !FlowJs::speculative_subtyping_succeeds(
                                cx,
                                &type_guard_with_neg_pred,
                                &empty_t,
                            )? {
                                flow_js::add_output_non_speculating(
                                    cx,
                                    ErrorMessage::ENegativeTypeGuardConsistency(Box::new(
                                        ENegativeTypeGuardConsistencyData {
                                            return_reason: return_reason.to_error_reference(),
                                            type_: flow_js_utils::type_reference_for_error(
                                                type_guard,
                                            ),
                                        },
                                    )),
                                );
                            }
                        }
                    }
                    Err(write_locs) => {
                        flow_js::add_output_non_speculating(
                            cx,
                            ErrorMessage::ETypeGuardFunctionInvalidWrites(Box::new(
                                ETypeGuardFunctionInvalidWritesData {
                                    loc: return_reason.loc().dupe(),
                                    type_guard: NamedReferenceData {
                                        loc: name_loc.dupe(),
                                        name: name.dupe(),
                                    },
                                    write_locs,
                                },
                            )),
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

// Assertion functions only narrow through abrupt completions: every point
// where the function completes normally must already have narrowed the
// asserted parameter. The check is one-sided since the throwing paths carry no
// information.
fn check_assertion_consistency<'cx>(
    cx: &mut Context<'cx>,
    reason: &Reason,
    param_loc: &ALoc,
    tg_param: &(ALoc, flow_data_structure_wrapper::smol_str::FlowSmolStr),
    type_guard: Option<&Type>,
) -> Result<(), JobError> {
    let info = {
        let env = cx.environment();
        env.var_info
            .assertion_consistency_maps
            .get(&tg_param.0)
            .cloned()
    };
    // Entry missing when the function never completes normally.
    let Some(AssertionConsistencyInfo { havoced, exits }) = info else {
        return Ok(());
    };
    let (name_loc, name) = tg_param;
    let is_this = name.as_str() == "this";
    let param_reason = if is_this {
        mk_reason(RThis, param_loc.dupe())
    } else {
        mk_reason(RParameter(Some(name.dupe())), param_loc.dupe())
    };
    if let Some(havoced_loc_set) = havoced {
        flow_js::add_output_non_speculating(
            cx,
            ErrorMessage::ETypeGuardFunctionParamHavoced(Box::new(
                ETypeGuardFunctionParamHavocedData {
                    type_guard: TypeGuardParameterData {
                        loc: name_loc.dupe(),
                        name: name.dupe(),
                    },
                    param_loc: param_loc.dupe(),
                    is_this,
                    call_locs: havoced_loc_set.iter().cloned().collect(),
                },
            )),
        );
        return Ok(());
    }
    for AssertionExit { return_loc, read } in exits {
        let implicit_return = return_loc.is_none();
        let exit_reason = match return_loc {
            Some(loc) => mk_reason(None, loc),
            None => reason.dupe(),
        };
        let exit_loc = exit_reason.loc().dupe();
        let t = match type_env::checked_type_guard_param_at_return(
            cx,
            param_reason.dupe(),
            param_loc.dupe(),
            exit_loc.dupe(),
            &read.write_locs,
        )? {
            Ok(t) => t,
            Err(write_locs) => {
                flow_js::add_output_non_speculating(
                    cx,
                    ErrorMessage::ETypeGuardFunctionInvalidWrites(Box::new(
                        ETypeGuardFunctionInvalidWritesData {
                            loc: exit_loc,
                            type_guard: NamedReferenceData {
                                loc: name_loc.dupe(),
                                name: name.dupe(),
                            },
                            write_locs,
                        },
                    )),
                );
                continue;
            }
        };
        match type_guard {
            Some(type_guard) => {
                let use_op = UseOp::Op(Arc::new(VirtualRootUseOp::AssertionFunctionConsistency(
                    Box::new(AssertionFunctionConsistencyData {
                        exit_reason,
                        param_loc: param_loc.dupe(),
                        param_name: name.dupe(),
                        guard_type_reference_loc: ref_loc_of_t(type_guard).dupe(),
                        guard_type: flow_js_utils::type_or_type_desc_for_error(type_guard),
                        implicit_return,
                    }),
                )));
                flow_js::flow_non_speculating(
                    cx,
                    (&t, &UseT::new(UseTInner::UseT(use_op, type_guard.dupe()))),
                )?;
            }
            None => {
                // A bare `asserts x` promises that `x` is truthy, but there is no
                // type for the truthy part of `t` to check against. Check the
                // complement instead: what `t` can still be when falsy must be empty.
                let falsy = predicate_kit::collect_predicate_for_filtering(
                    cx,
                    &t,
                    &Predicate::new(PredicateInner::NotP(Predicate::new(
                        PredicateInner::TruthyP,
                    ))),
                )?
                .union_opt(param_reason.dupe())
                .unwrap_or_else(|| default_no_lowers(&param_reason));
                let empty_t = flow_typing_type::type_::empty_t::at(ALoc::default());
                if !FlowJs::speculative_subtyping_succeeds(cx, &falsy, &empty_t)? {
                    flow_js::add_output_non_speculating(
                        cx,
                        ErrorMessage::EAssertionFunctionFalsyAtExit(Box::new(
                            EAssertionFunctionFalsyAtExitData {
                                loc: exit_loc,
                                param: NamedReferenceData {
                                    loc: name_loc.dupe(),
                                    name: name.dupe(),
                                },
                                falsy: flow_js_utils::type_reference_for_error(&falsy),
                                implicit_return,
                            },
                        )),
                    );
                }
            }
        }
    }
    Ok(())
}

/// `allow_this_type_guard` mirrors `type_annotation::allows_this_type_guards` on the
/// annotation-only path: it is set only for non-static class methods, the one
/// body-carrying context where `this` is bound to a class instance and can
/// therefore be refined.
pub fn check_type_guard<'cx>(
    cx: &mut Context<'cx>,
    params: &function::Params<ALoc, ALoc>,
    type_guard_val: &TypeGuard,
    allow_this_type_guard: bool,
) -> Result<(), JobError> {
    let TypeGuardInner {
        reason,
        inferred,
        kind,
        param_name,
        type_guard,
    } = type_guard_val.deref();

    let err_with_binding = |cx: &mut Context<'cx>,
                            binding_kind: TypeGuardBindingKind,
                            type_guard_loc: &ALoc,
                            name: &flow_data_structure_wrapper::smol_str::FlowSmolStr,
                            binding_loc: ALoc| {
        flow_js::add_output_non_speculating(
            cx,
            ErrorMessage::ETypeGuardInvalidParameter(Box::new(ETypeGuardInvalidParameterData {
                type_guard: TypeGuardParameterData {
                    loc: type_guard_loc.dupe(),
                    name: name.dupe(),
                },
                binding_loc,
                binding_kind,
            })),
        )
    };

    let error_on_non_root_binding =
        |cx: &mut Context<'cx>,
         type_guard_loc: &ALoc,
         name: &flow_data_structure_wrapper::smol_str::FlowSmolStr,
         binding: &(ALoc, Rc<pattern_helper::Binding<ALoc, ALoc>>)| {
            let (loc, binding_kind) = binding;
            match binding_kind.deref() {
                pattern_helper::Binding::Root => (),
                pattern_helper::Binding::Rest => err_with_binding(
                    cx,
                    TypeGuardBindingKind::RestParameter,
                    type_guard_loc,
                    name,
                    loc.dupe(),
                ),
                pattern_helper::Binding::Select { .. } => err_with_binding(
                    cx,
                    TypeGuardBindingKind::PatternParameter,
                    type_guard_loc,
                    name,
                    loc.dupe(),
                ),
            }
        };

    if !inferred {
        let (name_loc, name) = param_name;
        let is_this_guard = name.as_str() == "this";
        let tg_reason = if is_this_guard {
            mk_reason(RThis, name_loc.dupe())
        } else {
            mk_reason(None, name_loc.dupe())
        };
        let check_consistency = |cx: &mut Context<'cx>, binding_loc: &ALoc| match (kind, type_guard)
        {
            _ if cx.type_strictness_kind().is_typescript_loose() => Ok(()),
            (TypeGuardKind::Asserts, _) => check_assertion_consistency(
                cx,
                reason,
                binding_loc,
                param_name,
                type_guard.as_ref(),
            ),
            (_, Some(type_guard)) => check_type_guard_consistency(
                cx,
                reason,
                type_guard_val.one_sided(),
                binding_loc,
                param_name,
                &tg_reason,
                type_guard,
            ),
            (_, None) => Ok(()),
        };
        let bindings = pattern_helper::bindings_of_params(params);
        match bindings.get(name) {
            None => {
                if is_this_guard && allow_this_type_guard {
                    // `this` is not part of the parameter list, but it is still a
                    // binding of the method body, so consistency checking applies. The
                    // annotation's `this` location is what `record_type_guard_maps`
                    // keyed the consistency entries on. An explicit `this` param moves
                    // the binding write to that param, so the return-point check needs
                    // its location to recognize the write as the binding one.
                    let binding_loc = match &params.this_ {
                        Some(this_param) => &this_param.loc,
                        None => name_loc,
                    };
                    check_consistency(cx, binding_loc)?;
                } else if is_this_guard {
                    flow_js::add_output_non_speculating(
                        cx,
                        ErrorMessage::ETypeGuardThisParam(name_loc.dupe()),
                    );
                } else {
                    flow_js::add_output_non_speculating(
                        cx,
                        ErrorMessage::ETypeGuardParamUnbound(TypeGuardParameterData {
                            loc: name_loc.dupe(),
                            name: name.dupe(),
                        }),
                    );
                }
            }
            Some((p_loc, binding_kind))
                if matches!(binding_kind.deref(), pattern_helper::Binding::Root) =>
            {
                check_consistency(cx, p_loc)?;
            }
            Some(binding) => {
                error_on_non_root_binding(cx, name_loc, name, binding);
            }
        }
    }
    Ok(())
}

// Given an function of the form `(x: T) => e`, we will infer a type guard iff
// 1. the body `e` encodes some refinement,
// 2. the refinement P encoded in `e` does not include the truthy predicate,
// 3. P narrows the type of `x`, ie. if T' the type of `x` after P has been
//    applied to T, then it has to be the case that T </: T'. Due to T' being a
//    refined version of T it trivially holds that T' <: T.

// All predicates are allowed to contribute to the inferred type guard except
// for the trivial truthy predicate to avoid things like: `(x: mixed) => x`.
fn is_inferable_type_guard_predicate<'cx>(cx: &mut Context<'cx>, p: &Predicate) -> bool {
    struct InferableTypeGuardVisitor;

    impl TypeVisitor<Result<(), ()>> for InferableTypeGuardVisitor {
        fn predicate<'cx>(
            &mut self,
            cx: &mut Context<'cx>,
            acc: Result<(), ()>,
            p: &Predicate,
        ) -> Result<(), ()> {
            match p.deref() {
                PredicateInner::TruthyP => Err(()),
                _ => predicate_default(self, cx, acc, p),
            }
        }
    }

    let mut visitor = InferableTypeGuardVisitor;
    visitor.predicate(cx, Ok(()), p).is_ok()
}

fn is_inferable_type_guard_read<'cx>(
    cx: &mut Context<'cx>,
    read: &flow_env_builder::env_api::Read<ALoc>,
) -> Result<bool, JobError> {
    Ok(match type_env::read_to_predicate(cx, read)? {
        Some(p) => is_inferable_type_guard_predicate(cx, &p),
        None => false,
    })
}

fn infer_type_guard_from_read<'cx, F>(
    infer_expr: &F,
    cx: &mut Context<'cx>,
    name: &flow_parser::ast::Identifier<ALoc, ALoc>,
    return_expr: &Expression<ALoc, ALoc>,
    return_reason: &Reason,
    read: &flow_env_builder::env_api::Read<ALoc>,
) -> Result<Option<TypeGuard>, CheckExprError>
where
    F: Fn(
        &mut Context<'cx>,
        &Expression<ALoc, ALoc>,
    ) -> Result<Expression<ALoc, (ALoc, Type)>, CheckExprError>,
{
    let write_locs = &read.write_locs;
    let param_loc = &name.loc;
    let pname = &name.name;
    let param_reason = mk_reason(RParameter(Some(pname.dupe())), param_loc.dupe());

    let mk_guard = |tg: Type| -> TypeGuard {
        TypeGuard::new(TypeGuardInner {
            inferred: true,
            reason: param_reason.dupe(),
            param_name: (param_loc.dupe(), pname.dupe()),
            type_guard: Some(tg),
            kind: TypeGuardKind::Implies,
        })
    };
    let returns_bool = |cx: &mut Context<'cx>| -> Result<bool, CheckExprError> {
        let result = infer_expr(cx, return_expr)?;
        let (_aloc, body_t) = result.loc();
        let bool_general = Type::new(TypeInner::DefT(
            reason_of_t(body_t).dupe(),
            DefT::new(DefTInner::BoolGeneralT),
        ));
        Ok(FlowJs::speculative_subtyping_succeeds(
            cx,
            body_t,
            &bool_general,
        )?)
    };

    Ok(if !matches!(cx.typing_mode(), TypingMode::CheckingMode) {
        None
    } else if is_inferable_type_guard_read(cx, read)? {
        let return_loc = return_reason.loc().dupe();
        let param_t = type_env::find_write(
            cx,
            env_api::DefLocType::OrdinaryNameLoc,
            param_reason.dupe(),
        );
        let guard_t = type_env::inferred_type_guard_at_return(
            cx,
            param_reason.dupe(),
            return_loc,
            write_locs,
        )?;
        // Only keep the type guard if the function is actually refining the input
        // and is returning a boolean expression.
        if FlowJs::speculative_subtyping_succeeds(cx, &param_t, &guard_t)? || !returns_bool(cx)? {
            None
        } else {
            Some(mk_guard(guard_t))
        }
    } else {
        None
    })
}

pub fn infer_type_guard<'cx, F>(
    cx: &mut Context<'cx>,
    infer_expr: &F,
    params: &function::Params<ALoc, ALoc>,
) -> Result<Option<TypeGuard>, CheckExprError>
where
    F: Fn(
        &mut Context<'cx>,
        &Expression<ALoc, ALoc>,
    ) -> Result<Expression<ALoc, (ALoc, Type)>, CheckExprError>,
{
    let type_guard_consistency_maps = {
        let env = cx.environment();
        env.var_info.type_guard_consistency_maps.dupe()
    };
    if params.params.len() == 1
        && params.rest.is_none()
        && let function::Param::RegularParam { argument, .. } = &params.params[0]
        && let pattern::Pattern::Identifier { inner, .. } = argument
    {
        let name = &inner.name;
        let param_loc = &name.loc;
        match type_guard_consistency_maps.get(param_loc) {
            None => return Ok(None),
            Some((Some(_havoced_loc_set), _)) => return Ok(None),
            Some((None, entries)) if entries.len() == 1 => {
                let env_api::TypeGuardConsistencyEntry(ret_expr, return_reason, read, _neg_refi) =
                    &entries[0];
                return infer_type_guard_from_read(
                    infer_expr,
                    cx,
                    name,
                    ret_expr,
                    return_reason,
                    read,
                );
            }
            Some((None, _)) => return Ok(None),
        }
    }
    Ok(None)
}
