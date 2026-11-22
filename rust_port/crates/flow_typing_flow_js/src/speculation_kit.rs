/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_aloc::ALoc;
use flow_common::hint::HintKind;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_lazy::ContextFamily;
use flow_typing_context::Context;
use flow_typing_context::Cx;
use flow_typing_errors::error_message::EIncompatibleSpeculationData;
use flow_typing_errors::error_message::EUnionOptimizationData;
use flow_typing_errors::error_message::EUnionPartialOptimizationNonUniqueKeyData;
use flow_typing_errors::error_message::EUnionSpeculationFailedData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_flow_common::concrete_type_eq;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_common::flow_js_utils::SpeculativeError;
use flow_typing_flow_js_env::Branch as SpeculationBranch;
use flow_typing_flow_js_env::Case as SpeculationCase;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_flow_js_env::InformationForSynthesisLogging;
use flow_typing_type::type_::CallAction;
use flow_typing_type::type_::CallMData;
use flow_typing_type::type_::CallTData;
use flow_typing_type::type_::ChainMData;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::DepthTrace;
use flow_typing_type::type_::FuncallType;
use flow_typing_type::type_::HintEvalResult;
use flow_typing_type::type_::LazyHintT;
use flow_typing_type::type_::LookupAction;
use flow_typing_type::type_::LookupActionMatchPropData;
use flow_typing_type::type_::LookupTData;
use flow_typing_type::type_::MethodAction;
use flow_typing_type::type_::MethodTData;
use flow_typing_type::type_::ReactKitTData;
use flow_typing_type::type_::SpecState;
use flow_typing_type::type_::SpecializedCallee;
use flow_typing_type::type_::SpeculationHintSetData;
use flow_typing_type::type_::SpeculationHintState;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::UnifyCause;
use flow_typing_type::type_::UnionEnum;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::UseT;
use flow_typing_type::type_::UseTInner;
use flow_typing_type::type_::VirtualRootUseOp;
use flow_typing_type::type_::VirtualUseOp;
use flow_typing_type::type_::inter_rep;
use flow_typing_type::type_::nominal;
use flow_typing_type::type_::react;
use flow_typing_type::type_::union_rep;
use flow_typing_type::type_util;
use flow_typing_type::type_util::mod_use_op_of_use_t;
use flow_typing_visitors::type_mapper;
use vec1::Vec1;

use crate::default_resolve;
use crate::flow_js::FlowJs;

