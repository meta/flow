/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Type hint evaluation
//!
//! Ported from type_hint.rs

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::ops::Deref;
use std::rc::Rc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::hint::FunCallImplicitInstantiationHints;
use flow_common::hint::Hint;
use flow_common::hint::HintDecomposition;
use flow_common::hint::HintDecompositionInner;
use flow_common::hint::HintKind;
use flow_common::hint::JsxImplicitInstantiationHints;
use flow_common::hint::PredicateKind;
use flow_common::hint::SentinelRefinement;
use flow_common::options::JsxMode;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::subst_name::SubstName;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_env_builder::env_api::DefLocType;
use flow_lazy::Lazy;
use flow_typing_context::Context;
use flow_typing_context::Cx;
use flow_typing_errors::error_message::EUnionSpeculationFailedData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::flow_error::ErrorSet;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_common::flow_js_utils::SpeculativeError;
use flow_typing_flow_js::flow_js;
use flow_typing_flow_js::flow_js::FlowJs;
use flow_typing_flow_js::implicit_instantiation;
use flow_typing_flow_js::tvar_resolver;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_implicit_instantiation_check::ImplicitInstantiationCheck;
use flow_typing_type::type_::AnySource;
use flow_typing_type::type_::ArrRestTData;
use flow_typing_type::type_::ArrType;
use flow_typing_type::type_::ArrayATData;
use flow_typing_type::type_::BigIntLiteral;
use flow_typing_type::type_::BinaryTest;
use flow_typing_type::type_::CallAction;
use flow_typing_type::type_::CallArg;
use flow_typing_type::type_::CallArgInner;
use flow_typing_type::type_::CallTData;
use flow_typing_type::type_::ConcretizationKind;
use flow_typing_type::type_::ConcretizeTData;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::FunParam;
use flow_typing_type::type_::FunRestParam;
use flow_typing_type::type_::FunType;
use flow_typing_type::type_::FuncallType;
use flow_typing_type::type_::GetElemTData;
use flow_typing_type::type_::GetPrivatePropTData;
use flow_typing_type::type_::GetPropTData;
use flow_typing_type::type_::HintEvalResult;
use flow_typing_type::type_::Literal;
use flow_typing_type::type_::MethodAction;
use flow_typing_type::type_::NumberLiteral;
use flow_typing_type::type_::ObjKind;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::Predicate;
use flow_typing_type::type_::PredicateInner;
use flow_typing_type::type_::PrivateMethodTData;
use flow_typing_type::type_::PropRef;
use flow_typing_type::type_::ReactEffectType;
use flow_typing_type::type_::ReactKitTData;
use flow_typing_type::type_::SpeculationHintSetData;
use flow_typing_type::type_::SpeculationHintState;
use flow_typing_type::type_::Targ;
use flow_typing_type::type_::ThisInstanceTData;
use flow_typing_type::type_::ThisStatus;
use flow_typing_type::type_::Tvar;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeGuard;
use flow_typing_type::type_::TypeGuardInner;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::TypeParam;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::UseT;
use flow_typing_type::type_::UseTInner;
use flow_typing_type::type_::any_t;
use flow_typing_type::type_::concretize_seen::ConcretizeSeen;
use flow_typing_type::type_::empty_tuple_view;
use flow_typing_type::type_::hint_unavailable;
use flow_typing_type::type_::inter_rep;
use flow_typing_type::type_::poly;
use flow_typing_type::type_::react;
use flow_typing_type::type_::type_collector::TypeCollector;
use flow_typing_type::type_::union_rep::UnionKind;
use flow_typing_type::type_::unknown_use;
use flow_typing_type::type_::unsoundness;
use flow_typing_type::type_util;
use flow_utils_concurrency::job_error::JobError;
use vec1::Vec1;

use crate::speculation_flow;
use crate::type_env;

pub type ConcrArgListFn =
    Rc<dyn for<'cx> Fn(&mut Context<'cx>, Option<ALoc>) -> Result<Vec<(ALoc, CallArg)>, JobError>>;

pub type ConcrJsxProps = flow_typing_context::LazyHintMapJsx;

pub type ConcrBaseHintT = Result<Type, JobError>;

pub type ConcrHint =
    Hint<Cx, ConcrBaseHintT, Result<Option<Vec<Targ>>, JobError>, ConcrArgListFn, ConcrJsxProps>;

pub type ConcrHintDecomposition = HintDecomposition<
    Cx,
    ConcrBaseHintT,
    Result<Option<Vec<Targ>>, JobError>,
    ConcrArgListFn,
    ConcrJsxProps,
>;

pub type ConcrHintDecompositionInner = HintDecompositionInner<
    Cx,
    ConcrBaseHintT,
    Result<Option<Vec<Targ>>, JobError>,
    ConcrArgListFn,
    ConcrJsxProps,
>;

pub type ConcrFunCallHints = FunCallImplicitInstantiationHints<
    Cx,
    ConcrBaseHintT,
    Result<Option<Vec<Targ>>, JobError>,
    ConcrArgListFn,
    ConcrJsxProps,
>;

pub type ConcrJsxHints = JsxImplicitInstantiationHints<
    Cx,
    ConcrBaseHintT,
    Result<Option<Vec<Targ>>, JobError>,
    ConcrArgListFn,
    ConcrJsxProps,
>;

pub fn with_hint<A>(
    ok: impl FnOnce(Type) -> A,
    error: impl FnOnce() -> A,
    result: HintEvalResult,
) -> A {
    match result {
        HintEvalResult::HintAvailable(t, _) => ok(t),
        _ => error(),
    }
}

#[derive(Debug)]
struct UnconstrainedTvarException;

#[derive(Debug)]
struct DecompFuncParamOutOfBoundsException;

#[derive(Debug)]
enum SandboxError {
    SpeculationSingleton,
    UnconstrainedTvar(UnconstrainedTvarException),
    DecompFuncParamOutOfBounds(DecompFuncParamOutOfBoundsException),
    Speculative,
    LimitExceeded,
    WorkerCanceled(flow_utils_concurrency::worker_cancel::WorkerCanceled),
    TimedOut(flow_utils_concurrency::job_error::CheckTimeout),
    DebugThrow { loc: flow_aloc::ALoc },
}

impl From<flow_utils_concurrency::worker_cancel::WorkerCanceled> for SandboxError {
    fn from(e: flow_utils_concurrency::worker_cancel::WorkerCanceled) -> Self {
        SandboxError::WorkerCanceled(e)
    }
}

impl From<flow_utils_concurrency::job_error::CheckTimeout> for SandboxError {
    fn from(e: flow_utils_concurrency::job_error::CheckTimeout) -> Self {
        SandboxError::TimedOut(e)
    }
}

impl From<JobError> for SandboxError {
    fn from(e: JobError) -> Self {
        match e {
            JobError::Canceled(c) => SandboxError::WorkerCanceled(c),
            JobError::TimedOut(t) => SandboxError::TimedOut(t),
            JobError::DebugThrow { loc } => SandboxError::DebugThrow { loc },
        }
    }
}

impl From<UnconstrainedTvarException> for SandboxError {
    fn from(e: UnconstrainedTvarException) -> Self {
        SandboxError::UnconstrainedTvar(e)
    }
}

impl From<DecompFuncParamOutOfBoundsException> for SandboxError {
    fn from(e: DecompFuncParamOutOfBoundsException) -> Self {
        SandboxError::DecompFuncParamOutOfBounds(e)
    }
}

impl From<SpeculativeError> for SandboxError {
    fn from(_e: SpeculativeError) -> Self {
        SandboxError::Speculative
    }
}

impl From<FlowJsException> for SandboxError {
    fn from(e: FlowJsException) -> Self {
        match e {
            FlowJsException::Speculative(_) => SandboxError::Speculative,
            FlowJsException::SpeculationSingletonError => SandboxError::SpeculationSingleton,
            FlowJsException::LimitExceeded => SandboxError::LimitExceeded,
            FlowJsException::WorkerCanceled(c) => SandboxError::WorkerCanceled(c),
            FlowJsException::TimedOut(t) => SandboxError::TimedOut(t),
            FlowJsException::DebugThrow { loc } => SandboxError::DebugThrow { loc },
        }
    }
}

