/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::ops::Deref;
use std::rc::Rc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_common::alpha_rename;
use flow_common::enclosing_context::EnclosingContext;
use flow_common::polarity::Polarity;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::reason::mk_id;
use flow_common::subst_name;
use flow_common::subst_name::SubstName;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_lazy::ContextFamily;
use flow_typing_context::Context;
use flow_typing_context::Cx;
use flow_typing_context::TypingMode;
use flow_typing_errors::error_message::EImplicitInstantiationUnderconstrainedErrorData;
use flow_typing_errors::error_message::ETooFewTypeArgsData;
use flow_typing_errors::error_message::ETooManyTypeArgsData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::InternalError;
use flow_typing_errors::intermediate_error_types::ImplicitInstantiationReferenceData;
use flow_typing_flow_common::concrete_type_eq;
use flow_typing_flow_common::flow_cache;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_common::instantiation_utils;
use flow_typing_flow_common::obj_type;
use flow_typing_flow_common::type_subst;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_implicit_instantiation_check::ImplicitInstantiationCheck;
use flow_typing_implicit_instantiation_check::Operation;
use flow_typing_type::type_::AnySource;
use flow_typing_type::type_::ArrRestTData;
use flow_typing_type::type_::ArrType;
use flow_typing_type::type_::ArrayATData;
use flow_typing_type::type_::CallAction;
use flow_typing_type::type_::CallArg;
use flow_typing_type::type_::CallArgInner;
use flow_typing_type::type_::CallMData;
use flow_typing_type::type_::CallTData;
use flow_typing_type::type_::ChainMData;
use flow_typing_type::type_::ComponentKind;
use flow_typing_type::type_::ConcretizationKind;
use flow_typing_type::type_::ConcretizeTData;
use flow_typing_type::type_::ConstructorTData;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::DepthTrace;
use flow_typing_type::type_::Destructor;
use flow_typing_type::type_::DestructorConditionalTypeData;
use flow_typing_type::type_::DestructorMappedTypeData;
use flow_typing_type::type_::DestructorSpreadTupleTypeData;
use flow_typing_type::type_::DestructorSpreadTypeData;
use flow_typing_type::type_::EvalTypeDestructorTData;
use flow_typing_type::type_::FieldData;
use flow_typing_type::type_::FunParam;
use flow_typing_type::type_::FunRestParam;
use flow_typing_type::type_::FunType;
use flow_typing_type::type_::FuncallType;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::GetEnumKind;
use flow_typing_type::type_::GetEnumTData;
use flow_typing_type::type_::HintEvalResult;
use flow_typing_type::type_::ImplicitInstantiationReferenceKind;
use flow_typing_type::type_::ImplicitInstantiationTvarData;
use flow_typing_type::type_::LazyHintT;
use flow_typing_type::type_::MethodAction;
use flow_typing_type::type_::MethodCallType;
use flow_typing_type::type_::MethodTData;
use flow_typing_type::type_::NominalType;
use flow_typing_type::type_::NominalTypeInner;
use flow_typing_type::type_::ObjKind;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::Property;
use flow_typing_type::type_::PropertyInner;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::ReactAbstractComponentTData;
use flow_typing_type::type_::ReactKitTData;
use flow_typing_type::type_::ReposUseTData;
use flow_typing_type::type_::ResolveSpreadTData;
use flow_typing_type::type_::ResolveUnionTData;
use flow_typing_type::type_::ResolvedParam;
use flow_typing_type::type_::ResolvedSpreadArgData;
use flow_typing_type::type_::SpreadResolve;
use flow_typing_type::type_::Targ;
use flow_typing_type::type_::TupleATData;
use flow_typing_type::type_::TupleView;
use flow_typing_type::type_::Tvar;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeDestructorT;
use flow_typing_type::type_::TypeDestructorTInner;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::TypeParam;
use flow_typing_type::type_::TypeParamInner;
use flow_typing_type::type_::UnifyCause;
use flow_typing_type::type_::UnresolvedArgData;
use flow_typing_type::type_::UnresolvedParam;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::UseT;
use flow_typing_type::type_::UseTInner;
use flow_typing_type::type_::VirtualFrameUseOp;
use flow_typing_type::type_::any_t;
use flow_typing_type::type_::constraint;
use flow_typing_type::type_::empty_t;
use flow_typing_type::type_::eval;
use flow_typing_type::type_::exports;
use flow_typing_type::type_::hint_unavailable;
use flow_typing_type::type_::implicit_instantiation_reference_kind;
use flow_typing_type::type_::mixed_t;
use flow_typing_type::type_::nominal;
use flow_typing_type::type_::object;
use flow_typing_type::type_::object::ObjectToolObjectMapData;
use flow_typing_type::type_::object::ObjectToolReactConfigData;
use flow_typing_type::type_::open_tvar;
use flow_typing_type::type_::properties;
use flow_typing_type::type_::react;
use flow_typing_type::type_::string_of_use_ctor;
use flow_typing_type::type_::type_collector::TypeCollector;
use flow_typing_type::type_::union_rep;
use flow_typing_type::type_::unknown_use;
use flow_typing_type::type_::unsoundness;
use flow_typing_type::type_::void;
use flow_typing_type::type_util;
use flow_typing_visitors::type_mapper;
use flow_typing_visitors::type_mapper::TypeMapper;
use flow_typing_visitors::type_visitor;
use flow_typing_visitors::type_visitor::TypeVisitor;
use vec1::Vec1;

use crate::flow_js::FlowJs;
use crate::marked::Marked;
use crate::natural_inference;
use crate::speculation_kit;
use crate::tvar_resolver;

fn union_flatten_list(ts: impl IntoIterator<Item = Type>) -> Vec<Type> {
    ts.into_iter().flat_map(union_flatten).collect()
}

fn union_flatten(t: Type) -> Vec<Type> {
    match t.deref() {
        TypeInner::UnionT(_, rep) => union_flatten_list(rep.members_iter().duped()),
        _ => vec![t],
    }
}

// Makes sure the solution does not "leak" a potentially overly precise type to
// the output of a generic call, when this precision is not necessary for checking
// the call. Condider for example
//
// ```js
// declare function useState<S>(init: S): [S, (S)=>void];
// const [st, setSt] = useState(42);
// ```
//
// We would like the type of `st` and `setSt` to use the general form of the type
// for the initial state `42`, i.e. number (resp. `(number)=>void`), instead of
// the rather impractical `42` (resp. `(42)=>void`).
pub(crate) fn generalize_singletons<'cx>(
    cx: &mut Context<'cx>,
    call_loc: &flow_aloc::ALoc,
    has_syntactic_hint: bool,
    t: Type,
) -> Type {
    let needs_precise = has_syntactic_hint
        || cx.get_enclosing_context_for_call(call_loc) == Some(EnclosingContext::IndexContext);
    if needs_precise {
        t
    } else {
        let singleton_action = |cx: &Context<'_>, loc: &flow_aloc::ALoc| {
            let checked = cx.is_primitive_literal_checked(loc);
            if checked {
                natural_inference::SingletonAction::KeepAsIs
            } else {
                natural_inference::SingletonAction::DoNotKeep
            }
        };
        natural_inference::convert_implicit_instantiation_literal_type(cx, singleton_action, t)
    }
}

#[derive(Debug, Clone)]
pub struct GeneralizedTarg {
    pub tparam: TypeParam,
    pub inferred: Type,
    pub generalized: Type,
}

impl GeneralizedTarg {
    pub fn to_type(&self) -> Type {
        self.generalized.dupe()
    }
}

/// A type argument of a solve: an explicit one, or the tvar the solve infers it with.
struct SolveTarg {
    tparam: TypeParam,
    t: Type,
    inferred: bool,
}

fn get_t<'cx>(cx: &mut Context<'cx>, t: &Type) -> Type {
    let no_lowers = |_cx: &mut Context, r: &Reason| unsoundness::merged_any(r.dupe());
    match type_util::constraint_node_id(t) {
        Some(node) => {
            flow_js_utils::merge_tvar(cx, false, no_lowers, type_util::reason_of_t(t), node.id())
        }
        None => t.dupe(),
    }
}

// Sorting of the upper bounds is mostly done for compatibility with the version
// before EvalTypeDestructorT was a use_t.
fn sort_upper_bounds_for_merging<CX: ContextFamily>(
    xs: Vec<(UseT<CX>, DepthTrace)>,
) -> Vec<(UseT<CX>, DepthTrace)> {
    let (use_t, rest_t): (Vec<_>, Vec<_>) = xs.into_iter().partition(|(u, _): &(UseT<CX>, _)| {
        matches!(
            u.deref(),
            UseTInner::UseT(_, _) | UseTInner::EvalTypeDestructorT { .. }
        )
    });
    use_t.into_iter().chain(rest_t).collect()
}

fn speculative_subtyping_succeeds<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    l: &Type,
    u: &Type,
) -> Result<bool, FlowJsException> {
    match speculation_kit::try_singleton_throw_on_failure(
        cx,
        env,
        trace,
        l.dupe(),
        UseT::new(UseTInner::UseT(use_op, u.dupe())),
    ) {
        Ok(()) => Ok(true),
        Err(FlowJsException::SpeculationSingletonError) => Ok(false),
        Err(e) => Err(e),
    }
}

struct ImplicitInstantiationVisitor<'a> {
    tparams_map: &'a BTreeMap<SubstName, TypeParam>,
}

impl TypeVisitor<(Marked<SubstName>, FlowOrdSet<SubstName>)> for ImplicitInstantiationVisitor<'_> {
    fn type_<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: (Marked<SubstName>, FlowOrdSet<SubstName>),
        t: &Type,
    ) -> (Marked<SubstName>, FlowOrdSet<SubstName>) {
        let (mut marked, tparam_names) = acc;
        match t.deref() {
            TypeInner::GenericT(box GenericTData { name: s, .. }) => {
                if tparam_names.contains(s) {
                    match marked.add(s.dupe(), pole) {
                        None => (marked, tparam_names),
                        Some(_) => match self.tparams_map.get(s) {
                            None => (marked, tparam_names),
                            Some(tparam) => {
                                let bound = tparam.bound.dupe();
                                self.type_(cx, pole, (marked, tparam_names), &bound)
                            }
                        },
                    }
                } else {
                    type_visitor::type_default(self, cx, pole, (marked, tparam_names), t)
                }
            }
            // We remove any tparam names from the map when entering a PolyT to avoid naming conflicts.
            TypeInner::DefT(_, def_t) => {
                if let DefTInner::PolyT(box PolyTData {
                    tparams, t_out: t, ..
                }) = def_t.deref()
                {
                    let mut tparam_names_prime = tparam_names.clone();
                    for tp in tparams.iter() {
                        tparam_names_prime.remove(&tp.name);
                    }
                    let (marked, _) = self.type_(cx, pole, (marked, tparam_names_prime), t);
                    // TODO(jmbrown): Handle defaults on type parameters
                    (marked, tparam_names)
                } else {
                    type_visitor::type_default(self, cx, pole, (marked, tparam_names), t)
                }
            }
            TypeInner::TypeAppT(box TypeAppTData { type_, targs, .. }) => {
                self.typeapp(targs, cx, pole, (marked, tparam_names), type_)
            }
            // ThisTypeAppT is created from a new expression, which cannot be used as an annotation,
            // so we do not special case it like we do with TypeAppT
            _ => type_visitor::type_default(self, cx, pole, (marked, tparam_names), t),
        }
    }
}

impl ImplicitInstantiationVisitor<'_> {
    fn typeapp<'cx>(
        &mut self,
        targs: &[Type],
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: (Marked<SubstName>, FlowOrdSet<SubstName>),
        type_: &Type,
    ) -> (Marked<SubstName>, FlowOrdSet<SubstName>) {
        fn loop_<'cx>(
            visitor: &mut ImplicitInstantiationVisitor,
            cx: &mut Context<'cx>,
            pole: Polarity,
            mut acc: (Marked<SubstName>, FlowOrdSet<SubstName>),
            tparams: Option<&[TypeParam]>,
            targs: &[Type],
        ) -> (Marked<SubstName>, FlowOrdSet<SubstName>) {
            match (tparams, targs) {
                // Any arity erors are already handled in Flow_js
                (_, []) => acc,
                (Some([]), _) => acc,
                (None, [targ, rest @ ..]) => {
                    // In the absence of tparams we will just visit the args with a neutral polarity.
                    let param_polarity = Polarity::Neutral;
                    acc = visitor.type_(cx, param_polarity, acc, targ);
                    loop_(visitor, cx, pole, acc, None, rest)
                }
                (Some([tparam, rest_tparams @ ..]), [targ, rest_targs @ ..]) => {
                    let param_polarity = Polarity::mult(pole, tparam.polarity);
                    acc = visitor.type_(cx, param_polarity, acc, targ);
                    loop_(visitor, cx, pole, acc, Some(rest_tparams), rest_targs)
                }
            }
        }

        let resolved = get_t(cx, type_);
        match resolved.deref() {
            TypeInner::AnnotT(_, t, _) => self.typeapp(targs, cx, pole, acc, t),
            TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::PolyT(_)) => {
                if let DefTInner::PolyT(box PolyTData { tparams, .. }) = def_t.deref() {
                    loop_(self, cx, pole, acc, Some(tparams), targs)
                } else {
                    unreachable!()
                }
            }
            TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::EmptyT) => {
                loop_(self, cx, pole, acc, None, targs)
            }
            TypeInner::AnyT(..) => loop_(self, cx, pole, acc, None, targs),
            _ => loop_(self, cx, pole, acc, None, targs),
        }
    }
}

enum UseTResult {
    UpperEmpty,
    UpperPlaceholder(Type),
    UpperNonT(UseT<Cx>),
    UpperT(Type),
}

