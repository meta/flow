/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! This module contains a collection of functions that operate on types, with the LTI assumption
//! the inspected type will never receive any additional future bounds.

use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_aloc::ALoc;
use flow_common::enclosing_context::EnclosingContext;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast::expression;
use flow_typing_context::Context;
use flow_typing_context::Cx;
use flow_typing_errors::error_message::EBinaryInLHSData;
use flow_typing_errors::error_message::EBinaryInRHSData;
use flow_typing_errors::error_message::EComparisonData;
use flow_typing_errors::error_message::EForInRHSData;
use flow_typing_errors::error_message::EIllegalAssertOperatorData;
use flow_typing_errors::error_message::EInvalidThisArgData;
use flow_typing_errors::error_message::ENonStrictEqualityComparisonData;
use flow_typing_errors::error_message::EPropNotReadableData;
use flow_typing_errors::error_message::EReactIntrinsicOverlapData;
use flow_typing_errors::error_message::ETupleElementNotReadableData;
use flow_typing_errors::error_message::EnumErrorKind;
use flow_typing_errors::error_message::EnumNotIterableData;
use flow_typing_errors::error_message::EnumNotIterableForInData;
use flow_typing_errors::error_message::EnumReferenceData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::IncompatibleUpperData;
use flow_typing_errors::error_message::InvalidThisArgKind;
use flow_typing_errors::error_message::MatchErrorKind;
use flow_typing_errors::intermediate_error_types::ExpressionReferenceData;
use flow_typing_errors::intermediate_error_types::NamedReferenceData;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_js::flow_js;
use flow_typing_flow_js::flow_js::FlowJs;
use flow_typing_flow_js::tvar_resolver;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_key::Key;
use flow_typing_type::type_;
use flow_typing_type::type_::ArrType;
use flow_typing_type::type_::ArrayATData;
use flow_typing_type::type_::CallArg;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::EnumInfo;
use flow_typing_type::type_::EnumInfoInner;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::MixedFlavor;
use flow_typing_type::type_::ObjKind;
use flow_typing_type::type_::ObjLiteral;
use flow_typing_type::type_::Predicate;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::RootUseOp;
use flow_typing_type::type_::SwitchRefinementCheckData;
use flow_typing_type::type_::TupleATData;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::UnaryArithKind;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::UseT;
use flow_typing_type::type_::UseTInner;
use flow_typing_type::type_::arith_kind::ArithKind;
use flow_typing_type::type_::mixed_t;
use flow_typing_type::type_::type_or_type_desc::TypeOrTypeDescT;
use flow_typing_type::type_util;
use flow_typing_type::type_util::reason_of_t;
use flow_utils_concurrency::job_error::JobError;

use crate::predicate_kit;
use crate::speculation_flow;
use crate::type_filter;

pub mod distribute_union_intersection {
    use super::*;