struct HintOptions {
    expected_only: bool,
    skip_optional: bool,
}

fn in_sandbox_cx<'cx>(
    cx: &mut Context<'cx>,
    t: &Type,
    f: impl FnOnce(&mut Context<'cx>, Type) -> Result<Type, SandboxError>,
) -> Result<Option<Type>, JobError> {
    cx.run_and_rolled_back_cache(|cx| {
        let original_errors = cx.errors();
        cx.reset_errors(ErrorSet::empty());
        let resolved = tvar_resolver::resolved_t_abortable(false, cx, t.dupe());
        let resolved = match resolved {
            None => {
                cx.reset_errors(original_errors);
                return Ok(None);
            }
            Some(t) => t,
        };
        match f(cx, resolved) {
            Err(SandboxError::SpeculationSingleton)
            | Err(SandboxError::UnconstrainedTvar(_))
            | Err(SandboxError::DecompFuncParamOutOfBounds(_))
            | Err(SandboxError::Speculative)
            | Err(SandboxError::LimitExceeded) => {
                cx.reset_errors(original_errors);
                Ok(None)
            }
            Err(SandboxError::WorkerCanceled(c)) => {
                cx.reset_errors(original_errors);
                Err(JobError::Canceled(c))
            }
            Err(SandboxError::TimedOut(t)) => {
                cx.reset_errors(original_errors);
                Err(JobError::TimedOut(t))
            }
            Err(SandboxError::DebugThrow { loc }) => {
                cx.reset_errors(original_errors);
                Err(JobError::DebugThrow { loc })
            }
            Ok(t) => {
                let new_errors = cx.errors();
                cx.reset_errors(original_errors);
                if new_errors.is_lint_only_errorset() {
                    Ok(Some(t))
                } else {
                    Ok(None)
                }
            }
        }
    })
}

fn synthesis_speculation_call<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    call_reason: &Reason,
    reason: &Reason,
    rep: &inter_rep::InterRep,
    targs: Result<Option<Vec<Targ>>, JobError>,
    argts: Vec<CallArg>,
) -> Result<Type, SandboxError> {
    let intersection = Type::new(TypeInner::IntersectionT(reason.dupe(), rep.dupe()));
    let use_op = unknown_use();
    let tout_id = flow_typing_tvar::mk_no_wrap(cx, call_reason);
    let tout = Tvar::new(call_reason.dupe(), tout_id as u32);
    let call_speculation_hint_state =
        Rc::new(RefCell::new(SpeculationHintState::SpeculationHintUnset));
    let call_action = Box::new(CallAction::Funcalltype(Box::new(FuncallType {
        call_this_t: unsoundness::bound_fn_this_any(reason.dupe()),
        call_targs: targs?.map(|v| v.into()),
        call_args_tlist: argts.into(),
        call_tout: tout,
        call_strict_arity: true,
        call_speculation_hint_state: Some(call_speculation_hint_state.dupe()),
        call_specialized_callee: None,
    })));
    let use_t = UseT::new(UseTInner::CallT(Box::new(CallTData {
        use_op,
        reason: call_reason.dupe(),
        call_action,
        return_hint: hint_unavailable(),
    })));
    flow_js::flow_with_env(cx, env, (&intersection, &use_t))?;
    match &*call_speculation_hint_state.borrow() {
        SpeculationHintState::SpeculationHintUnset
        | SpeculationHintState::SpeculationHintInvalid => {
            flow_js::add_output_with_env(
                cx,
                env,
                ErrorMessage::EUnionSpeculationFailed(Box::new(EUnionSpeculationFailedData {
                    use_op: unknown_use(),
                    loc: reason.loc().dupe(),
                    branches: vec![],
                })),
            )?;
            Ok(any_t::error(reason.dupe()))
        }
        SpeculationHintState::SpeculationHintSet(box SpeculationHintSetData(_, t)) => Ok(t.dupe()),
    }
}

fn simplify_callee<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    use_op: UseOp,
    func_t: &Type,
) -> Result<Type, SandboxError> {
    let collector = TypeCollector::create();
    let call_action = Box::new(CallAction::ConcretizeCallee(collector.dupe()));
    let use_t = UseT::new(UseTInner::CallT(Box::new(CallTData {
        use_op,
        reason: reason.dupe(),
        call_action,
        return_hint: hint_unavailable(),
    })));
    flow_js::flow_with_env(cx, env, (func_t, &use_t))?;
    Ok(type_util::union_of_ts(
        reason.dupe(),
        collector.collect_to_vec(),
        Some(UnionKind::ResolvedKind),
    ))
}

// A cheaper version of `instantiate_poly_with_targs` from flow_js_utils that only performs
// substitution and skips any validation.
fn synthesis_instantiate_callee<'cx>(
    cx: &mut Context<'cx>,
    reason: &Reason,
    tparams: &[TypeParam],
    tout: &Type,
    targs: Vec<Type>,
) -> Type {
    let mut map: FlowOrdMap<SubstName, Type> = FlowOrdMap::new();
    let mut ts = VecDeque::from(targs);
    for typeparam in tparams {
        let t = if let Some(t) = ts.pop_front() {
            t
        } else {
            match &typeparam.default {
                Some(default) => flow_js::subst(cx, None, None, None, &map, default.dupe()),
                None => any_t::make(AnySource::AnyError(None), reason.dupe()),
            }
        };
        map.insert(typeparam.name.dupe(), t);
    }
    flow_js::subst(cx, None, None, None, &map, tout.dupe())
}

fn get_t<'cx>(cx: &mut Context<'cx>, t: Type) -> Type {
    fn get_t_with_depth<'cx>(cx: &mut Context<'cx>, mut depth: i32, mut t: Type) -> Type {
        loop {
            match t.deref() {
                TypeInner::AnnotT(_, inner_t, _) if depth >= 0 => {
                    depth -= 1;
                    t = inner_t.dupe();
                }
                _ if depth >= 0
                    && let Some(node) = type_util::constraint_node_id(&t) =>
                {
                    let merged = flow_js_utils::merge_tvar(
                        cx,
                        false,
                        |_cx, r| Type::new(TypeInner::DefT(r.dupe(), DefT::new(DefTInner::EmptyT))),
                        type_util::reason_of_t(&t),
                        node.id(),
                    );
                    depth -= 1;
                    t = merged;
                }
                _ => return t.dupe(),
            }
        }
    }

    // We choose a depth of 3 because it's sufficient to unwrap OpenT(AnnotT(OpenT)), which is the most
    // complicated case known. If we run into issues in the future, we can increase the depth limit.
    get_t_with_depth(cx, 3, t)
}

