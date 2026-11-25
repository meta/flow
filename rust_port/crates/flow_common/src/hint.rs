/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::rc::Rc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_data_structure_wrapper::vector::FlowVector;
use flow_lazy::ContextFamily;
use flow_lazy::Lazy;
use vec1::Vec1;

use crate::reason::Reason;

#[derive(Debug, Clone, PartialEq)]
pub enum HintKind {
    /// This kind of hint expects that the `typeof e <: hint` when `hint` is an available hint on `e`
    ExpectedTypeHint,
    /// This kind of hint doesn't have the same restriction as `ExpectedTypeHint`.
    /// Instead, it is designed to provide some context when there wasn't an ExpectedTypeHint available.
    /// e.g. We use the type of `a` to contextually type `b` in `a ?? b`.
    BestEffortHint,
}

// In contextual typing we often need to perform decompositions on type hints before
// we descend deeper into an expression during type checking. For example if an
// object literal `{ f: (x) => x + 1 }` is checked with a hint `{ f: (number) => number }`
// then we can check the value of property `f` with the hint `(number) => number`
// and, further, use `number` as the type of `x`.

/// The lazy parts of a hint are forced with a context of the family `CX`: `()` for hints built
/// from the AST, the checker's context family for hints mapped onto types.
#[derive(Clone)]
pub struct FunCallImplicitInstantiationHints<CX: ContextFamily, T, TArgs, Args, PropsAndChildren> {
    pub reason: Reason,
    pub return_hints: Rc<Lazy<CX, Vec<Hint<CX, T, TArgs, Args, PropsAndChildren>>>>,
    pub targs: Rc<Lazy<CX, TArgs>>,
    pub arg_list: Rc<Lazy<CX, Args>>,
    pub arg_index: i32,
    /// The number of arguments, and the index of the first spread argument.
    pub arg_count: i32,
    pub spread_index: Option<i32>,
}

#[derive(Clone)]
pub struct JsxImplicitInstantiationHints<CX: ContextFamily, T, TArgs, Args, PropsAndChildren> {
    pub jsx_reason: Reason,
    pub jsx_name: FlowSmolStr,
    pub jsx_targs: Rc<Lazy<CX, TArgs>>,
    pub jsx_props_and_children: PropsAndChildren,
    pub jsx_hints: Rc<Lazy<CX, Vec<Hint<CX, T, TArgs, Args, PropsAndChildren>>>>,
}

#[derive(Debug, Clone, Dupe, PartialEq)]
pub enum SentinelRefinement {
    SingletonNum(f64),
    SingletonBool(bool),
    SingletonStr(FlowSmolStr),
    SingletonBigInt(i64),
    Member(Reason),
}

#[derive(Debug, Clone, Dupe, PartialEq)]
pub enum PredicateKind {
    TypeGuardKind(ALoc, FlowSmolStr),
}

#[derive(Clone)]
pub struct HintDecomposition<CX: ContextFamily, T, TArgs, Args, PropsAndChildren>(
    Rc<HintDecompositionInner<CX, T, TArgs, Args, PropsAndChildren>>,
);

impl<CX: ContextFamily + Clone, T, TArgs, Args, PropsAndChildren> Dupe
    for HintDecomposition<CX, T, TArgs, Args, PropsAndChildren>
where
    T: Clone,
    TArgs: Clone,
    Args: Clone,
    PropsAndChildren: Clone,
{
}