fn t_of_use_t<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    seen: &mut BTreeSet<i32>,
    tvar: &Type,
    u: &UseT<Cx>,
) -> Result<UseTResult, FlowJsException> {
    fn use_t_result_of_t_option(
        opt: Result<Option<Type>, FlowJsException>,
    ) -> Result<UseTResult, FlowJsException> {
        match opt? {
            Some(t) => Ok(UseTResult::UpperT(t)),
            None => Ok(UseTResult::UpperEmpty),
        }
    }
    fn merge_lower_or_upper_bounds<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        seen: &mut BTreeSet<i32>,
        t: &Type,
    ) -> Result<UseTResult, FlowJsException> {
        match merge_lower_bounds(cx, env, false, t)? {
            Some(t) => Ok(UseTResult::UpperT(t)),
            None => merge_upper_bounds(cx, env, seen, t),
        }
    }
    fn bind_use_t_result<'cx>(
        cx: &mut Context<'cx>,
        result: UseTResult,
        f: impl FnOnce(&mut Context<'cx>, Type) -> Result<UseTResult, FlowJsException>,
    ) -> Result<UseTResult, FlowJsException> {
        match result {
            UseTResult::UpperEmpty => Ok(UseTResult::UpperEmpty),
            UseTResult::UpperPlaceholder(t) => Ok(UseTResult::UpperPlaceholder(t)),
            UseTResult::UpperNonT(u) => Ok(UseTResult::UpperNonT(u)),
            UseTResult::UpperT(t) => f(cx, t),
        }
    }

    match u.deref() {
        UseTInner::UseT(_, t) if type_util::constraint_node_id(t).is_some() => {
            merge_upper_bounds(cx, env, seen, t)
        }
        UseTInner::UseT(_, t) => Ok(UseTResult::UpperT(t.dupe())),
        UseTInner::EvalTypeDestructorT(box EvalTypeDestructorTData {
            reason: r,
            destructor,
            tout,
            ..
        }) => {
            let tout_t = Type::new(TypeInner::OpenT((**tout).dupe()));
            match destructor {
                box Destructor::PropertyType { .. }
                | box Destructor::ElementType { .. }
                | box Destructor::OptionalIndexedAccessNonMaybeType { .. }
                | box Destructor::OptionalIndexedAccessResultType { .. }
                | box Destructor::ValuesType
                | box Destructor::ConditionalType(box DestructorConditionalTypeData { .. })
                | box Destructor::TypeMap(_)
                | box Destructor::MappedType(box DestructorMappedTypeData { .. }) => {
                    // Mapped Type reversals
                    Ok(UseTResult::UpperEmpty)
                }
                box Destructor::EnumType => {
                    let result = merge_lower_or_upper_bounds(cx, env, seen, &tout_t)?;
                    bind_use_t_result(cx, result, |cx, tout_val: Type| {
                        let collector = TypeCollector::create();
                        let u_inner = UseTInner::GetEnumT(Box::new(GetEnumTData {
                            use_op: unknown_use(),
                            loc: r.loc().dupe(),
                            orig_t: None,
                            kind: GetEnumKind::GetEnumValue,
                            collector: collector.dupe(),
                        }));
                        FlowJs::flow_with_env(cx, env, &tout_val, &UseT::new(u_inner))?;
                        let mut collected = Vec::new();
                        collect_pinning_lowers(
                            cx,
                            env,
                            &mut BTreeSet::new(),
                            &mut collected,
                            collector.collect_to_vec(),
                        );
                        let filtered = union_flatten_list(collected)
                            .into_iter()
                            .filter(|t| !flow_js_utils::tvar_visitors::has_placeholders(cx, t))
                            .collect();
                        let result = type_util::union_of_ts_opt(
                            r.dupe(),
                            filtered,
                            Some(union_rep::UnionKind::ImplicitInstantiationKind),
                        );
                        Ok(match result {
                            Some(t) => UseTResult::UpperT(t),
                            None => UseTResult::UpperEmpty,
                        })
                    })
                }
                box Destructor::ReactElementConfigType => {
                    let result = merge_lower_or_upper_bounds(cx, env, seen, &tout_t)?;
                    bind_use_t_result(cx, result, |cx, config: Type| {
                        let react_node = FlowJs::get_builtin_react_type_with_env(
                            cx, env,
                            None,
                            r,
                            None,
                            flow_typing_errors::intermediate_error_types::ExpectedModulePurpose::ReactModuleForReactNodeType,
                        )?;
                        Ok(UseTResult::UpperT(Type::new(TypeInner::DefT(
                            r.dupe(),
                            DefT::new(DefTInner::ReactAbstractComponentT(Box::new(
                                ReactAbstractComponentTData {
                                    config,
                                    renders: react_node,
                                    component_kind: ComponentKind::Structural,
                                },
                            ))),
                        ))))
                    })
                }
                box Destructor::RestType(_, t_rest) => {
                    let result = merge_lower_or_upper_bounds(cx, env, seen, &tout_t)?;
                    let t_rest = t_rest.dupe();
                    bind_use_t_result(cx, result, |cx, tout_val: Type| {
                        let reversed = reverse_obj_kit_rest(cx, env, r, &t_rest, &tout_val, false)?;
                        use_t_result_of_t_option(merge_lower_bounds(cx, env, false, &reversed))
                    })
                }
                box Destructor::NonMaybeType => {
                    let result = merge_lower_or_upper_bounds(cx, env, seen, &tout_t)?;
                    bind_use_t_result(cx, result, |_, t: Type| {
                        Ok(UseTResult::UpperT(Type::new(TypeInner::MaybeT(
                            r.dupe(),
                            t,
                        ))))
                    })
                }
                box Destructor::NormalizeIntersection { .. }
                | box Destructor::ExactType
                | box Destructor::ReadOnlyType
                | box Destructor::ReactDRO(_)
                | box Destructor::PartialType
                | box Destructor::RequiredType => {
                    merge_lower_or_upper_bounds(cx, env, seen, &tout_t)
                }
                box Destructor::ReactCheckComponentConfig { props: pmap } => {
                    let result = merge_lower_or_upper_bounds(cx, env, seen, &tout_t)?;
                    let pmap = pmap.clone();
                    bind_use_t_result(cx, result, |cx, t: Type| {
                        let reversed =
                            reverse_component_check_config(cx, env, r, &pmap, &t, false)?;
                        use_t_result_of_t_option(merge_lower_bounds(cx, env, false, &reversed))
                    })
                }
                box Destructor::SpreadType(box DestructorSpreadTypeData(
                    _,
                    todo_rev,
                    head_slice,
                )) => {
                    let acc_elements: flow_data_structure_wrapper::list::FlowOcamlList<
                        object::spread::AccElement,
                    > = match head_slice {
                        Some(x) => flow_data_structure_wrapper::list::FlowOcamlList::unit(
                            object::spread::AccElement::InlineSlice(x.clone()),
                        ),
                        None => flow_data_structure_wrapper::list::FlowOcamlList::new(),
                    };
                    let result = merge_lower_or_upper_bounds(cx, env, seen, &tout_t)?;
                    let todo_rev = todo_rev.dupe();
                    bind_use_t_result(cx, result, |cx, t: Type| {
                        let reversed =
                            reverse_obj_spread(cx, env, r, &todo_rev, &acc_elements, &t, false)?;
                        use_t_result_of_t_option(merge_lower_bounds(cx, env, false, &reversed))
                    })
                }
                box Destructor::SpreadTupleType(box DestructorSpreadTupleTypeData {
                    reason_tuple: _,
                    inexact,
                    reason_spread,
                    resolved: resolved_rev,
                    unresolved,
                }) => {
                    fn is_spread(rp: &ResolvedParam) -> bool {
                        match rp {
                            ResolvedParam::ResolvedArg(..) => false,
                            ResolvedParam::ResolvedSpreadArg(..)
                            | ResolvedParam::ResolvedAnySpreadArg(..) => true,
                        }
                    }
                    // Reverse if the spread is the last item in the tuple, there are
                    // no other spreads, and the tuple type is not inexact.
                    match (unresolved.is_empty(), resolved_rev.iter().any(is_spread)) {
                        (true, false) if !inexact => {
                            let n = resolved_rev.len() as i32;
                            let result = merge_lower_or_upper_bounds(cx, env, seen, &tout_t)?;
                            let reason_spread = reason_spread.dupe();
                            bind_use_t_result(cx, result, |cx, t: Type| {
                                let arr_rest_result = flow_typing_tvar::mk_where(
                                    cx,
                                    reason_spread.dupe(),
                                    |cx, t_prime| {
                                        let u_inner = UseTInner::ArrRestT(Box::new(ArrRestTData {
                                            literal: false,
                                            use_op: unknown_use(),
                                            reason: reason_spread.dupe(),
                                            index: n,
                                            tout: t_prime.dupe(),
                                        }));
                                        FlowJs::flow_with_env(cx, env, &t, &UseT::new(u_inner))?;
                                        if !flow_js_utils::tvar_visitors::has_unresolved_tvars_or_placeholders(cx, &t) {
                                            tvar_resolver::resolve(
                                                cx,
                                                tvar_resolver::default_no_lowers,
                                                true,
                                                t_prime,
                                            );
                                        }
                                        Ok::<(), FlowJsException>(())
                                    },
                                )?;
                                match merge_lower_bounds(cx, env, false, &arr_rest_result)? {
                                    None => Ok(UseTResult::UpperEmpty),
                                    Some(solution) => {
                                        FlowJs::flow_t_with_env(cx, env, &solution, tvar)?;
                                        Ok(UseTResult::UpperT(solution))
                                    }
                                }
                            })
                        }
                        _ => Ok(UseTResult::UpperNonT(u.dupe())),
                    }
                }
            }
        }
        UseTInner::ArrRestT(box ArrRestTData { index: i, tout, .. }) => {
            if let TypeInner::DefT(_, def_t) = get_t(cx, tout).deref()
                && let DefTInner::ArrT(arr_t) = def_t.deref()
            {
                match arr_t.as_ref() {
                    ArrType::ArrayAT(box ArrayATData {
                        tuple_view: None, ..
                    })
                    | ArrType::ROArrayAT(box ROArrayATData { .. }) => {
                        identity_reverse_upper_bound(cx, env, seen, tvar, tout)
                    }
                    ArrType::ArrayAT(box ArrayATData {
                        tuple_view: Some(_),
                        ..
                    })
                    | ArrType::TupleAT(box TupleATData { .. })
                        if *i == 0 =>
                    {
                        identity_reverse_upper_bound(cx, env, seen, tvar, tout)
                    }
                    _ => Ok(UseTResult::UpperEmpty),
                }
            } else {
                Ok(UseTResult::UpperEmpty)
            }
        }
        // Call related upper bounds are ignored because there is not enough info to reverse.
        // Get/set-prop related upper bounds are ignored
        // because there is not enough info to reverse.
        UseTInner::CallT(..)
        | UseTInner::ConditionalT(..)
        | UseTInner::MethodT(..)
        | UseTInner::PrivateMethodT(..)
        | UseTInner::ConstructorT(..)
        | UseTInner::ToStringT { .. }
        | UseTInner::MapTypeT(..)
        | UseTInner::SetPropT(..)
        | UseTInner::SetPrivatePropT(..)
        | UseTInner::GetElemT { .. }
        | UseTInner::SetElemT(..)
        | UseTInner::CallElemT(..)
        | UseTInner::GetPropT(..)
        | UseTInner::GetPrivatePropT(..)
        | UseTInner::TestPropT { .. }
        | UseTInner::GetStaticsT(..)
        | UseTInner::GetProtoT(..)
        | UseTInner::ObjTestProtoT(..)
        | UseTInner::HasOwnPropT(..)
        | UseTInner::LookupT { .. }
        | UseTInner::OptionalIndexedAccessT { .. }
        | UseTInner::GetKeysT { .. }
        | UseTInner::GetKeysDictKeyT { .. }
        | UseTInner::GetValuesT(..)
        | UseTInner::GetDictValuesT(..)
        | UseTInner::SuperT(..)
        | UseTInner::ImplementsT(..)
        | UseTInner::MixinT(..)
        | UseTInner::ExtendsUseT(..)
        | UseTInner::GetEnumT { .. }
        | UseTInner::CondT(..)
        | UseTInner::ConvertEmptyPropsToMixedT(..)
        | UseTInner::ValueToTypeReferenceT(..)
        | UseTInner::ExitRendersT { .. }
        | UseTInner::ConcretizeT(box ConcretizeTData {
            kind: ConcretizationKind::ConcretizeForConstructorReturn,
            ..
        }) => Ok(UseTResult::UpperEmpty),
        // Remaining unhandled upper bounds
        UseTInner::SpecializeT(..)
        | UseTInner::ThisSpecializeT(..)
        | UseTInner::ConcretizeTypeAppsT(..)
        | UseTInner::ObjRestT(..)
        | UseTInner::ElemT(..)
        | UseTInner::ReactKitT(..)
        | UseTInner::ConcretizeT(..)
        | UseTInner::FilterOptionalT(..)
        | UseTInner::FilterMaybeT(..)
        | UseTInner::SealGenericT { .. } => Ok(UseTResult::UpperNonT(u.dupe())),
        UseTInner::DeepReadOnlyT(tvar_out, _) => {
            let tout = Type::new(TypeInner::OpenT((**tvar_out).dupe()));
            identity_reverse_upper_bound(cx, env, seen, tvar, &tout)
        }
        UseTInner::HooklikeT(tvar_out) => {
            let tout = Type::new(TypeInner::OpenT((**tvar_out).dupe()));
            identity_reverse_upper_bound(cx, env, seen, tvar, &tout)
        }
        UseTInner::ReposLowerT { use_t, .. } => t_of_use_t(cx, env, seen, tvar, use_t),
        UseTInner::ReposUseT(box ReposUseTData { type_: t, .. }) => {
            FlowJs::flow_t_with_env(cx, env, t, tvar)?;
            Ok(UseTResult::UpperT(t.dupe()))
        }
        UseTInner::ResolveSpreadT(box ResolveSpreadTData {
            reason,
            resolve_spread_type,
            ..
        }) if resolve_spread_type.rrt_unresolved.is_empty() => {
            match &resolve_spread_type.rrt_resolve_to {
                SpreadResolve::ResolveSpreadsToMultiflowSubtypeFull(_, funtype) => {
                    use_t_result_of_t_option(
                        reverse_resolve_spread_multiflow_subtype_full_partial_resolution(
                            cx,
                            env,
                            tvar,
                            reason,
                            &resolve_spread_type.rrt_resolved,
                            &funtype.params,
                            &funtype.rest_param,
                        ),
                    )
                }
                // The spread receives what the tuple does, minus the elements before it.
                SpreadResolve::ResolveSpreadsToTupleType {
                    inexact: false,
                    tout,
                    ..
                } => {
                    let Some(n) = resolved_elements_count(&resolve_spread_type.rrt_resolved) else {
                        return Ok(UseTResult::UpperNonT(u.dupe()));
                    };
                    let result = merge_lower_or_upper_bounds(cx, env, seen, tout)?;
                    bind_use_t_result(cx, result, |cx, t: Type| {
                        let arr_rest_result =
                            flow_typing_tvar::mk_where(cx, reason.dupe(), |cx, t_prime| {
                                let u_inner = UseTInner::ArrRestT(Box::new(ArrRestTData {
                                    literal: false,
                                    use_op: unknown_use(),
                                    reason: reason.dupe(),
                                    index: n as i32,
                                    tout: t_prime.dupe(),
                                }));
                                FlowJs::flow_with_env(cx, env, &t, &UseT::new(u_inner))?;
                                Ok::<(), FlowJsException>(())
                            })?;
                        match merge_lower_bounds(cx, env, false, &arr_rest_result)? {
                            None => Ok(UseTResult::UpperEmpty),
                            Some(solution) => {
                                FlowJs::flow_t_with_env(cx, env, &solution, tvar)?;
                                Ok(UseTResult::UpperT(solution))
                            }
                        }
                    })
                }
                SpreadResolve::ResolveSpreadsToTupleType { .. }
                | SpreadResolve::ResolveSpreadsToArrayLiteral { .. }
                | SpreadResolve::ResolveSpreadsToArray { .. }
                | SpreadResolve::ResolveSpreadsToMultiflowCallFull(..) => {
                    Ok(UseTResult::UpperNonT(u.dupe()))
                }
            }
        }
        UseTInner::ResolveSpreadT(..) => Ok(UseTResult::UpperNonT(u.dupe())),
        UseTInner::ResolveUnionT(box ResolveUnionTData { upper, .. }) => {
            t_of_use_t(cx, env, seen, tvar, upper)
        }
        UseTInner::ObjKitT(_, r, _, box tool, tout) => match tool {
            object::Tool::MakeExact
            | object::Tool::ReadOnly
            | object::Tool::Partial
            | object::Tool::Required
            | object::Tool::ObjectRep => identity_reverse_upper_bound(cx, env, seen, tvar, tout),
            object::Tool::ReactConfig(box ObjectToolReactConfigData {
                ref_manipulation: object::react_config::RefManipulation::KeepRef,
                ..
            }) => identity_reverse_upper_bound(cx, env, seen, tvar, tout),
            object::Tool::ReactConfig(box ObjectToolReactConfigData {
                ref_manipulation: object::react_config::RefManipulation::AddRef(ref_t),
                literal,
                ..
            }) => {
                let solution = merge_upper_bounds(cx, env, seen, tout)?;
                match solution {
                    UseTResult::UpperEmpty => Ok(UseTResult::UpperEmpty),
                    UseTResult::UpperPlaceholder(t) => Ok(UseTResult::UpperPlaceholder(t)),
                    UseTResult::UpperNonT(u) => Ok(UseTResult::UpperNonT(u)),
                    UseTResult::UpperT(t) => {
                        let pmap = properties::PropertiesMap::from_btree_map(BTreeMap::from([(
                            flow_common::reason::Name::new("ref"),
                            Property::new(PropertyInner::Field(Box::new(FieldData {
                                preferred_def_locs: None,
                                key_loc: None,
                                type_: ref_t.dupe(),
                                polarity: Polarity::Neutral,
                            }))),
                        )]));
                        let reversed =
                            reverse_component_check_config(cx, env, r, &pmap, &t, *literal)?;
                        match merge_lower_bounds(cx, env, false, &reversed)? {
                            None => Ok(UseTResult::UpperEmpty),
                            Some(reversed) => {
                                FlowJs::flow_t_with_env(cx, env, &reversed, tvar)?;
                                Ok(UseTResult::UpperT(reversed))
                            }
                        }
                    }
                }
            }
            object::Tool::ReactCheckComponentConfig { props: pmap } => {
                let solution = merge_upper_bounds(cx, env, seen, tout)?;
                match solution {
                    UseTResult::UpperEmpty => Ok(UseTResult::UpperEmpty),
                    UseTResult::UpperPlaceholder(t) => Ok(UseTResult::UpperPlaceholder(t)),
                    UseTResult::UpperNonT(u) => Ok(UseTResult::UpperNonT(u)),
                    UseTResult::UpperT(t) => {
                        let reversed = reverse_component_check_config(cx, env, r, pmap, &t, false)?;
                        match merge_lower_bounds(cx, env, false, &reversed)? {
                            None => Ok(UseTResult::UpperEmpty),
                            Some(reversed) => {
                                FlowJs::flow_t_with_env(cx, env, &reversed, tvar)?;
                                Ok(UseTResult::UpperT(reversed))
                            }
                        }
                    }
                }
            }
            object::Tool::ObjectMap(box ObjectToolObjectMapData { .. }) => {
                Ok(UseTResult::UpperEmpty)
            }
            object::Tool::Spread(box (target, state)) => {
                let literal = target.literal();
                let solution = merge_upper_bounds(cx, env, seen, tout)?;
                match solution {
                    UseTResult::UpperEmpty => Ok(UseTResult::UpperEmpty),
                    UseTResult::UpperPlaceholder(t) => Ok(UseTResult::UpperPlaceholder(t)),
                    UseTResult::UpperNonT(u) => Ok(UseTResult::UpperNonT(u)),
                    UseTResult::UpperT(t) => {
                        let reversed = reverse_obj_spread(
                            cx,
                            env,
                            r,
                            &state.todo_rev,
                            &state.acc,
                            &t,
                            literal,
                        )?;
                        match merge_lower_bounds(cx, env, false, &reversed)? {
                            None => Ok(UseTResult::UpperEmpty),
                            Some(reversed) => {
                                FlowJs::flow_t_with_env(cx, env, &reversed, tvar)?;
                                Ok(UseTResult::UpperT(reversed))
                            }
                        }
                    }
                }
            }
            object::Tool::Rest(box (_, object::rest::State::One(t_rest))) => {
                let result = merge_upper_bounds(cx, env, seen, tout)?;
                let t_rest = t_rest.dupe();
                bind_use_t_result(cx, result, |cx, t: Type| {
                    let reversed = reverse_obj_kit_rest(cx, env, r, &t_rest, &t, false)?;
                    match merge_lower_bounds(cx, env, false, &reversed)? {
                        None => Ok(UseTResult::UpperEmpty),
                        Some(reversed) => {
                            FlowJs::flow_t_with_env(cx, env, &reversed, tvar)?;
                            Ok(UseTResult::UpperT(reversed))
                        }
                    }
                })
            }
            object::Tool::Rest(box (_, object::rest::State::Done(_))) => {
                Ok(UseTResult::UpperNonT(u.dupe()))
            }
        },
    }
}