enum CasesSpec<'cx, 'a> {
    UnionCases {
        use_op: UseOp,
        reason_op: Reason,
        l: Type,
        union_rep: &'a union_rep::UnionRep,
        us: Vec<Type>,
        on_success: Option<Box<dyn FnOnce(&mut Context<'cx>, &FlowJsEnv) + 'a>>,
    },
    IntersectionCases {
        intersection: Type,
        ls: Vec<Type>,
        use_t: UseT<Cx>,
    },
    SingletonCase(Type, UseT<Cx>),
    SingletonUnifyCase(Type, UseOp, Type),
    SingletonCustomCase(
        Option<Box<dyn FnOnce(&mut Context<'cx>, &FlowJsEnv) -> Result<(), FlowJsException> + 'a>>,
    ),
    CustomCases {
        use_op: Option<UseOp>,
        no_match_error_loc: ALoc,
        use_t: Option<UseT<Cx>>,
        default_resolve: Option<
            Box<dyn FnOnce(&mut Context<'cx>, &FlowJsEnv) -> Result<(), FlowJsException> + 'a>,
        >,
        cases:
            Vec<Box<dyn FnOnce(&mut Context<'cx>, &FlowJsEnv) -> Result<(), FlowJsException> + 'a>>,
    },
}

fn mk_intersection_reason(r: &Reason, _ls: &[Type]) -> Reason {
    r.dupe().replace_desc(None)
}

fn log_synthesis_result(
    env: &FlowJsEnv,
    _trace: DepthTrace,
    case: &SpeculationCase,
    speculation_id: i32,
) {
    let information_for_synthesis_logging = &case.information_for_synthesis_logging;
    match information_for_synthesis_logging {
        InformationForSynthesisLogging::CallInformationForSynthesisLogging {
            lhs_t,
            call_callee_hint_ref,
        } => {
            let old_callee_hint = call_callee_hint_ref.borrow().clone();
            let new_callee_hint = match &old_callee_hint {
                SpeculationHintState::SpeculationHintUnset => {
                    let mut spec_id_path: Vec<_> = env.speculation_id_path().collect();
                    spec_id_path.push(speculation_id);
                    SpeculationHintState::SpeculationHintSet(Box::new(SpeculationHintSetData(
                        spec_id_path.into(),
                        lhs_t.dupe(),
                    )))
                }
                SpeculationHintState::SpeculationHintInvalid => {
                    SpeculationHintState::SpeculationHintInvalid
                }
                SpeculationHintState::SpeculationHintSet(box SpeculationHintSetData(
                    old_spec_id_path,
                    old_t,
                )) => {
                    if old_spec_id_path.contains(&speculation_id) {
                        // We are moving back a successful speculation path.
                        old_callee_hint.clone()
                    } else if lhs_t.ptr_eq(old_t) {
                        // We are in a different branch, but the outcome is the same, so keep it
                        old_callee_hint.clone()
                    } else {
                        SpeculationHintState::SpeculationHintInvalid
                    }
                }
            };
            *call_callee_hint_ref.borrow_mut() = new_callee_hint;
        }
        InformationForSynthesisLogging::NoInformationForSynthesisLogging => {}
    }
}

fn log_specialized_use<CX: ContextFamily>(
    use_t: &UseT<CX>,
    case: &SpeculationCase,
    speculation_id: i32,
) {
    let finalize = |c: &SpecializedCallee| {
        let spec_id = SpecState {
            speculation_id,
            case_id: case.case_id,
        };
        let candidates = c.speculative_candidates.borrow();
        if let Some((l, _)) = candidates
            .iter()
            .find(|(_, spec_id_prime)| spec_id == *spec_id_prime)
        {
            c.finalized.borrow_mut().push_front(l.dupe());
        }
    };
    match use_t.deref() {
        UseTInner::CallT(box CallTData {
            call_action:
                box CallAction::Funcalltype(box FuncallType {
                    call_specialized_callee: Some(c),
                    ..
                }),
            ..
        })
        | UseTInner::MethodT(box MethodTData {
            method_action:
                box MethodAction::CallM(box CallMData {
                    specialized_callee: Some(c),
                    ..
                })
                | box MethodAction::ChainM(box ChainMData {
                    specialized_callee: Some(c),
                    ..
                }),
            ..
        }) => finalize(c),
        UseTInner::ReactKitT(box ReactKitTData {
            tool:
                box react::Tool::CreateElement(box react::CreateElementData {
                    specialized_component,
                    specialized_props,
                    ..
                }),
            ..
        }) => specialized_component
            .iter()
            .chain(specialized_props)
            .for_each(finalize),
        _ => {}
    }
}

fn log_specialized_callee(spec: &CasesSpec<'_, '_>, case: &SpeculationCase, speculation_id: i32) {
    match spec {
        CasesSpec::IntersectionCases { use_t, .. } => {
            log_specialized_use(use_t, case, speculation_id);
        }
        CasesSpec::CustomCases {
            use_t: Some(use_t), ..
        } => {
            log_specialized_use(use_t, case, speculation_id);
        }
        _ => {}
    }
}

enum CaseSpec<'cx, 'a> {
    CustomCase(Box<dyn FnOnce(&mut Context<'cx>, &FlowJsEnv) -> Result<(), FlowJsException> + 'a>),
    FlowCase(Type, UseT<Cx>),
    UnifyCase(Type, UseOp, Type),
}

/// Internal error type for `speculative_matches` and its callees.
/// Singleton cases produce `Singleton` instead of calling `add_output`.
/// All other cases produce `Speculative` (from `add_output`).
/// `PropagatedSingleton` represents a `SpeculationSingletonError` from a nested
/// singleton speculation that is propagating through an outer speculation.
enum SpecMatchError {
    Speculative(SpeculativeError),
    Singleton(SpeculativeError),
    LimitExceeded,
    PropagatedSingleton,
    Cancel(flow_utils_concurrency::worker_cancel::WorkerCanceled),
    TimedOut(flow_utils_concurrency::job_error::CheckTimeout),
    DebugThrow { loc: ALoc },
}

impl From<SpeculativeError> for SpecMatchError {
    fn from(e: SpeculativeError) -> Self {
        SpecMatchError::Speculative(e)
    }
}

impl From<FlowJsException> for SpecMatchError {
    fn from(e: FlowJsException) -> Self {
        match e {
            FlowJsException::Speculative(e) => SpecMatchError::Speculative(e),
            FlowJsException::LimitExceeded => SpecMatchError::LimitExceeded,
            FlowJsException::SpeculationSingletonError => SpecMatchError::PropagatedSingleton,
            FlowJsException::WorkerCanceled(c) => SpecMatchError::Cancel(c),
            FlowJsException::TimedOut(t) => SpecMatchError::TimedOut(t),
            FlowJsException::DebugThrow { loc } => SpecMatchError::DebugThrow { loc },
        }
    }
}

impl SpecMatchError {
    fn expect_speculative(self) -> FlowJsException {
        match self {
            SpecMatchError::Speculative(e) => FlowJsException::Speculative(e),
            SpecMatchError::LimitExceeded => FlowJsException::LimitExceeded,
            SpecMatchError::PropagatedSingleton => FlowJsException::SpeculationSingletonError,
            SpecMatchError::Cancel(c) => FlowJsException::WorkerCanceled(c),
            SpecMatchError::TimedOut(t) => FlowJsException::TimedOut(t),
            SpecMatchError::DebugThrow { loc } => FlowJsException::DebugThrow { loc },
            SpecMatchError::Singleton(_) => {
                unreachable!("non-singleton spec produced singleton error")
            }
        }
    }

    fn into_flow_js_exception(self) -> FlowJsException {
        match self {
            SpecMatchError::Speculative(e) => FlowJsException::Speculative(e),
            SpecMatchError::LimitExceeded => FlowJsException::LimitExceeded,
            SpecMatchError::Cancel(c) => FlowJsException::WorkerCanceled(c),
            SpecMatchError::TimedOut(t) => FlowJsException::TimedOut(t),
            SpecMatchError::DebugThrow { loc } => FlowJsException::DebugThrow { loc },
            SpecMatchError::Singleton(_) | SpecMatchError::PropagatedSingleton => {
                FlowJsException::SpeculationSingletonError
            }
        }
    }
}

// Entry points into the process of trying different branches of union and
// intersection types.
//
// The problem we're trying to solve here is common to checking unions and
// intersections: how do we make a choice between alternatives, when we want
// to avoid regret (i.e., by not committing to an alternative that might not
// work out, when alternatives that were not considered could have worked out)?
//
// To appreciate the problem, consider what happens without choice. Partial
// information is not a problem: we emit constraints that must be satisfied for
// something to work, and either those constraints fail (indicating a problem)
// or they don't fail (indicating no problem). With choice we cannot naively
// emit constraints as we try alternatives *without also having a mechanism to
// roll back those constraints*. This is because those constraints don't *have*
// to be satisfied; some other alternative may end up not needing those
// constraints to be satisfied for things to work out!
//
// It is not too hard to imagine scary scenarios we can get into without a
// roll-back mechanism. (These scenarios are not theoretical, by the way: with a
// previous implementation of union and intersection types that didn't
// anticipate these scenarios, they consistently caused a lot of problems in
// real-world use cases.)
//
// * One bad state we can get into is where, when trying an alternative, we emit
//   constraints hoping they would be satisfied, and they appear to work. So we
//   commit to that particular alternative. Then much later find out that those
//   constraints are unsatified, at which point we have lost the ability to try
//   other alternatives that could have worked. This leads to a class of bugs
//   where a union or intersection type contains cases that should have worked,
//   but they don't.
//
//  * An even worse state we can get into is where we do discover that an
//    alternative won't work out while we're still in a position of choosing
//    another alternative, but in the process of making that discovery we emit
//    constraints that linger on in a ghost-like state. Meanwhile, we pick another
//    alternative, it works out, and we move on. Except that much later the ghost
//    constraints become unsatisfied, leading to much confusion on the source of
//    the resulting errors. This leads to a class of bugs where we get spurious
//    errors even when a union or intersection type seems to have worked.
//
// So, we just implement roll-back, right? Basically...yes. But rolling back
// constraints is really hard in the current implementation. Instead, we try to
// avoid processing constraints that have side effects as much as possible while
// trying alternatives: by ensuring that the constraints that have side effects
// get deferred, instead of being processed immediately, until a choice can be
// made, thereby not participating in the choice-making process.
//
// But not all types can be fully resolved. In particular, while union and
// intersection types themselves can be fully resolved, the lower and upper
// bounds we check them against could have still-to-be-inferred types in
// them. How do we ensure that for the potentially side-effectful constraints we
// do emit on these types, we avoid undue side effects? By explicitly marking
// these types as unresolved, and deferring the execution of constraints that
// involved such marked types until a choice can be made. The details of this
// process is described in Speculation.

// Every choice-making process on a union or intersection type is assigned a
// unique identifier, called the speculation_id. This identifier keeps track of
// unresolved tvars encountered when trying to fully resolve types.
pub fn try_union<'cx, 'a>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    on_success: Option<Box<dyn FnOnce(&mut Context<'cx>, &FlowJsEnv) + 'a>>,
    trace: DepthTrace,
    use_op: UseOp,
    l: Type,
    reason_op: Reason,
    rep: &union_rep::UnionRep,
) -> Result<(), FlowJsException> {
    let ts: Vec<Type> = rep.members_iter().duped().collect();
    speculative_matches(
        cx,
        env,
        trace,
        CasesSpec::UnionCases {
            use_op,
            reason_op,
            l,
            union_rep: rep,
            us: ts,
            on_success,
        },
    )
    .map_err(SpecMatchError::expect_speculative)
}

pub fn try_intersection<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_t: UseT<Cx>,
    intersection: Type,
    rep: &inter_rep::InterRep,
) -> Result<(), FlowJsException> {
    let ls: Vec<Type> = rep.members_iter().duped().collect();
    speculative_matches(
        cx,
        env,
        trace,
        CasesSpec::IntersectionCases {
            intersection,
            ls,
            use_t,
        },
    )
    .map_err(SpecMatchError::expect_speculative)
}

pub fn try_custom<'cx, 'a>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: Option<UseOp>,
    use_t: Option<UseT<Cx>>,
    default_resolve: Option<
        Box<dyn FnOnce(&mut Context<'cx>, &FlowJsEnv) -> Result<(), FlowJsException> + 'a>,
    >,
    no_match_error_loc: ALoc,
    cases: Vec<Box<dyn FnOnce(&mut Context<'cx>, &FlowJsEnv) -> Result<(), FlowJsException> + 'a>>,
) -> Result<(), FlowJsException> {
    speculative_matches(
        cx,
        env,
        DepthTrace::dummy_trace(),
        CasesSpec::CustomCases {
            use_op,
            no_match_error_loc,
            use_t,
            default_resolve,
            cases,
        },
    )
    .map_err(SpecMatchError::expect_speculative)
}

/// `try_singleton_throw_on_failure` runs the constraint
/// between (t, u) in a speculative environment. If an error is raised then a
/// SpeculationSingletonError exception is raised. This needs to be caught by
/// the caller of this function.
pub fn try_singleton_throw_on_failure<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    t: Type,
    u: UseT<Cx>,
) -> Result<(), FlowJsException> {
    speculative_matches(cx, env, trace, CasesSpec::SingletonCase(t, u))
        .map_err(SpecMatchError::into_flow_js_exception)
}

pub fn try_singleton_custom_throw_on_failure<'cx, 'a>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    f: Box<dyn FnOnce(&mut Context<'cx>, &FlowJsEnv) -> Result<(), FlowJsException> + 'a>,
) -> Result<(), FlowJsException> {
    speculative_matches(
        cx,
        env,
        DepthTrace::dummy_trace(),
        CasesSpec::SingletonCustomCase(Some(f)),
    )
    .map_err(SpecMatchError::into_flow_js_exception)
}

/// Runs a custom singleton speculation, committing its constraints on success
/// and returning the first error after rolling its constraints back on failure.
pub fn try_singleton_custom_with_error<'cx, 'a>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    f: Box<dyn FnOnce(&mut Context<'cx>, &FlowJsEnv) -> Result<(), FlowJsException> + 'a>,
) -> Result<Option<SpeculativeError>, FlowJsException> {
    match speculative_matches(
        cx,
        env,
        DepthTrace::dummy_trace(),
        CasesSpec::SingletonCustomCase(Some(f)),
    ) {
        Ok(()) => Ok(None),
        Err(SpecMatchError::Singleton(error)) => Ok(Some(error)),
        Err(other) => Err(other.into_flow_js_exception()),
    }
}

pub fn try_unify<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    t1: Type,
    use_op: UseOp,
    t2: Type,
) -> Result<(), FlowJsException> {
    speculative_matches(
        cx,
        env,
        trace,
        CasesSpec::SingletonUnifyCase(t1, use_op, t2),
    )
    .map_err(SpecMatchError::into_flow_js_exception)
}

// ************************
// * Speculative matching *
// ************************

// Speculatively match a pair of types, returning whether some error was
// encountered or not. Speculative matching happens in the context of a
// particular "branch": this context controls how some constraints emitted
// during the matching might be processed. See comments in Speculation for
// details on branches. See also speculative_matches, which calls this function
// iteratively and processes its results.
fn speculative_match(
    env: &FlowJsEnv,
    branch: SpeculationBranch,
    f: impl FnOnce(&FlowJsEnv) -> Result<(), FlowJsException>,
) -> Result<Option<SpeculativeError>, FlowJsException> {
    let r = f(&env.with_branch(branch));
    match r {
        Ok(()) => Ok(None),
        Err(FlowJsException::Speculative(e)) => Ok(Some(e)),
        Err(other) => Err(other), // LimitExceeded propagates through
    }
}

// Speculatively match several alternatives in turn, as presented when checking
// a union or intersection type. This process can terminate in various ways:
//
// (1) One of the alternatives definitely succeeds. This is straightforward: we
// can safely discard any later alternatives.
//
// (2) All alternatives fail. This is also straightforward: we emit an
// appropriate error message.
//
// See Speculation for more details on terminology and low-level mechanisms used
// here, including what bits of information are carried by case.
//
// Because this process is common to checking union and intersection types, we
// abstract the latter into a so-called "spec." The spec is used to customize
// error messages.
fn speculative_matches<'cx, 'a>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    mut spec: CasesSpec<'cx, 'a>,
) -> Result<(), SpecMatchError>
where
    'cx: 'a,
{
    // explore optimization opportunities
    if optimize_spec_try_shortcut(cx, env, trace, &mut spec)? {
        Ok(())
    } else {
        long_path_speculative_matches(cx, env, trace, &mut spec)
    }
}

fn long_path_speculative_matches<'cx, 'a>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    spec: &mut CasesSpec<'cx, 'a>,
) -> Result<(), SpecMatchError>
where
    'cx: 'a,
{
    let speculation_id = flow_common::reason::mk_id() as i32;
    // extract stuff to ignore while considering actions
    // split spec into a list of pairs of types to try speculative matching on
    let trials = trials_of_spec(spec);
    // Save the constraint cache once before trying any branches. The constraint
    // cache records (lower, upper) pairs that have already been fully checked.
    // When a branch fails, we roll back the cache to discard entries that may
    // have been valid only under speculative assumptions. When a branch succeeds,
    // we keep the accumulated cache entries so that subsequent type-checking does
    // not redundantly re-check the same flow pairs. The cache is only restored
    // to this snapshot if ALL branches fail.
    let cc_base_level = cx.constraint_cache().level_count();
    cx.constraint_cache_mut().push_level();

    fn return_errs<'cx, 'a>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        spec: &mut CasesSpec<'cx, 'a>,
        msgs: Vec<ErrorMessage<ALoc>>,
    ) -> Result<(), SpecMatchError> {
        // make a really detailed error message listing out the
        // error found for each alternative
        // Add the error
        match spec {
            CasesSpec::UnionCases {
                use_op,
                reason_op: _,
                l,
                us,
                ..
            } => {
                let loc = type_util::reason_of_t(l).loc().dupe();
                assert!(us.len() == msgs.len());
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::EUnionSpeculationFailed(Box::new(EUnionSpeculationFailedData {
                        use_op: use_op.dupe(),
                        loc,
                        branches: msgs,
                    })),
                )?;
                Ok(())
            }
            CasesSpec::SingletonCase(..)
            | CasesSpec::SingletonCustomCase(..)
            | CasesSpec::SingletonUnifyCase(..) => {
                let n = msgs.len();
                let mut msgs = msgs.into_iter();
                match (msgs.next(), msgs.next()) {
                    (Some(msg), None) => {
                        Err(SpecMatchError::Singleton(SpeculativeError(Box::new(msg))))
                    }
                    _ => panic!(
                        "SingletonCase should not have exactly one error, but we got {}",
                        n
                    ),
                }
            }
            CasesSpec::CustomCases {
                use_op,
                no_match_error_loc,
                use_t: _,
                default_resolve,
                cases: _,
            } => {
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::EIncompatibleSpeculation(Box::new(
                        EIncompatibleSpeculationData {
                            use_op: use_op.dupe(),
                            loc: no_match_error_loc.dupe(),
                            branches: msgs,
                        },
                    )),
                )?;
                if let Some(f) = default_resolve.take() {
                    f(cx, env)?;
                }
                Ok(())
            }
            CasesSpec::IntersectionCases {
                intersection,
                ls,
                use_t: upper,
            } => {
                let intersection_reason = type_util::reason_of_t(intersection);
                let reason_lower = mk_intersection_reason(intersection_reason, ls);
                let flow_fn = |cx: &mut Context<'cx>, t: &Type, u: &UseT<Cx>| {
                    FlowJs::flow_with_env(cx, env, t, u)
                };
                let resolve_callee_pair = (intersection_reason.dupe(), ls.clone());
                default_resolve::default_resolve_touts(
                    cx,
                    &flow_fn,
                    Some(&resolve_callee_pair),
                    env,
                    reason_lower.loc().dupe(),
                    upper,
                )?;
                assert!(ls.len() == msgs.len());
                let err = match &**upper {
                    UseTInner::UseT(use_op, t) => {
                        flow_js_utils::incompatible_types_error_with_branches(
                            intersection,
                            t,
                            use_op.dupe(),
                            None,
                            msgs,
                        )
                    }
                    UseTInner::LookupT(box LookupTData {
                        reason,
                        lookup_action:
                            box LookupAction::MatchProp(box LookupActionMatchPropData {
                                use_op, ..
                            }),
                        ..
                    }) => ErrorMessage::EUnionSpeculationFailed(Box::new(
                        EUnionSpeculationFailedData {
                            use_op: use_op.dupe(),
                            loc: reason.loc().dupe(),
                            branches: msgs,
                        },
                    )),
                    _ => {
                        let use_op = type_util::use_op_of_use_t(upper);
                        let loc = flow_js_utils::error_message_loc_of_upper(upper);
                        ErrorMessage::EIncompatibleSpeculation(Box::new(
                            EIncompatibleSpeculationData {
                                use_op,
                                loc,
                                branches: msgs,
                            },
                        ))
                    }
                };
                flow_js_utils::add_output_with_env(cx, env, err)?;
                Ok(())
            }
        }
    }

    // Here errs records all errors we have seen up to this point.
    let mut errs = Vec::new();

    for (case_id, case_spec) in trials {
        let information_for_synthesis_logging = match &case_spec {
            CaseSpec::FlowCase(lhs_t, use_t)
                if let UseTInner::CallT(box CallTData {
                    call_action: box CallAction::Funcalltype(box funcalltype),
                    ..
                }) = use_t.deref() =>
            {
                match &funcalltype.call_speculation_hint_state {
                    Some(call_callee_hint_ref) => {
                        InformationForSynthesisLogging::CallInformationForSynthesisLogging {
                            lhs_t: lhs_t.dupe(),
                            call_callee_hint_ref: call_callee_hint_ref.clone(),
                        }
                    }
                    None => InformationForSynthesisLogging::NoInformationForSynthesisLogging,
                }
            }
            CaseSpec::FlowCase(..) | CaseSpec::UnifyCase(..) | CaseSpec::CustomCase(..) => {
                InformationForSynthesisLogging::NoInformationForSynthesisLogging
            }
        };

        let case = SpeculationCase {
            case_id,
            errors: Rc::new(RefCell::new(Vec::new())),
            information_for_synthesis_logging,
        };

        // Save the constraint cache before this branch so we can roll back
        // on failure. Entries added during a failed speculative branch may
        // only be valid under the speculative assumption of that branch.
        cx.constraint_cache_mut().push_level();

        // speculatively match the pair of types in this trial
        let error = speculative_match(
            env,
            SpeculationBranch {
                speculation_id,
                case: case.clone(),
            },
            |env| match case_spec {
                CaseSpec::FlowCase(l, u) => {
                    FlowJs::rec_flow_with_env(cx, env, trace, &l, &u)?;
                    Ok(())
                }
                CaseSpec::UnifyCase(t1, use_op, t2) => {
                    FlowJs::rec_unify_with_env(
                        cx,
                        env,
                        trace,
                        use_op,
                        UnifyCause::Uncategorized,
                        None,
                        &t1,
                        &t2,
                    )?;
                    Ok(())
                }
                CaseSpec::CustomCase(f) => f(cx, env),
            },
        )?;

        match error {
            // no error, looking great so far...
            // Keep the constraint cache entries from this successful branch.
            None => {
                // Move entries from both pushed levels back below the
                // truncation line so they survive any outer truncate_to.
                {
                    let cc = cx.constraint_cache_mut();
                    cc.move_top_entries_to_level(cc_base_level - 1);
                    cc.pop_level();
                    cc.move_top_entries_to_level(cc_base_level - 1);
                    cc.pop_level();
                }
                fire_actions(cx, env, trace, spec, case, speculation_id)?;
                return Ok(());
            }
            // if an error is found, then throw away this alternative...
            Some(err) => {
                // Roll back the constraint cache to before this failed branch,
                // since entries added during speculation may depend on the
                // speculative assumption that was just disproven. That includes
                // the levels the branch left pushed, such as the preservation
                // level of an implicit instantiation.
                cx.constraint_cache_mut().truncate_to(cc_base_level + 1);
                // ...adding to the error list if no promising alternative has been
                // found yet
                errs.push(*err.0);
            }
        }
    }

    // everything failed; restore constraint cache to pre-speculation state
    cx.constraint_cache_mut().truncate_to(cc_base_level);
    // An expected-type return hint only chooses among the overloads that the arguments of a call
    // allow. When it rules out all of them, the call is resolved again with the hint as a best
    // effort, which guides inference without rejecting an overload, as in TypeScript.
    if !env.speculating()
        && let CasesSpec::IntersectionCases {
            intersection,
            ls,
            use_t,
        } = spec
        && let UseTInner::CallT(call) = &**use_t
        && let HintEvalResult::HintAvailable(_, HintKind::ExpectedTypeHint) =
            (call.return_hint.1)(cx, false, true, None, call.reason.dupe())
                .map_err(FlowJsException::from)?
    {
        let hint = call.return_hint.1.dupe();
        let return_hint = LazyHintT(
            call.return_hint.0,
            Rc::new(
                move |cx: &mut Context<'_>,
                      expected_only,
                      implicit_instantiation,
                      skip_optional,
                      reason| {
                    Ok(
                        match hint(
                            cx,
                            expected_only,
                            implicit_instantiation,
                            skip_optional,
                            reason,
                        )? {
                            HintEvalResult::HintAvailable(t, _) => {
                                HintEvalResult::HintAvailable(t, HintKind::BestEffortHint)
                            }
                            result => result,
                        },
                    )
                },
            ),
        );
        let use_t = UseT::new(UseTInner::CallT(Box::new(CallTData {
            return_hint,
            ..(**call).clone()
        })));
        return speculative_matches(
            cx,
            env,
            trace,
            CasesSpec::IntersectionCases {
                intersection: intersection.dupe(),
                ls: ls.clone(),
                use_t,
            },
        );
    }
    errs.reverse();
    return_errs(cx, env, spec, errs)
}