    /// For a type t, run the check defined by check_base.
    /// This function will break down the unions in t. When it encounters an intersection,
    /// the check can pass as long as the check can pass on one member of intersection
    pub(crate) fn distribute<'cx>(
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
                    speculation_flow::try_custom(
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
                        speculation_flow::try_custom(
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
                                dyn FnOnce(
                                        &mut Context<'cx>,
                                        &FlowJsEnv,
                                    )
                                        -> Result<(), FlowJsException>
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
                        speculation_flow::try_custom(
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
                        speculation_flow::try_custom(
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
}

pub mod operators {
    use flow_typing_type::type_::type_collector::TypeCollector;

    use super::*;

    fn is_string_like(t: &Type) -> bool {
        match t.deref() {
            TypeInner::DefT(_, def_t) => matches!(
                def_t.deref(),
                DefTInner::StrGeneralT(_) | DefTInner::SingletonStrT { .. }
            ),
            TypeInner::TemplateLiteralT { .. } => true,
            _ => false,
        }
    }

    pub fn arith<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        kind: &ArithKind,
        t1: &Type,
        t2: &Type,
    ) -> Result<Type, JobError> {
        let env = &FlowJsEnv::entry();
        let collector = TypeCollector::create();
        flow_js_utils::flow_js_result_to_job_error(distribute_union_intersection::distribute_2(
            cx,
            env,
            None,
            &|cx, reason, t| {
                FlowJs::possible_concrete_types_for_operators_checking_with_env(cx, env, reason, t)
            },
            &|_, _, _| reason.loc().dupe(),
            &|cx, env, (t1, t2)| {
                let result =
                    flow_js_utils::flow_arith(cx, env, reason.dupe(), t1, t2, kind.clone())?;
                collector.add(result);
                Ok(())
            },
            (t1, t2),
        ))?;
        Ok(collector
            .union_opt(reason.dupe())
            .unwrap_or_else(|| tvar_resolver::default_no_lowers(reason)))
    }

    pub fn check_comparator<'cx>(
        cx: &mut Context<'cx>,
        t1: &Type,
        t2: &Type,
    ) -> Result<(), JobError> {
        let env = &FlowJsEnv::entry();
        fn check_base<'cx>(
            cx: &mut Context<'cx>,
            env: &FlowJsEnv,
            (l, r): (&Type, &Type),
        ) -> Result<(), FlowJsException> {
            match (l.deref(), r.deref()) {
                _ if is_string_like(l) && is_string_like(r) => Ok(()),
                (TypeInner::DefT(_, def_l), TypeInner::DefT(_, def_r))
                    if matches!(
                        def_l.deref(),
                        DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. }
                    ) && matches!(
                        def_r.deref(),
                        DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. }
                    ) =>
                {
                    Ok(())
                }
                (TypeInner::DefT(_, def_l), TypeInner::DefT(_, def_r))
                    if matches!(
                        def_l.deref(),
                        DefTInner::BigIntGeneralT(_) | DefTInner::SingletonBigIntT { .. }
                    ) && matches!(
                        def_r.deref(),
                        DefTInner::BigIntGeneralT(_) | DefTInner::SingletonBigIntT { .. }
                    ) =>
                {
                    Ok(())
                }
                (TypeInner::DefT(_, def_l), _) if matches!(def_l.deref(), DefTInner::EmptyT) => {
                    Ok(())
                }
                (_, TypeInner::DefT(_, def_r)) if matches!(def_r.deref(), DefTInner::EmptyT) => {
                    Ok(())
                }
                (TypeInner::AnyT(_, _), _) => Ok(()),
                (_, TypeInner::AnyT(_, _)) => Ok(()),
                _ if flow_js_utils::is_date(l) && flow_js_utils::is_date(r) => Ok(()),
                _ => {
                    let (r1, r2) = flow_js_utils::ordered_reasons(
                        cx,
                        (reason_of_t(l).dupe(), reason_of_t(r).dupe()),
                    );
                    let (t1, t2) = flow_js_utils::ordered_types(cx, (l, r));
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EComparison(Box::new(EComparisonData {
                            r1: flow_js_utils::type_reference_with_reason_for_error(t1, r1),
                            r2: flow_js_utils::type_reference_with_reason_for_error(t2, r2),
                            loc_opt: None,
                            strict_comparison_opt: None,
                        })),
                    )
                }
            }
        }

        flow_js_utils::flow_js_result_to_job_error(distribute_union_intersection::distribute_2(
            cx,
            env,
            None,
            &|cx, reason, t| {
                FlowJs::possible_concrete_types_for_operators_checking_with_env(cx, env, reason, t)
            },
            &|cx, r1, r2| {
                flow_js_utils::ordered_reasons(cx, (r1.dupe(), r2.dupe()))
                    .0
                    .loc()
                    .dupe()
            },
            &check_base,
            (t1, t2),
        ))
    }

    fn eq_needs_concretization(t: &Type) -> bool {
        match t.deref() {
            TypeInner::OpenT(_)
            | TypeInner::GenericT(..)
            | TypeInner::EvalT { .. }
            | TypeInner::TypeAppT(..)
            | TypeInner::OptionalT { .. }
            | TypeInner::UnionT(_, _)
            | TypeInner::MaybeT(_, _)
            | TypeInner::AnnotT(_, _, _)
            | TypeInner::KeysT(_, _)
            | TypeInner::NullProtoT(_) => true,
            _ => false,
        }
    }

    pub fn check_eq<'cx>(cx: &mut Context<'cx>, pair: (&Type, &Type)) -> Result<(), JobError> {
        let env = &FlowJsEnv::entry();
        fn get_no_match_error_loc(cx: &Context<'_>, r1: &Reason, r2: &Reason) -> ALoc {
            flow_js_utils::ordered_reasons(cx, (r1.dupe(), r2.dupe()))
                .0
                .loc()
                .dupe()
        }

        fn will_fail_check_if_unmatched(t: &Type) -> bool {
            match t.deref() {
                TypeInner::DefT(_, def_t) => matches!(
                    def_t.deref(),
                    DefTInner::NumGeneralT(_)
                        | DefTInner::StrGeneralT(_)
                        | DefTInner::BoolGeneralT
                        | DefTInner::SingletonNumT { .. }
                        | DefTInner::SingletonStrT { .. }
                        | DefTInner::SingletonBoolT { .. }
                        | DefTInner::SymbolT
                        | DefTInner::UniqueSymbolT(_)
                        | DefTInner::EnumObjectT { .. }
                        | DefTInner::EnumValueT(_)
                ),
                _ => false,
            }
        }

        fn is_always_allowed_one_side_t(t: &Type) -> bool {
            match t.deref() {
                TypeInner::AnyT(_, _) => true,
                TypeInner::DefT(_, def_t) => matches!(
                    def_t.deref(),
                    DefTInner::EmptyT | DefTInner::MixedT(_) | DefTInner::VoidT | DefTInner::NullT
                ),
                _ => false,
            }
        }

        // If we allow `==` on these two types.
        fn equatable(l: &Type, r: &Type) -> bool {
            match (l.deref(), r.deref()) {
                (TypeInner::DefT(_, def_l), TypeInner::DefT(_, def_r))
                    if matches!(
                        def_l.deref(),
                        DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. }
                    ) && matches!(
                        def_r.deref(),
                        DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. }
                    ) =>
                {
                    true
                }
                (l_inner, r_inner) => {
                    if is_string_like(l) && is_string_like(r) {
                        return true;
                    }
                    match (l_inner, r_inner) {
                        (TypeInner::DefT(_, def_l), TypeInner::DefT(_, def_r))
                            if matches!(
                                def_l.deref(),
                                DefTInner::BoolGeneralT | DefTInner::SingletonBoolT { .. }
                            ) && matches!(
                                def_r.deref(),
                                DefTInner::BoolGeneralT | DefTInner::SingletonBoolT { .. }
                            ) =>
                        {
                            true
                        }
                        (TypeInner::DefT(_, def_l), TypeInner::DefT(_, def_r))
                            if matches!(
                                def_l.deref(),
                                DefTInner::SymbolT | DefTInner::UniqueSymbolT(_)
                            ) && matches!(
                                def_r.deref(),
                                DefTInner::SymbolT | DefTInner::UniqueSymbolT(_)
                            ) =>
                        {
                            true
                        }
                        _ => !will_fail_check_if_unmatched(l) && !will_fail_check_if_unmatched(r),
                    }
                }
            }
        }

        fn distribute<'cx>(
            cx: &mut Context<'cx>,
            env: &FlowJsEnv,
            t1: &Type,
            t2: &Type,
        ) -> Result<(), FlowJsException> {
            if is_always_allowed_one_side_t(t1) || is_always_allowed_one_side_t(t2) {
                return Ok(());
            }
            match (t1.deref(), t2.deref()) {
                (TypeInner::IntersectionT(r1, rep1), _) => {
                    let t2_clone = t2.dupe();
                    let cases: Vec<_> = rep1
                        .members_iter()
                        .map(|t1| {
                            let t1 = t1.dupe();
                            let t2 = t2_clone.dupe();
                            Box::new(move |cx: &mut Context<'cx>, env: &FlowJsEnv| {
                                distribute(cx, env, &t1, &t2)
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
                    speculation_flow::try_custom(
                        cx,
                        env,
                        None,
                        None,
                        None,
                        get_no_match_error_loc(cx, r1, reason_of_t(t2)),
                        cases,
                    )
                }
                (_, TypeInner::IntersectionT(r2, rep2)) => {
                    let t1_clone = t1.dupe();
                    let cases: Vec<_> = rep2
                        .members_iter()
                        .map(|t2| {
                            let t2 = t2.dupe();
                            let t1 = t1_clone.dupe();
                            Box::new(move |cx: &mut Context<'cx>, env: &FlowJsEnv| {
                                distribute(cx, env, &t1, &t2)
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
                    speculation_flow::try_custom(
                        cx,
                        env,
                        None,
                        None,
                        None,
                        get_no_match_error_loc(cx, reason_of_t(t1), r2),
                        cases,
                    )
                }
                _ if eq_needs_concretization(t1) => {
                    let ts = FlowJs::possible_concrete_types_for_operators_checking_with_env(
                        cx,
                        env,
                        reason_of_t(t1),
                        t1,
                    )?;
                    for t1i in &ts {
                        distribute(cx, env, t1i, t2)?;
                    }
                    Ok(())
                }
                _ if eq_needs_concretization(t2) => {
                    let ts = FlowJs::possible_concrete_types_for_operators_checking_with_env(
                        cx,
                        env,
                        reason_of_t(t2),
                        t2,
                    )?;
                    for t2i in &ts {
                        distribute(cx, env, t1, t2i)?;
                    }
                    Ok(())
                }
                _ => {
                    if equatable(t1, t2) {
                        Ok(())
                    } else {
                        let (lower_reason, upper_reason) = flow_js_utils::ordered_reasons(
                            cx,
                            (reason_of_t(t1).dupe(), reason_of_t(t2).dupe()),
                        );
                        let (lower, upper) = flow_js_utils::ordered_types(cx, (t1, t2));
                        flow_js_utils::add_output_with_env(
                            cx,
                            env,
                            ErrorMessage::ENonStrictEqualityComparison(Box::new(
                                ENonStrictEqualityComparisonData {
                                    lower: flow_js_utils::type_reference_with_reason_for_error(
                                        lower,
                                        lower_reason,
                                    ),
                                    upper: flow_js_utils::type_reference_with_reason_for_error(
                                        upper,
                                        upper_reason,
                                    ),
                                },
                            )),
                        )?;
                        Ok(())
                    }
                }
            }
        }

        let (t1, t2) = pair;
        flow_js_utils::flow_js_result_to_job_error(distribute(cx, env, t1, t2))
    }

    pub fn check_strict_eq<'cx>(
        encl_ctx: &EnclosingContext,
        cx: &mut Context<'cx>,
        pair: (&Type, &Type),
    ) -> Result<(), JobError> {
        let env = &FlowJsEnv::entry();
        fn get_no_match_error_loc(cx: &Context<'_>, r1: &Reason, r2: &Reason) -> ALoc {
            flow_js_utils::ordered_reasons(cx, (r1.dupe(), r2.dupe()))
                .0
                .loc()
                .dupe()
        }

        fn not_possible_enum_after_concretization(t: &Type) -> bool {
            match t.deref() {
                TypeInner::DefT(_, def_t) => !matches!(
                    def_t.deref(),
                    DefTInner::EnumValueT(_) | DefTInner::EnumObjectT { .. }
                ),
                TypeInner::IntersectionT(_, _) => false,
                _ => true,
            }
        }

        fn strict_equatable_error<'cx>(
            cx: &mut Context<'cx>,
            encl_ctx: &EnclosingContext,
            l: &Type,
            r: &Type,
        ) -> Option<ErrorMessage<ALoc>> {
            let comparison_error = || -> ErrorMessage<ALoc> {
                match encl_ctx {
                    EnclosingContext::SwitchTestContext {
                        case_test_loc,
                        switch_discriminant_loc,
                    } => {
                        let use_op = UseOp::Op(Arc::new(RootUseOp::SwitchRefinementCheck(
                            Box::new(SwitchRefinementCheckData {
                                test: case_test_loc.dupe(),
                                discriminant: switch_discriminant_loc.dupe(),
                            }),
                        )));
                        flow_js_utils::incompatible_types_error(l, r, use_op, None)
                    }
                    EnclosingContext::NoContext
                    | EnclosingContext::IndexContext
                    | EnclosingContext::OtherTestContext
                    | EnclosingContext::JsxTitleNameContext
                    | EnclosingContext::JsxAttrOrChildrenContext
                    | EnclosingContext::LiteralTestContext
                    | EnclosingContext::MatchPattern
                    | EnclosingContext::StrictComparison => {
                        let (r1, r2) = flow_js_utils::ordered_reasons(
                            cx,
                            (reason_of_t(l).dupe(), reason_of_t(r).dupe()),
                        );
                        let (t1, t2) = flow_js_utils::ordered_types(cx, (l, r));
                        ErrorMessage::EComparison(Box::new(EComparisonData {
                            r1: flow_js_utils::type_reference_with_reason_for_error(t1, r1),
                            r2: flow_js_utils::type_reference_with_reason_for_error(t2, r2),
                            loc_opt: None,
                            strict_comparison_opt: None,
                        }))
                    }
                }
            };

            match (l.deref(), r.deref()) {
                // We allow comparison between enums and enum values with the same id.
                (TypeInner::DefT(_, def_l), TypeInner::DefT(_, def_r))
                    if let (
                        DefTInner::EnumObjectT {
                            enum_info: info1, ..
                        },
                        DefTInner::EnumObjectT {
                            enum_info: info2, ..
                        },
                    ) = (def_l.deref(), def_r.deref())
                        && let (
                            EnumInfoInner::ConcreteEnum(c1),
                            EnumInfoInner::ConcreteEnum(c2),
                        ) = (info1.deref().deref(), info2.deref().deref())
                        && c1.enum_id == c2.enum_id =>
                {
                    None
                }
                (TypeInner::DefT(_, def_l), TypeInner::DefT(_, def_r))
                    if let (DefTInner::EnumValueT(info1), DefTInner::EnumValueT(info2)) =
                        (def_l.deref(), def_r.deref())
                        && let (
                            EnumInfoInner::ConcreteEnum(c1),
                            EnumInfoInner::ConcreteEnum(c2),
                        ) = (info1.deref().deref(), info2.deref().deref())
                        && c1.enum_id == c2.enum_id =>
                {
                    None
                }
                // We allow comparison between abstract and concrete enums and enum values.
                (TypeInner::DefT(_, def_l), TypeInner::DefT(_, def_r))
                    if let (
                        DefTInner::EnumObjectT {
                            enum_info: info1, ..
                        },
                        DefTInner::EnumObjectT {
                            enum_info: info2, ..
                        },
                    ) = (def_l.deref(), def_r.deref())
                        && (matches!(
                            info1.deref().deref(),
                            EnumInfoInner::AbstractEnum { .. }
                        ) || matches!(
                            info2.deref().deref(),
                            EnumInfoInner::AbstractEnum { .. }
                        )) =>
                {
                    None
                }
                (TypeInner::DefT(_, def_l), TypeInner::DefT(_, def_r))
                    if let (DefTInner::EnumValueT(info1), DefTInner::EnumValueT(info2)) =
                        (def_l.deref(), def_r.deref())
                        && (matches!(
                            info1.deref().deref(),
                            EnumInfoInner::AbstractEnum { .. }
                        ) || matches!(
                            info2.deref().deref(),
                            EnumInfoInner::AbstractEnum { .. }
                        )) =>
                {
                    None
                }
                // We allow the comparison of enums to null and void outside of switches.
                (TypeInner::DefT(_, def_l), TypeInner::DefT(_, def_r))
                    if matches!(
                        (def_l.deref(), def_r.deref()),
                        (
                            DefTInner::EnumValueT(_),
                            DefTInner::NullT | DefTInner::VoidT
                        ) | (
                            DefTInner::NullT | DefTInner::VoidT,
                            DefTInner::EnumValueT(_)
                        )
                    ) =>
                {
                    match encl_ctx {
                        EnclosingContext::SwitchTestContext { .. } => Some(comparison_error()),
                        _ => None,
                    }
                }
                // We don't allow the comparison of enums and other types in general.
                (TypeInner::DefT(_, def), _) | (_, TypeInner::DefT(_, def))
                    if matches!(
                        def.deref(),
                        DefTInner::EnumValueT(_) | DefTInner::EnumObjectT { .. }
                    ) =>
                {
                    None
                }
                // We don't check other strict equality comparisons.
                _ => None,
            }
        }

        fn distribute_strict<'cx>(
            cx: &mut Context<'cx>,
            env: &FlowJsEnv,
            encl_ctx: &EnclosingContext,
            t1: &Type,
            t2: &Type,
        ) -> Result<(), FlowJsException> {
            match (t1.deref(), t2.deref()) {
                (TypeInner::DefT(_, def_l), _) if matches!(def_l.deref(), DefTInner::EmptyT) => {
                    Ok(())
                }
                (_, TypeInner::DefT(_, def_r)) if matches!(def_r.deref(), DefTInner::EmptyT) => {
                    Ok(())
                }
                (TypeInner::AnyT(_, _), _) | (_, TypeInner::AnyT(_, _)) => Ok(()),
                (TypeInner::IntersectionT(r1, rep1), _) => {
                    let t2_clone = t2.dupe();
                    let cases: Vec<_> = rep1
                        .members_iter()
                        .map(|t1| {
                            let t1 = t1.dupe();
                            let t2 = t2_clone.dupe();
                            Box::new(move |cx: &mut Context<'cx>, env: &FlowJsEnv| {
                                distribute_strict(cx, env, encl_ctx, &t1, &t2)
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
                    speculation_flow::try_custom(
                        cx,
                        env,
                        None,
                        None,
                        None,
                        get_no_match_error_loc(cx, r1, reason_of_t(t2)),
                        cases,
                    )
                }
                (_, TypeInner::IntersectionT(r2, rep2)) => {
                    let t1_clone = t1.dupe();
                    let cases: Vec<_> = rep2
                        .members_iter()
                        .map(|t2| {
                            let t2 = t2.dupe();
                            let t1 = t1_clone.dupe();
                            Box::new(move |cx: &mut Context<'cx>, env: &FlowJsEnv| {
                                distribute_strict(cx, env, encl_ctx, &t1, &t2)
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
                    speculation_flow::try_custom(
                        cx,
                        env,
                        None,
                        None,
                        None,
                        get_no_match_error_loc(cx, reason_of_t(t1), r2),
                        cases,
                    )
                }
                _ => {
                    let t1_needs_concretization = eq_needs_concretization(t1);
                    let t2_needs_concretization = eq_needs_concretization(t2);
                    if !t1_needs_concretization && !t2_needs_concretization {
                        match strict_equatable_error(cx, encl_ctx, t1, t2) {
                            Some(error) => {
                                flow_js_utils::add_output_with_env(cx, env, error)?;
                                Ok(())
                            }
                            None => Ok(()),
                        }
                    } else {
                        let t1s = if t1_needs_concretization {
                            FlowJs::possible_concrete_types_for_operators_checking_with_env(
                                cx,
                                env,
                                reason_of_t(t1),
                                t1,
                            )?
                        } else {
                            vec![t1.dupe()]
                        };
                        let t2s = if t2_needs_concretization {
                            FlowJs::possible_concrete_types_for_operators_checking_with_env(
                                cx,
                                env,
                                reason_of_t(t2),
                                t2,
                            )?
                        } else {
                            vec![t2.dupe()]
                        };
                        if t1s.iter().all(not_possible_enum_after_concretization)
                            && t2s.iter().all(not_possible_enum_after_concretization)
                        {
                            Ok(())
                        } else {
                            for t1i in &t1s {
                                for t2i in &t2s {
                                    distribute_strict(cx, env, encl_ctx, t1i, t2i)?;
                                }
                            }
                            Ok(())
                        }
                    }
                }
            }
        }

        let (t1, t2) = pair;
        flow_js_utils::flow_js_result_to_job_error(distribute_strict(cx, env, encl_ctx, t1, t2))
    }

    pub fn unary_arith<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        kind: &UnaryArithKind,
        t: &Type,
    ) -> Result<Type, JobError> {
        let env = &FlowJsEnv::entry();
        let collector = TypeCollector::create();
        flow_js_utils::flow_js_result_to_job_error(distribute_union_intersection::distribute(
            cx,
            env,
            None,
            &|cx, reason, t| {
                FlowJs::possible_concrete_types_for_operators_checking_with_env(cx, env, reason, t)
            },
            &|_, r| r.loc().dupe(),
            &|cx, env, t| {
                let result = flow_js_utils::flow_unary_arith(cx, env, t, reason.dupe(), *kind)?;
                collector.add(result);
                Ok(())
            },
            t,
        ))?;
        Ok(collector
            .union_opt(reason.dupe())
            .unwrap_or_else(|| tvar_resolver::default_no_lowers(reason)))
    }

    fn collect_logical_union<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        f: impl FnOnce(&mut Context<'cx>, &UseT<Cx>) -> Result<(), FlowJsException>,
    ) -> Result<Type, FlowJsException> {
        use std::collections::BTreeSet;

        use flow_typing_type::type_::ConcretizationKind;
        use flow_typing_type::type_::ConcretizeTData;
        use flow_typing_type::type_::concretize_seen::ConcretizeSeen;
        use flow_typing_type::type_::type_collector::TypeCollector;

        let collector = TypeCollector::create();
        let use_t = UseT::new(UseTInner::ConcretizeT(Box::new(ConcretizeTData {
            reason: reason.dupe(),
            kind: ConcretizationKind::ConcretizeForLowerBounds,
            seen: ConcretizeSeen::new(),
            collector: collector.dupe(),
        })));
        f(cx, &use_t)?;
        let mut lowers = Vec::new();
        flow_js_utils::collect_lowers(
            true,
            cx,
            &mut BTreeSet::new(),
            &mut lowers,
            collector.collect_to_vec(),
        );
        Ok(type_util::union_of_ts_opt(
            reason.dupe(),
            lowers,
            Some(type_::union_rep::UnionKind::LogicalKind),
        )
        .unwrap_or_else(|| tvar_resolver::default_no_lowers(reason)))
    }

    pub fn logical_and<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        left: &Type,
        right: &Type,
    ) -> Result<Type, JobError> {
        let env = &FlowJsEnv::entry();
        flow_js_utils::flow_js_result_to_job_error(collect_logical_union(cx, reason, |cx, tout| {
            let ts = FlowJs::possible_concrete_types_for_inspection_with_env(
                cx,
                env,
                reason_of_t(left),
                left,
            )?;
            for left in &ts {
                if let TypeInner::DefT(reason, def_t) = left.deref() {
                    if matches!(
                        def_t.deref(),
                        DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. }
                    ) {
                        flow_js_utils::add_output_with_env(
                            cx,
                            env,
                            ErrorMessage::ESketchyNumberLint(
                                flow_lint_settings::lints::SketchyNumberKind::And,
                                flow_js_utils::type_reference_with_reason_for_error(
                                    left,
                                    reason.dupe(),
                                ),
                            ),
                        )?;
                    }
                }
                // a falsy && b ~> a
                // a truthy && b ~> b
                // a && b ~> a falsy | b
                match type_filter::truthy(cx, left.dupe()) {
                    type_filter::FilterResult { type_: t, .. } if matches!(t.deref(), TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::EmptyT)) =>
                    {
                        // falsy
                        predicate_kit::run_predicate_for_filtering_with_env(
                            cx,
                            env,
                            left,
                            &Predicate::new(type_::PredicateInner::NotP(Predicate::new(
                                type_::PredicateInner::TruthyP,
                            ))),
                            tout,
                        )?;
                    }
                    _ => {
                        match type_filter::not_truthy(cx, left.dupe()) {
                            type_filter::FilterResult { type_: t, .. } if matches!(t.deref(), TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::EmptyT)) =>
                            {
                                // truthy
                                flow_js::flow_with_env(cx, env, (right, tout))?;
                            }
                            _ => {
                                predicate_kit::run_predicate_for_filtering_with_env(
                                    cx,
                                    env,
                                    left,
                                    &Predicate::new(type_::PredicateInner::NotP(Predicate::new(
                                        type_::PredicateInner::TruthyP,
                                    ))),
                                    tout,
                                )?;
                                flow_js::flow_with_env(cx, env, (right, tout))?;
                            }
                        }
                    }
                }
            }
            Ok(())
        }))
    }

    pub fn logical_or<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        left: &Type,
        right: &Type,
    ) -> Result<Type, JobError> {
        let env = &FlowJsEnv::entry();
        flow_js_utils::flow_js_result_to_job_error(collect_logical_union(cx, reason, |cx, tout| {
            let ts = FlowJs::possible_concrete_types_for_inspection_with_env(
                cx,
                env,
                reason_of_t(left),
                left,
            )?;
            for left in &ts {
                // a truthy || b ~> a
                // a falsy || b ~> b
                // a || b ~> a truthy | b
                match type_filter::not_truthy(cx, left.dupe()) {
                    type_filter::FilterResult { type_: t, .. } if matches!(t.deref(), TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::EmptyT)) =>
                    {
                        // truthy
                        predicate_kit::run_predicate_for_filtering_with_env(
                            cx,
                            env,
                            left,
                            &Predicate::new(type_::PredicateInner::TruthyP),
                            tout,
                        )?;
                    }
                    _ => {
                        match type_filter::truthy(cx, left.dupe()) {
                            type_filter::FilterResult { type_: t, .. } if matches!(t.deref(), TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::EmptyT)) =>
                            {
                                // falsy
                                flow_js::flow_with_env(cx, env, (right, tout))?;
                            }
                            _ => {
                                predicate_kit::run_predicate_for_filtering_with_env(
                                    cx,
                                    env,
                                    left,
                                    &Predicate::new(type_::PredicateInner::TruthyP),
                                    tout,
                                )?;
                                flow_js::flow_with_env(cx, env, (right, tout))?;
                            }
                        }
                    }
                }
            }
            Ok(())
        }))
    }

    pub fn logical_nullish_coalesce<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        left: &Type,
        right: &Type,
    ) -> Result<Type, JobError> {
        let env = &FlowJsEnv::entry();
        flow_js_utils::flow_js_result_to_job_error(collect_logical_union(cx, reason, |cx, tout| {
            let ts = FlowJs::possible_concrete_types_for_inspection_with_env(
                cx,
                env,
                reason_of_t(left),
                left,
            )?;
            for left in &ts {
                let maybe_result = type_filter::maybe(cx, left.dupe());
                match &maybe_result {
                    type_filter::FilterResult { type_: t, .. } if matches!(t.deref(), TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::EmptyT)) =>
                    {
                        predicate_kit::run_predicate_for_filtering_with_env(
                            cx,
                            env,
                            left,
                            &Predicate::new(type_::PredicateInner::NotP(Predicate::new(
                                type_::PredicateInner::MaybeP,
                            ))),
                            tout,
                        )?;
                    }
                    // This `AnyT` case is required to have similar behavior to the other logical operators.
                    type_filter::FilterResult { type_: t, .. }
                        if matches!(t.deref(), TypeInner::AnyT(_, _)) =>
                    {
                        // not-nullish
                        predicate_kit::run_predicate_for_filtering_with_env(
                            cx,
                            env,
                            left,
                            &Predicate::new(type_::PredicateInner::NotP(Predicate::new(
                                type_::PredicateInner::MaybeP,
                            ))),
                            tout,
                        )?;
                    }
                    _ => {
                        match type_filter::not_maybe(cx, left.dupe()) {
                            type_filter::FilterResult { type_: t, .. } if matches!(t.deref(), TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::EmptyT)) =>
                            {
                                // nullish
                                flow_js::flow_with_env(cx, env, (right, tout))?;
                            }
                            _ => {
                                predicate_kit::run_predicate_for_filtering_with_env(
                                    cx,
                                    env,
                                    left,
                                    &Predicate::new(type_::PredicateInner::NotP(Predicate::new(
                                        type_::PredicateInner::MaybeP,
                                    ))),
                                    tout,
                                )?;
                                flow_js::flow_with_env(cx, env, (right, tout))?;
                            }
                        }
                    }
                }
            }
            Ok(())
        }))
    }

    pub fn unary_not<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        t: &Type,
    ) -> Result<Type, JobError> {
        let env = &FlowJsEnv::entry();
        fn f(reason: &Reason, t: &Type) -> Type {
            match t.deref() {
                TypeInner::AnyT(_, src) => type_::any_t::why(*src, reason.dupe()),
                TypeInner::DefT(_, def_t)
                    if matches!(
                        def_t.deref(),
                        DefTInner::BoolGeneralT
                            | DefTInner::StrGeneralT(type_::Literal::AnyLiteral)
                            | DefTInner::NumGeneralT(type_::Literal::AnyLiteral)
                    ) =>
                {
                    type_::bool_module_t::at(reason.loc().dupe())
                }
                // !x when x is falsy
                TypeInner::DefT(_, def_t)
                    if matches!(
                        def_t.deref(),
                        DefTInner::SingletonBoolT { value: false, .. }
                    ) =>
                {
                    let reason = reason
                        .dupe()
                        .replace_desc(flow_common::reason::VirtualReasonDesc::RBooleanLit(true));
                    Type::new(TypeInner::DefT(
                        reason,
                        DefT::new(DefTInner::SingletonBoolT {
                            value: true,
                            from_annot: false,
                        }),
                    ))
                }
                TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::SingletonStrT { value, .. } if value.is_empty()) =>
                {
                    let reason = reason
                        .dupe()
                        .replace_desc(flow_common::reason::VirtualReasonDesc::RBooleanLit(true));
                    Type::new(TypeInner::DefT(
                        reason,
                        DefT::new(DefTInner::SingletonBoolT {
                            value: true,
                            from_annot: false,
                        }),
                    ))
                }
                TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::SingletonNumT { value: type_::NumberLiteral(v, _), .. } if *v == 0.0) =>
                {
                    let reason = reason
                        .dupe()
                        .replace_desc(flow_common::reason::VirtualReasonDesc::RBooleanLit(true));
                    Type::new(TypeInner::DefT(
                        reason,
                        DefT::new(DefTInner::SingletonBoolT {
                            value: true,
                            from_annot: false,
                        }),
                    ))
                }
                TypeInner::DefT(_, def_t)
                    if matches!(def_t.deref(), DefTInner::NullT | DefTInner::VoidT) =>
                {
                    let reason = reason
                        .dupe()
                        .replace_desc(flow_common::reason::VirtualReasonDesc::RBooleanLit(true));
                    Type::new(TypeInner::DefT(
                        reason,
                        DefT::new(DefTInner::SingletonBoolT {
                            value: true,
                            from_annot: false,
                        }),
                    ))
                }
                // !x when x is truthy
                _ => {
                    let reason = reason
                        .dupe()
                        .replace_desc(flow_common::reason::VirtualReasonDesc::RBooleanLit(false));
                    Type::new(TypeInner::DefT(
                        reason,
                        DefT::new(DefTInner::SingletonBoolT {
                            value: false,
                            from_annot: false,
                        }),
                    ))
                }
            }
        }

        let reason = reason.dupe();
        flow_js_utils::flow_js_result_to_job_error(tvar_resolver::mk_tvar_and_fully_resolve_where(
            cx,
            reason.dupe(),
            |cx, tout| {
                distribute_union_intersection::distribute(
                    cx,
                    env,
                    None,
                    &|cx, r, t| {
                        FlowJs::possible_concrete_types_for_inspection_with_env(cx, env, r, t)
                    },
                    &|_, r| r.loc().dupe(),
                    &|cx, env, t| {
                        let result = f(&reason, t);
                        flow_js::flow_t_with_env(cx, env, (&result, tout))?;
                        Ok(())
                    },
                    t,
                )
            },
        ))
    }

    pub fn non_maybe<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        t: &Type,
    ) -> Result<Type, JobError> {
        let env = &FlowJsEnv::entry();
        fn f(t: &Type) -> Type {
            match t.deref() {
                TypeInner::DefT(r, def_t)
                    if matches!(def_t.deref(), DefTInner::NullT | DefTInner::VoidT) =>
                {
                    type_::empty_t::why(r.dupe())
                }
                TypeInner::DefT(r, def_t)
                    if matches!(
                        def_t.deref(),
                        DefTInner::MixedT(
                            MixedFlavor::MixedEverything
                                | MixedFlavor::MixedNonNull
                                | MixedFlavor::MixedNonVoid
                        )
                    ) =>
                {
                    Type::new(TypeInner::DefT(
                        r.dupe(),
                        DefT::new(DefTInner::MixedT(MixedFlavor::MixedNonMaybe)),
                    ))
                }
                _ => t.dupe(),
            }
        }

        let collector = TypeCollector::create();
        flow_js_utils::flow_js_result_to_job_error(distribute_union_intersection::distribute(
            cx,
            env,
            None,
            &|cx, r, t| FlowJs::possible_concrete_types_for_inspection_with_env(cx, env, r, t),
            &|_, r| r.loc().dupe(),
            &|_, _, t| {
                collector.add(f(t));
                Ok(())
            },
            t,
        ))?;
        Ok(collector
            .union_opt(reason.dupe())
            .unwrap_or_else(|| tvar_resolver::default_no_lowers(reason)))
    }
}