impl<CX: ContextFamily, T, TArgs, Args, PropsAndChildren>
    HintDecomposition<CX, T, TArgs, Args, PropsAndChildren>
{
    pub fn new(inner: HintDecompositionInner<CX, T, TArgs, Args, PropsAndChildren>) -> Self {
        HintDecomposition(Rc::new(inner))
    }

    pub fn inner(&self) -> &HintDecompositionInner<CX, T, TArgs, Args, PropsAndChildren> {
        &self.0
    }

    pub fn to_debug_string(&self) -> String {
        use HintDecompositionInner::*;
        match self.inner() {
            DecompObjProp(_) => "Decomp_ObjProp".to_string(),
            DecompObjComputed(_) => "Decomp_ObjComputed".to_string(),
            DecompObjSpread => "Decomp_ObjSpread".to_string(),
            DecompPrivateProp(..) => "Decomp_PrivateProp".to_string(),
            DecompArrElement(None) => "Decomp_ArrElement (no index)".to_string(),
            DecompArrElement(Some(i)) => format!("Decomp_ArrElement ({})", i),
            DecompArrSpread(i) => format!("Decomp_ArrSpread ({})", i),
            DecompEmptyArrayElement => "Decomp_EmptyArrayElement".to_string(),
            DecompMethodName(_) => "Decomp_MethodName".to_string(),
            DecompMethodPrivateName(..) => "Decomp_MethodPrivateName".to_string(),
            DecompMethodElem => "Decomp_MethodElem".to_string(),
            DecompCallNew => "Decomp_CallNew".to_string(),
            DecompCallSuper => "Decomp_CallSuper".to_string(),
            DecompFuncParam(xs, i, pred) => {
                let xs_str = xs
                    .iter()
                    .map(|x| x.as_deref().unwrap_or("_"))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "Decomp_FuncParam ([{}], {}, {})",
                    xs_str,
                    i,
                    string_of_predicate_kind(pred)
                )
            }
            DecompFuncRest(xs, pred) => {
                let xs_str = xs
                    .iter()
                    .map(|x| x.as_deref().unwrap_or("_"))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "Decomp_FuncRest ({}, {})",
                    xs_str,
                    string_of_predicate_kind(pred)
                )
            }
            DecompFuncReturn => "Decomp_FuncReturn".to_string(),
            CompImmediateFuncCall => "Comp_ImmediateFuncCall".to_string(),
            CompMaybeT => "Comp_MaybeT".to_string(),
            DecompJsxProps => "Decomp_JsxProps".to_string(),
            DecompSentinelRefinement(_) => "Decomp_SentinelRefinement".to_string(),
            DecompAwait => "Decomp_Await".to_string(),
            SimplifyCallee(_) => "Simplify_Callee".to_string(),
            InstantiateCallee(_) => "Instantiate_Callee".to_string(),
            InstantiateComponent(_) => "Instantiate_Component".to_string(),
            DecompPromise => "Decomp_Promise".to_string(),
            DecompYield(false) => "Decomp_Yield".to_string(),
            DecompYield(true) => "Decomp_Yield (async)".to_string(),
            DecompGeneratorReturn(false) => "Decomp_GeneratorReturn".to_string(),
            DecompGeneratorReturn(true) => "Decomp_GeneratorReturn (async)".to_string(),
        }
    }
}