fn resolved_elements_count(
    resolved: &flow_data_structure_wrapper::list::FlowOcamlList<ResolvedParam>,
) -> Option<usize> {
    resolved.iter().try_fold(0, |n, param| match param {
        ResolvedParam::ResolvedArg(_) => Some(n + 1),
        ResolvedParam::ResolvedSpreadArg(box ResolvedSpreadArgData(
            _,
            ArrType::TupleAT(box TupleATData {
                elements,
                inexact: false,
                rest: None,
                ..
            })
            | ArrType::ArrayAT(box ArrayATData {
                tuple_view:
                    Some(TupleView {
                        elements,
                        inexact: false,
                        rest: None,
                        ..
                    }),
                ..
            }),
            _,
        )) => Some(n + elements.len()),
        ResolvedParam::ResolvedSpreadArg(_) | ResolvedParam::ResolvedAnySpreadArg(..) => None,
    })
}

fn identity_reverse_upper_bound<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    seen: &mut BTreeSet<i32>,
    tvar: &Type,
    tout: &Type,
) -> Result<UseTResult, FlowJsException> {
    let solution = merge_upper_bounds(cx, env, seen, tout)?;
    match &solution {
        UseTResult::UpperT(t) => FlowJs::flow_t_with_env(cx, env, t, tvar)?,
        _ => {}
    }
    Ok(solution)
}

fn reverse_obj_spread<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    r: &Reason,
    todo_rev: &flow_data_structure_wrapper::list::FlowOcamlList<object::spread::Operand>,
    acc_elements: &flow_data_structure_wrapper::list::FlowOcamlList<object::spread::AccElement>,
    tout: &Type,
    literal: bool,
) -> Result<Type, FlowJsException> {
    let inline_slice_to_t = |cx: &mut Context<'cx>, s: &object::spread::OperandSlice| -> Type {
        let obj_kind = obj_type::obj_kind_from_optional_dict(s.dict.clone(), ObjKind::Exact);
        obj_type::mk_with_proto(
            cx,
            r.dupe(),
            obj_kind,
            Some(s.reachable_targs.dupe()),
            None,
            Some(s.prop_map.dupe()),
            None,
            Type::new(TypeInner::ObjProtoT(r.dupe())),
            literal,
        )
    };

    let slice_to_t = |cx: &mut Context<'cx>, s: &object::Slice| -> Type {
        let props: properties::PropertiesMap = s.props.dupe();
        obj_type::mk_with_proto(
            cx,
            r.dupe(),
            s.flags.obj_kind.clone(),
            Some(s.reachable_targs.dupe()),
            None,
            Some(props),
            None,
            Type::new(TypeInner::ObjProtoT(r.dupe())),
            literal,
        )
    };

    let operand_to_t = |cx: &mut Context<'cx>, o: &object::spread::Operand| -> Type {
        match o {
            object::spread::Operand::Slice(s) => inline_slice_to_t(cx, s),
            object::spread::Operand::Type(t) => t.dupe(),
        }
    };

    let acc_element_to_ts = |cx: &mut Context<'cx>, e: &object::spread::AccElement| -> Vec<Type> {
        match e {
            object::spread::AccElement::InlineSlice(s) => vec![inline_slice_to_t(cx, s)],
            object::spread::AccElement::ResolvedSlice(resolved) => {
                resolved.0.iter().map(|s| slice_to_t(cx, s)).collect()
            }
        }
    };

    let rest_type = |cx: &mut Context<'cx>, l: Type, rest: Type| -> Result<Type, FlowJsException> {
        flow_typing_tvar::mk_where(cx, r.dupe(), |cx, tout| {
            let u_inner = UseTInner::ObjKitT(
                unknown_use(),
                r.dupe(),
                Box::new(object::ResolveTool::Resolve(object::Resolve::Next)),
                Box::new(object::Tool::Rest(Box::new((
                    object::rest::MergeMode::SpreadReversal,
                    object::rest::State::One(rest),
                )))),
                tout.dupe(),
            );
            FlowJs::flow_with_env(cx, env, &l, &UseT::new(u_inner))?;
            Ok::<(), FlowJsException>(())
        })
    };

    let mut tout_val = flow_typing_tvar::mk_where(cx, r.dupe(), |cx, t_prime| {
        FlowJs::flow_t_with_env(cx, env, tout, t_prime)?;
        Ok::<(), FlowJsException>(())
    })?;
    for e in acc_elements.iter() {
        for t in acc_element_to_ts(cx, e) {
            tout_val = rest_type(cx, tout_val, t)?;
        }
    }
    for o in todo_rev.iter() {
        let operand = operand_to_t(cx, o);
        tout_val = rest_type(cx, tout_val, operand)?;
    }
    Ok(tout_val)
}

fn reverse_component_check_config<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    pmap: &properties::PropertiesMap,
    tout: &Type,
    literal: bool,
) -> Result<Type, FlowJsException> {
    if pmap.is_empty() {
        return Ok(tout.dupe());
    }
    flow_typing_tvar::mk_where(cx, reason.dupe(), |cx, t_prime| {
        let rest = obj_type::mk_with_proto(
            cx,
            reason.dupe(),
            ObjKind::Exact,
            None,
            None,
            Some(pmap.dupe()),
            None,
            Type::new(TypeInner::ObjProtoT(reason.dupe())),
            literal,
        );
        let u_inner = UseTInner::ObjKitT(
            unknown_use(),
            reason.dupe(),
            Box::new(object::ResolveTool::Resolve(object::Resolve::Next)),
            Box::new(object::Tool::Rest(Box::new((
                object::rest::MergeMode::SpreadReversal,
                object::rest::State::One(rest),
            )))),
            t_prime.dupe(),
        );
        FlowJs::flow_with_env(cx, env, tout, &UseT::new(u_inner))?;
        Ok(())
    })
}

fn reverse_obj_kit_rest<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t_rest: &Type,
    tout: &Type,
    literal: bool,
) -> Result<Type, FlowJsException> {
    flow_typing_tvar::mk_no_wrap_where(cx, reason.dupe(), |cx, _r, t_prime_id| {
        let tool = object::ResolveTool::Resolve(object::Resolve::Next);
        let options = object::spread::Target::Value {
            make_seal: obj_type::mk_seal(false, false),
            strictness_kind: cx.type_strictness_kind(),
            literal,
        };
        let state = object::spread::State {
            todo_rev: flow_data_structure_wrapper::list::FlowOcamlList::unit(
                object::spread::Operand::Type(t_rest.dupe()),
            ),
            acc: flow_data_structure_wrapper::list::FlowOcamlList::new(),
            spread_id: flow_common::reason::mk_id() as i32,
            union_t: None,
            curr_resolve_idx: 0,
        };
        let open_t_prime = Type::new(TypeInner::OpenT(Tvar::new(
            reason.dupe(),
            t_prime_id as u32,
        )));
        let u_inner = UseTInner::ObjKitT(
            unknown_use(),
            reason.dupe(),
            Box::new(tool),
            Box::new(object::Tool::Spread(Box::new((options, state)))),
            open_t_prime,
        );
        FlowJs::flow_with_env(cx, env, tout, &UseT::new(u_inner))?;
        Ok(())
    })
}

// The params as a tuple type: `[...params, ...rest]`.
fn reverse_resolve_spread_multiflow_subtype_full_no_resolution<'cx>(
    cx: &mut Context<'cx>,
    reason: &Reason,
    params: &[FunParam],
    rest_param: &Option<FunRestParam>,
) -> Result<Type, FlowJsException> {
    // Trailing optional params become optional tuple elements.
    let required_count = params
        .iter()
        .rposition(|FunParam(_, t)| !matches!(t.deref(), TypeInner::OptionalT { .. }))
        .map_or(0, |index| index + 1);
    let mut tuple_elements = Vec::new();
    let mut tuple_ts = Vec::new();
    for (index, param) in params.iter().enumerate() {
        let name = &param.0;
        let (t, optional) = match param.1.deref() {
            // Unlike an optional tuple element, an optional param also accepts `undefined`.
            TypeInner::OptionalT { reason, type_, .. } if index >= required_count => (
                type_util::union_of_ts(
                    reason.dupe(),
                    vec![type_.dupe(), void::why(reason.dupe())],
                    None,
                ),
                true,
            ),
            _ => (param.1.dupe(), false),
        };
        let el = type_util::mk_tuple_element(
            type_util::reason_of_t(&t).dupe(),
            t.dupe(),
            name.as_ref().map(|s| s.dupe()),
            optional,
            Polarity::Neutral,
        );
        tuple_elements.push(el);
        tuple_ts.push(t);
    }
    let (reason_out, arr_type) = match rest_param {
        None => {
            let elem_t = match tuple_ts.as_slice() {
                [] => empty_t::why(reason.dupe()),
                [t] => t.dupe(),
                [t0, t1, ts @ ..] => Type::new(TypeInner::UnionT(
                    reason.dupe(),
                    union_rep::make(
                        None,
                        union_rep::UnionKind::ImplicitInstantiationKind,
                        t0.dupe(),
                        t1.dupe(),
                        ts.iter().duped().collect::<Rc<[_]>>(),
                    ),
                )),
            };
            let len = tuple_ts.len() as i32;
            let t = ArrType::TupleAT(Box::new(TupleATData {
                elem_t,
                elements: tuple_elements.into(),
                arity: (required_count as i32, len),
                inexact: false,
                rest: None,
                react_dro: None,
                strictness_kind: Default::default(),
                literal: false,
                fresh: false,
            }));
            let reason_out = reason
                .dupe()
                .update_desc(|_| Some(flow_common::reason::VirtualReasonDesc::RTupleType));
            (reason_out, t)
        }
        // `[...rest]` is `rest`, which keeps a generic rest param.
        Some(FunRestParam(_, _, rest_param_t)) if params.is_empty() => {
            return Ok(rest_param_t.dupe());
        }
        Some(FunRestParam(_, _, rest_param_t)) => {
            let elements = tuple_elements
                .into_iter()
                .map(|el| UnresolvedParam::UnresolvedArg(Box::new(UnresolvedArgData(el, None))))
                .chain(std::iter::once(UnresolvedParam::UnresolvedSpreadArg(
                    rest_param_t.dupe(),
                )))
                .collect();
            let reason_out = reason
                .dupe()
                .update_desc(|_| Some(flow_common::reason::VirtualReasonDesc::RTupleType));
            return flow_js_utils::mk_tuple_type(
                cx,
                cx.type_strictness_kind(),
                eval::Id::generate_id(),
                |_cx, use_op, reason, t, destructor, id| {
                    Ok(Type::new(TypeInner::EvalT {
                        type_: t,
                        defer_use_t: TypeDestructorT::new(TypeDestructorTInner(
                            use_op,
                            reason,
                            Rc::new(destructor),
                        )),
                        id,
                    }))
                },
                false,
                reason_out,
                elements,
            );
        }
    };
    Ok(Type::new(TypeInner::DefT(
        reason_out,
        DefT::new(DefTInner::ArrT(std::rc::Rc::new(arr_type))),
    )))
}

fn reverse_resolve_spread_multiflow_subtype_full_partial_resolution<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    tvar: &Type,
    reason: &Reason,
    resolved: &flow_data_structure_wrapper::list::FlowOcamlList<ResolvedParam>,
    params: &[FunParam],
    rest_param: &Option<FunRestParam>,
) -> Result<Option<Type>, FlowJsException> {
    // The spread receives the params after the resolved arguments. The resolved arguments past
    // the positional params went to the rest param, so they are dropped from it.
    let remaining_params = params.get(resolved.len()..).unwrap_or_default();
    let params_t = reverse_resolve_spread_multiflow_subtype_full_no_resolution(
        cx,
        reason,
        remaining_params,
        rest_param,
    )?;
    let solution = match resolved.len().saturating_sub(params.len()) {
        0 => Some(params_t),
        index => {
            let arr_rest_result = flow_typing_tvar::mk_where(cx, reason.dupe(), |cx, tout| {
                let u_inner = UseTInner::ArrRestT(Box::new(ArrRestTData {
                    literal: false,
                    use_op: unknown_use(),
                    reason: reason.dupe(),
                    index: index as i32,
                    tout: tout.dupe(),
                }));
                FlowJs::flow_with_env(cx, env, &params_t, &UseT::new(u_inner))?;
                Ok::<(), FlowJsException>(())
            })?;
            merge_lower_bounds(cx, env, false, &arr_rest_result)?
        }
    };
    if let Some(solution) = &solution {
        FlowJs::flow_t_with_env(cx, env, solution, tvar)?;
    }
    Ok(solution)
}

enum PinningSource {
    Constraints {
        id: i32,
        constraints: constraint::Constraints<Cx>,
    },
    Concrete(Type),
}

fn pinning_source<'cx>(cx: &mut Context<'cx>, env: &FlowJsEnv, t: &Type) -> PinningSource {
    match type_util::constraint_node_id(t) {
        Some(node) => {
            let id = node.id();
            let (root_id, constraints) = cx.find_constraints(id);
            if env
                .frozen_implicit_instantiation_tvar(|frozen_id| {
                    cx.find_constraints(frozen_id).0 == root_id
                })
                .is_some()
            {
                PinningSource::Concrete(t.dupe())
            } else {
                PinningSource::Constraints { id, constraints }
            }
        }
        None => PinningSource::Concrete(t.dupe()),
    }
}

fn collect_pinning_lowers<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    seen: &mut BTreeSet<i32>,
    acc: &mut Vec<Type>,
    ts: Vec<Type>,
) {
    let mut work_list = VecDeque::from(ts);
    while let Some(t) = work_list.pop_front() {
        match pinning_source(cx, env, &t) {
            PinningSource::Constraints { id, .. } if seen.insert(id) => {
                for t in flow_js_utils::possible_types(cx, id).into_iter().rev() {
                    work_list.push_front(t);
                }
            }
            PinningSource::Constraints { .. } => {}
            PinningSource::Concrete(t) => acc.push(t),
        }
    }
}

// merge_upper_bounds is complex — it looks at upper bounds and tries to find a consistent type.
// For now we provide a simplified version that handles the common cases.
fn merge_upper_bounds<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    seen: &mut BTreeSet<i32>,
    tvar: &Type,
) -> Result<UseTResult, FlowJsException> {
    let filter_placeholder = |cx: &mut Context<'cx>, t: Type| -> UseTResult {
        if flow_js_utils::tvar_visitors::has_placeholders(cx, &t) {
            UseTResult::UpperEmpty
        } else {
            UseTResult::UpperT(t)
        }
    };
    let equal = |cx: &mut Context<'cx>, t1: &Type, t2: &Type| {
        concrete_type_eq::eq_with_env(cx, env, t1, t2)
    };

    match pinning_source(cx, env, tvar) {
        PinningSource::Constraints { id, constraints } => {
            if seen.contains(&id) {
                return Ok(UseTResult::UpperEmpty);
            }
            match constraints {
                constraint::Constraints::FullyResolved(s) => {
                    let t = cx.force_fully_resolved_tvar(&s);
                    Ok(filter_placeholder(cx, t))
                }
                constraint::Constraints::Resolved(t) => {
                    if flow_js_utils::tvar_visitors::has_placeholders(cx, &t) {
                        Ok(match *cx.typing_mode() {
                            TypingMode::CheckingMode => UseTResult::UpperEmpty,
                            TypingMode::SynthesisMode { .. } | TypingMode::HintEvaluationMode => {
                                UseTResult::UpperPlaceholder(t)
                            }
                        })
                    } else {
                        Ok(UseTResult::UpperT(t))
                    }
                }
                constraint::Constraints::Unresolved(bounds) => {
                    let uppers: Vec<_> = bounds
                        .borrow()
                        .upper
                        .iter()
                        .map(|(k, v)| (k.use_t.dupe(), *v))
                        .collect();
                    let inserted = seen.insert(id);
                    let sorted_uppers = sort_upper_bounds_for_merging(uppers);

                    let mut acc = UseTResult::UpperEmpty;
                    for (u, _) in sorted_uppers {
                        // Pinning ignores a bound that mentions a placeholder.
                        let result = match t_of_use_t(cx, env, seen, tvar, &u)? {
                            UseTResult::UpperT(t)
                                if flow_js_utils::tvar_visitors::has_placeholders(cx, &t) =>
                            {
                                UseTResult::UpperEmpty
                            }
                            result => result,
                        };
                        acc = match (acc, result) {
                            (UseTResult::UpperNonT(u), _) | (_, UseTResult::UpperNonT(u)) => {
                                UseTResult::UpperNonT(u)
                            }
                            (acc, UseTResult::UpperEmpty) | (UseTResult::UpperEmpty, acc) => acc,
                            (UseTResult::UpperPlaceholder(t), UseTResult::UpperPlaceholder(_)) => {
                                UseTResult::UpperPlaceholder(t)
                            }
                            (UseTResult::UpperPlaceholder(_), UseTResult::UpperT(t))
                            | (UseTResult::UpperT(t), UseTResult::UpperPlaceholder(_)) => {
                                UseTResult::UpperT(t)
                            }
                            (UseTResult::UpperT(t_prime), UseTResult::UpperT(t)) => {
                                if equal(cx, &t_prime, &t)
                                    || type_util::quick_subtype(
                                        None::<&mut fn(&Type)>,
                                        &t,
                                        &t_prime,
                                    )
                                    || speculative_subtyping_succeeds(
                                        cx,
                                        env,
                                        DepthTrace::dummy_trace(),
                                        unknown_use(),
                                        &t,
                                        &t_prime,
                                    )?
                                {
                                    UseTResult::UpperT(t)
                                } else {
                                    UseTResult::UpperT(t_prime)
                                }
                            }
                        };
                    }
                    if inserted {
                        seen.remove(&id);
                    }
                    Ok(acc)
                }
            }
        }
        PinningSource::Concrete(t) => Ok(UseTResult::UpperT(t)),
    }
}