fn instantiate_callee<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    opts: &HintOptions,
    target_reason: &Reason,
    fn_t: &Type,
    instantiation_hint: &ConcrFunCallHints,
) -> Result<Type, SandboxError> {
    let reason = &instantiation_hint.reason;
    let targs = &instantiation_hint.targs;
    let arg_list = &instantiation_hint.arg_list;
    let return_hints = &instantiation_hint.return_hints;
    let arg_index = instantiation_hint.arg_index;

    fn handle_poly<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        opts: &HintOptions,
        target_reason: &Reason,
        reason: &Reason,
        targs: &Rc<Lazy<Cx, Result<Option<Vec<Targ>>, JobError>>>,
        arg_list: &Rc<Lazy<Cx, ConcrArgListFn>>,
        return_hints: &Rc<Lazy<Cx, Vec<ConcrHint>>>,
        arg_index: i32,
        t: &Type,
    ) -> Result<Type, SandboxError> {
        match t.deref() {
            TypeInner::DefT(_, def) => {
                match def.deref() {
                    DefTInner::ObjT(obj) if let Some(id) = obj.call_t.as_ref() => {
                        let call_t = cx.find_call(*id);
                        handle_poly(
                            cx,
                            env,
                            opts,
                            target_reason,
                            reason,
                            targs,
                            arg_list,
                            return_hints,
                            arg_index,
                            &call_t,
                        )
                    }
                    DefTInner::InstanceT(inst_t)
                        if let Some(id) = inst_t.inst.inst_call_t.as_ref() =>
                    {
                        let call_t = cx.find_call(*id);
                        handle_poly(
                            cx,
                            env,
                            opts,
                            target_reason,
                            reason,
                            targs,
                            arg_list,
                            return_hints,
                            arg_index,
                            &call_t,
                        )
                    }
                    DefTInner::ClassT(instance) => {
                        let class_reason = type_util::reason_of_t(t);
                        let collector = TypeCollector::create();
                        let use_t = UseT::new(UseTInner::GetStaticsT(
                            class_reason.dupe(),
                            collector.dupe(),
                        ));
                        flow_js::flow_with_env(cx, env, (instance, &use_t))?;
                        let statics = collector
                            .union_opt(class_reason.dupe())
                            .unwrap_or_else(|| tvar_resolver::default_no_lowers(class_reason));
                        let resolved = get_t(cx, statics);
                        handle_poly(
                            cx,
                            env,
                            opts,
                            target_reason,
                            reason,
                            targs,
                            arg_list,
                            return_hints,
                            arg_index,
                            &resolved,
                        )
                    }
                    DefTInner::PolyT(box PolyTData {
                        kind: _,
                        tparams_loc,
                        tparams,
                        t_out,
                        id: _,
                        strictness_kind: _,
                    }) => {
                        if let Some((class_r, inst_r, i, this, this_name)) =
                            if let TypeInner::DefT(class_r, inner_def) = t_out.deref()
                                && let DefTInner::ClassT(class_inner) = inner_def.deref()
                                && let TypeInner::ThisInstanceT(box ThisInstanceTData {
                                    reason: inst_r,
                                    instance: i,
                                    is_this: this,
                                    subst_name: this_name,
                                }) = class_inner.deref()
                            {
                                Some((class_r, inst_r, i, this, this_name))
                            } else {
                                None
                            }
                        {
                            let subst_map: FlowOrdMap<SubstName, Type> = tparams
                                .iter()
                                .map(|tparam| (tparam.name.dupe(), tparam.bound.dupe()))
                                .collect();
                            let substed_i =
                                flow_typing_flow_common::type_subst::subst_instance_type(
                                    cx,
                                    None,
                                    true, // force
                                    false,
                                    flow_typing_flow_common::type_subst::Purpose::Normal,
                                    &subst_map,
                                    i,
                                );
                            let fixed = flow_js_utils::fix_this_instance(
                                cx,
                                inst_r.dupe(),
                                inst_r.dupe(),
                                &substed_i,
                                *this,
                                this_name.dupe(),
                            );
                            let result_t = Type::new(TypeInner::DefT(
                                class_r.dupe(),
                                DefT::new(DefTInner::ClassT(fixed)),
                            ));
                            let resolved = get_t(cx, result_t);
                            handle_poly(
                                cx,
                                env,
                                opts,
                                target_reason,
                                reason,
                                targs,
                                arg_list,
                                return_hints,
                                arg_index,
                                &resolved,
                            )
                        } else {
                            let call_targs: Option<Vec<Targ>> = targs.get_forced(cx).clone()?;
                            match type_util::all_explicit_targ_ts(call_targs.as_deref()) {
                                Some(targ_ts) => Ok(synthesis_instantiate_callee(
                                    cx, reason, tparams, t_out, targ_ts,
                                )),
                                None => {
                                    let call_args_tlist: Vec<CallArg> = {
                                        let checked_t =
                                            |cx: &mut Context<'cx>, t: &Type, loc: ALoc| -> Type {
                                                let desc =
                                                    type_util::reason_of_t(t).desc(true).cloned();
                                                let reason =
                                                    flow_common::reason::mk_reason(desc, loc);
                                                type_env::find_write(
                                                    cx,
                                                    DefLocType::ExpressionLoc,
                                                    reason,
                                                )
                                            };
                                        let arg_list_fn: &ConcrArgListFn = arg_list.get_forced(cx);
                                        let target_loc = Some(target_reason.loc().dupe());
                                        let args = arg_list_fn(cx, target_loc)?;
                                        args.iter()
                                            .enumerate()
                                            .map(|(i, (loc, call_arg))| {
                                                if i as i32 >= arg_index {
                                                    call_arg.clone()
                                                } else {
                                                    match call_arg.deref() {
                                                        CallArgInner::Arg(t) => CallArg::arg(
                                                            checked_t(cx, t, loc.dupe()),
                                                        ),
                                                        CallArgInner::SpreadArg(t) => {
                                                            CallArg::spread_arg(checked_t(
                                                                cx,
                                                                t,
                                                                loc.dupe(),
                                                            ))
                                                        }
                                                    }
                                                }
                                            })
                                            .collect()
                                    };
                                    let return_hint = {
                                        let return_hints_val: &Vec<ConcrHint> =
                                            return_hints.get_forced(cx);
                                        match evaluate_hints_inner(
                                            cx,
                                            env,
                                            opts,
                                            reason,
                                            return_hints_val.clone(),
                                        )? {
                                            HintEvalResult::HintAvailable(t, k) => Some((t, k)),
                                            _ => None,
                                        }
                                    };
                                    let check = ImplicitInstantiationCheck::of_call(
                                        t.dupe(),
                                        (
                                            tparams_loc.dupe(),
                                            Vec1::try_from_vec(tparams.to_vec())
                                                .unwrap_or_else(|_| Vec1::new(tparams[0].dupe())),
                                            t_out.dupe(),
                                        ),
                                        unknown_use(),
                                        reason.dupe(),
                                        flow_typing_implicit_instantiation_check::Call {
                                            call_this_t: unsoundness::unresolved_any(reason.dupe()),
                                            call_targs: call_targs.clone().map(|v| v.into()),
                                            call_args_tlist: call_args_tlist.into(),
                                            call_strict_arity: true,
                                            call_speculation_hint_state: None,
                                            call_specialized_callee: None,
                                        },
                                    );
                                    let subst_map = {
                                        let env = &env.solving_implicit_instantiation();
                                        let (targ_map, _) =
                                        implicit_instantiation::instantiation_solver::solve_targs(
                                            cx,env,  unknown_use(), false, false, return_hint, &check
                                        )?;
                                        let map: FlowOrdMap<SubstName, Type> = targ_map
                                            .into_iter()
                                            .map(|(k, v)| (k, v.to_type()))
                                            .collect();
                                        Ok::<_, SandboxError>(map)
                                    }?;
                                    Ok(flow_js::subst(
                                        cx,
                                        None,
                                        None,
                                        None,
                                        &subst_map,
                                        t_out.dupe(),
                                    ))
                                }
                            }
                        }
                    }
                    _ => Ok(t.dupe()),
                }
            }
            _ => Ok(t.dupe()),
        }
    }

    let mut resolve_overload_and_targs = |fn_t: &Type| -> Result<Type, SandboxError> {
        let t = match fn_t.deref() {
            TypeInner::IntersectionT(r, rep) => {
                let targs_val: &Result<Option<Vec<Targ>>, JobError> = targs.get_forced(cx);
                let arg_list_fn: &ConcrArgListFn = arg_list.get_forced(cx);
                let target_loc = Some(target_reason.loc().dupe());
                let args_val = arg_list_fn(cx, target_loc)?;
                let argts: Vec<CallArg> = args_val.iter().map(|(_, arg)| arg.clone()).collect();
                synthesis_speculation_call(cx, env, reason, r, rep, targs_val.clone(), argts)?
            }
            _ => fn_t.dupe(),
        };
        let resolved = get_t(cx, t);
        handle_poly(
            cx,
            env,
            opts,
            target_reason,
            reason,
            targs,
            arg_list,
            return_hints,
            arg_index,
            &resolved,
        )
    };

    match fn_t.deref() {
        TypeInner::UnionT(_, rep) => {
            let new_rep = rep.ident_map(false, |t| {
                resolve_overload_and_targs(t).unwrap_or_else(|_| t.dupe())
            });
            Ok(Type::new(TypeInner::UnionT(reason.dupe(), new_rep)))
        }
        _ => resolve_overload_and_targs(fn_t),
    }
}

