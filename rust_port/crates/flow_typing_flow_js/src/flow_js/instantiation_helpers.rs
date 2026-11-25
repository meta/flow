/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::rc::Rc;

use flow_typing_flow_common::instantiation_utils::implicit_type_argument;
use flow_typing_flow_common::instantiation_utils::type_app_expansion;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_flow_js_env::type_app_expansion::Bound;
use flow_typing_implicit_instantiation_check::ImplicitInstantiationCheck;
use flow_typing_type::type_::SpecializeTData;

use super::helpers::*;
use super::*;

impl flow_js_utils::InstantiationHelper for FlowJs {
    fn reposition<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: Option<DepthTrace>,
        loc: ALoc,
        t: Type,
    ) -> Result<Type, FlowJsException> {
        helpers::reposition(cx, env, trace, loc, None, None, t)
    }

    fn is_subtype<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: UseOp,
        t1: Type,
        t2: Type,
    ) -> Result<(), FlowJsException> {
        rec_flow_t(cx, env, trace, use_op, (&t1, &t2))
    }

    fn unify<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: UseOp,
        t1: Type,
        t2: Type,
    ) -> Result<(), FlowJsException> {
        rec_unify(
            cx,
            env,
            trace,
            use_op,
            UnifyCause::Uncategorized,
            Some(true),
            &t1,
            &t2,
        )
    }

    fn mk_targ<'cx>(
        cx: &mut Context<'cx>,
        typeparam: &TypeParam,
        reason_op: &Reason,
        reason_tapp: &Reason,
    ) -> Type {
        implicit_type_argument::mk_targ(cx, typeparam, reason_op, reason_tapp)
    }
}

/// Instantiate a polymorphic definition given tparam instantiations in a Call or New expression.  
pub(super) fn instantiate_with_targs_with_soln<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    reason_op: &Reason,
    reason_tapp: &Reason,
    poly_t: (ALoc, Vec1<TypeParam>, Type),
    targs: Vec<Targ>,
) -> Result<(Type, Vec<(Type, SubstName)>), FlowJsException> {
    let (tparams_loc, xs, t) = poly_t;
    let ts: Vec<Type> = targs
        .into_iter()
        .map(|targ| match targ {
            Targ::ExplicitArg(t) => t,
            Targ::ImplicitArg(_) => {
                panic!(
                    "targs containing ImplicitArg should be handled by ImplicitInstantiationKit instead."
                );
            }
        })
        .collect();
    FlowJs::instantiate_poly_with_targs_with_env(
        cx,
        env,
        trace,
        use_op,
        reason_op,
        reason_tapp,
        None,
        (tparams_loc, xs, t),
        ts,
    )
}

pub(super) fn instantiate_with_targs<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    reason_op: &Reason,
    reason_tapp: &Reason,
    poly_t: (ALoc, Vec1<TypeParam>, Type),
    targs: Vec<Targ>,
) -> Result<Type, FlowJsException> {
    let (t, _) = instantiate_with_targs_with_soln(
        cx,
        env,
        trace,
        use_op,
        reason_op,
        reason_tapp,
        poly_t,
        targs,
    )?;
    Ok(t)
}

pub(super) fn instantiate_poly_call_or_new_with_soln<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    lparts: (Reason, ALoc, Vec1<TypeParam>, Type),
    uparts: (UseOp, Reason, Option<Rc<[Targ]>>, LazyHintT<Cx>),
    check: impl FnOnce() -> ImplicitInstantiationCheck,
) -> Result<(Type, Vec<(Type, SubstName)>), FlowJsException> {
    let (reason_tapp, tparams_loc, xs, t) = lparts;
    let (use_op, reason_op, targs, return_hint) = uparts;
    match type_util::all_explicit_targs(targs.as_deref()) {
        Some(targs) => instantiate_with_targs_with_soln(
            cx,
            env,
            trace,
            use_op,
            &reason_op,
            &reason_tapp,
            (tparams_loc, xs, t),
            targs,
        ),
        None => {
            let check = check();
            let result = crate::implicit_instantiation::kit::run_call(
                cx,
                env,
                &check,
                &return_hint,
                trace,
                use_op,
                &reason_op,
                &reason_tapp,
            )?;
            Ok(result)
        }
    }
}

pub(super) fn instantiate_poly_call_or_new<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    lparts: (Reason, ALoc, Vec1<TypeParam>, Type),
    uparts: (UseOp, Reason, Option<Rc<[Targ]>>, LazyHintT<Cx>),
    check: impl FnOnce() -> ImplicitInstantiationCheck,
) -> Result<Type, FlowJsException> {
    let (reason_tapp, tparams_loc, xs, t) = lparts;
    let (use_op, reason_op, targs, return_hint) = uparts;
    match type_util::all_explicit_targs(targs.as_deref()) {
        Some(targs) => instantiate_with_targs(
            cx,
            env,
            trace,
            use_op,
            &reason_op,
            &reason_tapp,
            (tparams_loc, xs, t),
            targs,
        ),
        None => {
            let check = check();
            let (t, _) = crate::implicit_instantiation::kit::run_call(
                cx,
                env,
                &check,
                &return_hint,
                trace,
                use_op,
                &reason_op,
                &reason_tapp,
            )?;
            Ok(t)
        }
    }
}

/// Instantiate a polymorphic definition with stated bound or 'any' for args  
/// Needed only for `instanceof` refis and React.PropTypes.instanceOf types  
pub(super) fn instantiate_poly_default_args<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    reason_op: &Reason,
    reason_tapp: &Reason,
    tparams_loc: ALoc,
    xs: Vec1<TypeParam>,
    t: Type,
) -> Result<Type, FlowJsException> {
    // Remember: other_bound might refer to other type params
    let mut ts: Vec<Type> = Vec::new();
    for _ in xs.iter() {
        let t = unsoundness::why(UnsoundnessKind::InstanceOfRefinement, reason_op.dupe());
        ts.push(t);
    }
    let (t, _) = FlowJs::instantiate_poly_with_targs_with_env(
        cx,
        env,
        trace,
        use_op,
        reason_op,
        reason_tapp,
        None,
        (tparams_loc, xs, t),
        ts,
    )?;
    Ok(t)
}

// Specialize This in a class. Eventually this causes substitution
pub(super) fn instantiate_this_class<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    reason_op: &Reason,
    reason_tapp: &Reason,
    c: &Type,
    ts: Option<Rc<[Type]>>,
    this: &Type,
    k: &Cont<Cx>,
) -> Result<(), FlowJsException> {
    let side = match k {
        Cont::Upper(_) | Cont::Collect(_) => Bound::Lower,
        Cont::Lower(..) => Bound::Upper,
    };
    let Some(env) =
        type_app_expansion::push_unless_loop(cx, env, side, c, ts.as_deref().unwrap_or_default())
    else {
        return Ok(());
    };
    let this_use = UseT::new(UseTInner::ThisSpecializeT(
        reason_tapp.dupe(),
        this.dupe(),
        Box::new(k.clone()),
    ));
    let use_t = match ts {
        None => this_use,
        Some(ts) => UseT::new(UseTInner::SpecializeT(Box::new(SpecializeTData {
            use_op: unknown_use(),
            reason: reason_op.dupe(),
            reason2: reason_tapp.dupe(),
            targs: Some(ts),
            upper: Some(Box::new(Cont::Upper(Box::new(this_use)))),
            from_value: true,
            // The code being checked instantiates the class.
            strictness_kind: cx.type_strictness_kind(),
        }))),
    };
    rec_flow(cx, &env, trace, (c, &use_t))
}