fn merge_lower_bounds<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    filter_empty: bool,
    t: &Type,
) -> Result<Option<Type>, FlowJsException> {
    // When the input tvar has a ReposUseT upper bound it means that we might be
    // discounting lower bounds that are just waiting to be added as soon as the
    // ReposUseT fires. Here, we make sure we record the result of the ReposUseT
    // before we make a decision based on lower bounds.
    if let PinningSource::Constraints {
        constraints: constraint::Constraints::Unresolved(bounds),
        ..
    } = pinning_source(cx, env, t)
    {
        let upper = bounds.borrow().upper.clone();
        for key in upper.keys() {
            if let UseTInner::ReposUseT(box ReposUseTData { type_: ref l, .. }) = *key.use_t {
                FlowJs::flow_t_with_env(cx, env, l, t)?;
            }
        }
    }

    Ok(match pinning_source(cx, env, t) {
        PinningSource::Constraints { id, constraints } => match constraints {
            constraint::Constraints::FullyResolved(s) => {
                let t = cx.force_fully_resolved_tvar(&s);
                if flow_js_utils::tvar_visitors::has_placeholders(cx, &t) {
                    None
                } else {
                    Some(t)
                }
            }
            constraint::Constraints::Resolved(t) => {
                if flow_js_utils::tvar_visitors::has_placeholders(cx, &t) {
                    None
                } else {
                    Some(t)
                }
            }
            constraint::Constraints::Unresolved(bounds) => {
                let lower_types: Vec<Type> = bounds.borrow().lower.keys().duped().collect();
                if lower_types.is_empty() {
                    None
                } else {
                    let mut seen = BTreeSet::new();
                    seen.insert(id);
                    let mut collected = Vec::new();
                    collect_pinning_lowers(cx, env, &mut seen, &mut collected, lower_types);
                    let flattened = union_flatten_list(collected);
                    let filtered: Vec<Type> = flattened
                        .into_iter()
                        .filter(|t| match t.deref() {
                            TypeInner::DefT(_, def_t) => {
                                !(filter_empty && matches!(def_t.deref(), DefTInner::EmptyT))
                            }
                            _ => true,
                        })
                        .filter(|t| !flow_js_utils::tvar_visitors::has_placeholders(cx, t))
                        .collect();
                    type_util::union_of_ts_opt(
                        type_util::reason_of_t(t).dupe(),
                        filtered,
                        Some(union_rep::UnionKind::ImplicitInstantiationKind),
                    )
                }
            }
        },
        PinningSource::Concrete(t) => Some(t),
    })
}

// Flowing into a return hint that contains placeholders can leak them into the
// bounds of a type parameter, e.g. by unification in an invariant position or by
// any-propagation into the type arguments of an existing bound. Pinning discards
// bounds that contain placeholders, so the type parameter could be reported as
// underconstrained even though it had other bounds. Bounds that are themselves
// placeholders are harmless, since pinning just ignores them.
fn placeholders_leaked_into_bounds<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    targs: &[SolveTarg],
) -> bool {
    for SolveTarg { t, inferred, .. } in targs {
        if !*inferred {
            continue;
        }
        let bounds = match pinning_source(cx, env, t) {
            PinningSource::Constraints {
                id,
                constraints: constraint::Constraints::Unresolved(bounds),
            } => {
                let (lower_types, upper_types): (Vec<Type>, Vec<Type>) = {
                    let bounds = bounds.borrow();
                    (
                        bounds.lower.keys().duped().collect(),
                        bounds
                            .upper
                            .keys()
                            .filter_map(|k| match k.use_t.deref() {
                                UseTInner::UseT(_, t) => Some(t.dupe()),
                                _ => None,
                            })
                            .collect(),
                    )
                };
                let mut seen = BTreeSet::from([id]);
                let mut collected = Vec::new();
                collect_pinning_lowers(cx, env, &mut seen, &mut collected, lower_types);
                let mut bounds = union_flatten_list(collected);
                bounds.extend(upper_types);
                bounds
            }
            PinningSource::Constraints {
                constraints: constraint::Constraints::Resolved(t),
                ..
            } => {
                if flow_js_utils::tvar_visitors::has_placeholders(cx, &t) {
                    return true;
                }
                continue;
            }
            PinningSource::Constraints {
                constraints: constraint::Constraints::FullyResolved(_),
                ..
            }
            | PinningSource::Concrete(_) => continue,
        };
        for t in bounds {
            if !matches!(t.deref(), TypeInner::AnyT(_, AnySource::Placeholder))
                && flow_js_utils::tvar_visitors::has_placeholders(cx, &t)
            {
                return true;
            }
        }
    }
    false
}

/// What a solve pins an unmarked type parameter to when it has neither usable bounds nor a
/// default.
///
/// A type parameter is *marked* with its polarity when the result of the operation mentions it,
/// and the solve does not allow underconstrained type parameters. Nothing observes the solution
/// of an unmarked one. A marked type parameter without usable bounds or a default is always
/// underconstrained (see `underconstrained`).
#[derive(Clone, Copy)]
enum UnmarkedFallback {
    /// Its bound.
    Bound,
    /// Underconstrained, like a marked one. This is what the hint round of `solve_targs` uses,
    /// which runs in hint evaluation mode, so it gives a placeholder.
    ///
    /// The hint round's solution is the type that a generic value is checked against. In
    ///
    ///   declare function on<T>(make: () => T): void;
    ///   declare function create<U>(): Array<U>;
    ///   on(create);
    ///
    /// `create` still has placeholder type arguments in the hint round, so `T` only gets
    /// `Array<placeholder>`, which pinning ignores. Pinned to its bound, `T` would make `create`
    /// get checked against `() => mixed`, even though nothing said `mixed`. As a placeholder, `T`
    /// makes it get checked against `() => <placeholder>`, which says nothing about what it
    /// returns.
    Underconstrained,
}

impl UnmarkedFallback {
    /// The solution for a type parameter without usable bounds.
    fn without_bounds<'cx>(
        self,
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        use_op: &UseOp,
        tparam: &TypeParam,
        marked: bool,
        tparam_binder_reason: &Reason,
        instantiation_reason: &Reason,
    ) -> Result<Type, FlowJsException> {
        match (self, marked, &tparam.default) {
            (UnmarkedFallback::Bound, false, default) => {
                let inferred = default.as_ref().unwrap_or(&tparam.bound).dupe();
                flow_typing_debug::verbose::print_if_verbose_lazy(cx, None, None, None, || {
                    vec![format!(
                        "Constant type parameter {} is pinned to {}",
                        tparam.name.string_of_subst_name(),
                        flow_typing_debug::dump_t(Some(3), cx, &inferred),
                    )]
                });
                Ok(inferred)
            }
            (_, _, Some(default)) => Ok(default.dupe()),
            (_, _, None) => underconstrained(
                cx,
                env,
                use_op,
                tparam,
                tparam_binder_reason,
                instantiation_reason,
            ),
        }
    }
}

/// A type parameter that a solve can't determine: a placeholder outside checking mode, and an
/// `underconstrained-implicit-instantiation` error in checking mode.
fn underconstrained<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: &UseOp,
    tparam: &TypeParam,
    tparam_binder_reason: &Reason,
    instantiation_reason: &Reason,
) -> Result<Type, FlowJsException> {
    if !matches!(*cx.typing_mode(), TypingMode::CheckingMode) {
        return Ok(cx.mk_placeholder(tparam_binder_reason.dupe()));
    }
    flow_js_utils::add_output_with_env(
        cx,
        env,
        ErrorMessage::EImplicitInstantiationUnderconstrainedError(Box::new(
            EImplicitInstantiationUnderconstrainedErrorData {
                call: ImplicitInstantiationReferenceData {
                    loc: instantiation_reason.loc().dupe(),
                    kind: implicit_instantiation_reference_kind(use_op)
                        .map(|kind| kind.dupe())
                        .unwrap_or(ImplicitInstantiationReferenceKind::Call(None)),
                },
                type_param_loc: tparam_binder_reason.def_loc().dupe(),
                type_param_name: tparam.name.string_of_subst_name().dupe(),
                use_op: use_op.dupe(),
            },
        )),
    )?;
    Ok(any_t::why(
        AnySource::AnyError(None),
        tparam_binder_reason.dupe(),
    ))
}

fn log_pinned(cx: &mut Context<'_>, tparam: &TypeParam, inferred: &Type) {
    flow_typing_debug::verbose::print_if_verbose_lazy(cx, None, None, None, || {
        vec![format!(
            "Type parameter {} is pinned to {}",
            tparam.name.string_of_subst_name(),
            flow_typing_debug::dump_t(Some(3), cx, inferred),
        )]
    });
}

/// Pins `t`, the tvar a solve instantiated `tparam` with, to the merge of its bounds.
///
/// `polarity` is the mark of `tparam` (see `UnmarkedFallback`). A negatively marked type parameter
/// prefers its upper bounds, any other its lower bounds. Without usable bounds, a marked one is
/// pinned to `error_fallback` if there is one, then to its default, and otherwise it is
/// underconstrained. An unmarked one is pinned to its default, and otherwise as
/// `unmarked_fallback` says. An upper bound that can't be merged makes a marked type parameter
/// underconstrained.
fn pin_type<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    unmarked_fallback: UnmarkedFallback,
    use_op: &UseOp,
    tparam: &TypeParam,
    polarity: Option<Polarity>,
    error_fallback: Option<Type>,
    instantiation_reason: &Reason,
    t: &Type,
) -> Result<Type, FlowJsException> {
    let tparam_binder_reason = type_util::reason_of_t(t);
    // An `empty` lower bound carries no values, e.g. the one an omitted optional argument gives
    // `T` in `(x?: T)`. It is not evidence against a default, so a tparam with a default falls
    // back to it as if it had no lower bounds. Without a default, `empty` is still the solution.
    let filter_empty = tparam.default.is_some();
    // The merged lower bounds, in the form of merged upper bounds.
    let lower = |cx: &mut Context<'cx>| {
        Ok::<_, FlowJsException>(match merge_lower_bounds(cx, env, filter_empty, t)? {
            Some(t) => UseTResult::UpperT(t),
            None => UseTResult::UpperEmpty,
        })
    };
    let upper = |cx: &mut Context<'cx>| merge_upper_bounds(cx, env, &mut BTreeSet::new(), t);
    let bounds = match polarity {
        Some(Polarity::Negative) => match upper(cx)? {
            UseTResult::UpperEmpty => lower(cx)?,
            bounds => bounds,
        },
        None | Some(Polarity::Positive | Polarity::Neutral) => match lower(cx)? {
            UseTResult::UpperEmpty => upper(cx)?,
            bounds => bounds,
        },
    };
    let marked = polarity.is_some();
    match bounds {
        UseTResult::UpperT(inferred) => {
            log_pinned(cx, tparam, &inferred);
            Ok(inferred)
        }
        UseTResult::UpperPlaceholder(inferred) => {
            cx.set_synthesis_produced_uncacheable_result();
            Ok(inferred)
        }
        UseTResult::UpperEmpty if marked && let Some(inferred) = &error_fallback => {
            log_pinned(cx, tparam, inferred);
            Ok(inferred.dupe())
        }
        UseTResult::UpperNonT(u) if marked => {
            flow_typing_debug::verbose::print_if_verbose_lazy(cx, None, None, None, || {
                vec![
                    "Underconstrained due to upper_non_t".to_string(),
                    string_of_use_ctor(&u),
                ]
            });
            underconstrained(
                cx,
                env,
                use_op,
                tparam,
                tparam_binder_reason,
                instantiation_reason,
            )
        }
        UseTResult::UpperEmpty | UseTResult::UpperNonT(_) => unmarked_fallback.without_bounds(
            cx,
            env,
            use_op,
            tparam,
            marked,
            tparam_binder_reason,
            instantiation_reason,
        ),
    }
}

fn mk_inference_targ<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    typeparam: &TypeParam,
    reason_op: &Reason,
    reason_tapp: &Reason,
) -> Type {
    let reason = instantiation_utils::implicit_type_argument::mk_targ_reason(
        typeparam,
        reason_op,
        reason_tapp,
    );
    let inference_id = mk_id() as i32;
    cx.add_inference_node(inference_id);
    let tvar = Type::new(TypeInner::ImplicitInstantiationTvar(Box::new(
        ImplicitInstantiationTvarData {
            reason,
            name: typeparam.name.dupe(),
            bound: typeparam.bound.dupe(),
            id: inference_id,
        },
    )));
    env.add_implicit_instantiation_tvar(inference_id, tvar.dupe(), typeparam.default.is_some());
    tvar
}

fn force_type_arguments(cx: &mut Context<'_>, targs: &[Targ]) {
    struct Forcer {
        seen: BTreeSet<i32>,
        // Whether the walk skipped an unresolved tvar or a state that is being forced. Either can
        // gain unforced lazy tvars later, so the seen tvars are not fully forced.
        //
        // An EvalT missing from `evaluated` does not make the walk incomplete. Annotation EvalTs
        // of the checked file are evaluated when converted, and evaluations during a solve are
        // rolled back, so counting it would keep walks through generic library types from ever
        // being recorded.
        incomplete: bool,
    }

    impl TypeVisitor<()> for Forcer {
        fn tvar<'cx>(
            &mut self,
            cx: &mut Context<'cx>,
            pole: Polarity,
            (): (),
            _reason: &Reason,
            id: u32,
        ) {
            let (root_id, constraints) = cx.find_constraints(id as i32);
            if cx.is_fully_forced_tvar(root_id) || !self.seen.insert(root_id) {
                return;
            }
            match constraints {
                constraint::Constraints::FullyResolved(state)
                    if !cx.is_active_tvar_forcing_state(&state) =>
                {
                    let t = cx.force_fully_resolved_tvar(&state);
                    self.type_(cx, pole, (), &t);
                }
                constraint::Constraints::Resolved(t) => self.type_(cx, pole, (), &t),
                _ => self.incomplete = true,
            }
        }
    }

    let mut forcer = Forcer {
        seen: BTreeSet::new(),
        incomplete: false,
    };
    for targ in targs {
        if let Targ::ExplicitArg(t) = targ {
            forcer.type_(cx, Polarity::Positive, (), t);
        }
    }
    if !forcer.incomplete {
        cx.add_fully_forced_tvars(forcer.seen);
    }
}