fn instantiate_component<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    opts: &HintOptions,
    component: &Type,
    instantiation_hint: &ConcrJsxHints,
) -> Result<Type, SandboxError> {
    let component_resolved = get_t(cx, component.dupe());
    match component_resolved.deref() {
        TypeInner::DefT(_, def) => match def.deref() {
            DefTInner::PolyT(box PolyTData {
                kind: _,
                tparams_loc,
                tparams,
                t_out,
                id: _,
                strictness_kind: _,
            }) if *cx.jsx() == JsxMode::JsxReact => {
                let reason = &instantiation_hint.jsx_reason;
                let jsx_targs = &instantiation_hint.jsx_targs;
                let jsx_props_and_children = &instantiation_hint.jsx_props_and_children;
                let jsx_hints = &instantiation_hint.jsx_hints;

                let return_hint = {
                    let hints_val: &Vec<ConcrHint> = jsx_hints.get_forced(cx);
                    match evaluate_hints_inner(cx, env, opts, reason, hints_val.clone())? {
                        HintEvalResult::HintAvailable(t, k) => Some((t, k)),
                        _ => None,
                    }
                };
                let check = {
                    let jsx_targs_val: Option<Vec<Targ>> = jsx_targs.get_forced(cx).clone()?;
                    ImplicitInstantiationCheck::of_react_jsx(
                        component.dupe(),
                        (
                            tparams_loc.dupe(),
                            Vec1::try_from_vec(tparams.to_vec())
                                .unwrap_or_else(|_| Vec1::new(tparams[0].dupe())),
                            t_out.dupe(),
                        ),
                        unknown_use(),
                        reason.dupe(),
                        jsx_props_and_children.try_get_forced(cx)?.dupe(),
                        jsx_targs_val.as_ref().map(|v| Rc::from(v.as_slice())),
                    )
                };
                let subst_map = {
                    let env = &env.solving_implicit_instantiation();
                    let (targ_map, _) = implicit_instantiation::instantiation_solver::solve_targs(
                        cx,
                        env,
                        unknown_use(),
                        false,
                        false,
                        return_hint,
                        &check,
                    )?;
                    let map: FlowOrdMap<SubstName, Type> = targ_map
                        .into_iter()
                        .map(|(k, v)| (k, v.to_type()))
                        .collect();
                    Ok::<_, SandboxError>(map)
                }?;
                Ok(flow_js::subst(
                    cx,
                    None,
                    None,
                    None,
                    &subst_map,
                    t_out.dupe(),
                ))
            }
            DefTInner::ObjT(obj) if let Some(id) = obj.call_t.as_ref() => {
                let call_fn = cx.find_call(*id);
                let is_poly_fun = matches!(
                    call_fn.deref(),
                    TypeInner::DefT(_, inner_def)
                        if matches!(
                            inner_def.deref(),
                            DefTInner::PolyT(box PolyTData { t_out, .. })
                                if matches!(
                                    t_out.deref(),
                                    TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::FunT(..))
                                )
                        )
                );
                if is_poly_fun {
                    instantiate_component(cx, env, opts, &call_fn, instantiation_hint)
                } else {
                    Ok(component_resolved.dupe())
                }
            }
            _ => Ok(component_resolved.dupe()),
        },
        TypeInner::TypeAppT(box TypeAppTData { reason, .. }) => {
            let concrete = FlowJs::singleton_concrete_type_for_inspection_with_env(
                cx,
                env,
                reason,
                &component_resolved,
            )?;
            instantiate_component(cx, env, opts, &concrete, instantiation_hint)
        }
        _ => Ok(component_resolved.dupe()),
    }
}