impl<T, TArgs, Args, PropsAndChildren> HintDecomposition<(), T, TArgs, Args, PropsAndChildren> {
    pub fn map<CX: ContextFamily, T2, TArgs2, Args2, PropsAndChildren2>(
        &self,
        cx: &mut CX::Context<'_>,
        map_base_hint: &Rc<impl for<'c> Fn(&mut CX::Context<'c>, T) -> T2 + 'static>,
        map_targs: &Rc<impl for<'c> Fn(&mut CX::Context<'c>, TArgs) -> TArgs2 + 'static>,
        map_arg_list: &Rc<impl for<'c> Fn(&mut CX::Context<'c>, Args) -> Args2 + 'static>,
        map_jsx: &Rc<
            impl for<'c> Fn(
                &mut CX::Context<'c>,
                Reason,
                FlowSmolStr,
                PropsAndChildren,
            ) -> PropsAndChildren2
            + 'static,
        >,
    ) -> HintDecomposition<CX, T2, TArgs2, Args2, PropsAndChildren2>
    where
        T: Clone + 'static,
        TArgs: Clone + 'static,
        Args: Clone + 'static,
        PropsAndChildren: Clone + 'static,
        T2: Clone + 'static,
        TArgs2: Clone + 'static,
        Args2: Clone + 'static,
        PropsAndChildren2: Clone + 'static,
    {
        use HintDecompositionInner::*;
        let inner = match self.inner() {
            DecompObjProp(prop) => DecompObjProp(prop.dupe()),
            DecompObjComputed(l) => DecompObjComputed(l.dupe()),
            DecompObjSpread => DecompObjSpread,
            DecompPrivateProp(prop, class_stack) => {
                DecompPrivateProp(prop.dupe(), class_stack.clone())
            }
            DecompArrElement(o) => DecompArrElement(*o),
            DecompArrSpread(o) => DecompArrSpread(*o),
            DecompEmptyArrayElement => DecompEmptyArrayElement,
            DecompAwait => DecompAwait,
            DecompMethodName(name) => DecompMethodName(name.dupe()),
            DecompMethodPrivateName(name, class_stack) => {
                DecompMethodPrivateName(name.dupe(), class_stack.clone())
            }
            DecompMethodElem => DecompMethodElem,
            DecompCallNew => DecompCallNew,
            DecompCallSuper => DecompCallSuper,
            DecompFuncParam(i, s, p) => DecompFuncParam(i.clone(), *s, p.dupe()),
            DecompFuncRest(i, p) => DecompFuncRest(i.clone(), p.dupe()),
            DecompFuncReturn => DecompFuncReturn,
            CompImmediateFuncCall => CompImmediateFuncCall,
            CompMaybeT => CompMaybeT,
            DecompJsxProps => DecompJsxProps,
            DecompSentinelRefinement(checks) => DecompSentinelRefinement(checks.clone()),
            SimplifyCallee(r) => SimplifyCallee(r.dupe()),
            InstantiateCallee(hints) => {
                let FunCallImplicitInstantiationHints {
                    reason,
                    return_hints,
                    targs,
                    arg_list,
                    arg_index,
                    arg_count,
                    spread_index,
                } = hints;

                // IMPORTANT: These must remain LAZY — only map when accessed.
                let return_hints_mapped = Rc::new(Lazy::<CX, _>::new({
                    let return_hints = return_hints.dupe();
                    let map_base_hint = map_base_hint.dupe();
                    let map_targs = map_targs.dupe();
                    let map_arg_list = map_arg_list.dupe();
                    let map_jsx = map_jsx.dupe();
                    move |cx: &mut CX::Context<'_>| {
                        return_hints
                            .get_forced(&mut ())
                            .iter()
                            .map(|h| {
                                h.clone().map::<CX, _, _, _, _>(
                                    cx,
                                    &map_base_hint,
                                    &map_targs,
                                    &map_arg_list,
                                    &map_jsx,
                                )
                            })
                            .collect()
                    }
                }));
                let targs_mapped = Rc::new(Lazy::<CX, _>::new({
                    let targs = targs.dupe();
                    let map_targs = map_targs.dupe();
                    move |cx: &mut CX::Context<'_>| {
                        (map_targs.as_ref())(cx, targs.get_forced(&mut ()).clone())
                    }
                }));
                let arg_list_mapped = Rc::new(Lazy::<CX, _>::new({
                    let arg_list = arg_list.dupe();
                    let map_arg_list = map_arg_list.dupe();
                    move |cx: &mut CX::Context<'_>| {
                        (map_arg_list.as_ref())(cx, arg_list.get_forced(&mut ()).clone())
                    }
                }));

                InstantiateCallee(FunCallImplicitInstantiationHints {
                    reason: reason.dupe(),
                    return_hints: return_hints_mapped,
                    targs: targs_mapped,
                    arg_list: arg_list_mapped,
                    arg_index: *arg_index,
                    arg_count: *arg_count,
                    spread_index: *spread_index,
                })
            }
            InstantiateComponent(hints) => {
                let JsxImplicitInstantiationHints {
                    jsx_reason,
                    jsx_name,
                    jsx_targs,
                    jsx_props_and_children,
                    jsx_hints,
                } = hints;

                let jsx_targs_mapped = Rc::new(Lazy::<CX, _>::new({
                    let jsx_targs = jsx_targs.dupe();
                    let map_targs = map_targs.dupe();
                    move |cx: &mut CX::Context<'_>| {
                        (map_targs.as_ref())(cx, jsx_targs.get_forced(&mut ()).clone())
                    }
                }));

                let jsx_props_and_children_mapped = (map_jsx.as_ref())(
                    cx,
                    jsx_reason.dupe(),
                    jsx_name.dupe(),
                    jsx_props_and_children.clone(),
                );

                let jsx_hints_mapped = Rc::new(Lazy::<CX, _>::new({
                    let jsx_hints = jsx_hints.dupe();
                    let map_base_hint = map_base_hint.dupe();
                    let map_targs = map_targs.dupe();
                    let map_arg_list = map_arg_list.dupe();
                    let map_jsx = map_jsx.dupe();
                    move |cx: &mut CX::Context<'_>| {
                        jsx_hints
                            .get_forced(&mut ())
                            .iter()
                            .map(|h| {
                                h.clone().map::<CX, _, _, _, _>(
                                    cx,
                                    &map_base_hint,
                                    &map_targs,
                                    &map_arg_list,
                                    &map_jsx,
                                )
                            })
                            .collect()
                    }
                }));

                InstantiateComponent(JsxImplicitInstantiationHints {
                    jsx_reason: jsx_reason.dupe(),
                    jsx_name: jsx_name.dupe(),
                    jsx_targs: jsx_targs_mapped,
                    jsx_props_and_children: jsx_props_and_children_mapped,
                    jsx_hints: jsx_hints_mapped,
                })
            }
            DecompPromise => DecompPromise,
            DecompYield(async_) => DecompYield(*async_),
            DecompGeneratorReturn(async_) => DecompGeneratorReturn(*async_),
        };
        HintDecomposition::new(inner)
    }
}