// Checks the operation of `check` with the poly type instantiated with its explicit type
// arguments, and with new tvars for the others. Returns its type arguments and the result of the
// operation, if it has one.
fn check_instantiation<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    check: &ImplicitInstantiationCheck,
) -> Result<(Vec<SolveTarg>, Option<Type>), FlowJsException> {
    let (ref use_op, ref reason_op, ref op) = check.operation;
    let reason_tapp = type_util::reason_of_t(&check.lhs);
    let (ref tparams_loc, ref tparams, _) = check.poly_t;

    let merge_targs = |cx: &mut Context<'cx>,
                       explicit_targs: Option<&[Targ]>|
     -> Result<Vec<SolveTarg>, FlowJsException> {
        if let Some(explicit_targs) = explicit_targs
            && explicit_targs.len() > tparams.len()
        {
            flow_js_utils::add_output_with_env(
                cx,
                env,
                ErrorMessage::ETooManyTypeArgs(Box::new(ETooManyTypeArgsData {
                    reason_tapp: reason_tapp.to_error_reference(),
                    arity_loc: tparams_loc.dupe(),
                    maximum_arity: tparams.len() as i32,
                })),
            )?;
        }
        let mut targs = Vec::with_capacity(tparams.len());
        for (i, tparam) in tparams.iter().enumerate() {
            let targ = match explicit_targs.map(|explicit_targs| explicit_targs.get(i)) {
                None => SolveTarg {
                    tparam: tparam.dupe(),
                    t: mk_inference_targ(cx, env, tparam, reason_op, reason_tapp),
                    inferred: true,
                },
                Some(None) => {
                    let t = mk_inference_targ(cx, env, tparam, reason_op, reason_tapp);
                    if tparam.default.is_none() {
                        flow_js_utils::add_output_with_env(
                            cx,
                            env,
                            ErrorMessage::ETooFewTypeArgs(Box::new(ETooFewTypeArgsData {
                                reason_tapp: reason_tapp.to_error_reference(),
                                arity_loc: tparams_loc.dupe(),
                                minimum_arity: flow_js_utils::poly_minimum_arity(tparams) as i32,
                            })),
                        )?;
                    }
                    SolveTarg {
                        tparam: tparam.dupe(),
                        t,
                        inferred: true,
                    }
                }
                Some(Some(Targ::ExplicitArg(t))) => SolveTarg {
                    tparam: tparam.dupe(),
                    t: t.dupe(),
                    inferred: false,
                },
                Some(Some(Targ::ImplicitArg(tvar))) => {
                    let reason = flow_common::reason::mk_reason(
                        VirtualReasonDesc::RImplicitInstantiation,
                        tvar.reason().loc().dupe(),
                    );
                    let t = mk_inference_targ(cx, env, tparam, &reason, reason_tapp);
                    FlowJs::flow_with_env(
                        cx,
                        env,
                        &t,
                        &UseT::new(UseTInner::UseT(
                            use_op.dupe(),
                            Type::new(TypeInner::OpenT(tvar.dupe())),
                        )),
                    )?;
                    // It is important to convert implicit args to explicit args so there won't be
                    // infinite loops between this module and flow_js.
                    SolveTarg {
                        tparam: tparam.dupe(),
                        t,
                        inferred: true,
                    }
                }
            };
            targs.push(targ);
        }
        Ok(targs)
    };
    let explicit_targs = |targs: &[SolveTarg]| -> Rc<[Targ]> {
        targs
            .iter()
            .map(|targ| Targ::ExplicitArg(targ.t.dupe()))
            .collect()
    };
    let instance = |cx: &mut Context<'cx>, targs: &[SolveTarg]| {
        FlowJs::mk_typeapp_instance_annot_with_env(
            cx,
            env,
            None,
            use_op.dupe(),
            reason_op,
            reason_tapp,
            true,
            None,
            &check.lhs,
            targs.iter().map(|targ| targ.t.dupe()).collect(),
        )
    };

    let (targs, lower_t, use_t, tout) = match op {
        Operation::Call(calltype) => {
            let new_tout = flow_typing_tvar::mk_no_wrap(cx, reason_op);
            let targs = merge_targs(cx, calltype.call_targs.as_deref())?;
            let call_t = UseT::new(UseTInner::CallT(Box::new(CallTData {
                use_op: use_op.dupe(),
                reason: reason_op.dupe(),
                call_action: Box::new(CallAction::Funcalltype(Box::new(FuncallType {
                    call_targs: Some(explicit_targs(&targs)),
                    call_tout: Tvar::new(reason_op.dupe(), new_tout as u32),
                    call_this_t: calltype.call_this_t.dupe(),
                    call_args_tlist: calltype.call_args_tlist.dupe(),
                    call_strict_arity: calltype.call_strict_arity,
                    call_speculation_hint_state: calltype.call_speculation_hint_state.dupe(),
                    call_specialized_callee: calltype.call_specialized_callee.clone(),
                }))),
                return_hint: hint_unavailable(),
            })));
            let tout = Type::new(TypeInner::OpenT(Tvar::new(
                reason_op.dupe(),
                new_tout as u32,
            )));
            (targs, check.lhs.dupe(), call_t, Some(tout))
        }
        Operation::SubtypeLowerPoly(u) => {
            let targs = merge_targs(cx, None)?;
            let lower_t = instance(cx, &targs)?;
            let use_t = UseT::new(UseTInner::UseT(use_op.dupe(), u.dupe()));
            (targs, lower_t, use_t, None)
        }
        Operation::Method {
            prop_reason,
            propref,
            call,
            optional_chain,
        } => {
            let targs = merge_targs(cx, None)?;
            let new_tout = flow_typing_tvar::mk(cx, reason_op.dupe());
            let methodcalltype = MethodCallType {
                meth_tout: open_tvar(&new_tout).dupe(),
                ..call.clone()
            };
            let method_action = match optional_chain {
                Some(chain) => MethodAction::ChainM(Box::new(ChainMData {
                    exp_reason: chain.exp_reason.dupe(),
                    lhs_reason: chain.lhs_reason.dupe(),
                    lhs_expression: chain.lhs_expression.dupe(),
                    methodcalltype,
                    voided_out_collector: None,
                    return_hint: hint_unavailable(),
                    specialized_callee: None,
                })),
                None => MethodAction::CallM(Box::new(CallMData {
                    methodcalltype,
                    return_hint: hint_unavailable(),
                    specialized_callee: None,
                })),
            };
            let lower_t = instance(cx, &targs)?;
            let method_t = UseT::new(UseTInner::MethodT(Box::new(MethodTData {
                use_op: use_op.dupe(),
                reason: reason_op.dupe(),
                prop_reason: prop_reason.dupe(),
                propref: Box::new(propref.clone()),
                method_action: Box::new(method_action),
            })));
            (targs, lower_t, method_t, Some(new_tout))
        }
        Operation::Constructor(explicit, call_args) => {
            let new_tout = flow_typing_tvar::mk(cx, reason_op.dupe());
            let targs = merge_targs(cx, explicit.as_deref())?;
            let constructor_t = UseT::new(UseTInner::ConstructorT(Box::new(ConstructorTData {
                use_op: use_op.dupe(),
                reason: reason_op.dupe(),
                targs: Some(explicit_targs(&targs)),
                args: call_args.dupe(),
                tout: new_tout.dupe(),
                return_hint: hint_unavailable(),
                specialized_ctor: None,
            })));
            (targs, check.lhs.dupe(), constructor_t, Some(new_tout))
        }
        Operation::ReactJSX {
            jsx_props,
            targs: explicit,
        } => {
            let new_tout = flow_typing_tvar::mk(cx, reason_op.dupe());
            let targs = merge_targs(cx, explicit.as_deref())?;
            let lower = instance(cx, &targs)?;
            let react_kit_t = UseT::new(UseTInner::ReactKitT(Box::new(ReactKitTData {
                use_op: use_op.dupe(),
                reason: reason_op.dupe(),
                tool: Box::new(react::Tool::<Cx>::CreateElement(Box::new(
                    react::CreateElementData {
                        component: lower.dupe(),
                        jsx_props: jsx_props.dupe(),
                        targs: None,
                        tout: open_tvar(&new_tout).dupe(),
                        return_hint: hint_unavailable(),
                        inferred_targs: None,
                        specialized_component: None,
                        specialized_props: None,
                    },
                ))),
            })));
            (targs, lower, react_kit_t, Some(new_tout))
        }
    };
    FlowJs::flow_with_env(cx, env, &lower_t, &use_t)?;
    Ok((targs, tout))
}

fn type_param_pin_order<'cx>(cx: &mut Context<'cx>, tparams: &[TypeParam]) -> Vec<usize> {
    fn visit(
        index: usize,
        dependencies: &[BTreeSet<usize>],
        visited: &mut [bool],
        order: &mut Vec<usize>,
    ) {
        if visited[index] {
            return;
        }
        visited[index] = true;
        for dependency in &dependencies[index] {
            visit(*dependency, dependencies, visited, order);
        }
        order.push(index);
    }

    let indices = tparams
        .iter()
        .enumerate()
        .map(|(index, tparam)| (tparam.name.dupe(), index))
        .collect::<BTreeMap<_, _>>();
    let dependencies = tparams
        .iter()
        .map(|tparam| {
            let mut dependencies = type_subst::free_var_finder(cx, None, &tparam.bound);
            if let Some(default) = &tparam.default {
                dependencies.extend(type_subst::free_var_finder(cx, None, default));
            }
            dependencies
                .iter()
                .filter_map(|name| indices.get(name).copied())
                .collect::<BTreeSet<_>>()
        })
        .collect::<Vec<_>>();
    let mut visited = vec![false; tparams.len()];
    let mut order = Vec::with_capacity(tparams.len());
    for index in 0..tparams.len() {
        visit(index, &dependencies, &mut visited, &mut order);
    }
    // Postorder is dependency-first. Pin dependents first, while retaining the
    // old reverse-source order for unrelated parameters.
    order.reverse();
    order
}

fn pin_types<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    unmarked_fallback: UnmarkedFallback,
    use_op: &UseOp,
    has_new_errors: bool,
    has_syntactic_hint: bool,
    targs: &[SolveTarg],
    marked_tparams: &Marked<SubstName>,
    check: &ImplicitInstantiationCheck,
) -> Result<(BTreeMap<SubstName, GeneralizedTarg>, Vec<(Type, SubstName)>), FlowJsException> {
    let (_, ref instantiation_reason, ref op) = check.operation;

    // Implicit instantiation can bail out early (e.g. a non-`DefT` lower bound or a static call
    // through a `this` instance) without any type arguments, even though the poly type still
    // declares tparams.
    if targs.is_empty() {
        return Ok((BTreeMap::new(), Vec::new()));
    }

    let subst_map: FlowOrdMap<SubstName, Type> = targs
        .iter()
        .map(|targ| (targ.tparam.name.dupe(), targ.t.dupe()))
        .collect();

    let mut result_map = BTreeMap::new();
    let mut result_list = vec![None; targs.len()];
    for index in type_param_pin_order(cx, &check.poly_t.1) {
        let SolveTarg {
            tparam,
            t,
            inferred,
        } = &targs[index];
        let inferred = if *inferred {
            // If the operation failed, a type parameter without usable bounds is not reported as
            // underconstrained.
            let error_fallback = has_new_errors.then(|| {
                let reason = type_util::reason_of_t(t).dupe();
                match *cx.typing_mode() {
                    TypingMode::CheckingMode => any_t::error(reason),
                    TypingMode::SynthesisMode { .. } | TypingMode::HintEvaluationMode => {
                        any_t::placeholder(reason)
                    }
                }
            });
            pin_type(
                cx,
                env,
                unmarked_fallback,
                use_op,
                tparam,
                marked_tparams.get(&tparam.name),
                error_fallback,
                instantiation_reason,
                t,
            )?
        } else {
            log_pinned(cx, tparam, t);
            t.dupe()
        };

        let bound_t = type_subst::subst(
            cx,
            Some(unknown_use()),
            true,
            false,
            type_subst::Purpose::Normal,
            &subst_map,
            tparam.bound.dupe(),
        );
        FlowJs::flow_t_with_env(cx, env, t, &bound_t)?;

        let call_loc = instantiation_reason.loc().dupe();
        let generalized = match op {
            Operation::Call(_) | Operation::Constructor(_, _) | Operation::Method { .. }
                if tparam.is_const =>
            {
                // Adjust 'const' type parameters. Keeping container reasons for
                // compatibility with existing code.
                natural_inference::convert_literal_type_to_const(call_loc, cx, inferred.dupe())
            }
            _ if let deref!(TypeInner::DefT(_, deref!(DefTInner::NumGeneralT(_)))) = &bound_t => {
                inferred.dupe()
            }
            _ => {
                // Prevent leaking of precise singleton types
                generalize_singletons(cx, &call_loc, has_syntactic_hint, inferred.dupe())
            }
        };
        result_map.insert(
            tparam.name.dupe(),
            GeneralizedTarg {
                tparam: tparam.dupe(),
                inferred,
                generalized: generalized.dupe(),
            },
        );
        result_list[index] = Some((generalized, tparam.name.dupe()));
    }

    let result_list = result_list.into_iter().flatten().collect();
    Ok((result_map, result_list))
}

// The type arguments of `check`'s operation and its result, as `check_instantiation` gives them,
// and the polarity of each type parameter that the result mentions.
fn implicitly_instantiate<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    check: &ImplicitInstantiationCheck,
) -> Result<(Vec<SolveTarg>, Marked<SubstName>, Option<Type>), FlowJsException> {
    let (_, ref tparams, ref t) = check.poly_t;
    let (_, _, ref op) = check.operation;
    let resolved_t = get_t(cx, t);
    // There are no other valid cases of implicit instantiation, but it is still possible
    // reach them via non-sensical cases that usually are downstream of some other error.
    // Since there's no reasonable thing to do in these cases we just ignore them.
    let bail_out = Ok((Vec::new(), Marked::new(), None));
    let marked_tparams = match (resolved_t.deref(), op) {
        (_, Operation::SubtypeLowerPoly(_) | Operation::Method { .. }) => Marked::new(),
        (TypeInner::DefT(_, def_t), _) => match (def_t.deref(), op) {
            // A JSX element is typed by its component's render type, which does not expose the
            // props, so no type parameter is observable in the result and an unconstrained one
            // can take its bound.
            (DefTInner::ReactAbstractComponentT(_), _)
            | (DefTInner::FunT(..), Operation::ReactJSX { .. }) => Marked::new(),
            (DefTInner::FunT(_, funtype), _) => {
                let tparams_map: BTreeMap<SubstName, TypeParam> = tparams
                    .iter()
                    .map(|tparam| (tparam.name.dupe(), tparam.dupe()))
                    .collect();
                let tparam_names: FlowOrdSet<SubstName> =
                    tparams.iter().map(|tparam| tparam.name.dupe()).collect();
                let mut visitor = ImplicitInstantiationVisitor {
                    tparams_map: &tparams_map,
                };
                let (marked_tparams, _) = visitor.type_(
                    cx,
                    Polarity::Positive,
                    (Marked::new(), tparam_names),
                    &funtype.return_t,
                );
                marked_tparams
            }
            // This case is hit when calling a static function. We will implicitly
            // instantiate the type variables on the class, but using an instance's
            // type params in a static method does not make sense. We ignore this case
            // intentionally
            (DefTInner::ClassT(inner), Operation::Call(_))
                if matches!(inner.deref(), TypeInner::ThisInstanceT(..)) =>
            {
                return bail_out;
            }
            (DefTInner::ClassT(inner), _)
                if matches!(inner.deref(), TypeInner::ThisInstanceT(..)) =>
            {
                tparams.iter().fold(Marked::new(), |mut marked, tparam| {
                    marked.add(tparam.name.dupe(), tparam.polarity);
                    marked
                })
            }
            _ => return bail_out,
        },
        _ => return bail_out,
    };
    let (targs, tout) = check_instantiation(cx, env, check)?;
    Ok((targs, marked_tparams, tout))
}

/// Pins `t`, the element type of an empty array literal decomposed from its hint, to the merge of
/// its bounds, or to a fresh tvar if it has none.
pub fn pin_tvar<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    t: &Type,
) -> Result<Type, FlowJsException> {
    let bounds = match merge_lower_bounds(cx, env, false, t)? {
        Some(t) => UseTResult::UpperT(t),
        None => merge_upper_bounds(cx, env, &mut BTreeSet::new(), t)?,
    };
    match bounds {
        UseTResult::UpperT(inferred) => Ok(inferred),
        UseTResult::UpperPlaceholder(inferred) => {
            cx.set_synthesis_produced_uncacheable_result();
            Ok(inferred)
        }
        UseTResult::UpperEmpty | UseTResult::UpperNonT(_) => {
            Ok(flow_typing_tvar::mk(cx, type_util::reason_of_t(t).dupe()))
        }
    }
}

pub mod instantiation_solver {

    use super::*;