fn type_of_hint_decomposition<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    opts: &HintOptions,
    op: &ConcrHintDecompositionInner,
    reason: &Reason,
    t: &Type,
) -> Result<Option<Type>, JobError> {
    let make_fun_t = |reason: &Reason,
                      cx: &mut Context<'cx>,
                      params: Vec<FunParam>,
                      rest_param: Option<FunRestParam>,
                      return_t: Type,
                      type_guard: &Option<PredicateKind>|
     -> Type {
        let statics_reason = flow_common::reason::func_reason(false, false, reason.loc().dupe());
        let statics = flow_typing_flow_common::obj_type::mk_with_proto(
            cx,
            statics_reason,
            ObjKind::Inexact,
            None,
            None,
            None,
            None,
            Type::new(TypeInner::FunProtoT(reason.dupe())),
            true,
        );
        let type_guard = type_guard.as_ref().map(|tg| match tg {
            PredicateKind::TypeGuardKind(param_loc, param_name) => TypeGuard::new(TypeGuardInner {
                reason: reason.dupe(),
                kind: flow_typing_type::type_::TypeGuardKind::Default,
                inferred: false,
                param_name: (param_loc.dupe(), param_name.dupe()),
                type_guard: Some(unsoundness::unresolved_any(reason.dupe())),
            }),
        });
        let func = FunType {
            this_t: (
                unsoundness::unresolved_any(reason.dupe()),
                ThisStatus::ThisFunction,
            ),
            params: params.into(),
            rest_param,
            return_t,
            type_guard,
            def_reason: reason.dupe(),
            function_reference_kind: flow_common::error_ref::FunctionReferenceKind::FunctionType,
            effect_: ReactEffectType::AnyEffect,
            strictness_kind: Default::default(),
            literal: false,
        };
        Type::new(TypeInner::DefT(
            reason.dupe(),
            DefT::new(DefTInner::FunT(statics, Rc::new(func))),
        ))
    };

    let map_intersection = |cx: &mut Context<'cx>,
                            t: &Type,
                            f: &dyn Fn(&mut Context<'cx>, &Type) -> Result<Type, SandboxError>|
     -> Result<Type, SandboxError> {
        let resolved = get_t(cx, t.dupe());
        match resolved.deref() {
            TypeInner::IntersectionT(r, rep) => {
                let new_rep = rep.ident_map(|inner_t| {
                    let resolved_inner = get_t(cx, inner_t.dupe());
                    f(cx, &resolved_inner).unwrap_or(resolved_inner)
                });
                Ok(Type::new(TypeInner::IntersectionT(r.dupe(), new_rep)))
            }
            _ => f(cx, &resolved),
        }
    };

    let get_constructor_type = |cx: &mut Context<'cx>, t: &Type| -> Result<Type, SandboxError> {
        let get_constructor_method_type =
            |cx: &mut Context<'cx>, t: &Type| -> Result<Type, FlowJsException> {
                speculation_flow::get_method_type_unsafe(
                    cx,
                    env,
                    t,
                    reason.dupe(),
                    type_util::mk_named_prop(reason.dupe(), false, Name::new("constructor")),
                )
            };
        let mod_ctor_return = |cx: &mut Context<'cx>, instance_type: &Type, t: &Type| -> Type {
            match t.deref() {
                TypeInner::DefT(fun_reason, def) => match def.deref() {
                    DefTInner::FunT(statics, func) => {
                        let new_func = FunType {
                            this_t: func.this_t.clone(),
                            params: func.params.clone(),
                            rest_param: func.rest_param.clone(),
                            return_t: instance_type.dupe(),
                            type_guard: func.type_guard.clone(),
                            def_reason: func.def_reason.dupe(),
                            function_reference_kind: func.function_reference_kind,
                            effect_: func.effect_.clone(),
                            strictness_kind: func.strictness_kind,
                            literal: func.literal,
                        };
                        Type::new(TypeInner::DefT(
                            fun_reason.dupe(),
                            DefT::new(DefTInner::FunT(statics.dupe(), Rc::new(new_func))),
                        ))
                    }
                    _ => get_t(cx, t.dupe()),
                },
                _ => get_t(cx, t.dupe()),
            }
        };
        if let TypeInner::DefT(_, def) = get_t(cx, t.dupe()).deref()
            && let DefTInner::PolyT(box PolyTData {
                kind: poly_kind,
                tparams_loc,
                tparams,
                t_out: instance_type,
                id: _,
                strictness_kind,
            }) = def.deref()
        {
            let ctor_method = get_constructor_method_type(cx, instance_type)?;
            let tparams_loc = tparams_loc.dupe();
            let tparams = tparams.dupe();
            let instance_type = instance_type.dupe();
            map_intersection(
                cx,
                &ctor_method,
                &|cx: &mut Context<'cx>, t_inner: &Type| -> Result<Type, SandboxError> {
                    if let TypeInner::DefT(_, inner_def) = t_inner.deref()
                        && let DefTInner::PolyT(box PolyTData {
                            kind: poly_kind2,
                            tparams_loc: tparams_loc2,
                            tparams: tparams2,
                            t_out,
                            id: _,
                            strictness_kind: strictness_kind2,
                        }) = inner_def.deref()
                    {
                        let t_out = mod_ctor_return(cx, &instance_type, t_out);
                        let mut combined_tparams: Vec<_> = tparams.to_vec();
                        combined_tparams.extend(tparams2.iter().cloned());
                        Ok(Type::new(TypeInner::DefT(
                            reason.dupe(),
                            DefT::new(DefTInner::PolyT(Box::new(PolyTData {
                                kind: *poly_kind2,
                                tparams_loc: tparams_loc2.dupe(),
                                tparams: combined_tparams.into(),
                                t_out,
                                id: poly::Id::generate_id(),
                                strictness_kind: strictness_kind.join(*strictness_kind2),
                            }))),
                        )))
                    } else {
                        let t_out = mod_ctor_return(cx, &instance_type, t_inner);
                        Ok(Type::new(TypeInner::DefT(
                            reason.dupe(),
                            DefT::new(DefTInner::PolyT(Box::new(PolyTData {
                                kind: *poly_kind,
                                tparams_loc: tparams_loc.dupe(),
                                tparams: tparams.dupe(),
                                t_out,
                                id: poly::Id::generate_id(),
                                strictness_kind: *strictness_kind,
                            }))),
                        )))
                    }
                },
            )
        } else {
            let t_resolved = get_t(cx, t.dupe());
            let ctor_method = get_constructor_method_type(cx, &t_resolved)?;
            let t_resolved2 = t_resolved.dupe();
            map_intersection(
                cx,
                &ctor_method,
                &|cx: &mut Context<'cx>, t_inner: &Type| -> Result<Type, SandboxError> {
                    Ok(mod_ctor_return(cx, &t_resolved2, t_inner))
                },
            )
        }
    };

    in_sandbox_cx(cx, t, |cx, t| {
        match op {
            ConcrHintDecompositionInner::DecompArrElement(index) => {
                let num = match index {
                    Some(i) => DefTInner::SingletonNumT {
                        from_annot: true,
                        value: NumberLiteral(*i as f64, FlowSmolStr::new(format!("{}", i))),
                    },
                    None => DefTInner::NumGeneralT(Literal::AnyLiteral),
                };
                let t = t.dupe();
                let reason2 = reason.dupe();
                let skip_optional = opts.skip_optional;
                Ok(flow_typing_tvar::mk_no_wrap_where(
                    cx,
                    reason.dupe(),
                    move |cx, r, id| {
                        let tout = Tvar::new(r.dupe(), id as u32);
                        let use_t = UseT::new(UseTInner::GetElemT(Box::new(GetElemTData {
                            use_op: unknown_use(),
                            reason: reason2.dupe(),
                            id: None,
                            from_annot: true,
                            skip_optional,
                            access_iterables: true,
                            key_t: Type::new(TypeInner::DefT(
                                reason2.dupe(),
                                DefT::new(num.clone()),
                            )),
                            tout: Box::new(tout),
                        })));
                        speculation_flow::resolved_lower_flow_unsafe(
                            cx,
                            env,
                            &reason2,
                            (&t, &use_t),
                        )?;
                        Ok::<(), FlowJsException>(())
                    },
                )?)
            }
            ConcrHintDecompositionInner::DecompArrSpread(i) => {
                let t = t.dupe();
                let reason2 = reason.dupe();
                let i = *i;
                Ok(flow_typing_tvar::mk_no_wrap_where(
                    cx,
                    reason.dupe(),
                    move |cx, r, id| {
                        let tout = Tvar::new(r.dupe(), id as u32);
                        let use_t = UseT::new(UseTInner::ArrRestT(Box::new(ArrRestTData {
                            literal: false,
                            use_op: unknown_use(),
                            reason: reason2.dupe(),
                            index: i,
                            tout: Type::new(TypeInner::OpenT(tout)),
                        })));
                        speculation_flow::resolved_lower_flow_unsafe(
                            cx,
                            env,
                            &reason2,
                            (&t, &use_t),
                        )?;
                        Ok::<(), FlowJsException>(())
                    },
                )?)
            }
            ConcrHintDecompositionInner::DecompEmptyArrayElement => {
                if flow_js_utils::tvar_visitors::has_placeholders(cx, &t) {
                    flow_typing_debug::verbose::print_if_verbose_lazy(cx, None, None, None, || {
                        vec![format!(
                            "Encountered placeholder type: {}",
                            flow_typing_debug::dump_t(Some(3), cx, &t),
                        )]
                    });
                    Ok(cx.mk_placeholder(reason.dupe()))
                } else {
                    let elem_t = flow_typing_tvar::mk(cx, reason.dupe());
                    let arr_t = Type::new(TypeInner::DefT(
                        reason.dupe(),
                        DefT::new(DefTInner::ArrT(Rc::new(ArrType::ArrayAT(Box::new(
                            ArrayATData {
                                elem_t: elem_t.dupe(),
                                tuple_view: Some(empty_tuple_view()),
                                react_dro: None,
                                strictness_kind: cx.type_strictness_kind(),
                                literal: false,
                                fresh: false,
                            },
                        ))))),
                    ));
                    {
                        let env = &env.solving_implicit_instantiation();
                        speculation_flow::flow_t_unsafe(cx, env, (arr_t, t.dupe()))
                    }?;
                    Ok(implicit_instantiation::pin_tvar(cx, env, &elem_t)?)
                }
            }
            ConcrHintDecompositionInner::DecompAwait => {
                let collector = TypeCollector::create();
                let tout = UseT::new(UseTInner::ConcretizeT(Box::new(ConcretizeTData {
                    reason: reason.dupe(),
                    kind: ConcretizationKind::ConcretizeForLowerBounds,
                    seen: ConcretizeSeen::new(),
                    collector: collector.dupe(),
                })));
                flow_js::flow_with_env(cx, env, (&t, &tout))?;
                let promise_t = FlowJs::get_builtin_typeapp_with_env(
                    cx,
                    env,
                    reason,
                    None,
                    "Promise",
                    vec![t.dupe()],
                );
                speculation_flow::resolved_lower_flow_unsafe(cx, env, reason, (&promise_t, &tout))?;
                let mut lowers = Vec::new();
                flow_js_utils::collect_lowers(
                    false,
                    cx,
                    &mut BTreeSet::new(),
                    &mut lowers,
                    collector.collect_to_vec(),
                );
                type_util::union_of_ts_opt(reason.dupe(), lowers, Some(UnionKind::ResolvedKind))
                    .ok_or_else(|| UnconstrainedTvarException.into())
            }
            ConcrHintDecompositionInner::DecompCallNew => {
                let concretize =
                    |cx: &mut Context<'_>, t: &Type| -> Result<Vec<Type>, FlowJsException> {
                        FlowJs::possible_concrete_types_for_inspection_with_env(
                            cx,
                            env,
                            type_util::reason_of_t(t),
                            t,
                        )
                    };
                // A binding annotated `typeof Promise`, where `Promise` is a
                // `declare var` of a constructor interface, sits behind more
                // tvars than [get_t] unwraps.
                let callee = match concretize(cx, &t)?.as_slice() {
                    [callee] => callee.dupe(),
                    _ => get_t(cx, t.dupe()),
                };
                // For interfaces with a construct signature (own or inherited via
                // [extends]), extract it via the same helper subtyping uses, so the
                // hint matches what would actually flow.
                match callee.deref() {
                    TypeInner::DefT(_, def_t)
                        if let DefTInner::InstanceT(_inst_t) = def_t.deref() =>
                    {
                        // Interface without a construct signature — `new` on it is an
                        // error, but we don't have a sensible hint to give. Return an
                        // error-any so downstream hint consumers treat it as "no hint"
                        // rather than misrouting into the class-unwrap branch below
                        // (which would silently produce nothing).
                        match flow_js_utils::combine_construct_ts(
                            flow_js_utils::collect_construct_ts(&concretize, cx, &t)?,
                        ) {
                            Some(lt) => Ok(lt),
                            None => Ok(any_t::error(reason.dupe())),
                        }
                    }
                    _ => {
                        // For `new A(...)`, The initial base type we have is `Class<A>`. We need to first unwrap
                        // it, so that we can access the `constructor` method (which is considered an instance
                        // method).
                        let get_this_t =
                            |cx: &mut Context<'cx>, t: &Type| -> Result<Type, SandboxError> {
                                let t = t.dupe();
                                let reason2 = reason.dupe();
                                let tvar_result = flow_typing_tvar::mk_where(
                                    cx,
                                    reason.dupe(),
                                    move |cx, t_prime| {
                                        let class_t = Type::new(TypeInner::DefT(
                                            reason2.dupe(),
                                            DefT::new(DefTInner::ClassT(t_prime.dupe())),
                                        ));
                                        speculation_flow::resolved_lower_flow_t_unsafe(
                                            cx,
                                            env,
                                            &reason2,
                                            (&t, &class_t),
                                        )?;
                                        Ok::<(), FlowJsException>(())
                                    },
                                )?;
                                Ok(get_t(cx, tvar_result))
                            };
                        let this_t = match get_t(cx, t.dupe()).deref() {
                            TypeInner::DefT(poly_reason, def) => match def.deref() {
                                DefTInner::PolyT(box PolyTData {
                                    kind: poly_kind,
                                    tparams_loc,
                                    tparams,
                                    t_out,
                                    id: _,
                                    strictness_kind,
                                }) => Type::new(TypeInner::DefT(
                                    poly_reason.dupe(),
                                    DefT::new(DefTInner::PolyT(Box::new(PolyTData {
                                        kind: *poly_kind,
                                        tparams_loc: tparams_loc.dupe(),
                                        tparams: tparams.dupe(),
                                        t_out: get_this_t(cx, t_out)?,
                                        id: poly::Id::generate_id(),
                                        strictness_kind: *strictness_kind,
                                    }))),
                                )),
                                _ => {
                                    let t = get_t(cx, t);
                                    get_this_t(cx, &t)?
                                }
                            },
                            _ => {
                                let t = get_t(cx, t);
                                get_this_t(cx, &t)?
                            }
                        };
                        Ok(get_constructor_type(cx, &this_t)?)
                    }
                }
            }
            ConcrHintDecompositionInner::DecompCallSuper => Ok(get_constructor_type(cx, &t)?),
            ConcrHintDecompositionInner::DecompFuncParam(xs, i, type_guard) => {
                let i = *i as usize;
                if i > xs.len() {
                    // This is an internal error. We shouldn't be creating Decomp_FuncParam
                    // where [i] is not a valid index of [xs].
                    return Err(SandboxError::DecompFuncParamOutOfBounds(
                        DecompFuncParamOutOfBoundsException,
                    ));
                }
                let xs: Vec<_> = xs.iter().take(i + 1).collect();
                let t = t.dupe();
                let reason2 = reason.dupe();
                let type_guard = type_guard.clone();
                Ok(flow_typing_tvar::mk_where(
                    cx,
                    reason.dupe(),
                    move |cx, param_t| {
                        let params: Vec<FunParam> = xs
                            .iter()
                            .enumerate()
                            .map(|(idx, x)| {
                                if i == idx {
                                    FunParam(x.as_ref().map(|s| s.dupe()), param_t.dupe())
                                } else {
                                    FunParam(
                                        x.as_ref().map(|s| s.dupe()),
                                        unsoundness::unresolved_any(reason2.dupe()),
                                    )
                                }
                            })
                            .collect();
                        let fun_type = make_fun_t(
                            &reason2,
                            cx,
                            params,
                            None,
                            unsoundness::unresolved_any(reason2.dupe()),
                            &type_guard,
                        );
                        speculation_flow::resolved_upper_flow_t_unsafe(
                            cx,
                            env,
                            &reason2,
                            (&fun_type, &t),
                        )?;
                        Ok::<(), FlowJsException>(())
                    },
                )?)
            }
            ConcrHintDecompositionInner::DecompFuncRest(xs, type_guard) => {
                let t = t.dupe();
                let reason2 = reason.dupe();
                let type_guard = type_guard.clone();
                let xs = xs.clone();
                Ok(flow_typing_tvar::mk_where(
                    cx,
                    reason.dupe(),
                    move |cx, rest_t| {
                        let params: Vec<FunParam> = xs
                            .iter()
                            .map(|x| {
                                FunParam(
                                    x.as_ref().map(|s| s.dupe()),
                                    unsoundness::unresolved_any(reason2.dupe()),
                                )
                            })
                            .collect();
                        let fun_type = make_fun_t(
                            &reason2,
                            cx,
                            params,
                            Some(FunRestParam(None, ALoc::default(), rest_t.dupe())),
                            unsoundness::unresolved_any(reason2.dupe()),
                            &type_guard,
                        );
                        speculation_flow::resolved_upper_flow_t_unsafe(
                            cx,
                            env,
                            &reason2,
                            (&fun_type, &t),
                        )?;
                        Ok::<(), FlowJsException>(())
                    },
                )?)
            }
            ConcrHintDecompositionInner::DecompFuncReturn => {
                let concrete_types =
                    FlowJs::possible_concrete_types_for_inspection_with_env(cx, env, reason, &t)?;
                let rest_param = if let [single_t] = concrete_types.as_slice()
                    && let TypeInner::DefT(_, def) = single_t.deref()
                    && let DefTInner::FunT(_, func) = def.deref()
                    && let Some(FunRestParam(_, _, rest_t)) = &func.rest_param
                {
                    Some(FunRestParam(None, ALoc::default(), rest_t.dupe()))
                } else {
                    Some(FunRestParam(
                        None,
                        ALoc::default(),
                        unsoundness::unresolved_any(reason.dupe()),
                    ))
                };
                let t = t.dupe();
                let reason2 = reason.dupe();
                Ok(flow_typing_tvar::mk_where(
                    cx,
                    reason.dupe(),
                    move |cx, return_t| {
                        let fun_type =
                            make_fun_t(&reason2, cx, vec![], rest_param, return_t.dupe(), &None);
                        speculation_flow::resolved_lower_flow_t_unsafe(
                            cx,
                            env,
                            &reason2,
                            (&t, &fun_type),
                        )?;
                        Ok::<(), FlowJsException>(())
                    },
                )?)
            }
            ConcrHintDecompositionInner::CompImmediateFuncCall => {
                Ok(make_fun_t(reason, cx, vec![], None, t.dupe(), &None))
            }
            ConcrHintDecompositionInner::CompMaybeT => {
                Ok(Type::new(TypeInner::MaybeT(reason.dupe(), t.dupe())))
            }
            ConcrHintDecompositionInner::DecompJsxProps => {
                let t = t.dupe();
                let reason2 = reason.dupe();
                Ok(flow_typing_tvar::mk_no_wrap_where(
                    cx,
                    reason.dupe(),
                    move |cx, r, id| {
                        let props_t = Tvar::new(r.dupe(), id as u32);
                        let use_t = UseT::new(UseTInner::ReactKitT(Box::new(ReactKitTData {
                            use_op: unknown_use(),
                            reason: reason2.dupe(),
                            tool: Box::new(react::Tool::GetConfig {
                                tout: Type::new(TypeInner::OpenT(props_t)),
                            }),
                        })));
                        speculation_flow::resolved_lower_flow_unsafe(
                            cx,
                            env,
                            &reason2,
                            (&t, &use_t),
                        )?;
                        Ok::<(), FlowJsException>(())
                    },
                )?)
            }
            ConcrHintDecompositionInner::DecompMethodElem => {
                Ok(speculation_flow::get_method_type_unsafe(
                    cx,
                    env,
                    &t,
                    reason.dupe(),
                    PropRef::Computed(Type::new(TypeInner::DefT(
                        reason.dupe(),
                        DefT::new(DefTInner::StrGeneralT(Literal::AnyLiteral)),
                    ))),
                )?)
            }
            ConcrHintDecompositionInner::DecompMethodName(name) => {
                Ok(speculation_flow::get_method_type_unsafe(
                    cx,
                    env,
                    &t,
                    reason.dupe(),
                    type_util::mk_named_prop(reason.dupe(), false, Name::new(name.dupe())),
                )?)
            }
            ConcrHintDecompositionInner::DecompMethodPrivateName(name, class_stack) => {
                let old_stack = {
                    let env = cx.environment_mut();
                    std::mem::replace(&mut env.class_stack, class_stack.dupe())
                };
                let class_entries = type_env::get_class_entries(cx);
                let collector = TypeCollector::create();
                let use_t = UseT::new(UseTInner::PrivateMethodT(Box::new(PrivateMethodTData {
                    use_op: unknown_use(),
                    reason: reason.dupe(),
                    prop_reason: reason.dupe(),
                    name: name.dupe(),
                    class_bindings: class_entries.into(),
                    static_: false,
                    method_action: Box::new(MethodAction::NoMethodAction(collector.dupe())),
                })));
                let result =
                    speculation_flow::resolved_lower_flow_unsafe(cx, env, reason, (&t, &use_t));
                let env = cx.environment_mut();
                env.class_stack = old_stack;
                result?;
                Ok(collector
                    .union_opt(reason.dupe())
                    .unwrap_or_else(|| tvar_resolver::default_no_lowers(reason)))
            }
            ConcrHintDecompositionInner::DecompObjProp(name) => {
                let t = t.dupe();
                let reason2 = reason.dupe();
                let name = name.dupe();
                let skip_optional = opts.skip_optional;
                Ok(flow_typing_tvar::mk_no_wrap_where(
                    cx,
                    reason.dupe(),
                    move |cx, r, id| {
                        let tout = Tvar::new(r.dupe(), id as u32);
                        let use_t = UseT::new(UseTInner::GetPropT(Box::new(GetPropTData {
                            use_op: unknown_use(),
                            reason: reason2.dupe(),
                            id: Some(flow_common::reason::mk_id() as i32),
                            from_annot: false,
                            skip_optional,
                            propref: Box::new(type_util::mk_named_prop(
                                reason2.dupe(),
                                false,
                                Name::new(name),
                            )),
                            tout: Box::new(tout),
                            hint: hint_unavailable(),
                        })));
                        speculation_flow::resolved_lower_flow_unsafe(
                            cx,
                            env,
                            &reason2,
                            (&t, &use_t),
                        )?;
                        Ok::<(), FlowJsException>(())
                    },
                )?)
            }
            ConcrHintDecompositionInner::DecompObjComputed(comp_reason) => {
                let key_t = type_env::find_write(cx, DefLocType::ExpressionLoc, comp_reason.dupe());
                let t = t.dupe();
                let comp_reason2 = comp_reason.dupe();
                let skip_optional = opts.skip_optional;
                Ok(flow_typing_tvar::mk_no_wrap_where(
                    cx,
                    comp_reason.dupe(),
                    move |cx, r, id| {
                        let tout = Tvar::new(r.dupe(), id as u32);
                        let use_t = UseT::new(UseTInner::GetElemT(Box::new(GetElemTData {
                            use_op: unknown_use(),
                            reason: comp_reason2.dupe(),
                            id: None,
                            from_annot: true,
                            skip_optional,
                            access_iterables: false,
                            key_t,
                            tout: Box::new(tout),
                        })));
                        speculation_flow::resolved_lower_flow_unsafe(
                            cx,
                            env,
                            &comp_reason2,
                            (&t, &use_t),
                        )?;
                        Ok::<(), FlowJsException>(())
                    },
                )?)
            }
            ConcrHintDecompositionInner::DecompObjSpread => {
                let t = t.dupe();
                let reason2 = reason.dupe();
                Ok(flow_typing_tvar::mk_no_wrap_where(
                    cx,
                    reason.dupe(),
                    move |cx, r, id| {
                        let tout = Tvar::new(r.dupe(), id as u32);
                        let use_t = UseT::new(UseTInner::ObjRestT(
                            reason2.dupe(),
                            Rc::from([]),
                            Type::new(TypeInner::OpenT(tout)),
                            flow_common::reason::mk_id() as i32,
                            false,
                        ));
                        speculation_flow::resolved_lower_flow_unsafe(
                            cx,
                            env,
                            &reason2,
                            (&t, &use_t),
                        )?;
                        Ok::<(), FlowJsException>(())
                    },
                )?)
            }
            ConcrHintDecompositionInner::DecompPrivateProp(name, class_stack) => {
                let old_stack = {
                    let env = cx.environment_mut();
                    std::mem::replace(&mut env.class_stack, class_stack.clone())
                };
                let class_entries = type_env::get_class_entries(cx);
                let t2 = t.dupe();
                let reason2 = reason.dupe();
                let name2 = name.dupe();
                let result =
                    flow_typing_tvar::mk_no_wrap_where(cx, reason.dupe(), move |cx, r, id| {
                        let prop_t = Tvar::new(r.dupe(), id as u32);
                        let use_t =
                            UseT::new(UseTInner::GetPrivatePropT(Box::new(GetPrivatePropTData {
                                use_op: unknown_use(),
                                reason: reason2.dupe(),
                                name: name2,
                                class_bindings: class_entries.into(),
                                static_: false,
                                tout: Box::new(prop_t),
                            })));
                        speculation_flow::resolved_lower_flow_unsafe(
                            cx,
                            env,
                            &reason2,
                            (&t2, &use_t),
                        )?;
                        Ok::<(), FlowJsException>(())
                    });
                let env = cx.environment_mut();
                env.class_stack = old_stack;
                Ok(result?)
            }
            ConcrHintDecompositionInner::DecompSentinelRefinement(checks) => {
                let elements: Vec<_> = checks.iter().collect();
                match elements.as_slice() {
                    [] => Ok(t.dupe()),
                    [first, rest @ ..] => {
                        let concrete_types =
                            FlowJs::possible_concrete_types_for_inspection_with_env(
                                cx, env, reason, &t,
                            )?;
                        match concrete_types.as_slice() {
                            [] => Ok(t.dupe()),
                            [_single] => Ok(t.dupe()),
                            _ => {
                                let mut predicate_of_check =
                                    |prop: &FlowSmolStr,
                                     literal_check: &SentinelRefinement|
                                     -> PredicateInner {
                                        let other_t = match literal_check {
                                            SentinelRefinement::SingletonBool(b) => {
                                                Type::new(TypeInner::DefT(
                                                    reason.dupe(),
                                                    DefT::new(DefTInner::SingletonBoolT {
                                                        from_annot: true,
                                                        value: *b,
                                                    }),
                                                ))
                                            }
                                            SentinelRefinement::SingletonNum(n) => {
                                                Type::new(TypeInner::DefT(
                                                    reason.dupe(),
                                                    DefT::new(DefTInner::SingletonNumT {
                                                        from_annot: true,
                                                        value: NumberLiteral(
                                                            *n,
                                                            FlowSmolStr::new(format!("{}", n)),
                                                        ),
                                                    }),
                                                ))
                                            }
                                            SentinelRefinement::SingletonStr(s) => {
                                                Type::new(TypeInner::DefT(
                                                    reason.dupe(),
                                                    DefT::new(DefTInner::SingletonStrT {
                                                        from_annot: true,
                                                        value: s.dupe(),
                                                    }),
                                                ))
                                            }
                                            SentinelRefinement::SingletonBigInt(n) => {
                                                Type::new(TypeInner::DefT(
                                                    reason.dupe(),
                                                    DefT::new(DefTInner::SingletonBigIntT {
                                                        from_annot: true,
                                                        value: BigIntLiteral(
                                                            Some(*n),
                                                            FlowSmolStr::new(format!("{}", n)),
                                                        ),
                                                    }),
                                                ))
                                            }
                                            SentinelRefinement::Member(member_reason) => {
                                                type_env::find_write(
                                                    cx,
                                                    DefLocType::ExpressionLoc,
                                                    member_reason.dupe(),
                                                )
                                            }
                                        };
                                        PredicateInner::BinaryP(
                                            BinaryTest::SentinelProp(prop.dupe()),
                                            other_t,
                                        )
                                    };
                                let (first_prop, first_check) = first;
                                let mut pred = predicate_of_check(first_prop, first_check);
                                for (prop, check) in rest {
                                    pred = PredicateInner::AndP(
                                        Predicate::new(pred),
                                        Predicate::new(predicate_of_check(prop, check)),
                                    );
                                }
                                let predicate = Predicate::new(pred);
                                Ok(
                                    crate::predicate_kit::collect_predicate_for_filtering_with_env(
                                        cx, env, &t, &predicate,
                                    )?
                                    .union_opt(reason.dupe())
                                    .unwrap_or_else(|| tvar_resolver::default_no_lowers(reason)),
                                )
                            }
                        }
                    }
                }
            }
            ConcrHintDecompositionInner::SimplifyCallee(callee_reason) => {
                let mut simplify = |fn_t: &Type| -> Result<Type, SandboxError> {
                    simplify_callee(cx, env, callee_reason, unknown_use(), fn_t)
                };
                let simplified = simplify(&t)?;
                match simplified.deref() {
                    TypeInner::UnionT(_, rep) => {
                        let new_rep = rep.ident_map(false, |inner_t| {
                            simplify(inner_t).unwrap_or_else(|_| inner_t.dupe())
                        });
                        Ok(Type::new(TypeInner::UnionT(callee_reason.dupe(), new_rep)))
                    }
                    TypeInner::MaybeT(_, inner_t) => Ok(simplify(inner_t)?),
                    TypeInner::OptionalT { type_, .. } => Ok(simplify(type_)?),
                    _ => Ok(simplified.dupe()),
                }
            }
            ConcrHintDecompositionInner::InstantiateCallee(instantiation_hint) => Ok(
                instantiate_callee(cx, env, opts, reason, &t, instantiation_hint)?,
            ),
            ConcrHintDecompositionInner::InstantiateComponent(instantiation_hint) => Ok(
                instantiate_component(cx, env, opts, &t, instantiation_hint)?,
            ),
            ConcrHintDecompositionInner::DecompPromise => {
                let t = t.dupe();
                let reason2 = reason.dupe();
                Ok(flow_typing_tvar::mk_where(
                    cx,
                    reason.dupe(),
                    move |cx, inner_t| {
                        let promise_t = FlowJs::get_builtin_typeapp_with_env(
                            cx,
                            env,
                            &reason2,
                            None,
                            "Promise",
                            vec![inner_t.dupe()],
                        );
                        speculation_flow::resolved_lower_flow_t_unsafe(
                            cx,
                            env,
                            &reason2,
                            (&t, &promise_t),
                        )?;
                        speculation_flow::resolved_lower_flow_t_unsafe(
                            cx,
                            env,
                            &reason2,
                            (&t, inner_t),
                        )?;
                        Ok::<(), FlowJsException>(())
                    },
                )?)
            }
        }
    })
}