// Returns a lazy iterator instead of materializing all cases up front so that
// when speculation succeeds early the unused tail cases are never constructed.
// For our hot `keyof T`/`T[K]` workload the LHS is usually a singleton string
// and the first matching RHS member wins, so ~3 of every 5 per-case
// `UseT::new` allocations + drops are skipped.
//
// To avoid borrowing `*spec` for the iterator's lifetime (which would conflict
// with the surrounding `fire_actions(cx, env, trace, spec, ...)` call inside the
// consuming for-loop), we pull every value the iterator needs out of `spec`
// via `dupe`/`clone` for read-only fields and `mem::take`/`Option::take` for
// the variants that mutate (`SingletonCustomCase`, `CustomCases`).
fn trials_of_spec<'cx, 'a>(
    spec: &mut CasesSpec<'cx, 'a>,
) -> Box<dyn Iterator<Item = (i32, CaseSpec<'cx, 'a>)> + 'a>
where
    'cx: 'a,
{
    match spec {
        // NB: Even though we know the use_op for the original constraint, don't
        // embed it in the nested constraints to avoid unnecessary verbosity. We
        // will unwrap the original use_op once in EUnionSpeculationFailed.
        CasesSpec::UnionCases { use_op, l, us, .. } => {
            let spec_use_op = VirtualUseOp::Op(Arc::new(VirtualRootUseOp::Speculation(Arc::new(
                use_op.dupe(),
            ))));
            let l = l.dupe();
            let us = us.clone();
            Box::new(us.into_iter().enumerate().map(move |(i, u)| {
                (
                    i as i32,
                    CaseSpec::FlowCase(l.dupe(), UseT::new(UseTInner::UseT(spec_use_op.dupe(), u))),
                )
            }))
        }
        CasesSpec::IntersectionCases { ls, use_t: u, .. } => {
            let u = u.clone();
            let ls = ls.clone();
            Box::new(ls.into_iter().enumerate().map(move |(i, l)| {
                let u_mod = mod_use_op_of_use_t(
                    |use_op| {
                        VirtualUseOp::Op(Arc::new(VirtualRootUseOp::Speculation(Arc::new(
                            use_op.dupe(),
                        ))))
                    },
                    &u,
                );
                (i as i32, CaseSpec::FlowCase(l, u_mod))
            }))
        }
        CasesSpec::SingletonCase(l, u) => {
            let u_mod = mod_use_op_of_use_t(
                |use_op| {
                    VirtualUseOp::Op(Arc::new(VirtualRootUseOp::Speculation(Arc::new(
                        use_op.dupe(),
                    ))))
                },
                u,
            );
            Box::new(std::iter::once((0, CaseSpec::FlowCase(l.dupe(), u_mod))))
        }
        CasesSpec::SingletonCustomCase(f) => Box::new(std::iter::once((
            0,
            CaseSpec::CustomCase(f.take().unwrap()),
        ))),
        CasesSpec::SingletonUnifyCase(t1, use_op, t2) => {
            let spec_use_op = VirtualUseOp::Op(Arc::new(VirtualRootUseOp::Speculation(Arc::new(
                use_op.dupe(),
            ))));
            Box::new(std::iter::once((
                0,
                CaseSpec::UnifyCase(t1.dupe(), spec_use_op, t2.dupe()),
            )))
        }
        CasesSpec::CustomCases { cases, .. } => Box::new(
            std::mem::take(cases)
                .into_iter()
                .enumerate()
                .map(|(i, f)| (i as i32, CaseSpec::CustomCase(f))),
        ),
    }
}