    // Solves the type arguments of `check`'s operation, including how to instantiate the generic
    // values that the solve meets ("polymorphic lowers" in the names below).
    //
    // A generic value can't be instantiated until the solve knows the type it flows to, and that
    // type can depend on the rest of the operation:
    //
    //   declare function id<T>(x: T): T;
    //   declare function apply<S>(fn: (S) => S, initial: S): S;
    //   apply(id, 1);
    //
    // `id` flows to `(S) => S` and needs `S`, which comes from `1`. So the solve runs up to three
    // times:
    //
    // 1. Solve once, giving placeholder type arguments to each generic value that flows to a type
    //    mentioning the solve's tvars (see `kit::polymorphic_lower_targs`). If no generic value
    //    needed placeholders, this solve is the answer. That is the common case.
    // 2. Solve as a hint, with those generic values still placeholders. Pinning ignores the bounds
    //    that `id<placeholder>` gives `S`, so `1` decides: `S = number`.
    // 3. Solve for real. Each of those generic values is checked where this run meets it, against
    //    the type it flows to with the hint solution in place of the solve's tvars. `id` is
    //    checked against `(number) => number`, so it is instantiated as `id<number>`.
    //
    // A run doesn't look for the generic values: it meets them. It checks the operation as usual:
    // `apply` is instantiated with new tvars for its type parameters,
    // `(fn: (S') => S', initial: S') => S'`, and each argument flows to its parameter. So the type
    // a generic value flows to is known when the run meets it: `id ~> (S') => S'` reaches the
    // `PolyT` lower case in `subtyping_kit` with both. Every run does the same flows, so it meets
    // the same generic values at the same places, each time with its own tvars. Only the hint
    // solution is kept from one run to the next, by type parameter name.
    //
    // This applies to any generic value the solve meets, whether it is an argument like `id` or,
    // say, the generic `then` method of a `Promise` argument checked against an interface.
    //
    // A generic value does not see how the others are instantiated. With
    //
    //   declare function pipe<A, B, C>(a: A, f: (A) => B, g: (B) => C): C;
    //   declare function wrap<T>(x: T): Array<T>;
    //   pipe('a', wrap, wrap);
    //
    // the second `wrap` needs `B`, which only the first `wrap`'s instantiation provides. The hint
    // solve knows neither `B` nor `C`, so in step 3 the second `wrap` is checked against
    // `(<placeholder>) => <placeholder>`, its `T` is its bound, and the call returns
    // `Array<unknown>`.
    //
    // The hint solve uses the return hint too. With
    //
    //   declare function lift<T>(f: (T) => T): (T) => T;
    //   const lifted: (string) => string = lift(id);
    //
    // `id` is checked against `(string) => string`. The errors of step 1, including the ones
    // deferred to an enclosing speculation branch, are dropped, because step 3 redoes it.
    pub fn solve_targs<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        use_op: UseOp,
        allow_underconstrained: bool,
        has_syntactic_hint: bool,
        return_hint: Option<(Type, flow_common::hint::HintKind)>,
        check: &ImplicitInstantiationCheck,
    ) -> Result<
        (
            BTreeMap<subst_name::SubstName, GeneralizedTarg>,
            Vec<(Type, subst_name::SubstName)>,
        ),
        FlowJsException,
    > {
        let init_errors = cx.errors();
        let init_deferred_errors = env.deferred_error_count();
        let solution = solve_targs_once(
            cx,
            env,
            UnmarkedFallback::Bound,
            use_op.dupe(),
            allow_underconstrained,
            has_syntactic_hint,
            return_hint.clone(),
            check,
        )?;
        if !env.gave_polymorphic_lower_placeholders() {
            return Ok(solution);
        }
        cx.reset_errors(init_errors);
        env.truncate_deferred_errors(init_deferred_errors);

        // The first solve's solution can't be the hint. It is pinned in checking mode, as the
        // answer: there, a type parameter without evidence or a default is an error `any`, and its
        // literals stay precise under a syntactic hint. A hint is what the generic values are
        // checked against, so a type parameter the solve knows nothing about has to stay a
        // placeholder, which tells the check nothing. Nor can the first solve run in hint
        // evaluation mode, because it is the answer when no generic value needed placeholders.
        //
        // The hint solve is sandboxed. Hint evaluation mode makes a type parameter without
        // evidence or a default a placeholder, and the caches are restored after it. It runs
        // without speculation, so that its errors can't fail an enclosing speculation branch or be
        // deferred to it, and they are dropped. Underconstrained type parameters aren't allowed,
        // so pinning follows the polarity of each one in the return type. There is no syntactic
        // hint, so literals are widened: `apply(id, 1)` hints `S = number`, not `S = 1`.
        let original_errors = cx.errors();
        let hint_solution = cx.run_in_hint_eval_mode(|cx| {
            solve_targs_once(
                cx,
                &env.without_speculation(),
                UnmarkedFallback::Underconstrained,
                unknown_use(),
                false,
                false,
                return_hint.clone(),
                check,
            )
        });
        cx.reset_errors(original_errors);
        let hints = sandboxed(hint_solution)?
            .map(|(solution, _)| {
                solution
                    .into_iter()
                    .map(|(name, targ)| (name, targ.to_type()))
                    .collect()
            })
            .unwrap_or_default();

        env.set_polymorphic_lower_hints(hints);
        solve_targs_once(
            cx,
            env,
            UnmarkedFallback::Bound,
            use_op,
            allow_underconstrained,
            has_syntactic_hint,
            return_hint,
            check,
        )
    }

    // The outcome of a sandboxed solve: its result, or `None` if it failed a speculation or hit a
    // limit. Those failures only mean there is no result, as when a hint evaluation fails.
    // Cancellation and timeouts still abort the whole check.
    pub(super) fn sandboxed<T>(
        result: Result<T, FlowJsException>,
    ) -> Result<Option<T>, FlowJsException> {
        match result {
            Ok(t) => Ok(Some(t)),
            Err(
                FlowJsException::Speculative(_)
                | FlowJsException::SpeculationSingletonError
                | FlowJsException::LimitExceeded,
            ) => Ok(None),
            Err(
                e @ (FlowJsException::WorkerCanceled(_)
                | FlowJsException::TimedOut(_)
                | FlowJsException::DebugThrow { .. }),
            ) => Err(e),
        }
    }

    fn solve_targs_once<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        unmarked_fallback: UnmarkedFallback,
        use_op: UseOp,
        allow_underconstrained: bool,
        has_syntactic_hint: bool,
        return_hint: Option<(Type, flow_common::hint::HintKind)>,
        check: &ImplicitInstantiationCheck,
    ) -> Result<
        (
            BTreeMap<subst_name::SubstName, GeneralizedTarg>,
            Vec<(Type, subst_name::SubstName)>,
        ),
        FlowJsException,
    > {
        let targs = match &check.operation.2 {
            Operation::Call(call) => call.call_targs.as_deref(),
            Operation::Constructor(targs, _) | Operation::ReactJSX { targs, .. } => {
                targs.as_deref()
            }
            Operation::SubtypeLowerPoly(_) => None,
            Operation::Method { call, .. } => call.meth_targs.as_deref(),
        };
        if let Some(targs) = targs {
            force_type_arguments(cx, targs);
        }
        // Push a preservation level OUTSIDE run_and_rolled_back_cache so it
        // sits below the rollback truncation line and survives the restore.
        // Entries moved here remain visible to add() (which checks all levels),
        // so subsequent operations still get cache hits — exactly matching the
        // original behavior where the captured snapshot stayed in the live cache.
        let preserve_level = cx.constraint_cache().level_count();
        cx.constraint_cache_mut().push_level();
        let mut preserved = false;
        let result = cx.run_and_rolled_back_cache(|cx| {
            let init_errors = cx.errors();
            let mut cache_snapshot = match &return_hint {
                Some((_, flow_common::hint::HintKind::BestEffortHint)) => {
                    Some(cx.take_cache_snapshot())
                }
                _ => None,
            };
            // Instantiates the poly type and checks the operation. If that raises no errors, the
            // constraint cache entries it added are preserved, before the return hint or pinning
            // add entries that may be associated with error-producing checks.
            let instantiate = |cx: &mut Context<'cx>| {
                let (targs, marked_tparams, tout) = implicitly_instantiate(cx, env, check)?;
                let has_new_errors = !init_errors.ptr_eq(&cx.errors());
                if !has_new_errors {
                    cx.constraint_cache_mut()
                        .move_top_entries_to_level(preserve_level);
                }
                Ok::<_, FlowJsException>((targs, marked_tparams, tout, has_new_errors))
            };
            let (mut targs, mut marked_tparams, tout, mut has_new_errors) = instantiate(cx)?;
            preserved = !has_new_errors;

            if let (Some((hint, kind)), Some(tout)) = (&return_hint, &tout) {
                let errors_before_using_return_hint = cx.errors();
                let hint_exn = match FlowJs::flow_t_with_env(cx, env, tout, hint) {
                    Ok(()) => None,
                    Err(
                        e @ (FlowJsException::WorkerCanceled(_)
                        | FlowJsException::TimedOut(_)
                        | FlowJsException::DebugThrow { .. }),
                    ) => return Err(e),
                    Err(e) => Some(e),
                };
                let hint_failed = hint_exn.is_some()
                    || !errors_before_using_return_hint.ptr_eq(&cx.errors())
                    || (flow_js_utils::tvar_visitors::has_placeholders(cx, hint)
                        && placeholders_leaked_into_bounds(cx, env, &targs));
                if hint_failed && *kind == flow_common::hint::HintKind::BestEffortHint {
                    // Solve again without the hint, from the same state as before it.
                    cx.restore_cache_snapshot(cache_snapshot.take().expect(
                        "best-effort return hints should have captured a cache snapshot",
                    ));
                    cx.reset_errors(init_errors.dupe());
                    cx.constraint_cache_mut().clear_level(preserve_level);
                    (targs, marked_tparams, _, has_new_errors) = instantiate(cx)?;
                    preserved = !has_new_errors;
                } else if let Some(e) = hint_exn {
                    return Err(e);
                }
            }

            cx.reset_errors(Default::default());

            // A solve that allows underconstrained type parameters pins them all as unmarked.
            if allow_underconstrained {
                marked_tparams = Marked::new();
            }
            let result = pin_types(
                cx,
                env,
                unmarked_fallback,
                &use_op,
                has_new_errors,
                has_syntactic_hint,
                &targs,
                &marked_tparams,
                check,
            );
            let implicit_instantiation_errors = cx.errors().filter(|error| {
                // Since we will be performing the same check again using the solution
                // of the implicit instantiation, we only need to keep errors related
                // to pinning types, eg. [underconstrained-implicit-instantiation].
                matches!(
                    error.msg,
                    ErrorMessage::EImplicitInstantiationUnderconstrainedError(box EImplicitInstantiationUnderconstrainedErrorData { .. })
                        | ErrorMessage::EInternal(box (_, _))
                )
            });
            cx.reset_errors(init_errors.union(&implicit_instantiation_errors));
            result
        });
        // After run_and_rolled_back_cache truncates above the preserve level,
        // the preserve level survives with the entries from implicitly_instantiate.
        // If preserved, leave the level as-is — entries are visible to add()/fold().
        // If not preserved, discard the empty preserve level.
        if !preserved {
            cx.constraint_cache_mut().truncate_to(preserve_level);
        }
        result
    }

    pub(crate) fn solve_conditional_type_targs<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: &UseOp,
        reason: &Reason,
        tparams: &[TypeParam],
        check_t: &Type,
        extends_t: &Type,
        true_t: &Type,
        restrictive_frozen_tvars: bool,
    ) -> Result<Option<FlowOrdMap<SubstName, Type>>, FlowJsException> {
        let mut subst_map = FlowOrdMap::new();
        let mut inferred_targ_list: Vec<(SubstName, Type, Type)> = Vec::new();
        for tp in tparams {
            let targ = instantiation_utils::implicit_type_argument::mk_targ(cx, tp, reason, reason);
            subst_map.insert(tp.name.dupe(), targ.dupe());
            inferred_targ_list.push((tp.name.dupe(), targ, tp.bound.dupe()));
        }
        // OCaml builds this list with `::` (prepend) inside a left fold and does
        // not reverse it afterwards, so the list ends up in reverse tparam order
        // and is pinned in that order below. We push forward (keeping `mk_targ`'s
        // tvar-creation order identical to OCaml's left fold) and reverse once to
        // get that same pinning order in O(n). The order is significant: e.g. for
        // `then(cb: infer F, ...args: infer _)`, pinning the rest tparam `_` first
        // reverse-solves its spread to `[]`, which unblocks the multiflow so the
        // regular param `F` gets its real bound.
        inferred_targ_list.reverse();

        let extends_subst = type_subst::subst(
            cx,
            Some(unknown_use()),
            true,
            false,
            type_subst::Purpose::Normal,
            &subst_map,
            extends_t.dupe(),
        );
        let check_env = if restrictive_frozen_tvars {
            env.with_restrictive_frozen_tvars()
        } else {
            env.dupe()
        };
        if speculative_subtyping_succeeds(
            cx,
            &check_env,
            trace,
            use_op.dupe(),
            check_t,
            &extends_subst,
        )? {
            let (tparams_map, tparams_set) = tparams.iter().fold(
                (BTreeMap::new(), FlowOrdSet::new()),
                |(mut map, mut set), tp| {
                    map.insert(tp.name.dupe(), tp.dupe());
                    set.insert(tp.name.dupe());
                    (map, set)
                },
            );
            let (marked_tparams, _) = {
                let mut visitor = ImplicitInstantiationVisitor {
                    tparams_map: &tparams_map,
                };
                visitor.type_(cx, Polarity::Positive, (Marked::new(), tparams_set), true_t)
            };

            // Pinning can finish the extends check: a reversed solution flows back into its tvar
            // and resolves what waited on it, e.g. the tuple type `[infer A, ...infer B]` or the
            // params `(a: infer A, ...rest: infer B)` once `B` is pinned. If that fails, so does
            // the check.
            let mut solution = None;
            let pinning = speculation_kit::try_singleton_custom_throw_on_failure(
                cx,
                env,
                Box::new(|cx, env| {
                    let mut result_map = FlowOrdMap::new();
                    for (name, targ, bound) in &inferred_targ_list {
                        let tparam = match tparams_map.get(name) {
                            Some(tp) => tp,
                            None => return Ok(()),
                        };
                        let inferred = pin_type(
                            cx,
                            env,
                            UnmarkedFallback::Bound,
                            use_op,
                            tparam,
                            marked_tparams.get(name),
                            Some(bound.dupe()),
                            reason,
                            targ,
                        )?;
                        if speculative_subtyping_succeeds(
                            cx,
                            env,
                            trace,
                            unknown_use(),
                            &inferred,
                            bound,
                        )? {
                            result_map.insert(name.dupe(), inferred);
                        } else {
                            return Ok(());
                        }
                    }
                    solution = Some(result_map);
                    Ok(())
                }),
            );
            match pinning {
                Ok(()) => Ok(solution),
                Err(FlowJsException::SpeculationSingletonError) => Ok(None),
                Err(e) => Err(e),
            }
        } else {
            Ok(None)
        }
    }
}

pub mod kit {
    use super::instantiation_solver::solve_conditional_type_targs;
    use super::instantiation_solver::solve_targs;
    use super::*;

    #[derive(Clone, Copy)]
    struct FrozenTvarInfo {
        has_default: bool,
    }