pub mod promise {

    use super::*;

    pub fn await_<'cx>(cx: &mut Context<'cx>, reason: &Reason, t: &Type) -> Result<Type, JobError> {
        let env = &FlowJsEnv::entry();
        // await distributes over union: await (Promise<T> | void) = T | void
        let ts = flow_js_utils::flow_js_result_to_job_error(
            FlowJs::possible_concrete_types_for_inspection_with_env(cx, env, reason, t),
        )?;
        let mut results = Vec::with_capacity(ts.len());
        for t in ts.iter() {
            results.push(flow_js_utils::flow_js_result_to_job_error(
                FlowJs::run_await(cx, env, type_::unknown_use(), reason, t),
            )?);
        }
        Ok(match results.as_slice() {
            [] => type_::empty_t::why(reason.dupe()),
            [t] => t.dupe(),
            [t0, t1, ts @ ..] => Type::new(TypeInner::UnionT(
                reason.dupe(),
                type_::union_rep::make(
                    None,
                    type_::union_rep::UnionKind::UnknownKind,
                    t0.dupe(),
                    t1.dupe(),
                    Rc::from(ts),
                ),
            )),
        })
    }
}

pub mod special_cased_functions {
    use super::*;

    pub fn object_assign<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        target_t: &Type,
        rest_arg_ts: &[CallArg],
    ) -> Result<Type, JobError> {
        flow_js_utils::flow_js_result_to_job_error(object_assign_with_env(
            cx,
            &FlowJsEnv::entry(),
            reason,
            target_t,
            rest_arg_ts,
        ))
    }

    fn object_assign_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason: &Reason,
        target_t: &Type,
        rest_arg_ts: &[CallArg],
    ) -> Result<Type, FlowJsException> {
        use std::cell::RefCell;
        use std::collections::HashMap;

        use flow_common::polarity::Polarity;
        use flow_typing_type::type_::CallArgInner;
        use flow_typing_type::type_::ObjAssignKind;
        use flow_typing_type::type_::SetMode;
        use flow_typing_type::type_::UseT;
        use flow_typing_type::type_::UseTInner;
        use flow_typing_type::type_::WriteCtx;

        /*
        What is this EVIL cache doing? Consider the following example:

        ```
        declare const inter: {foo: string, ...} & {bar: number, ...};
        Object.assign(inter, inter);
        ```

        Of course, we want it to pass, because it should be equivalent to

        ```
        declare const inter: {foo: string, bar: number, ...};
        Object.assign(inter, inter);
        ```

        which should pass.

        However, without the cache, the above code will be treated like

        - Try to assign `{foo: string, ...} & {bar: number, ...}` to `{foo: string, ...}`
            - Assign `{foo: string, ...}` to `{foo: string, ...}` -> Good
            - Assign `{bar: number, ...}` to `{foo: string, ...}` -> Failed
            - This branch of speculation has failed
        - Try to assign `{foo: string, ...} & {bar: number, ...}` to `{bar: number, ...}`
            - Assign `{foo: string, ...}` to `{bar: number, ...}` -> Failed
            - This branch of speculation has failed
        - Speculation failed!

        The cache is trying to "fix" this by trying to assign each member of intersection only once,
        so the above example becomes something like

        - Try to assign `{foo: string, ...} & {bar: number, ...}` to `{foo: string, ...}`
            - Assign `{foo: string, ...}` to `{foo: string, ...}` -> Good
            - Assign `{bar: number, ...}` to `{foo: string, ...}` -> Failed
            - This branch of speculation has failed
        - Try to assign `{foo: string, ...} & {bar: number, ...}` to `{bar: number, ...}`
            - Assign `{foo: string, ...}` to `{bar: number, ...}` -> Cache hit
            - Assign `{bar: number, ...}` to `{bar: number, ...}` -> Cache hit
            - This branch of speculation has succeed by doing nothing...
        - Speculation succeeded!

        This is a very bad fix, since it's easily broken by the following example:

        ```
        declare const inter: {foo: string, ...} & {bar: number, ...};
        declare const inter2: {bar: number, ...} & {foo: string, ...};
        Object.assign(inter, inter2);
        ```

        Since `Object.assign` support is on the chopping block, it might not make sense to fix
        it in a proper way, but the least we can do is to localize the broken cache. The cache used
        to be global one.
        */
        let fix_cache: Rc<RefCell<HashMap<Type, Type>>> = Rc::new(RefCell::new(HashMap::new()));

        fn assign_from<'cx>(
            cx: &mut Context<'cx>,
            env: &FlowJsEnv,
            reason: &Reason,
            fix_cache: &Rc<RefCell<HashMap<Type, Type>>>,
            l: &Type,
            use_op: &UseOp,
            reason_op: &Reason,
            to_obj: &Type,
            t: &Type,
            kind: &ObjAssignKind,
        ) -> Result<(), FlowJsException> {
            match to_obj.deref() {
                // Special case any. Otherwise this will lead to confusing errors when
                // any tranforms to an object type.
                TypeInner::AnyT(_, _) => {
                    flow_js::flow_t_with_env(cx, env, (to_obj, t))?;
                    Ok(())
                }
                _ => {
                    let ls = FlowJs::possible_concrete_types_for_object_assign(
                        cx,
                        env,
                        reason_of_t(l),
                        l,
                    )?;
                    for l in &ls {
                        assign_from_after_concretization(
                            cx, env, reason, fix_cache, l, use_op, reason_op, to_obj, t, kind,
                        )?;
                    }
                    Ok(())
                }
            }
        }

        fn assign_from_after_concretization<'cx>(
            cx: &mut Context<'cx>,
            env: &FlowJsEnv,
            reason: &Reason,
            fix_cache: &Rc<RefCell<HashMap<Type, Type>>>,
            l: &Type,
            use_op: &UseOp,
            reason_op: &Reason,
            to_obj: &Type,
            t: &Type,
            kind: &ObjAssignKind,
        ) -> Result<(), FlowJsException> {
            match (l.deref(), kind) {
                // assign_from copies multiple properties from its incoming LB.
                // Here we simulate a merged object type by iterating over the
                // entire intersection.
                (TypeInner::IntersectionT(_, rep), kind) => {
                    let kind = kind.clone();
                    let members: Vec<Type> = rep.members_iter().duped().collect();
                    let mut previous: Option<Type> = None;
                    for member in &members {
                        let cached = fix_cache.borrow().get(member).cloned();
                        let member_tvar = match cached {
                            Some(tv) => tv,
                            None => flow_typing_tvar::mk_where(cx, reason_op.dupe(), |cx, tv| {
                                fix_cache.borrow_mut().insert(member.dupe(), tv.dupe());
                                assign_from(
                                    cx, env, reason, fix_cache, member, use_op, reason_op, to_obj,
                                    tv, &kind,
                                )
                            })?,
                        };
                        if let Some(previous) = previous {
                            flow_js::flow_t_with_env(cx, env, (&member_tvar, &previous))?;
                        }
                        previous = Some(member_tvar);
                    }
                    if let Some(previous) = previous {
                        flow_js::flow_t_with_env(cx, env, (&previous, t))?;
                    }
                    Ok(())
                }
                (TypeInner::DefT(lreason, def_t), ObjAssignKind::ObjAssign { .. })
                    if let DefTInner::ObjT(obj) = def_t.deref() =>
                {
                    let mapr = obj.props_tmap.dupe();
                    let flags = &obj.flags;
                    cx.try_iter_props(mapr.dupe(), |cx, name, p| -> Result<(), FlowJsException> {
                        let reason_prop = lreason
                            .dupe()
                            .replace_desc(flow_common::reason::VirtualReasonDesc::RProperty(Some(
                                name.dupe(),
                            )))
                            .reposition(reason_op.loc().dupe());
                        match type_::property::read_t(p) {
                            Some(prop_t) => {
                                let propref = flow_typing_type::type_util::mk_named_prop(
                                    reason_prop.dupe(),
                                    false,
                                    name.dupe(),
                                );
                                let prop_t = tvar_resolver::mk_tvar_and_fully_resolve_where(
                                    cx,
                                    reason_prop.dupe(),
                                    |cx, tout| -> Result<(), FlowJsException> {
                                        let use_t = UseT::new(UseTInner::FilterOptionalT(
                                            type_::unknown_use(),
                                            tout.dupe(),
                                        ));
                                        flow_js::flow_with_env(cx, env, (&prop_t, &use_t))?;
                                        Ok(())
                                    },
                                )?;
                                let set_prop_use = UseT::new(UseTInner::SetPropT(
                                    use_op.clone(),
                                    reason_prop,
                                    Box::new(propref),
                                    SetMode::Assign,
                                    WriteCtx::Normal,
                                    prop_t,
                                    None,
                                ));
                                flow_js::flow_with_env(cx, env, (to_obj, &set_prop_use))?;
                            }
                            None => {
                                flow_js_utils::add_output_with_env(
                                    cx,
                                    env,
                                    ErrorMessage::EPropNotReadable(Box::new(
                                        EPropNotReadableData {
                                            prop_loc: reason_prop.loc().dupe(),
                                            prop_name: Some(name.dupe()),
                                            use_op: use_op.clone(),
                                        },
                                    )),
                                )?;
                            }
                        }
                        Ok(())
                    })?;
                    match &flags.obj_kind {
                        ObjKind::Indexed(_) => {
                            let any =
                                type_::any_t::make(type_::AnySource::Untyped, reason_op.dupe());
                            flow_js::flow_t_with_env(cx, env, (&any, t))?;
                            Ok(())
                        }
                        ObjKind::Exact | ObjKind::Inexact => {
                            flow_js::flow_t_with_env(cx, env, (to_obj, t))?;
                            Ok(())
                        }
                    }
                }
                (TypeInner::DefT(lreason, def_t), ObjAssignKind::ObjAssign { .. })
                    if let DefTInner::InstanceT(inst_t) = def_t.deref() =>
                {
                    let inst = &inst_t.inst;
                    let own_props = cx.find_props(inst.own_props.dupe());
                    let proto_props = cx.find_props(inst.proto_props.dupe());
                    for (name, p) in own_props.iter().chain(
                        proto_props
                            .iter()
                            .filter(|(n, _)| !own_props.contains_key(n)),
                    ) {
                        match type_::property::read_t(p) {
                            Some(prop_t) => {
                                let propref = flow_typing_type::type_util::mk_named_prop(
                                    reason_op.dupe(),
                                    false,
                                    name.dupe(),
                                );
                                let set_prop_use = UseT::new(UseTInner::SetPropT(
                                    use_op.clone(),
                                    reason_op.dupe(),
                                    Box::new(propref),
                                    SetMode::Assign,
                                    WriteCtx::Normal,
                                    prop_t,
                                    None,
                                ));
                                flow_js::flow_with_env(cx, env, (to_obj, &set_prop_use))?;
                            }
                            None => {
                                flow_js_utils::add_output_with_env(
                                    cx,
                                    env,
                                    ErrorMessage::EPropNotReadable(Box::new(
                                        EPropNotReadableData {
                                            prop_loc: lreason.loc().dupe(),
                                            prop_name: Some(name.dupe()),
                                            use_op: use_op.clone(),
                                        },
                                    )),
                                )?;
                            }
                        }
                    }
                    flow_js::flow_t_with_env(cx, env, (to_obj, t))?;
                    Ok(())
                }
                (TypeInner::AnyT(_, src), ObjAssignKind::ObjAssign { .. }) => {
                    let any = type_::any_t::make(*src, reason.dupe());
                    flow_js::flow_t_with_env(cx, env, (&any, t))?;
                    Ok(())
                }
                (TypeInner::AnyT(_, _), _) => {
                    flow_js::flow_t_with_env(cx, env, (l, t))?;
                    Ok(())
                }
                (TypeInner::ObjProtoT(_), ObjAssignKind::ObjAssign { .. }) => {
                    flow_js::flow_t_with_env(cx, env, (to_obj, t))?;
                    Ok(())
                }
                // Object.assign semantics
                (TypeInner::DefT(_, def_t), ObjAssignKind::ObjAssign { .. })
                    if matches!(def_t.deref(), DefTInner::NullT | DefTInner::VoidT) =>
                {
                    flow_js::flow_t_with_env(cx, env, (to_obj, t))?;
                    Ok(())
                }
                // {...mixed} is the equivalent of {...{[string]: mixed}}
                (TypeInner::DefT(mixed_reason, def_t), ObjAssignKind::ObjAssign { .. })
                    if matches!(def_t.deref(), DefTInner::MixedT(_)) =>
                {
                    let dict = type_::DictType {
                        dict_name: None,
                        key: type_::str_module_t::make(mixed_reason.dupe()),
                        value: l.dupe(),
                        dict_polarity: Polarity::Neutral,
                    };
                    let proto = Type::new(TypeInner::ObjProtoT(mixed_reason.dupe()));
                    let o = flow_typing_flow_common::obj_type::mk_with_proto(
                        cx,
                        mixed_reason.dupe(),
                        ObjKind::Indexed(dict),
                        None,
                        None,
                        None,
                        None,
                        proto,
                        ObjLiteral::NonLiteral,
                    );
                    assign_from_after_concretization(
                        cx, env, reason, fix_cache, &o, use_op, reason_op, to_obj, t, kind,
                    )
                }
                (TypeInner::DefT(reason_arr, def_t), ObjAssignKind::ObjSpreadAssign)
                    if let DefTInner::ArrT(arrtype) = def_t.deref() =>
                {
                    match arrtype.deref() {
                        ArrType::ArrayAT(box ArrayATData {
                            elem_t,
                            tuple_view: None,
                            ..
                        })
                        | ArrType::ArrayAT(box ArrayATData {
                            elem_t,
                            tuple_view: Some(type_::TupleView { inexact: true, .. }),
                            ..
                        })
                        | ArrType::ROArrayAT(box ROArrayATData { elem_t, .. }) => {
                            // Object.assign(o, ...Array<x>) -> Object.assign(o, x)
                            let default_kind = type_::default_obj_assign_kind();
                            assign_from(
                                cx,
                                env,
                                reason,
                                fix_cache,
                                elem_t,
                                use_op,
                                reason_op,
                                to_obj,
                                t,
                                &default_kind,
                            )
                        }
                        ArrType::TupleAT(box TupleATData { elements, rest, .. }) => {
                            // Object.assign(o, ...[x,y,z]) -> Object.assign(o, x, y, z)
                            // Object.assign(o, ...[x, ...Array<y>]) -> Object.assign(o, x, y)
                            for (n, elem) in elements.iter().chain(rest).enumerate() {
                                if !Polarity::compat(elem.polarity, Polarity::Positive) {
                                    flow_js_utils::add_output_with_env(
                                        cx,
                                        env,
                                        ErrorMessage::ETupleElementNotReadable(Box::new(
                                            ETupleElementNotReadableData {
                                                use_op: use_op.clone(),
                                                loc: reason_arr.loc().dupe(),
                                                index_def_loc: reason_arr.def_loc().dupe(),
                                                index: n as i32,
                                                name: elem.name.clone(),
                                            },
                                        )),
                                    )?;
                                }
                                let default_kind = type_::default_obj_assign_kind();
                                assign_from(
                                    cx,
                                    env,
                                    reason,
                                    fix_cache,
                                    &elem.t,
                                    use_op,
                                    reason_op,
                                    to_obj,
                                    t,
                                    &default_kind,
                                )?;
                            }
                            Ok(())
                        }
                        ArrType::ArrayAT(box ArrayATData {
                            tuple_view: Some(tuple_view),
                            ..
                        }) => {
                            // Object.assign(o, ...[x,y,z]) -> Object.assign(o, x, y, z)
                            let ts: Vec<Type> = tuple_view
                                .elements
                                .iter()
                                .chain(&tuple_view.rest)
                                .map(|elem| elem.t.dupe())
                                .collect();
                            let default_kind = type_::default_obj_assign_kind();
                            for from in &ts {
                                assign_from(
                                    cx,
                                    env,
                                    reason,
                                    fix_cache,
                                    from,
                                    use_op,
                                    reason_op,
                                    to_obj,
                                    t,
                                    &default_kind,
                                )?;
                            }
                            Ok(())
                        }
                    }
                }
                (TypeInner::GenericT(box GenericTData { reason, bound, .. }), _) => {
                    let repositioned =
                        flow_js::reposition_with_env(cx, env, reason.loc().dupe(), bound.dupe())?;
                    assign_from(
                        cx,
                        env,
                        reason,
                        fix_cache,
                        &repositioned,
                        use_op,
                        reason_op,
                        to_obj,
                        t,
                        kind,
                    )
                }
                _ => {
                    let upper_kind = match kind {
                        ObjAssignKind::ObjSpreadAssign => {
                            flow_typing_errors::error_message::UpperKind::IncompatibleObjAssignFromTSpread
                        }
                        ObjAssignKind::ObjAssign { .. } => {
                            flow_typing_errors::error_message::UpperKind::IncompatibleObjAssignFromT
                        }
                    };
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        flow_js_utils::incompatible_type_error(
                            l,
                            IncompatibleUpperData {
                                loc: reason_op.loc().dupe(),
                                kind: upper_kind,
                            },
                            Some(use_op.clone()),
                        ),
                    )?;
                    let any = type_::any_t::error(reason_op.dupe());
                    flow_js::flow_t_with_env(cx, env, (&any, t))?;
                    Ok(())
                }
            }
        }

        let reason = reason.dupe();
        let mut result = target_t.dupe();
        for that in rest_arg_ts {
            let (that, kind) = match that.deref() {
                CallArgInner::Arg(t) => (t.dupe(), type_::default_obj_assign_kind()),
                CallArgInner::SpreadArg(t) => (t.dupe(), ObjAssignKind::ObjSpreadAssign),
            };
            let chain_use_op = UseOp::Op(Arc::new(RootUseOp::ObjectChain { op: reason.dupe() }));
            result = tvar_resolver::mk_tvar_and_fully_resolve_where(
                cx,
                reason.dupe(),
                |cx, inner_t| -> Result<(), FlowJsException> {
                    let ls = FlowJs::possible_concrete_types_for_object_assign(
                        cx, env, &reason, &result,
                    )?;
                    for l in &ls {
                        match l.deref() {
                            TypeInner::IntersectionT(r, rep) => {
                                // This is insufficient to deal with nested intersections.
                                // However, it's unlikely to cause issues, and we should instead
                                // focus our energy on killing `Object.assign` support instead.
                                let that_clone = that.clone();
                                let chain_use_op_clone = chain_use_op.clone();
                                let reason_clone = reason.dupe();
                                let kind_clone = kind.clone();
                                let inner_t_clone = inner_t.dupe();
                                let outer_reason_clone = reason.dupe();
                                let cases: Vec<_> = rep
                                    .members_iter()
                                    .map(|to_obj| {
                                        let to_obj = to_obj.clone();
                                        let that = that_clone.clone();
                                        let chain_use_op = chain_use_op_clone.clone();
                                        let reason = reason_clone.dupe();
                                        let kind = kind_clone.clone();
                                        let inner_t = inner_t_clone.dupe();
                                        let fix_cache = fix_cache.dupe();
                                        let outer_reason = outer_reason_clone.dupe();
                                        Box::new(move |cx: &mut Context<'cx>, env: &FlowJsEnv| {
                                            assign_from(
                                                cx,
                                                env,
                                                &outer_reason,
                                                &fix_cache,
                                                &that,
                                                &chain_use_op,
                                                &reason,
                                                &to_obj,
                                                &inner_t,
                                                &kind,
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
                                speculation_flow::try_custom(
                                    cx,
                                    env,
                                    Some(chain_use_op.clone()),
                                    None,
                                    None,
                                    r.loc().dupe(),
                                    cases,
                                )?;
                            }
                            _ => {
                                assign_from(
                                    cx,
                                    env,
                                    &reason,
                                    &fix_cache,
                                    &that,
                                    &chain_use_op,
                                    &reason,
                                    l,
                                    inner_t,
                                    &kind,
                                )?;
                            }
                        }
                    }
                    Ok(())
                },
            )?;
        }
        flow_js::reposition_with_env(cx, env, reason.loc().dupe(), result)
    }
}

pub fn perform_type_cast<'cx>(
    cx: &mut Context<'cx>,
    use_op: UseOp,
    l: &Type,
    cast_to_t: &Type,
) -> Result<(), FlowJsException> {
    perform_type_cast_with_env(cx, &FlowJsEnv::entry(), use_op, l, cast_to_t)
}

fn perform_type_cast_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: UseOp,
    l: &Type,
    cast_to_t: &Type,
) -> Result<(), FlowJsException> {
    let flow = |cx: &mut Context<'cx>, source_t: &Type| -> Result<(), FlowJsException> {
        FlowJs::flow_with_env(
            cx,
            env,
            source_t,
            &UseT::new(UseTInner::UseT(use_op.dupe(), cast_to_t.dupe())),
        )
    };
    let resolved_cast_to = FlowJs::singleton_concrete_type_for_inspection_with_env(
        cx,
        env,
        reason_of_t(cast_to_t),
        cast_to_t,
    )?;
    let should_short_circuit_empty_cast = match resolved_cast_to.deref() {
        TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::EmptyT) => {
            let resolved_l = FlowJs::singleton_concrete_type_for_inspection_with_env(
                cx,
                env,
                reason_of_t(l),
                l,
            )?;
            match resolved_l.deref() {
                TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::EnumValueT(_)) => {
                    false
                }
                TypeInner::UnionT(_, rep) => {
                    let mut all_object_like = true;
                    for member in rep.members_iter() {
                        let member = FlowJs::singleton_concrete_type_for_inspection_with_env(
                            cx,
                            env,
                            reason_of_t(member),
                            member,
                        )?;
                        if !flow_js_utils::object_like(&member) {
                            all_object_like = false;
                            break;
                        }
                    }
                    all_object_like
                }
                _ => true,
            }
        }
        _ => false,
    };
    if should_short_circuit_empty_cast {
        return flow(cx, l);
    }
    let concrete_l = FlowJs::singleton_concrete_type_for_type_cast(cx, env, reason_of_t(l), l)?;
    match concrete_l.deref() {
        TypeInner::DefT(reason, def_t) if let DefTInner::EnumValueT(enum_info) = def_t.deref() => {
            let representation_t = match EnumInfo::deref(enum_info) {
                EnumInfoInner::ConcreteEnum(concrete) => &concrete.representation_t,
                EnumInfoInner::AbstractEnum { representation_t } => representation_t,
            };
            if type_util::quick_subtype(None::<&mut fn(&Type)>, representation_t, cast_to_t)
                || type_util::quick_subtype(
                    None::<&mut fn(&Type)>,
                    representation_t,
                    &resolved_cast_to,
                )
            {
                flow(cx, representation_t)
            } else {
                flow(
                    cx,
                    &Type::new(TypeInner::DefT(
                        reason.dupe(),
                        DefT::new(DefTInner::EnumValueT(enum_info.dupe())),
                    )),
                )
            }
        }
        TypeInner::UnionT(_, rep) => {
            if matches!(resolved_cast_to.deref(), TypeInner::UnionT(_, _)) {
                flow(cx, &concrete_l)
            } else {
                let flowed_single = {
                    if !rep.is_optimized_finally() {
                        rep.optimize_enum_only(cx, |cx, ts| {
                            flow_typing_visitors::type_mapper::union_flatten(cx, ts.duped())
                        });
                    }
                    match rep.check_enum_with_tag() {
                        Some((enums, Some(_))) if enums.len() > 1 => {
                            let representative = rep.members_iter().next().unwrap().dupe();
                            let cast_succeeds = {
                                let resolved_repr =
                                    FlowJs::singleton_concrete_type_for_inspection_with_env(
                                        cx,
                                        env,
                                        reason_of_t(&representative),
                                        &representative,
                                    )?;
                                match resolved_repr.deref() {
                                    TypeInner::DefT(_, def_t)
                                        if let DefTInner::EnumValueT(enum_info) = def_t.deref() =>
                                    {
                                        let repr_t = match EnumInfo::deref(enum_info) {
                                            EnumInfoInner::ConcreteEnum(concrete) => {
                                                &concrete.representation_t
                                            }
                                            EnumInfoInner::AbstractEnum { representation_t } => {
                                                representation_t
                                            }
                                        };
                                        type_util::quick_subtype(
                                            None::<&mut fn(&Type)>,
                                            repr_t,
                                            cast_to_t,
                                        ) || type_util::quick_subtype(
                                            None::<&mut fn(&Type)>,
                                            repr_t,
                                            &resolved_cast_to,
                                        )
                                    }
                                    _ => FlowJs::speculative_subtyping_succeeds_with_env(
                                        cx,
                                        env,
                                        &representative,
                                        cast_to_t,
                                    )?,
                                }
                            };
                            if cast_succeeds {
                                false
                            } else {
                                let use_op = flow_js_utils::union_representative_use_op(
                                    cx,
                                    &concrete_l,
                                    &representative,
                                    use_op.dupe(),
                                );
                                perform_type_cast_with_env(
                                    cx,
                                    env,
                                    use_op,
                                    &representative,
                                    cast_to_t,
                                )?;
                                true
                            }
                        }
                        _ => false,
                    }
                };
                if !flowed_single {
                    for member in rep.members_iter() {
                        perform_type_cast_with_env(cx, env, use_op.dupe(), member, cast_to_t)?;
                    }
                }
                Ok(())
            }
        }
        // A value of `E & ...` is a value of `E`, so it casts as `E` when `E` can.
        TypeInner::IntersectionT(_, rep) => {
            for member in rep.members_iter() {
                let member = FlowJs::singleton_concrete_type_for_type_cast(
                    cx,
                    env,
                    reason_of_t(member),
                    member,
                )?;
                if let TypeInner::DefT(_, def_t) = member.deref()
                    && let DefTInner::EnumValueT(enum_info) = def_t.deref()
                {
                    let representation_t = match EnumInfo::deref(enum_info) {
                        EnumInfoInner::ConcreteEnum(concrete) => &concrete.representation_t,
                        EnumInfoInner::AbstractEnum { representation_t } => representation_t,
                    };
                    if type_util::quick_subtype(None::<&mut fn(&Type)>, representation_t, cast_to_t)
                        || type_util::quick_subtype(
                            None::<&mut fn(&Type)>,
                            representation_t,
                            &resolved_cast_to,
                        )
                    {
                        return flow(cx, representation_t);
                    }
                }
            }
            flow(cx, l)
        }

        _ => {
            let resolved = FlowJs::singleton_concrete_type_for_inspection_with_env(
                cx,
                env,
                reason_of_t(l),
                l,
            )?;
            match resolved.deref() {
                TypeInner::DefT(reason, def_t)
                    if let DefTInner::EnumValueT(enum_info) = def_t.deref() =>
                {
                    let representation_t = match EnumInfo::deref(enum_info) {
                        EnumInfoInner::ConcreteEnum(concrete) => &concrete.representation_t,
                        EnumInfoInner::AbstractEnum { representation_t } => representation_t,
                    };
                    if type_util::quick_subtype(None::<&mut fn(&Type)>, representation_t, cast_to_t)
                        || type_util::quick_subtype(
                            None::<&mut fn(&Type)>,
                            representation_t,
                            &resolved_cast_to,
                        )
                    {
                        flow(cx, representation_t)
                    } else {
                        flow(
                            cx,
                            &Type::new(TypeInner::DefT(
                                reason.dupe(),
                                DefT::new(DefTInner::EnumValueT(enum_info.dupe())),
                            )),
                        )
                    }
                }
                _ => flow(cx, l),
            }
        }
    }
}