// spec optimization
//
// Currently, the only optimizations we do are for enums and for disjoint unions.
//
// When a literal type is checked against a union of literal types, we hope the union is an enum and
// try to optimize the representation of the union as such. We also try to use our optimization to
// do a quick membership check, potentially avoiding the speculative matching process altogether.
//
// When an object type is checked against an union of object types, we hope the union is a disjoint
// union and try to guess and record sentinel properties across object types in the union. Later,
// during speculative matching, by checking sentinel properties first we force immediate match
// failures in the vast majority of cases without having to do any useless additional work.
fn optimize_spec_try_shortcut<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    spec: &mut CasesSpec<'cx, '_>,
) -> Result<bool, FlowJsException> {
    match spec {
        CasesSpec::UnionCases {
            use_op: _,
            reason_op: _,
            l,
            union_rep: rep,
            us,
            on_success,
        } if matches!(
            &**l,
            TypeInner::NominalT {
                nominal_type,
                ..
            } if nominal_type.nominal_id == nominal::Id::InternalEnforceUnionOptimized
        ) =>
        {
            let reason = match &**l {
                TypeInner::NominalT { reason, .. } => reason,
                _ => unreachable!(),
            };
            let specialization = rep.optimize_(
                cx,
                |cx, t1, t2| concrete_type_eq::eq_with_env(cx, env, t1, t2),
                |cx, ts: &mut dyn Iterator<Item = &Type>| {
                    type_mapper::union_flatten(cx, ts.duped())
                },
                |cx, t| cx.find_resolved(t),
                |cx, id| cx.find_props(id),
            );
            match specialization {
                Err(kind) => {
                    flow_js_utils::add_output_with_env(
                        cx,env,
                        ErrorMessage::EUnionOptimization(Box::new(EUnionOptimizationData {
                            loc: reason.loc().dupe(),
                            kind,
                        })),
                    )?;
                }
                Ok(
                    union_rep::FinallyOptimizedRep::AlmostDisjointUnionWithPossiblyNonUniqueKeys(
                        ref map,
                    )
                    | union_rep::FinallyOptimizedRep::PartiallyOptimizedAlmostDisjointUnionWithPossiblyNonUniqueKeys(
                        ref map,
                    ),
                ) => {
                    let non_unique_keys: BTreeMap<Name, BTreeMap<UnionEnum, Vec1<_>>> = map
                        .iter()
                        .filter_map(|(name, inner_map)| {
                            let filtered = inner_map
                                .iter()
                                .filter(|(_, nel)| nel.len() > 1)
                                .map(|(k, nel)| {
                                    (
                                        k.clone(),
                                        nel.mapped_ref(|t| {
                                            flow_js_utils::type_reference_with_reason_for_error(
                                                t,
                                                type_util::reason_of_t(t).dupe(),
                                            )
                                        }),
                                    )
                                })
                                .collect::<BTreeMap<_, _>>();
                            if filtered.is_empty() {
                                None
                            } else {
                                Some((name.dupe(), filtered))
                            }
                        })
                        .collect();
                    if !non_unique_keys.is_empty() {
                        flow_js_utils::add_output_with_env(
                            cx,env,
                            ErrorMessage::EUnionPartialOptimizationNonUniqueKey(Box::new(EUnionPartialOptimizationNonUniqueKeyData {
                                loc: reason.loc().dupe(),
                                non_unique_keys,
                            })),
                        )?;
                    }
                }
                Ok(_) => {}
            }
            if let Some(f) = on_success.take() {
                f(cx, env);
            }
            Ok(true)
        }
        CasesSpec::UnionCases {
            use_op,
            reason_op,
            l,
            union_rep: rep,
            us,
            on_success,
        } => {
            if !rep.is_optimized_finally() {
                rep.optimize(
                    cx,
                    |cx, t1, t2| concrete_type_eq::eq_with_env(cx, env, t1, t2),
                    |cx, ts: &mut dyn Iterator<Item = &Type>| {
                        type_mapper::union_flatten(cx, ts.duped())
                    },
                    |cx, t| cx.find_resolved(t),
                    |cx, id| cx.find_props(id),
                );
            }
            let result = match &**l {
                TypeInner::DefT(_, def_t)
                    if matches!(
                        def_t.deref(),
                        DefTInner::SingletonStrT { .. }
                            | DefTInner::SingletonNumT { .. }
                            | DefTInner::SingletonBoolT { .. }
                            | DefTInner::SingletonBigIntT { .. }
                            | DefTInner::VoidT
                            | DefTInner::NullT
                    ) =>
                {
                    shortcut_enum(cx, env, trace, reason_op, use_op, l, rep)?
                }
                //  Types that are definitely incompatible with enums, after the above case
                TypeInner::DefT(_, def_t)
                    if matches!(
                        def_t.deref(),
                        DefTInner::NumGeneralT(..)
                            | DefTInner::BigIntGeneralT(..)
                            | DefTInner::StrGeneralT(..)
                            | DefTInner::MixedT(..)
                            | DefTInner::SymbolT
                            | DefTInner::UniqueSymbolT(_)
                            | DefTInner::FunT(..)
                            | DefTInner::ObjT(..)
                            | DefTInner::ArrT(..)
                            | DefTInner::ClassT(..)
                            | DefTInner::InstanceT(..)
                            | DefTInner::TypeT(..)
                            | DefTInner::PolyT(_)
                            | DefTInner::ReactAbstractComponentT(_)
                            | DefTInner::EnumValueT(..)
                            | DefTInner::EnumObjectT { .. }
                    ) && rep.check_enum().is_some() =>
                {
                    let upper = type_util::union_of_ts(reason_op.dupe(), us.clone(), None);
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        flow_js_utils::incompatible_types_error(l, &upper, use_op.dupe(), None),
                    )?;
                    true
                }
                TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::ObjT(..)) => {
                    shortcut_disjoint_union(cx, env, trace, reason_op, use_op, l, rep)?
                }
                _ => false,
            };
            if result {
                if let Some(f) = on_success.take() {
                    f(cx, env);
                }
            }
            Ok(result)
        }
        CasesSpec::IntersectionCases { .. }
        | CasesSpec::CustomCases { .. }
        | CasesSpec::SingletonCase(..)
        | CasesSpec::SingletonCustomCase(..)
        | CasesSpec::SingletonUnifyCase(..) => Ok(false),
    }
}