fn fully_resolve_final_result<'cx>(
    cx: &mut Context<'cx>,
    t: Type,
    kind: HintKind,
) -> HintEvalResult {
    if flow_js_utils::tvar_visitors::has_placeholders(cx, &t) {
        flow_typing_debug::verbose::print_if_verbose_lazy(cx, None, None, None, || {
            vec![format!(
                "Encountered placeholder type: {}",
                flow_typing_debug::dump_t(Some(3), cx, &t),
            )]
        });
        HintEvalResult::EncounteredPlaceholder(tvar_resolver::resolved_t_abortable(false, cx, t))
    } else {
        match tvar_resolver::resolved_t_abortable(false, cx, t) {
            None => HintEvalResult::DecompositionError,
            Some(resolved) => HintEvalResult::HintAvailable(resolved, kind),
        }
    }
}

fn evaluate_hint_ops<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    opts: &HintOptions,
    reason: &Reason,
    t: Type,
    kind: HintKind,
    ops: Vec<(usize, ConcrHintDecomposition)>,
) -> Result<HintEvalResult, JobError> {
    fn loop_ops<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        opts: &HintOptions,
        reason: &Reason,
        mut t: Type,
        ops: &[(usize, ConcrHintDecomposition)],
    ) -> Result<Option<Type>, JobError> {
        for (id, op) in ops {
            let result = match cx.hint_eval_cache_find_opt(*id as i32) {
                Some(result) => result,
                None => {
                    let result = type_of_hint_decomposition(cx, env, opts, op.inner(), reason, &t)?;
                    cx.add_hint_eval_cache_entry(*id as i32, result.clone());
                    result
                }
            };
            match result {
                Some(t1) => t = t1,
                None => return Ok(None),
            }
        }
        Ok(Some(t))
    }

    // We evaluate the decompositions in synthesis mode, but fully resolve the final result in
    // checking mode, so that any unresolved tvars in the midddle won't fail the evaluation, but
    // unsolved tvars in the final result will fail the evaluation.
    let hint_env = env.signature_tvar_env();
    let result = cx.run_in_hint_eval_mode(|cx| loop_ops(cx, &hint_env, opts, reason, t, &ops))?;
    Ok(match result {
        None => HintEvalResult::DecompositionError,
        Some(t) => fully_resolve_final_result(cx, t, kind),
    })
}