pub mod type_assertions {
    use super::*;

    fn has_nontrivial_this_param<'cx>(cx: &mut Context<'cx>, t: &Type) -> Result<bool, JobError> {
        fn collect_this_params(t: &Type, out: &mut Vec<Type>) {
            match t.deref() {
                TypeInner::DefT(_, def_t) => match def_t.deref() {
                    DefTInner::FunT(_, ft) => out.push(ft.this_t.0.dupe()),
                    DefTInner::PolyT(box type_::PolyTData { t_out, .. }) => {
                        collect_this_params(t_out, out)
                    }
                    _ => {}
                },
                TypeInner::IntersectionT(_, rep) => {
                    for member in rep.members_iter() {
                        collect_this_params(member, out);
                    }
                }
                _ => {}
            }
        }

        let reason = reason_of_t(t);
        let concrete = match FlowJs::possible_concrete_types_for_inspection(cx, reason, t) {
            Ok(concrete) => concrete,
            Err(FlowJsException::WorkerCanceled(c)) => return Err(JobError::Canceled(c)),
            Err(FlowJsException::TimedOut(t)) => return Err(JobError::TimedOut(t)),
            Err(_) => return Ok(false),
        };

        let mut this_params = Vec::new();
        for t in &concrete {
            collect_this_params(t, &mut this_params);
        }
        if this_params.is_empty() {
            return Ok(false);
        }

        for this_t in this_params {
            let mixed = mixed_t::why(reason_of_t(&this_t).dupe());
            match speculation_flow::is_subtyping_successful(cx, mixed, this_t) {
                Ok(true) => return Ok(false),
                Ok(false) => {}
                Err(FlowJsException::WorkerCanceled(c)) => return Err(JobError::Canceled(c)),
                Err(FlowJsException::TimedOut(t)) => return Err(JobError::TimedOut(t)),
                Err(_) => return Ok(false),
            }
        }
        Ok(true)
    }

    pub fn check_function_proto_this_arg<'cx>(
        cx: &mut Context<'cx>,
        name: &FlowSmolStr,
        prop_loc: &ALoc,
        callee_object: &expression::Expression<ALoc, ALoc>,
        callee_object_t: &Type,
        arguments: &expression::ArgList<ALoc, ALoc>,
        expression_key: impl Fn(&expression::Expression<ALoc, ALoc>) -> Option<Key>,
    ) -> Result<(), JobError> {
        if !matches!(name.as_str(), "apply" | "bind" | "call")
            || !cx.is_fun_proto_method_lookup(prop_loc)
        {
            return Ok(());
        }

        let receiver = match callee_object.deref() {
            expression::ExpressionInner::Member { inner, .. } => Some(&inner.object),
            expression::ExpressionInner::OptionalMember { inner, .. } => Some(&inner.member.object),
            _ => None,
        };

        if let Some(receiver) = receiver {
            let this_arg_key = match arguments.arguments.first() {
                Some(expression::ExpressionOrSpread::Expression(arg)) => expression_key(arg),
                _ => None,
            };
            if let (Some(receiver_key), Some(this_arg_key)) =
                (expression_key(receiver), this_arg_key)
                && receiver_key == this_arg_key
            {
                return Ok(());
            }
        }

        if !has_nontrivial_this_param(cx, callee_object_t)? {
            return Ok(());
        }

        let (kind, receiver_expression) = match receiver {
            Some(receiver) => (
                InvalidThisArgKind::ReceiverMismatch,
                ExpressionReferenceData {
                    loc: receiver.loc().dupe(),
                    kind: flow_js_utils::expression_reference_kind_for_error(receiver),
                },
            ),
            None => (
                InvalidThisArgKind::MissingReceiver,
                ExpressionReferenceData {
                    loc: callee_object.loc().dupe(),
                    kind: flow_js_utils::expression_reference_kind_for_error(callee_object),
                },
            ),
        };
        flow_js_utils::add_output_non_speculating(
            cx,
            ErrorMessage::EInvalidThisArg(Box::new(EInvalidThisArgData {
                loc: prop_loc.dupe(),
                name: name.dupe(),
                receiver_expression,
                kind,
            })),
        );
        Ok(())
    }

    pub fn assert_binary_in_lhs<'cx>(cx: &mut Context<'cx>, t: &Type) -> Result<(), JobError> {
        let env = &FlowJsEnv::entry();
        flow_js_utils::flow_js_result_to_job_error(distribute_union_intersection::distribute(
            cx,
            env,
            None,
            &|cx, reason, t| {
                FlowJs::possible_concrete_types_for_operators_checking_with_env(cx, env, reason, t)
            },
            &|_, r| r.loc().dupe(),
            &|cx, env, t| {
                match t.deref() {
                    TypeInner::AnyT(_, _) => Ok(()),
                    // the left-hand side of a `(x in y)` expression is a string,
                    // number, or symbol
                    TypeInner::DefT(_, def_t)
                        if matches!(
                            def_t.deref(),
                            DefTInner::StrGeneralT(_)
                                | DefTInner::SingletonStrT { .. }
                                | DefTInner::NumGeneralT(_)
                                | DefTInner::SingletonNumT { .. }
                                | DefTInner::SymbolT
                                | DefTInner::UniqueSymbolT(_)
                        ) =>
                    {
                        Ok(())
                    }
                    _ => flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EBinaryInLHS(Box::new(EBinaryInLHSData {
                            loc: type_util::loc_of_t(t).dupe(),
                            lhs_loc: type_util::ref_loc_of_t(t).dupe(),
                            lhs_desc: flow_js_utils::type_or_type_desc_for_error(t),
                        })),
                    ),
                }
            },
            t,
        ))
    }

    pub fn assert_binary_in_rhs<'cx>(cx: &mut Context<'cx>, t: &Type) -> Result<(), JobError> {
        let env = &FlowJsEnv::entry();
        flow_js_utils::flow_js_result_to_job_error(distribute_union_intersection::distribute(
            cx,
            env,
            None,
            &|cx, reason, t| {
                FlowJs::possible_concrete_types_for_operators_checking_with_env(cx, env, reason, t)
            },
            &|_, r| r.loc().dupe(),
            &|cx, env, t| {
                match t.deref() {
                    TypeInner::AnyT(_, _) => Ok(()),
                    // the right-hand side of a `(x in y)` expression must be object-like
                    TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::ArrT(_)) => {
                        Ok(())
                    }
                    _ if flow_js_utils::object_like(t) => Ok(()),
                    _ => flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EBinaryInRHS(Box::new(EBinaryInRHSData {
                            loc: type_util::loc_of_t(t).dupe(),
                            rhs_loc: type_util::ref_loc_of_t(t).dupe(),
                            rhs_desc: flow_js_utils::type_or_type_desc_for_error(t),
                        })),
                    ),
                }
            },
            t,
        ))
    }

    pub fn assert_export_is_type<'cx>(
        cx: &mut Context<'cx>,
        name: &Name,
        t: &Type,
    ) -> Result<Type, JobError> {
        let env = &FlowJsEnv::entry();
        let t = flow_js_utils::flow_js_result_to_job_error(
            FlowJs::singleton_concretize_type_for_imports_exports_with_env(
                cx,
                env,
                reason_of_t(t),
                t,
            ),
        )?;
        flow_js_utils::flow_js_result_to_job_error(
            flow_js_utils::assert_export_is_type_t_kit::on_concrete_type(cx, name.dupe(), t),
        )
    }

    pub fn assert_for_in_rhs<'cx>(cx: &mut Context<'cx>, t: &Type) -> Result<(), JobError> {
        let env = &FlowJsEnv::entry();
        flow_js_utils::flow_js_result_to_job_error(distribute_union_intersection::distribute(
            cx,
            env,
            None,
            &|cx, reason, t| {
                FlowJs::possible_concrete_types_for_operators_checking_with_env(cx, env, reason, t)
            },
            &|_, r| r.loc().dupe(),
            &|cx, env, t| {
                match t.deref() {
                    _ if flow_js_utils::object_like(t) => Ok(()),
                    TypeInner::AnyT(_, _) => Ok(()),
                    TypeInner::ObjProtoT(_) => Ok(()),
                    // null/undefined are allowed
                    TypeInner::DefT(_, def_t)
                        if matches!(def_t.deref(), DefTInner::NullT | DefTInner::VoidT) =>
                    {
                        Ok(())
                    }
                    TypeInner::DefT(enum_reason, def_t)
                        if let DefTInner::EnumObjectT { enum_info, .. } = def_t.deref() =>
                    {
                        flow_js_utils::add_output_with_env(
                            cx,
                            env,
                            ErrorMessage::EEnumError(EnumErrorKind::EnumNotIterableForIn(
                                Box::new(EnumNotIterableForInData {
                                    loc: enum_reason.loc().dupe(),
                                    enum_: EnumReferenceData {
                                        loc: type_util::ref_loc_of_t(t).dupe(),
                                        name: enum_info.enum_name().map(Dupe::dupe),
                                    },
                                }),
                            )),
                        )
                    }
                    _ => flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EForInRHS(Box::new(EForInRHSData {
                            loc: type_util::loc_of_t(t).dupe(),
                            rhs_loc: type_util::ref_loc_of_t(t).dupe(),
                            rhs_desc: TypeOrTypeDescT::Type(t.dupe()),
                        })),
                    ),
                }
            },
            t,
        ))
    }

    pub fn assert_non_component_like_base<'cx>(
        cx: &mut Context<'cx>,
        def_loc: ALoc,
        use_: NamedReferenceData<ALoc>,
        t: &Type,
    ) -> Result<(), JobError> {
        let env = &FlowJsEnv::entry();
        fn check_base<'cx>(
            def_loc: &ALoc,
            use_: &NamedReferenceData<ALoc>,
            cx: &mut Context<'cx>,
            env: &FlowJsEnv,
            t: &Type,
        ) -> Result<(), FlowJsException> {
            match t.deref() {
                TypeInner::DefT(reason_type, def_t)
                    if matches!(def_t.deref(), DefTInner::MixedT(_)) =>
                {
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EReactIntrinsicOverlap(Box::new(
                            EReactIntrinsicOverlapData {
                                use_: use_.dupe(),
                                def: def_loc.dupe(),
                                type_: reason_type.loc().dupe(),
                                mixed: true,
                            },
                        )),
                    )
                }
                TypeInner::DefT(reason_type, def_t) => {
                    let matches = match def_t.deref() {
                        DefTInner::ObjT(obj) => obj.call_t.is_some(),
                        DefTInner::FunT { .. } => true,
                        DefTInner::ClassT(_) => true,
                        DefTInner::ReactAbstractComponentT(_) => true,
                        _ => false,
                    };
                    if matches {
                        flow_js_utils::add_output_with_env(
                            cx,
                            env,
                            ErrorMessage::EReactIntrinsicOverlap(Box::new(
                                EReactIntrinsicOverlapData {
                                    use_: use_.dupe(),
                                    def: def_loc.dupe(),
                                    type_: reason_type.loc().dupe(),
                                    mixed: false,
                                },
                            )),
                        )
                    } else {
                        Ok(())
                    }
                }
                _ => Ok(()),
            }
        }

        flow_js_utils::flow_js_result_to_job_error(distribute_union_intersection::distribute(
            cx,
            env,
            None,
            &|cx, reason, t| {
                FlowJs::possible_concrete_types_for_operators_checking_with_env(cx, env, reason, t)
            },
            &|_, _| use_.loc.dupe(),
            &|cx, env, t| check_base(&def_loc, &use_, cx, env, t),
            t,
        ))
    }

    pub fn assert_instanceof_rhs<'cx>(cx: &mut Context<'cx>, t: &Type) -> Result<(), JobError> {
        let env = &FlowJsEnv::entry();
        flow_js_utils::flow_js_result_to_job_error(distribute_union_intersection::distribute(
            cx,
            env,
            None,
            &|cx, reason, t| {
                FlowJs::possible_concrete_types_for_operators_checking_with_env(cx, env, reason, t)
            },
            &|_, r| r.loc().dupe(),
            &|cx, env, t| {
                match t.deref() {
                    // `instanceof` RHS
                    // right side of an `instanceof` binary expression must be an object
                    _ if flow_js_utils::object_like(t) => Ok(()),
                    // arrays are objects too, but not in `object_like`
                    TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::ArrT(_)) => {
                        Ok(())
                    }
                    TypeInner::AnyT(_, _) => Ok(()),
                    _ => flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        flow_js_utils::instanceof_rhs_error(t),
                    ),
                }
            },
            t,
        ))
    }

    pub(crate) fn assert_match_instance_pattern_constructor<'cx>(
        cx: &mut Context<'cx>,
        t: &Type,
    ) -> Result<(), JobError> {
        let env = &FlowJsEnv::entry();
        match crate::exhaustive::get_class_info(cx, t)? {
            Some(_) => Ok(()),
            None => flow_js_utils::flow_js_result_to_job_error(flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidInstancePattern(
                    reason_of_t(t).loc().dupe(),
                )),
            )),
        }
    }

    pub fn assert_iterable<'cx>(
        cx: &mut Context<'cx>,
        loc: ALoc,
        is_async: bool,
        use_op: &UseOp,
        t: &Type,
        targs_to_infer: &[Type],
    ) -> Result<(), JobError> {
        let env = &FlowJsEnv::entry();
        let ts = flow_js_utils::flow_js_result_to_job_error(
            FlowJs::possible_concrete_types_for_operators_checking_with_env(
                cx,
                env,
                reason_of_t(t),
                t,
            ),
        )?;
        for ti in &ts {
            match ti.deref() {
                TypeInner::DefT(enum_reason, def_t)
                    if let DefTInner::EnumObjectT { enum_info, .. } = def_t.deref() =>
                {
                    flow_js_utils::flow_js_result_to_job_error(
                        flow_js_utils::add_output_with_env(
                            cx,
                            env,
                            ErrorMessage::EEnumError(EnumErrorKind::EnumNotIterable(Box::new(
                                EnumNotIterableData {
                                    enum_: EnumReferenceData {
                                        loc: enum_reason.loc().dupe(),
                                        name: enum_info.enum_name().map(Dupe::dupe),
                                    },
                                },
                            ))),
                        ),
                    )?;
                    let any = type_::any_t::at(type_::AnySource::AnyError(None), loc.dupe());
                    for targ in targs_to_infer {
                        flow_js_utils::flow_js_result_to_job_error(flow_js::unify_with_env(
                            cx,
                            env,
                            Some(use_op.clone()),
                            &any,
                            targ,
                        ))?;
                    }
                }
                TypeInner::AnyT(reason, src) => {
                    let src = flow_js_utils::any_mod_src_keep_placeholder(
                        type_::AnySource::AnyError(None),
                        src,
                    );
                    for targ in targs_to_infer {
                        let any = type_::any_t::why(src.clone(), reason.dupe());
                        flow_js_utils::flow_js_result_to_job_error(flow_js::unify_with_env(
                            cx,
                            env,
                            Some(use_op.clone()),
                            &any,
                            targ,
                        ))?;
                    }
                }
                _ => {
                    let (reason, name) = if is_async {
                        let reason = flow_common::reason::mk_reason(None, loc.dupe());
                        let any = type_::any_t::why(type_::AnySource::Untyped, reason.dupe());
                        let async_iterable = FlowJs::get_builtin_typeapp_with_env(
                            cx,
                            env,
                            &reason,
                            None,
                            "AsyncIterable",
                            vec![any.dupe(), any.dupe(), any],
                        );
                        let name = if flow_js_utils::flow_js_result_to_job_error(
                            FlowJs::speculative_subtyping_succeeds_with_flow_errors(
                                cx,
                                env,
                                ti,
                                &async_iterable,
                            ),
                        )? {
                            "AsyncIterable"
                        } else {
                            "Iterable"
                        };
                        (reason, name)
                    } else {
                        let reason = flow_common::reason::mk_reason(None, loc.dupe());
                        (reason, "Iterable")
                    };
                    let iterable = FlowJs::get_builtin_typeapp_with_env(
                        cx,
                        env,
                        &reason,
                        None,
                        name,
                        targs_to_infer.to_vec(),
                    );
                    let use_t = type_::UseT::new(type_::UseTInner::UseT(use_op.clone(), iterable));
                    flow_js_utils::flow_js_result_to_job_error(flow_js::flow_with_env(
                        cx,
                        env,
                        (ti, &use_t),
                    ))?;
                }
            }
        }
        Ok(())
    }

    pub fn non_exhaustive<'cx>(cx: &mut Context<'cx>, ts: &[Type]) -> Result<bool, JobError> {
        let env = &FlowJsEnv::entry();
        for t in ts {
            let concrete_types = flow_js_utils::flow_js_result_to_job_error(
                FlowJs::possible_concrete_types_for_inspection_with_env(cx, env, reason_of_t(t), t),
            )?;
            if !concrete_types.is_empty() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn assert_operator_receiver_base<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        op_reason: &Reason,
        obj_expression: &ExpressionReferenceData<ALoc>,
        obj: &Type,
        prop: Option<Name>,
    ) -> Result<(), FlowJsException> {
        match (obj.deref(), &prop) {
            (TypeInner::AnyT(_, _), _) => Ok(()),
            (TypeInner::DefT(_, def_t), _)
                if matches!(
                    def_t.deref(),
                    DefTInner::EmptyT | DefTInner::NullT | DefTInner::VoidT
                ) =>
            {
                Ok(())
            }
            (TypeInner::DefT(_, def_t), _) if matches!(def_t.deref(), DefTInner::ArrT(arr) if matches!(arr.deref(), ArrType::ROArrayAT(box ROArrayATData { .. }) | ArrType::ArrayAT(box ArrayATData { .. }))) => {
                Ok(())
            }
            (TypeInner::DefT(_, def_t), Some(prop_name))
                if matches!(def_t.deref(), DefTInner::ObjT(_)) =>
            {
                if let DefTInner::ObjT(obj_t) = def_t.deref() {
                    if cx.has_prop(obj_t.props_tmap.dupe(), prop_name) {
                        return flow_js_utils::add_output_with_env(
                            cx,
                            env,
                            ErrorMessage::EIllegalAssertOperator(Box::new(
                                EIllegalAssertOperatorData {
                                    op_loc: op_reason.loc().dupe(),
                                    obj: obj_expression.dupe(),
                                    specialized: true,
                                },
                            )),
                        );
                    }
                }
                if let DefTInner::ObjT(obj_t) = def_t.deref() {
                    if matches!(obj_t.flags.obj_kind, ObjKind::Indexed(_)) {
                        return Ok(());
                    }
                }
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::EIllegalAssertOperator(Box::new(EIllegalAssertOperatorData {
                        op_loc: op_reason.loc().dupe(),
                        obj: obj_expression.dupe(),
                        specialized: true,
                    })),
                )
            }
            (TypeInner::DefT(_, def_t), _) if matches!(def_t.deref(), DefTInner::ObjT(obj_t) if matches!(obj_t.flags.obj_kind, ObjKind::Indexed(_))) => {
                Ok(())
            }
            _ => flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::EIllegalAssertOperator(Box::new(EIllegalAssertOperatorData {
                    op_loc: op_reason.loc().dupe(),
                    obj: obj_expression.dupe(),
                    specialized: true,
                })),
            ),
        }
    }

    pub fn check_specialized_assert_operator_property<'cx>(
        cx: &mut Context<'cx>,
        op_reason: &Reason,
        obj_expression: &ExpressionReferenceData<ALoc>,
        t: &Type,
        prop: &str,
    ) {
        let env = &FlowJsEnv::entry();
        distribute_union_intersection::distribute(
            cx,
            env,
            None,
            &|cx, reason, t| {
                FlowJs::possible_concrete_types_for_operators_checking_with_env(cx, env, reason, t)
            },
            &|_, r| r.loc().dupe(),
            &|cx, env, t| {
                assert_operator_receiver_base(
                    cx,
                    env,
                    op_reason,
                    obj_expression,
                    t,
                    Some(Name::new(prop)),
                )
            },
            t,
        )
        .unwrap()
    }

    pub fn check_specialized_assert_operator_lookup<'cx>(
        cx: &mut Context<'cx>,
        op_reason: &Reason,
        obj_expression: &ExpressionReferenceData<ALoc>,
        t1: &Type,
        t2: &Type,
    ) {
        let env = &FlowJsEnv::entry();
        fn check_base<'cx>(
            cx: &mut Context<'cx>,
            env: &FlowJsEnv,
            op_reason: &Reason,
            obj_expression: &ExpressionReferenceData<ALoc>,
            (obj, prop): (&Type, &Type),
        ) -> Result<(), FlowJsException> {
            match prop.deref() {
                TypeInner::DefT(_, def_t) => {
                    if let DefTInner::SingletonStrT { value, .. } = def_t.deref() {
                        return assert_operator_receiver_base(
                            cx,
                            env,
                            op_reason,
                            obj_expression,
                            obj,
                            Some(Name::new(value.dupe())),
                        );
                    }
                }
                _ => {}
            }
            assert_operator_receiver_base(cx, env, op_reason, obj_expression, obj, None)
        }

        distribute_union_intersection::distribute_2(
            cx,
            env,
            None,
            &|cx, reason, t| {
                FlowJs::possible_concrete_types_for_operators_checking_with_env(cx, env, reason, t)
            },
            &|cx, r1, r2| {
                flow_js_utils::ordered_reasons(cx, (r1.dupe(), r2.dupe()))
                    .0
                    .loc()
                    .dupe()
            },
            &|cx, env, (obj, prop)| check_base(cx, env, op_reason, obj_expression, (obj, prop)),
            (t1, t2),
        )
        .unwrap()
    }

    pub fn check_assert_operator_implicitly_nullable<'cx>(
        cx: &mut Context<'cx>,
        t: &Type,
    ) -> Result<bool, JobError> {
        let env = &FlowJsEnv::entry();
        fn valid_target(t: &Type) -> bool {
            match t.deref() {
                TypeInner::AnyT(_, _) => true,
                TypeInner::DefT(_, def_t) => match def_t.deref() {
                    DefTInner::ObjT(obj_t) => matches!(obj_t.flags.obj_kind, ObjKind::Indexed(_)),
                    DefTInner::ArrT(arr) => {
                        matches!(
                            arr.deref(),
                            ArrType::ArrayAT(box ArrayATData { .. }) | ArrType::ROArrayAT(box ROArrayATData { .. })
                        )
                    }
                    _ => false,
                },
                _ => false,
            }
        }

        let ts = flow_js_utils::flow_js_result_to_job_error(
            FlowJs::possible_concrete_types_for_operators_checking_with_env(
                cx,
                env,
                reason_of_t(t),
                t,
            ),
        )?;
        Ok(ts.iter().any(valid_target))
    }

    pub fn check_assert_operator_nullable<'cx>(
        cx: &mut Context<'cx>,
        t: &Type,
    ) -> Result<bool, JobError> {
        let env = &FlowJsEnv::entry();
        fn valid_target(t: &Type) -> bool {
            match t.deref() {
                TypeInner::AnyT(_, _) => true,
                TypeInner::DefT(_, def_t) => matches!(
                    def_t.deref(),
                    DefTInner::NullT
                        | DefTInner::VoidT
                        | DefTInner::MixedT(
                            MixedFlavor::MixedEverything
                                | MixedFlavor::MixedNonNull
                                | MixedFlavor::MixedNonVoid
                        )
                ),
                _ => false,
            }
        }

        let ts = flow_js_utils::flow_js_result_to_job_error(
            FlowJs::possible_concrete_types_for_operators_checking_with_env(
                cx,
                env,
                reason_of_t(t),
                t,
            ),
        )?;
        Ok(ts.iter().any(valid_target))
    }
}