    fn frozen_tvar<'cx>(
        env: &FlowJsEnv,
        cx: &mut Context<'cx>,
        t: &Type,
    ) -> Option<(i32, FrozenTvarInfo)> {
        let node = type_util::constraint_node_id(t)?;
        let root_id = cx.find_constraints(node.id()).0;
        let (tvar, has_default) = env.frozen_implicit_instantiation_tvar_with_default(|id| {
            cx.find_constraints(id).0 == root_id
        })?;
        let TypeInner::ImplicitInstantiationTvar(data) = tvar.deref() else {
            return None;
        };
        Some((data.id, FrozenTvarInfo { has_default }))
    }

    fn frozen_tvar_id<'cx>(env: &FlowJsEnv, cx: &mut Context<'cx>, t: &Type) -> Option<i32> {
        frozen_tvar(env, cx, t).map(|(id, _)| id)
    }

    struct FrozenTvarSubstituter<'a> {
        env: &'a FlowJsEnv,
        // What to replace a frozen tvar with, by id. `None` leaves it to `unresolved`.
        substitution: &'a dyn Fn(i32) -> Option<Type>,
        // If set, every other unresolved tvar is replaced too: by the union of what has flowed
        // into it so far, or by this when nothing has. `expanding` holds the tvars whose lower
        // bounds are being replaced, so that a tvar that flows into itself stops there.
        unresolved: Option<Type>,
        expanding: BTreeSet<i32>,
        replaced_frozen_tvar_ids: BTreeSet<i32>,
    }

    impl<'cx> TypeMapper<'cx, ()> for FrozenTvarSubstituter<'_> {
        fn tvar(&mut self, _cx: &mut Context<'cx>, _map_cx: &(), _r: &Reason, id: u32) -> u32 {
            id
        }

        fn exports(&mut self, cx: &mut Context<'cx>, map_cx: &(), id: exports::Id) -> exports::Id {
            type_subst::exports(self, cx, map_cx, id)
        }

        fn call_prop(&mut self, cx: &mut Context<'cx>, map_cx: &(), id: i32) -> i32 {
            type_subst::call_prop(self, cx, map_cx, id)
        }

        fn props(
            &mut self,
            cx: &mut Context<'cx>,
            map_cx: &(),
            id: properties::Id,
        ) -> properties::Id {
            type_subst::props(self, cx, map_cx, id)
        }

        fn eval_id(&mut self, _cx: &mut Context<'cx>, _map_cx: &(), id: eval::Id) -> eval::Id {
            id
        }

        fn type_(&mut self, cx: &mut Context<'cx>, map_cx: &(), t: Type) -> Type {
            if let Some(id) = frozen_tvar_id(self.env, cx, &t)
                && let Some(replacement) = (self.substitution)(id)
            {
                self.replaced_frozen_tvar_ids.insert(id);
                return replacement;
            }
            if let Some(replacement) = self.unresolved.clone()
                && let Some(node) = type_util::constraint_node_id(&t)
                && let (root_id, constraint::Constraints::Unresolved(_)) =
                    cx.find_constraints(node.id())
            {
                // A frozen tvar that `substitution` left alone belongs to an enclosing solve that
                // is still inferring it, so what has flowed into it so far is not its answer.
                if frozen_tvar_id(self.env, cx, &t).is_some() || !self.expanding.insert(root_id) {
                    return replacement;
                }
                let lowers = flow_js_utils::possible_types(cx, root_id)
                    .into_iter()
                    .map(|lower| self.type_(cx, map_cx, lower))
                    .collect::<Vec<_>>();
                self.expanding.remove(&root_id);
                return if lowers.is_empty() {
                    replacement
                } else {
                    type_util::union_of_ts(type_util::reason_of_t(&t).dupe(), lowers, None)
                };
            }
            if let TypeInner::EvalT {
                type_, defer_use_t, ..
            } = t.deref()
            {
                let type_prime = self.type_(cx, map_cx, type_.dupe());
                let destructor_prime = self.destructor(cx, map_cx, defer_use_t.2.dupe());
                if !type_.ptr_eq(&type_prime) || !Rc::ptr_eq(&defer_use_t.2, &destructor_prime) {
                    return flow_cache::eval::id(
                        cx,
                        type_prime,
                        TypeDestructorT::new(TypeDestructorTInner(
                            defer_use_t.0.dupe(),
                            defer_use_t.1.dupe(),
                            destructor_prime,
                        )),
                    );
                }
                return t;
            }
            type_mapper::type_default(self, cx, map_cx, t)
        }
    }

    struct FrozenTvarCollector<'a> {
        env: &'a FlowJsEnv,
        frozen_tvars: BTreeMap<i32, FrozenTvarInfo>,
    }

    impl TypeVisitor<()> for FrozenTvarCollector<'_> {
        fn type_<'cx>(&mut self, cx: &mut Context<'cx>, pole: Polarity, _acc: (), t: &Type) {
            if let Some((id, info)) = frozen_tvar(self.env, cx, t) {
                self.frozen_tvars.insert(id, info);
            } else {
                type_visitor::type_default(self, cx, pole, (), t);
            }
        }
    }

    /// The type arguments to instantiate the generic value `l` with when it flows to `u` during a
    /// solve, or `None` if `l` doesn't flow to `u` at all.
    ///
    /// The examples use
    ///
    ///   declare function id<T>(x: T): T;
    ///   declare function apply<S>(fn: (S) => S, initial: S): S;
    ///   declare function f<T>(x: T, g: (string) => number): T;
    ///
    /// If the innermost solve (the innermost one that has tvars) checks a generic value against a
    /// type, `l` doesn't flow. That check only infers the type arguments of the value it checks,
    /// and the flow of that value, instantiated, meets `l` again outside the check. In
    /// `apply(id, 1)`, `id` is checked against `(number) => number`. Any generic value met while
    /// solving `id`'s `T` doesn't flow.
    ///
    /// Otherwise, which case applies depends on the tvars of ongoing solves that `l` or `u`
    /// mention.
    ///
    /// - Tvars of the innermost solve. That solve decides the type arguments (see
    ///   `instantiation_solver::solve_targs`). In its first two runs they are placeholders. In its
    ///   final run, `l` is checked against `u` with the solve's hints in place of its tvars. In
    ///   `apply(id, 1)`, `id` flows to `(S) => S`. It gets placeholders in the first two runs, and
    ///   in the final run it is checked against `(number) => number`, which gives `[number]`.
    /// - Only tvars of outer solves. The type arguments are placeholders. The outer solve meets
    ///   `l` again when it redoes its solve with what this solve computes, and decides then.
    /// - No tvars. `l` is checked against `u` right away. In `f(1, id)`, `id` flows to
    ///   `(string) => number`, so it is instantiated as `id<string>`, and the flow then reports
    ///   that `string` is not a `number`.
    pub(crate) fn polymorphic_lower_targs<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        l: &Type,
        tparams: &Vec1<TypeParam>,
        u: &Type,
    ) -> Result<Option<Vec<Type>>, FlowJsException> {
        // The tvars of all ongoing solves that `ts` mention. While solving a call of
        // `applyTo<T, U>(f: (T) => U, x: T): U`, the type `(T) => U` mentions `T` and `U`, and
        // `(string) => number` mentions none. A solve nested in the current one would see all of
        // them as frozen, which is how they are found.
        fn implicit_instantiation_tvars_in<'cx>(
            cx: &mut Context<'cx>,
            env: &FlowJsEnv,
            ts: &[&Type],
        ) -> BTreeSet<i32> {
            let mut collector = FrozenTvarCollector {
                env: &env.solving_implicit_instantiation(),
                frozen_tvars: BTreeMap::new(),
            };
            for t in ts {
                collector.type_(cx, Polarity::Positive, (), t);
            }
            collector.frozen_tvars.into_keys().collect()
        }

        if env.in_polymorphic_lower_check() {
            return Ok(None);
        }
        let mentioned = implicit_instantiation_tvars_in(cx, env, &[l, u]);
        let checked = if mentioned.is_empty() {
            Some(check_polymorphic_lower(cx, env, &BTreeMap::new(), l, u)?)
        } else {
            env.polymorphic_lower_hints(|id| mentioned.contains(&id))
                .map(|(owner, hints)| check_polymorphic_lower(cx, &owner, &hints, l, u))
                .transpose()?
        };
        Ok(Some(match checked {
            Some(targs) if targs.len() == tparams.len() => targs.to_vec(),
            _ => tparams
                .iter()
                .map(|tparam| cx.mk_placeholder(tparam.reason.dupe()))
                .collect(),
        }))
    }

    /// Checks the generic value `l` against `u`, the type it flows to, and returns the type
    /// arguments to instantiate `l` with.
    ///
    /// The examples use
    ///
    ///   declare function id<T>(x: T): T;
    ///   declare function pipe<A, B, C>(a: A, f: (A) => B, g: (B) => C): C;
    ///   declare function compose<A, B, C>(f: (B) => C, g: (A) => B): (A) => C;
    ///
    /// The tvars of the innermost solve in `u` are replaced, as in a hint, by what `solution`
    /// maps their names to, or by placeholders where it has nothing. In `pipe('a', id, id)`, the
    /// first `id` flows to `(A) => B`. With `A = string` and nothing for `B`, it is checked against
    /// `(string) => <placeholder>`, which gives `[string]`. They are looked up by name because
    /// every run of the solve has new tvars for the same type parameters, and all of them stay in
    /// its frame.
    ///
    /// The other tvars in `l` and `u` are replaced too: a tvar of an outer solve, which is still
    /// inferring it, by a placeholder, and any other unresolved tvar by what has flowed into it so
    /// far. This is what is known when the solve meets `l`, and `l` is checked only then. Bounds
    /// that reach those tvars later in the solve are not seen.
    ///
    /// A type argument that the check does not decide is its bound. In `compose(id, id)`,
    /// nothing decides either `id`'s `T`, so `compose(id, id)('a')` is `unknown`.
    ///
    /// The check is a single solve, and the generic values it meets don't flow (see
    /// `polymorphic_lower_targs`), so it never starts another check.
    pub(super) fn check_polymorphic_lower<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        solution: &BTreeMap<SubstName, Type>,
        l: &Type,
        u: &Type,
    ) -> Result<Rc<[Type]>, FlowJsException> {
        let TypeInner::DefT(_, def_t) = l.deref() else {
            return Ok(Rc::from([]));
        };
        let DefTInner::PolyT(box PolyTData { tparams, .. }) = def_t.deref() else {
            return Ok(Rc::from([]));
        };
        let placeholders = || {
            tparams
                .iter()
                .map(|tparam| any_t::placeholder(tparam.reason.dupe()))
                .collect()
        };
        let nested_env = env.without_speculation().checking_polymorphic_lower();
        let hints: BTreeMap<i32, Type> = env
            .implicit_instantiation_tvars()
            .iter()
            .filter_map(|tvar| match tvar.deref() {
                TypeInner::ImplicitInstantiationTvar(data) => Some((
                    data.id,
                    solution
                        .get(&data.name)
                        .map(|t| t.dupe())
                        .unwrap_or_else(|| any_t::placeholder(data.reason.dupe())),
                )),
                _ => None,
            })
            .collect();
        let substitution = |id| hints.get(&id).cloned();
        // Every other unresolved tvar is replaced too, by what has flowed into it so far, or by a
        // placeholder. Otherwise the check's own tvars, like `id`'s `T`, would flow into those
        // tvars and leave bounds on them that outlive the check.
        let mut substituter = FrozenTvarSubstituter {
            env: &nested_env,
            substitution: &substitution,
            unresolved: Some(any_t::placeholder(type_util::reason_of_t(u).dupe())),
            expanding: BTreeSet::new(),
            replaced_frozen_tvar_ids: BTreeSet::new(),
        };
        let hinted_l = substituter.type_(cx, &(), l.dupe());
        let hinted_u = substituter.type_(cx, &(), u.dupe());
        let poly_t = match hinted_l.deref() {
            TypeInner::DefT(_, def_t)
                if let DefTInner::PolyT(box PolyTData {
                    tparams_loc,
                    tparams,
                    t_out,
                    ..
                }) = def_t.deref() =>
            {
                (
                    tparams_loc.dupe(),
                    Vec1::try_from_vec(tparams.to_vec())
                        .expect("should have nonempty polymorphic type parameters"),
                    t_out.dupe(),
                )
            }
            _ => return Ok(placeholders()),
        };
        let check = ImplicitInstantiationCheck {
            lhs: hinted_l.dupe(),
            poly_t,
            operation: (
                unknown_use(),
                type_util::reason_of_t(u).dupe(),
                Operation::SubtypeLowerPoly(hinted_u),
            ),
        };
        // The check is sandboxed: its errors are dropped, and the caches are restored so that a
        // later flow of the same types is not skipped as already done. The real flow of `l`,
        // instantiated with these type arguments, reports the errors.
        let original_errors = cx.errors();
        let solution = cx.run_and_rolled_back_cache(|cx| {
            solve_targs(cx, &nested_env, unknown_use(), false, false, None, &check)
        });
        cx.reset_errors(original_errors);
        let Some((solution, _)) = super::instantiation_solver::sandboxed(solution)? else {
            return Ok(placeholders());
        };
        Ok(tparams
            .iter()
            .map(|tparam| match solution.get(&tparam.name) {
                Some(targ) => targ.generalized.dupe(),
                None => any_t::placeholder(tparam.reason.dupe()),
            })
            .collect())
    }

    // Where in the polytype body a tparam occurrence sits, for per-occurrence routing of
    // inferred vs generalized targs. In `mymap<T, U>(xs: Array<T>, cb: (x: T) => U): Array<U>`
    // called as `mymap(strs, x => ['x'])`, when the call is checked against the instantiated
    // signature:
    // - `xs: Array<T>` is CallbackSupplied; `x: T` and the outer `Array<U>` are Plain. All
    //   three keep the generalized targ: the caller's array is checked against
    //   `Array<string>` and the caller receives `Array<string>`, never narrowed to the
    //   call-site literal `'x'`.
    // - The callback return `U` is UserCallbackReturn, so it takes the inferred targ and the
    //   body `['x']` checks against `Array<'x'>` instead of tripping invariance against
    //   `Array<string>`.
    // Only callback returns take the inferred targ; all other occurrences share one
    // generalized targ.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Zone {
        // A position inside an outermost-FunT param: any FunT here is a user-supplied callback.
        CallbackSupplied,
        // Inside the return of a user-supplied callback: uses the inferred targ.
        UserCallbackReturn,
        // Caller-facing: uses the generalized targ.
        Plain,
    }

    // (zone, tparams bound by enclosing PolyT/ThisInstanceT, is outermost FunT)
    type InwardSplitCx = (Zone, BTreeSet<SubstName>, bool);

    // Renames occurrences of `targets` in `UserCallbackReturn` zones to fresh names, so that a
    // single substitution can map them to inferred targs and the rest to generalized ones.
    struct InwardSplitter<'a> {
        targets: &'a BTreeMap<SubstName, GeneralizedTarg>,
        renames: BTreeMap<SubstName, SubstName>,
        used: FlowOrdSet<SubstName>,
    }

    impl<'cx> TypeMapper<'cx, InwardSplitCx> for InwardSplitter<'_> {
        fn tvar(
            &mut self,
            _cx: &mut Context<'cx>,
            _map_cx: &InwardSplitCx,
            _r: &Reason,
            id: u32,
        ) -> u32 {
            id
        }

        fn exports(
            &mut self,
            cx: &mut Context<'cx>,
            map_cx: &InwardSplitCx,
            id: exports::Id,
        ) -> exports::Id {
            type_subst::exports(self, cx, map_cx, id)
        }

        fn call_prop(&mut self, cx: &mut Context<'cx>, map_cx: &InwardSplitCx, id: i32) -> i32 {
            type_subst::call_prop(self, cx, map_cx, id)
        }

        fn props(
            &mut self,
            cx: &mut Context<'cx>,
            map_cx: &InwardSplitCx,
            id: properties::Id,
        ) -> properties::Id {
            type_subst::props(self, cx, map_cx, id)
        }

        fn eval_id(
            &mut self,
            _cx: &mut Context<'cx>,
            _map_cx: &InwardSplitCx,
            id: eval::Id,
        ) -> eval::Id {
            id
        }

        fn type_(&mut self, cx: &mut Context<'cx>, map_cx: &InwardSplitCx, t: Type) -> Type {
            let (zone, bound, is_outermost_funt) = map_cx;
            match t.deref() {
                TypeInner::GenericT(generic)
                    if *zone == Zone::UserCallbackReturn
                        && self.targets.contains_key(&generic.name)
                        && !bound.contains(&generic.name) =>
                {
                    let name = self
                        .renames
                        .entry(generic.name.dupe())
                        .or_insert_with(|| {
                            // Minted against every free name in the polytype body, so a
                            // rename can never alias an unrelated generic.
                            let fresh = alpha_rename::subst_name(&generic.name, &self.used);
                            self.used.insert(fresh.dupe());
                            fresh
                        })
                        .dupe();
                    Type::new(TypeInner::GenericT(Box::new(GenericTData {
                        name,
                        ..(**generic).clone()
                    })))
                }
                TypeInner::DefT(r, def_t) if let DefTInner::FunT(statics, ft) = def_t.deref() => {
                    // The outermost FunT's params are what the user supplies, and its return faces
                    // the caller. A FunT nested in a user-supplied position is itself a callback:
                    // its return is the callback body's output, its params are values it receives.
                    let (params_zone, return_zone) = if *is_outermost_funt {
                        (Zone::CallbackSupplied, Zone::Plain)
                    } else if *zone != Zone::Plain {
                        (Zone::Plain, Zone::UserCallbackReturn)
                    } else {
                        (Zone::Plain, Zone::Plain)
                    };
                    let params_cx = (params_zone, bound.clone(), false);
                    let return_cx = (return_zone, bound.clone(), false);
                    let this_t = self.type_(cx, &params_cx, ft.this_t.0.dupe());
                    let params: Rc<[FunParam]> = ft
                        .params
                        .iter()
                        .map(|FunParam(name, t)| {
                            FunParam(name.dupe(), self.type_(cx, &params_cx, t.dupe()))
                        })
                        .collect();
                    let rest_param = ft.rest_param.as_ref().map(|FunRestParam(name, loc, t)| {
                        FunRestParam(
                            name.dupe(),
                            loc.dupe(),
                            self.type_(cx, &params_cx, t.dupe()),
                        )
                    });
                    let return_t = self.type_(cx, &return_cx, ft.return_t.dupe());
                    let type_guard = ft
                        .type_guard
                        .as_ref()
                        .map(|tg| self.func_type_guard(cx, &return_cx, tg));
                    let unchanged = this_t.ptr_eq(&ft.this_t.0)
                        && params
                            .iter()
                            .zip(ft.params.iter())
                            .all(|(a, b)| a.1.ptr_eq(&b.1))
                        && match (&rest_param, &ft.rest_param) {
                            (Some(a), Some(b)) => a.2.ptr_eq(&b.2),
                            _ => true,
                        }
                        && return_t.ptr_eq(&ft.return_t)
                        && match (&type_guard, &ft.type_guard) {
                            (Some(a), Some(b)) => a.ptr_eq(b),
                            _ => true,
                        };
                    if unchanged {
                        t
                    } else {
                        let ft = FunType {
                            this_t: (this_t, ft.this_t.1.clone()),
                            params,
                            rest_param,
                            return_t,
                            type_guard,
                            ..(**ft).clone()
                        };
                        Type::new(TypeInner::DefT(
                            r.dupe(),
                            DefT::new(DefTInner::FunT(statics.dupe(), Rc::new(ft))),
                        ))
                    }
                }
                TypeInner::DefT(_, def_t) if let DefTInner::PolyT(poly) = def_t.deref() => {
                    let mut bound = bound.clone();
                    bound.extend(poly.tparams.iter().map(|tp| tp.name.dupe()));
                    type_mapper::type_default(self, cx, &(*zone, bound, *is_outermost_funt), t)
                }
                TypeInner::ThisInstanceT(this) => {
                    let mut bound = bound.clone();
                    bound.insert(this.subst_name.dupe());
                    type_mapper::type_default(self, cx, &(*zone, bound, *is_outermost_funt), t)
                }
                TypeInner::EvalT {
                    type_, defer_use_t, ..
                } => {
                    let type_prime = self.type_(cx, map_cx, type_.dupe());
                    let destructor_prime = self.destructor(cx, map_cx, defer_use_t.2.dupe());
                    if type_.ptr_eq(&type_prime) && Rc::ptr_eq(&defer_use_t.2, &destructor_prime) {
                        t
                    } else {
                        flow_cache::eval::id(
                            cx,
                            type_prime,
                            TypeDestructorT::new(TypeDestructorTInner(
                                defer_use_t.0.dupe(),
                                defer_use_t.1.dupe(),
                                destructor_prime,
                            )),
                        )
                    }
                }
                _ => type_mapper::type_default(self, cx, map_cx, t),
            }
        }
    }

    // A generic callback pins its own tparams from both the param side and the
    // return side, so routing those sides to different precisions breaks the
    // callback's internal coherence. Conservatively matches any generic
    // argument; non-callback generic values rarely widen, so they skip anyway.
    fn has_generic_arg<'cx>(cx: &mut Context<'cx>, check: &ImplicitInstantiationCheck) -> bool {
        let args: &[CallArg] = match &check.operation.2 {
            Operation::Call(calltype) => calltype.call_args_tlist.as_ref(),
            Operation::Constructor(_, args) => args.as_ref(),
            Operation::Method { call, .. } => call.meth_args_tlist.as_ref(),
            _ => return false,
        };
        args.iter().any(|arg| {
            let t = match &**arg {
                CallArgInner::Arg(t) | CallArgInner::SpreadArg(t) => t,
            };
            match cx.find_resolved(t) {
                Some(resolved) => matches!(
                    resolved.deref(),
                    TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::PolyT(_))
                ),
                None => false,
            }
        })
    }

    fn instantiate_poly_with_subst_map<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        poly_t: &Type,
        subst_map: &BTreeMap<SubstName, GeneralizedTarg>,
        use_op: &UseOp,
        reason_op: &Reason,
        reason_tapp: &Reason,
        skip_routing: bool,
    ) -> Result<Type, FlowJsException> {
        let generalized_targ_map: BTreeMap<SubstName, GeneralizedTarg> = subst_map
            .iter()
            .map(
                |(name, gt)| -> Result<(SubstName, GeneralizedTarg), FlowJsException> {
                    // Create an OpenT indirection of inferred result.
                    // We specifically want the inferred type argument to have RTypeParam reason desc,
                    // so that we can detect loops in type expansion.
                    // See Instantiation_utils.ImplicitTypeArgument.abstract_targ.
                    //
                    // This OpenT indirection is also needed for performance purposes, since it prevents
                    // unnecessary deep substitution traversals.
                    let generalized_prime = instantiation_utils::implicit_type_argument::mk_targ(
                        cx,
                        &gt.tparam,
                        reason_op,
                        reason_tapp,
                    );
                    FlowJs::rec_unify_with_env(
                        cx,
                        env,
                        trace,
                        use_op.dupe(),
                        UnifyCause::Uncategorized,
                        Some(true),
                        &gt.generalized,
                        &generalized_prime,
                    )?;
                    Ok((
                        name.dupe(),
                        GeneralizedTarg {
                            tparam: gt.tparam.dupe(),
                            inferred: gt.inferred.dupe(),
                            generalized: generalized_prime,
                        },
                    ))
                },
            )
            .collect::<Result<BTreeMap<_, _>, FlowJsException>>()?;

        let final_subst_map: FlowOrdMap<SubstName, Type> = generalized_targ_map
            .iter()
            .map(|(name, gt)| (name.dupe(), gt.generalized.dupe()))
            .collect();

        for gt in generalized_targ_map.values() {
            let frame = UseOp::Frame(
                std::sync::Arc::new(VirtualFrameUseOp::TypeParamBound {
                    name: gt.tparam.name.dupe(),
                }),
                std::sync::Arc::new(use_op.dupe()),
            );
            let bound_subst = type_subst::subst(
                cx,
                Some(use_op.dupe()),
                true,
                false,
                type_subst::Purpose::Normal,
                &final_subst_map,
                gt.tparam.bound.dupe(),
            );
            FlowJs::rec_flow_t_with_env(cx, env, trace, frame, &gt.generalized, &bound_subst)?;
        }

        // For `<U>(f: () => U): U`, `U` in the callback's return wants the inferred (precise)
        // targ, so literals in the callback body don't trip invariance checks (`Array<'x'>` vs
        // `Array<string>`), while the outer return wants the generalized one, so callers don't
        // receive overly narrow singletons. Skipped when no widening happened or a
        // callback argument is itself generic (its tparams would straddle both
        // precisions).
        let needs_routing =
            !skip_routing && subst_map.values().any(|gt| gt.inferred != gt.generalized);
        let (poly_t, subst_map) = if needs_routing {
            // Fresh names must avoid every free name in the polytype (and every
            // substitution key); alpha_rename then guarantees they alias nothing.
            let mut used = type_subst::free_var_finder(cx, None, poly_t);
            used.extend(subst_map.keys().duped());
            let mut splitter = InwardSplitter {
                targets: subst_map,
                renames: BTreeMap::new(),
                used,
            };
            let poly_t = splitter.type_(cx, &(Zone::Plain, BTreeSet::new(), true), poly_t.dupe());
            let mut combined_subst_map = final_subst_map.dupe();
            for (name, inward_name) in splitter.renames {
                let gt = &subst_map[&name];
                let inferred_prime = instantiation_utils::implicit_type_argument::mk_targ(
                    cx,
                    &gt.tparam,
                    reason_op,
                    reason_tapp,
                );
                FlowJs::rec_unify_with_env(
                    cx,
                    env,
                    trace,
                    use_op.dupe(),
                    UnifyCause::Uncategorized,
                    Some(true),
                    &gt.inferred,
                    &inferred_prime,
                )?;
                combined_subst_map.insert(inward_name, inferred_prime);
            }
            (poly_t, combined_subst_map)
        } else {
            (poly_t.dupe(), final_subst_map)
        };
        let result = type_subst::subst(
            cx,
            Some(use_op.dupe()),
            true,
            false,
            type_subst::Purpose::Normal,
            &subst_map,
            poly_t,
        );
        FlowJs::reposition_with_env(
            cx,
            env,
            Some(trace),
            reason_tapp.loc().dupe(),
            None,
            None,
            result,
        )
    }

    pub fn run_call<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        check: &ImplicitInstantiationCheck,
        return_hint: &LazyHintT<Cx>,
        trace: DepthTrace,
        use_op: UseOp,
        reason_op: &Reason,
        reason_tapp: &Reason,
    ) -> Result<(Type, Vec<(Type, subst_name::SubstName)>), FlowJsException> {
        // A type parameter that nothing constrains can take its bound when the result is checked
        // against a full return hint, which then reports a conflict with that bound. Without one,
        // the bound would reach the caller silently, so it is underconstrained instead. A partial
        // hint, which has placeholders, says nothing about their positions, so it only guides the
        // solve as a best-effort hint.
        let (allow_underconstrained, rh) = match (return_hint.1)(cx, false, None, reason_op.dupe())?
        {
            HintEvalResult::HintAvailable(t, kind) => (true, Some((t, kind))),
            HintEvalResult::DecompositionError => (true, None),
            HintEvalResult::EncounteredPlaceholder(Some(t)) => (
                false,
                Some((t, flow_common::hint::HintKind::BestEffortHint)),
            ),
            HintEvalResult::NoHint | HintEvalResult::EncounteredPlaceholder(None) => (false, None),
        };

        // The `has_syntactic_hint` flag will inform the decision on whether to keep
        // precise literal types in the instantiation result. Here, we avoid using
        // Type_env.has_hint, as this function always returns `false` in non-checking
        // mode.
        let has_syntactic_hint = {
            let env = cx.environment();
            env.hint_map
                .get_ordinary(reason_op.loc())
                .map(|h| h.0)
                .unwrap_or(false)
        };

        let env = &match check.operation.2 {
            Operation::SubtypeLowerPoly(_) => env.checking_polymorphic_lower(),
            _ => env.solving_implicit_instantiation(),
        };
        let (_, _, ref t) = check.poly_t;
        let (soln, inferred_targs) = solve_targs(
            cx,
            env,
            use_op.dupe(),
            allow_underconstrained,
            has_syntactic_hint,
            rh,
            check,
        )?;
        let has_generic_arg = has_generic_arg(cx, check);
        let result_t = instantiate_poly_with_subst_map(
            cx,
            env,
            trace,
            t,
            &soln,
            &use_op,
            reason_op,
            reason_tapp,
            has_generic_arg,
        )?;
        Ok((result_t, inferred_targs))
    }

    // Infers `T` in `t extends extends_t<T>`, as a conditional type with `infer T` would, where
    // `extends_t` builds the extends type from `T`. Returns `None` if `t` does not extend it.
    fn extract<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        use_op: &UseOp,
        reason: &Reason,
        t: &Type,
        extends_t: impl FnOnce(&mut Context<'cx>, &FlowJsEnv, &Type) -> Type,
    ) -> Result<Option<Type>, FlowJsException> {
        let name = SubstName::name("T".into());
        let bound = mixed_t::make(reason.dupe());
        let id = cx.make_generic_id(name.dupe(), reason.loc());
        let generic_t = Type::new(TypeInner::GenericT(Box::new(GenericTData {
            reason: reason.dupe(),
            name: name.dupe(),
            bound: bound.dupe(),
            no_infer: false,
            id,
        })));
        let tparam = TypeParam::new(TypeParamInner {
            reason: reason.dupe(),
            name,
            bound,
            polarity: Polarity::Positive,
            default: None,
            is_this: false,
            is_const: false,
        });
        let env = &env.solving_implicit_instantiation();
        let extends_t = extends_t(cx, env, &generic_t);
        let solution = solve_conditional_type_targs(
            cx,
            env,
            DepthTrace::dummy_trace(),
            use_op,
            reason,
            &[tparam],
            t,
            &extends_t,
            &generic_t,
            false,
        )?;
        Ok(solution.map(|subst_map| {
            type_subst::subst(
                cx,
                Some(unknown_use()),
                true,
                false,
                type_subst::Purpose::Normal,
                &subst_map,
                generic_t,
            )
        }))
    }

    pub fn run_render_extractor<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        use_op: UseOp,
        reason: &Reason,
        t: &Type,
    ) -> Result<Type, FlowJsException> {
        let renders = extract(cx, env, &use_op, reason, t, |_, _, generic_t| {
            Type::new(TypeInner::DefT(
                reason.dupe(),
                DefT::new(DefTInner::ReactAbstractComponentT(Box::new(
                    ReactAbstractComponentTData {
                        config: empty_t::why(reason.dupe()),
                        renders: generic_t.dupe(),
                        component_kind: ComponentKind::Structural,
                    },
                ))),
            ))
        })?;
        Ok(renders.unwrap_or_else(|| empty_t::why(reason.dupe())))
    }

    // TODO: await should look up Promise in the environment instead of going directly to
    // the core definition. Otherwise, the following won't work with a polyfilled Promise!
    // If argument is a Promise<T>, then (await argument) returns T.
    // otherwise it just returns the argument type.
    // TODO update this comment when recursive unwrapping of Promise is done.
    pub fn run_await<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        use_op: UseOp,
        reason: &Reason,
        t: &Type,
    ) -> Result<Type, FlowJsException> {
        let awaited = extract(cx, env, &use_op, reason, t, |cx, env, generic_t| {
            let promise_t =
                flow_js_utils::lookup_builtin_type_with_env(cx, env, "Promise", reason.dupe());
            type_util::typeapp(
                false,
                false,
                reason.dupe(),
                promise_t,
                vec![generic_t.dupe()],
            )
        })?;
        let message = match awaited {
            Some(_) => "We are awaiting a Promise.",
            None => "We are awaiting a non-Promise.",
        };
        flow_typing_debug::verbose::print_if_verbose(
            cx,
            None,
            None,
            None,
            vec![message.to_string()],
        );
        Ok(awaited.unwrap_or_else(|| t.dupe()))
    }

    pub fn run_monomorphize<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: UseOp,
        reason_op: &Reason,
        reason_tapp: &Reason,
        tparams: Vec1<TypeParam>,
        t: &Type,
    ) -> Result<Type, FlowJsException> {
        let mut subst_map = BTreeMap::new();
        for tparam in tparams.iter() {
            let targ = instantiation_utils::implicit_type_argument::mk_targ(
                cx,
                tparam,
                reason_op,
                reason_tapp,
            );
            let inferred = pin_type(
                cx,
                env,
                UnmarkedFallback::Bound,
                &use_op,
                tparam,
                None,
                None,
                reason_op,
                &targ,
            )?;
            let gt = GeneralizedTarg {
                tparam: tparam.dupe(),
                inferred: inferred.dupe(),
                generalized: inferred,
            };
            subst_map.insert(tparam.name.dupe(), gt);
        }
        instantiate_poly_with_subst_map(
            cx,
            env,
            trace,
            t,
            &subst_map,
            &use_op,
            reason_op,
            reason_tapp,
            // Monomorphize pins inferred == generalized, so routing is off anyway.
            false,
        )
    }

    pub fn run_conditional<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: UseOp,
        reason: &Reason,
        tparams: &[TypeParam],
        check_t: &Type,
        extends_t: &Type,
        true_t: &Type,
        false_t: &Type,
    ) -> Result<Type, FlowJsException> {
        if flow_js_utils::tvar_visitors::has_placeholders(cx, check_t)
            || flow_js_utils::tvar_visitors::has_placeholders(cx, extends_t)
            || flow_js_utils::tvar_visitors::has_placeholders(cx, true_t)
            || flow_js_utils::tvar_visitors::has_placeholders(cx, false_t)
        {
            flow_typing_debug::verbose::print_if_verbose(
                cx,
                None,
                None,
                None,
                vec![
                    "Conditional type refuses to evaluate because we have placeholders".to_string(),
                ],
            );
            // Placeholder in, placeholder out
            return Ok(cx.mk_placeholder(reason.dupe()));
        }
        let nested_env = env.solving_implicit_instantiation();
        let has_unresolved_tvars = flow_js_utils::tvar_visitors::has_unresolved_tvars(cx, check_t)
            || flow_js_utils::tvar_visitors::has_unresolved_tvars(cx, extends_t)
            || flow_js_utils::tvar_visitors::has_unresolved_tvars(cx, true_t)
            || flow_js_utils::tvar_visitors::has_unresolved_tvars(cx, false_t);
        if has_unresolved_tvars && !env.in_implicit_instantiation() {
            flow_js_utils::add_output_non_speculating(
                cx,
                ErrorMessage::EInternal(Box::new((
                    reason.loc().dupe(),
                    InternalError::UnresolvedTvarInConditionalType,
                ))),
            );
        }
        let (can_run_nested_solve, infer_through_frozen_tvar) = if env.in_implicit_instantiation()
            && has_unresolved_tvars
        {
            let mut condition_collector = FrozenTvarCollector {
                env: &nested_env,
                frozen_tvars: BTreeMap::new(),
            };
            condition_collector.type_(cx, Polarity::Positive, (), check_t);
            condition_collector.type_(cx, Polarity::Positive, (), extends_t);

            let mut branch_collector = FrozenTvarCollector {
                env: &nested_env,
                frozen_tvars: BTreeMap::new(),
            };
            branch_collector.type_(cx, Polarity::Positive, (), true_t);
            branch_collector.type_(cx, Polarity::Positive, (), false_t);
            let infer_through_frozen_tvar =
                branch_collector.frozen_tvars.iter().any(|(id, info)| {
                    info.has_default && !condition_collector.frozen_tvars.contains_key(id)
                });
            let condition_has_frozen_tvar = !condition_collector.frozen_tvars.is_empty();
            let branch_has_frozen_tvar = !branch_collector.frozen_tvars.is_empty();
            let can_run_nested_solve = infer_through_frozen_tvar
                || (!tparams.is_empty() && (condition_has_frozen_tvar || branch_has_frozen_tvar))
                || (!condition_has_frozen_tvar && branch_has_frozen_tvar);
            (can_run_nested_solve, infer_through_frozen_tvar)
        } else {
            (false, false)
        };
        if has_unresolved_tvars && env.in_implicit_instantiation() && !can_run_nested_solve {
            return Ok(cx.mk_placeholder(reason.dupe()));
        }

        let t = {
            let result = solve_conditional_type_targs(
                cx,
                &nested_env,
                trace,
                &use_op,
                reason,
                tparams,
                check_t,
                extends_t,
                true_t,
                true,
            )?;
            match result {
                // If the subtyping can succeed even when the GenericTs are still abstract, then it must
                // succeed under every possible instantiation, so we can take the true branch.
                Some(subst_map) => {
                    flow_typing_debug::verbose::print_if_verbose(
                        cx,
                        None,
                        None,
                        None,
                        vec!["Conditional type evaluates to the true branch.".to_string()],
                    );
                    type_subst::subst(
                        cx,
                        Some(use_op.dupe()),
                        true,
                        false,
                        type_subst::Purpose::Normal,
                        &subst_map,
                        true_t.dupe(),
                    )
                }
                None => {
                    let free_vars = {
                        let bound: FlowOrdSet<SubstName> =
                            tparams.iter().map(|tp| tp.name.dupe()).collect();
                        let fv1 = type_subst::free_var_finder(cx, None, check_t);
                        let fv2 = type_subst::free_var_finder(cx, Some(bound), extends_t);
                        fv1.union(fv2)
                    };
                    let any_t = any_t::placeholder(reason.dupe());
                    let mut frozen_tvar_substituter = FrozenTvarSubstituter {
                        env: &nested_env,
                        substitution: &|_| Some(any_t.dupe()),
                        unresolved: None,
                        expanding: BTreeSet::new(),
                        replaced_frozen_tvar_ids: BTreeSet::new(),
                    };
                    let any_check = frozen_tvar_substituter.type_(cx, &(), check_t.dupe());
                    let any_extends = frozen_tvar_substituter.type_(cx, &(), extends_t.dupe());
                    let condition_frozen_tvar_ids =
                        frozen_tvar_substituter.replaced_frozen_tvar_ids;
                    let replaced_frozen_tvar = !condition_frozen_tvar_ids.is_empty();
                    if free_vars.is_empty() && !replaced_frozen_tvar {
                        flow_typing_debug::verbose::print_if_verbose(
                            cx,
                            None,
                            None,
                            None,
                            vec!["Conditional type evaluates to the false branch.".to_string()],
                        );
                        false_t.dupe()
                    } else {
                        let mut any_subst_map: FlowOrdMap<SubstName, Type> = free_vars
                            .iter()
                            .map(|name| (name.dupe(), any_t::placeholder(reason.dupe())))
                            .collect();
                        for tp in tparams {
                            any_subst_map.insert(tp.name.dupe(), any_t::placeholder(reason.dupe()));
                        }
                        let any_check = type_subst::subst(
                            cx,
                            Some(unknown_use()),
                            true,
                            false,
                            type_subst::Purpose::ConditionalTypeAnySubst,
                            &any_subst_map,
                            any_check,
                        );
                        let any_extends = type_subst::subst(
                            cx,
                            Some(unknown_use()),
                            true,
                            false,
                            type_subst::Purpose::ConditionalTypeAnySubst,
                            &any_subst_map,
                            any_extends,
                        );

                        match speculation_kit::try_singleton_throw_on_failure(
                            cx,
                            env,
                            trace,
                            any_check,
                            UseT::new(UseTInner::UseT(use_op.dupe(), any_extends)),
                        ) {
                            Err(FlowJsException::SpeculationSingletonError) => {
                                // When all the GenericT and infer types are replaced with any, and subtyping
                                // check still cannot succeed, then we can safely conclude that, in every possible
                                // instantiation, we will always take the false branch
                                flow_typing_debug::verbose::print_if_verbose(
                                    cx,
                                    None,
                                    None,
                                    None,
                                    vec!["Conditional type evaluates to the false branch because we will always enter the false branch.".to_string()],
                                );
                                false_t.dupe()
                            }
                            Ok(()) => {
                                if replaced_frozen_tvar && !infer_through_frozen_tvar {
                                    cx.mk_placeholder(reason.dupe())
                                } else {
                                    flow_typing_debug::verbose::print_if_verbose(
                                        cx,
                                        None,
                                        None,
                                        None,
                                        vec!["Conditional type is kept abstract.".to_string()],
                                    );
                                    // A conditional type with GenericTs in check type and extends type is tricky.
                                    // We cannot conservatively decide which branch we will take. To maintain
                                    // soundness in this general case, we make the type abstract.
                                    let bound = Type::new(TypeInner::UnionT(
                                        reason.dupe(),
                                        union_rep::make(
                                            None,
                                            union_rep::UnionKind::UnknownKind,
                                            true_t.dupe(),
                                            false_t.dupe(),
                                            Rc::from([]),
                                        ),
                                    ));
                                    let lower_t = infer_through_frozen_tvar.then(|| bound.dupe());
                                    let nominal_type_args: Rc<[_]> =
                                        [check_t, extends_t, true_t, false_t]
                                            .iter()
                                            .enumerate()
                                            .map(|(i, t)| {
                                                (
                                                    SubstName::synthetic(
                                                        i.to_string().into(),
                                                        None,
                                                        vec![],
                                                    ),
                                                    type_util::reason_of_t(t).dupe(),
                                                    (*t).dupe(),
                                                    Polarity::Neutral,
                                                )
                                            })
                                            .collect();
                                    Type::new(TypeInner::NominalT {
                                        reason: reason.dupe(),
                                        nominal_type: std::rc::Rc::new(NominalType::new(
                                            NominalTypeInner {
                                                nominal_id: nominal::Id::StuckEval(
                                                    nominal::StuckEvalKind::StuckEvalForConditionalType {
                                                        infer_tparam_ids: Rc::new(tparams.iter().map(|tp| cx.make_aloc_id(tp.reason.loc())).collect()),
                                                    },
                                                ),
                                                underlying_t: nominal::UnderlyingT::FullyOpaque,
                                                lower_t,
                                                upper_t: Some(bound),
                                                nominal_type_args,
                                            },
                                        )),
                                    })
                                }
                            }
                            Err(e) => return Err(e),
                        }
                    }
                }
            }
        };
        FlowJs::reposition_with_env(cx, env, Some(trace), reason.loc().dupe(), None, None, t)
    }
}