fn evaluate_hint_inner<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    opts: &HintOptions,
    reason: &Reason,
    hint: ConcrHint,
) -> Result<HintEvalResult, JobError> {
    if opts.expected_only {
        match &hint {
            Hint::HintPlaceholder
            | Hint::HintT(_, HintKind::BestEffortHint)
            | Hint::HintDecomp(_, _, HintKind::BestEffortHint) => {
                return Ok(HintEvalResult::DecompositionError);
            }
            _ => {}
        }
    }
    Ok(match hint {
        Hint::HintPlaceholder => HintEvalResult::HintAvailable(
            any_t::annot(flow_common::reason::mk_reason(None, ALoc::default())),
            HintKind::ExpectedTypeHint,
        ),
        Hint::HintT(t, kind) => fully_resolve_final_result(cx, t?, kind),
        Hint::HintDecomp(ops, t, kind) => {
            let ops_vec = ops.into_vec();
            evaluate_hint_ops(cx, env, opts, reason, t?, kind, ops_vec)?
        }
    })
}

fn evaluate_hints_inner<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    opts: &HintOptions,
    reason: &Reason,
    hints: Vec<ConcrHint>,
) -> Result<HintEvalResult, JobError> {
    let mut result = HintEvalResult::NoHint;
    for hint in hints {
        match evaluate_hint_inner(cx, env, opts, reason, hint)? {
            HintEvalResult::HintAvailable(t, kind) => {
                result = HintEvalResult::HintAvailable(t, kind);
                break;
            }
            r => {
                result = r;
            }
        }
    }
    Ok(result)
}