fn shortcut_enum<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    reason_op: &Reason,
    use_op: &UseOp,
    l: &Type,
    rep: &union_rep::UnionRep,
) -> Result<bool, FlowJsException> {
    let result = union_rep::quick_mem_enum(
        cx,
        |_, t1, t2| type_util::quick_subtype(None::<&mut fn(&Type)>, t1, t2),
        l,
        rep,
    );
    quick_mem_result(cx, env, trace, reason_op, use_op, l, result)
}

fn shortcut_disjoint_union<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    reason_op: &Reason,
    use_op: &UseOp,
    l: &Type,
    rep: &union_rep::UnionRep,
) -> Result<bool, FlowJsException> {
    let result = union_rep::quick_mem_disjoint_union(
        cx,
        |cx, t| cx.find_resolved(t),
        |cx, id| cx.find_props(id),
        |_, t1, t2| type_util::quick_subtype(None::<&mut fn(&Type)>, t1, t2),
        l,
        rep,
    );
    quick_mem_result(cx, env, trace, reason_op, use_op, l, result)
}

fn quick_mem_result<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    reason_op: &Reason,
    use_op: &UseOp,
    l: &Type,
    result: union_rep::QuickMemResult,
) -> Result<bool, FlowJsException> {
    match result {
        // membership check succeeded
        union_rep::QuickMemResult::Yes => Ok(true),
        //  membership check failed
        union_rep::QuickMemResult::No => {
            FlowJs::rec_flow_with_env(
                cx,
                env,
                trace,
                l,
                &UseT::new(UseTInner::UseT(
                    use_op.dupe(),
                    Type::new(TypeInner::DefT(
                        reason_op.dupe(),
                        DefT::new(DefTInner::EmptyT),
                    )),
                )),
            )?;
            Ok(true)
        }
        // Our work here is done, so no need to continue.
        union_rep::QuickMemResult::Conditional(t) => {
            FlowJs::rec_flow_with_env(
                cx,
                env,
                trace,
                l,
                &UseT::new(UseTInner::UseT(use_op.dupe(), t)),
            )?;
            Ok(true)
        }
        // membership check was inconclusive
        union_rep::QuickMemResult::Unknown => Ok(false),
    }
}

fn fire_actions<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    spec: &CasesSpec<'cx, '_>,
    case: SpeculationCase,
    speculation_id: i32,
) -> Result<(), FlowJsException> {
    log_synthesis_result(env, trace, &case, speculation_id);
    log_specialized_callee(spec, &case, speculation_id);
    for err in case.errors.borrow().iter() {
        flow_js_utils::add_output_with_env(cx, env, err.clone())?;
    }
    Ok(())
}