pub enum HintDecompositionInner<CX: ContextFamily, T, TArgs, Args, PropsAndChildren> {
    /// Hint on `{ f: e }` becomes hint on `e`
    DecompObjProp(FlowSmolStr),
    /// Hint on `{ [k]: e }` becomes hint on `e`
    DecompObjComputed(Reason),
    /// Hint on `{ ...e }` becomes hint on `e`
    DecompObjSpread,
    /// Type of `obj` becomes type of `obj.#privateName`
    DecompPrivateProp(FlowSmolStr, FlowVector<ALoc>),
    /// Hint on array literal `[e]` becomes hint on `e`
    DecompArrElement(Option<i32>),
    /// Hint on array literal `[...e]` becomes hint on `e`
    DecompArrSpread(i32),
    /// Hint on empty array literal `[]` becomes hint on its element type
    DecompEmptyArrayElement,
    /// Type of `await e` becomes hint on `e`
    DecompAwait,
    /// Type of `o` in `o.m(..)` becomes the type of `o.m`
    DecompMethodName(FlowSmolStr),
    /// Type of `o` in `o.#m(..)` becomes the type of `o.m`
    DecompMethodPrivateName(FlowSmolStr, FlowVector<ALoc>),
    /// Type of `o` in `o[e](..)` becomes the type of `o[e]`
    DecompMethodElem,
    /// Type of `C` in `new C(..)` becomes the type of the constructor of C
    DecompCallNew,
    /// Type of the super-class becomes the type of the super constructor
    DecompCallSuper,
    /// Type of function becomes hint on the i-th argument
    DecompFuncParam(Vec<Option<FlowSmolStr>>, i32, Option<PredicateKind>),
    /// Type of function becomes hint on rest argument
    DecompFuncRest(Vec<Option<FlowSmolStr>>, Option<PredicateKind>),
    /// Type of function becomes hint on return
    DecompFuncReturn,
    /// Hint on call `f()` becomes hint on `f`. This is only meant to be used for the
    /// case of immediate function call `(function() {})()`.
    CompImmediateFuncCall,
    /// Given t, returns `?t`.
    /// This is helpful for constructing a hint for `a` in `a ?? b` when the hint on `a ?? b` is t.
    CompMaybeT,
    /// Type of C in `<C [props]/>` becomes hint on `props`
    DecompJsxProps,
    DecompSentinelRefinement(BTreeMap<FlowSmolStr, SentinelRefinement>),
    /// Type of a callee is concretized so that we can inspect its structure
    SimplifyCallee(Reason),
    /// Type of f in f(...) is instantiated with arguments and return hint.
    /// Returns f if the type of f is not polymorphic.
    InstantiateCallee(FunCallImplicitInstantiationHints<CX, T, TArgs, Args, PropsAndChildren>),
    /// Type of Comp in <Comp ... /> is instantiated with props and children.
    /// Returns Comp if the type of Comp is not polymorphic.
    InstantiateComponent(JsxImplicitInstantiationHints<CX, T, TArgs, Args, PropsAndChildren>),
    /// T of Promise<T> becomes hint on return in async scope
    DecompPromise,
    /// T of Iterable<T, R, N> (AsyncIterable<T, R, N> if async) becomes hint on yield argument
    DecompYield(bool),
    /// R of Iterable<T, R, N> (AsyncIterable<T, R, N> if async) becomes hint on return in a
    /// generator
    DecompGeneratorReturn(bool),
}

#[derive(Clone)]
pub enum Hint<CX: ContextFamily, T, TArgs, Args, PropsAndChildren> {
    HintT(T, HintKind),
    HintDecomp(
        Vec1<(
            usize,
            HintDecomposition<CX, T, TArgs, Args, PropsAndChildren>,
        )>,
        T,
        HintKind,
    ),
    /// The hint placeholder used in env_resolution to pass to expression type checkers.
    /// It will eventually be removed once we move all hint decomposition logic into env_resolution.
    HintPlaceholder,
}