pub fn evaluate_hint<'cx>(
    cx: &mut Context<'cx>,
    expected_only: bool,
    skip_optional: Option<bool>,
    reason: &Reason,
    hint: ConcrHint,
) -> Result<HintEvalResult, JobError> {
    evaluate_hint_with_env(
        cx,
        &FlowJsEnv::entry(),
        expected_only,
        skip_optional,
        reason,
        hint,
    )
}

fn evaluate_hint_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    expected_only: bool,
    skip_optional: Option<bool>,
    reason: &Reason,
    hint: ConcrHint,
) -> Result<HintEvalResult, JobError> {
    let opts = HintOptions {
        expected_only,
        skip_optional: skip_optional.unwrap_or(false),
    };
    evaluate_hint_inner(cx, env, &opts, reason, hint)
}

pub fn evaluate_hints<'cx>(
    cx: &mut Context<'cx>,
    expected_only: bool,
    skip_optional: Option<bool>,
    reason: &Reason,
    hints: Vec<ConcrHint>,
) -> Result<HintEvalResult, JobError> {
    evaluate_hints_with_env(
        cx,
        &FlowJsEnv::entry(),
        expected_only,
        skip_optional,
        reason,
        hints,
    )
}

fn evaluate_hints_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    expected_only: bool,
    skip_optional: Option<bool>,
    reason: &Reason,
    hints: Vec<ConcrHint>,
) -> Result<HintEvalResult, JobError> {
    let opts = HintOptions {
        expected_only,
        skip_optional: skip_optional.unwrap_or(false),
    };
    evaluate_hints_inner(cx, env, &opts, reason, hints)
}