impl<CX: ContextFamily + Clone, T, TArgs, Args, PropsAndChildren>
    Hint<CX, T, TArgs, Args, PropsAndChildren>
{
    pub fn decompose(
        decomp: HintDecomposition<CX, T, TArgs, Args, PropsAndChildren>,
        hints: Vec<Self>,
    ) -> Vec<Self>
    where
        T: Clone,
        TArgs: Clone,
        Args: Clone,
        PropsAndChildren: Clone,
    {
        hints
            .into_iter()
            .map(|hint| match hint {
                Hint::HintT(t, kind) => Hint::HintDecomp(
                    Vec1::try_from_vec(vec![(crate::reason::mk_id(), decomp.dupe())]).unwrap(),
                    t,
                    kind,
                ),
                Hint::HintDecomp(mut decomps, t, kind) => {
                    decomps.push((crate::reason::mk_id(), decomp.dupe()));
                    Hint::HintDecomp(decomps, t, kind)
                }
                Hint::HintPlaceholder => Hint::HintPlaceholder,
            })
            .collect()
    }

    pub fn to_debug_string(&self, on_hint: impl Fn(&T) -> String) -> String
    where
        T: Clone + PartialEq,
        TArgs: Clone + PartialEq,
        Args: Clone + PartialEq,
        PropsAndChildren: Clone + PartialEq,
    {
        match self {
            Hint::HintT(t, _) => format!("Hint_t ({})", on_hint(t)),
            Hint::HintDecomp(ops, t, _) => {
                let ops_str = ops
                    .iter()
                    .map(|(_, op)| op.to_debug_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("Hint_Decomp ({})({})", ops_str, on_hint(t))
            }
            Hint::HintPlaceholder => "Hint_Placeholder".to_string(),
        }
    }

    pub fn string_of_hints(on_hint: impl Fn(&T) -> String, hints: &[Self]) -> String
    where
        T: Clone + PartialEq,
        TArgs: Clone + PartialEq,
        Args: Clone + PartialEq,
        PropsAndChildren: Clone + PartialEq,
    {
        let hints_str = hints
            .iter()
            .map(|hint| hint.to_debug_string(&on_hint))
            .collect::<Vec<_>>()
            .join(",");
        format!("[{}]", hints_str)
    }
}

impl<T, TArgs, Args, PropsAndChildren> Hint<(), T, TArgs, Args, PropsAndChildren> {
    pub fn map<CX: ContextFamily, T2, TArgs2, Args2, PropsAndChildren2>(
        self,
        cx: &mut CX::Context<'_>,
        map_base_hint: &Rc<impl for<'c> Fn(&mut CX::Context<'c>, T) -> T2 + 'static>,
        map_targs: &Rc<impl for<'c> Fn(&mut CX::Context<'c>, TArgs) -> TArgs2 + 'static>,
        map_arg_list: &Rc<impl for<'c> Fn(&mut CX::Context<'c>, Args) -> Args2 + 'static>,
        map_jsx: &Rc<
            impl for<'c> Fn(
                &mut CX::Context<'c>,
                Reason,
                FlowSmolStr,
                PropsAndChildren,
            ) -> PropsAndChildren2
            + 'static,
        >,
    ) -> Hint<CX, T2, TArgs2, Args2, PropsAndChildren2>
    where
        T: Clone + 'static,
        TArgs: Clone + 'static,
        Args: Clone + 'static,
        PropsAndChildren: Clone + 'static,
        T2: Clone + 'static,
        TArgs2: Clone + 'static,
        Args2: Clone + 'static,
        PropsAndChildren2: Clone + 'static,
    {
        match self {
            Hint::HintT(t, kind) => Hint::HintT((map_base_hint.as_ref())(cx, t), kind),
            Hint::HintDecomp(ops, t, kind) => {
                let mapped_ops = ops
                    .into_iter()
                    .map(|(i, op)| {
                        (
                            i,
                            op.map::<CX, _, _, _, _>(
                                cx,
                                map_base_hint,
                                map_targs,
                                map_arg_list,
                                map_jsx,
                            ),
                        )
                    })
                    .collect::<Vec<_>>();
                Hint::HintDecomp(
                    Vec1::try_from_vec(mapped_ops).unwrap(),
                    (map_base_hint.as_ref())(cx, t),
                    kind,
                )
            }
            Hint::HintPlaceholder => Hint::HintPlaceholder,
        }
    }
}

fn string_of_predicate_kind(pred: &Option<PredicateKind>) -> &'static str {
    match pred {
        None => "no pred",
        Some(PredicateKind::TypeGuardKind(..)) => "type guard kind",
    }
}
