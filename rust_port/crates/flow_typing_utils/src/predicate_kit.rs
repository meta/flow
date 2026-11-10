/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::Cell;
use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_aloc::ALocId;
use flow_common::polarity::Polarity;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::reason::mk_reason;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_typing_context::Context;
use flow_typing_context::Cx;
use flow_typing_errors::error_message::EPropNotReadableData;
use flow_typing_errors::error_message::ETupleElementNotReadableData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_common::obj_type;
use flow_typing_flow_js::flow_js;
use flow_typing_flow_js::flow_js::FlowJs;
use flow_typing_flow_js::intersection_kit::disjoint;
use flow_typing_flow_js::slice_utils::is_prop_optional;
use flow_typing_flow_js::speculation_kit;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_type::type_::ArrType;
use flow_typing_type::type_::ArrayATData;
use flow_typing_type::type_::BinaryTest;
use flow_typing_type::type_::CallArg;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::DepthTrace;
use flow_typing_type::type_::ExtendsUseTData;
use flow_typing_type::type_::FunParam;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::ImplicitInstantiationTvarData;
use flow_typing_type::type_::ImplicitInstantiationTvarKind;
use flow_typing_type::type_::InstanceKind;
use flow_typing_type::type_::InstanceT;
use flow_typing_type::type_::NominalType;
use flow_typing_type::type_::NominalTypeInner;
use flow_typing_type::type_::ObjKind;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::Predicate;
use flow_typing_type::type_::PredicateConcretetizerVariant;
use flow_typing_type::type_::PredicateInner;
use flow_typing_type::type_::PropertyInner;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::Targ;
use flow_typing_type::type_::TupleATData;
use flow_typing_type::type_::TupleElement;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeGuardInner;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::UnionEnum;
use flow_typing_type::type_::UnionEnumStar;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::UseT;
use flow_typing_type::type_::UseTInner;
use flow_typing_type::type_::elemt_of_arrtype;
use flow_typing_type::type_::hint_unavailable;
use flow_typing_type::type_::inter_rep;
use flow_typing_type::type_::nominal;
use flow_typing_type::type_::properties;
use flow_typing_type::type_::properties::PropertiesMap;
use flow_typing_type::type_::property;
use flow_typing_type::type_::type_collector::TypeCollector;
use flow_typing_type::type_::union_rep;
use flow_typing_type::type_::union_rep::QuickMemResult;
use flow_typing_type::type_::unknown_use;
use flow_typing_type::type_util;
use flow_typing_type::type_util::loc_of_t;
use flow_typing_type::type_util::reason_of_t;
use flow_utils_concurrency::job_error::JobError;
use vec1::Vec1;

use crate::speculation_flow;
use crate::type_filter;
use crate::type_filter::FilterResult;

#[derive(Clone)]
enum InstanceofRhs {
    TypeOperand(Type),
    /// We are walking `c`'s prototype chain looking for `a`.
    InternalExtendsOperand {
        reason: Reason,
        c: Type,
        a: Type,
        /// The classes visited so far on this walk. Persistent, so extending it as the walk
        /// descends shares the existing entries instead of copying them.
        seen: SeenClasses,
    },
}

type SeenClasses = FlowOrdSet<ALocId>;

/// Adds `instance` to `seen`, or returns `None` if it was already there, which means the
/// prototype chain loops back on itself and the walk can never reach the root.
fn visit_class(seen: &SeenClasses, instance: &InstanceT) -> Option<SeenClasses> {
    let class_id = &instance.inst.class_id;
    // Instances without a class id (see `is_same_instance_type`) are indistinguishable from
    // each other, so recording them would report cycles that aren't there.
    if *class_id == ALocId::none() {
        return Some(seen.dupe());
    }
    if seen.contains(class_id) {
        return None;
    }
    Some(seen.update(class_id.dupe()).into())
}

#[derive(Clone, Copy)]
enum PropGuard {
    PropGuardTruthy,
    PropGuardNotTruthy,
    PropGuardMaybe,
    PropGuardNotMaybe,
    PropGuardNull,
    PropGuardNotNull,
    PropGuardVoid,
    PropGuardNotVoid,
}

fn concretization_variant_of_predicate(p: &Predicate) -> PredicateConcretetizerVariant {
    match p.deref() {
        PredicateInner::MaybeP | PredicateInner::TruthyP | PredicateInner::LatentP(..) => {
            PredicateConcretetizerVariant::ConcretizeKeepOptimizedUnions
        }
        PredicateInner::NotP(inner) => match inner.deref() {
            PredicateInner::MaybeP | PredicateInner::TruthyP | PredicateInner::LatentP(..) => {
                PredicateConcretetizerVariant::ConcretizeKeepOptimizedUnions
            }
            _ => PredicateConcretetizerVariant::ConcretizeForGeneralPredicateTest,
        },
        _ => PredicateConcretetizerVariant::ConcretizeForGeneralPredicateTest,
    }
}

struct PredicateResultCollector {
    collector: TypeCollector,
    changed: Rc<RefCell<bool>>,
    // Set when refining a union member: an `instanceof` against a provably-disjoint
    // class prunes the member to `empty` rather than keeping the conservative guard.
    from_union: Cell<bool>,
    // Shared across a union's members: the guard a pruned member would have produced,
    // used as the fallback if every member prunes (avoids a bare `empty`).
    disjoint_fallback: TypeCollector,
    // Set while refining the bound of a generic.
    refined_generic: Option<RefinedGeneric>,
}

#[derive(Clone, Dupe)]
struct RefinedGeneric {
    generic: Type,
    // Whether `generic` is a subtype of each type guard checked so far, shared across the members
    // of a union bound so that the check runs once per guard rather than once per member.
    subsumed_by_guard: Rc<RefCell<Vec<(Type, bool)>>>,
}

// The concretized right-hand sides of the binary tests in one predicate run. A right-hand side
// does not depend on the member of the refined type being tested, so it is concretized once
// instead of once per member.
#[derive(Default)]
struct RhsConcretizations(RefCell<Vec<(Type, PredicateConcretetizerVariant, Rc<[Type]>)>>);

impl RhsConcretizations {
    fn get_or_concretize<'cx>(
        &self,
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        variant: PredicateConcretetizerVariant,
        r: &Type,
    ) -> Result<Rc<[Type]>, FlowJsException> {
        if let Some((_, _, ts)) = self
            .0
            .borrow()
            .iter()
            .find(|(t, v, _)| t.ptr_eq(r) && *v == variant)
        {
            return Ok(ts.dupe());
        }
        let ts: Rc<[Type]> =
            FlowJs::possible_concrete_types_for_predicate(variant, cx, env, reason_of_t(r), r)?
                .into();
        self.0.borrow_mut().push((r.dupe(), variant, ts.dupe()));
        Ok(ts)
    }
}

// Simple wrapper to protect against using non-concretized types in type-guard
// filtering operations (see `intersect` below).
struct ConcretizedType(Type);

impl ConcretizedType {
    fn unwrap(&self) -> &Type {
        &self.0
    }

    fn for_all_concrete_ts<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        t: &Type,
        f: &dyn Fn(&mut Context<'cx>, &ConcretizedType) -> bool,
    ) -> Result<bool, FlowJsException> {
        let ts =
            FlowJs::possible_concrete_types_for_inspection_with_env(cx, env, reason_of_t(t), t)?;
        Ok(ts.iter().all(|t| f(cx, &ConcretizedType(t.dupe()))))
    }

    fn wrap_unsafe(t: Type) -> ConcretizedType {
        ConcretizedType(t)
    }
}

fn report_changes_to_input(result_collector: &PredicateResultCollector) {
    *result_collector.changed.borrow_mut() = true;
}

fn report_filtering_result_to_predicate_result(
    filter_result: FilterResult,
    result_collector: &PredicateResultCollector,
) {
    let FilterResult { type_, changed } = filter_result;
    result_collector.collector.add(type_);
    let prev = *result_collector.changed.borrow();
    *result_collector.changed.borrow_mut() = prev || changed;
}

fn report_unchanged_filtering_result_to_predicate_result(
    type_: Type,
    result_collector: &PredicateResultCollector,
) {
    report_filtering_result_to_predicate_result(
        FilterResult {
            type_,
            changed: false,
        },
        result_collector,
    );
}

fn report_changed_filtering_result_to_predicate_result(
    type_: Type,
    result_collector: &PredicateResultCollector,
) {
    report_filtering_result_to_predicate_result(
        FilterResult {
            type_,
            changed: true,
        },
        result_collector,
    );
}

// Refines each member of a union scrutinee independently. A member provably
// disjoint from the guard prunes to `empty` (its sub-collector stays empty)
// rather than leaking the guard-with-`any`, so the disjoint members drop out of
// the result union. If *every* member prunes, emitting a bare `empty` is a
// footgun (and unreliable for library `declare class` hierarchies), so we fall
// back to the conservative guard types recorded while pruning (in
// `disjoint_fallback`) — each member is refined exactly once, so any diagnostics
// the predicate emits are not duplicated.
fn run_predicate_over_union_members(
    ts: &[Type],
    result_collector: &PredicateResultCollector,
    process_one: &mut dyn FnMut(&Type, &PredicateResultCollector) -> Result<(), FlowJsException>,
) -> Result<(), FlowJsException> {
    let disjoint_fallback = TypeCollector::create();
    let mut any_survived = false;
    let mut changed = false;
    for t in ts.iter() {
        let sub = PredicateResultCollector {
            collector: TypeCollector::create(),
            changed: Rc::new(RefCell::new(false)),
            from_union: Cell::new(true),
            disjoint_fallback: disjoint_fallback.dupe(),
            refined_generic: result_collector.refined_generic.dupe(),
        };
        process_one(t, &sub)?;
        if *sub.changed.borrow() {
            changed = true;
        }
        let collected = sub.collector.collect_to_vec();
        if !collected.is_empty() {
            any_survived = true;
            for ty in collected {
                result_collector.collector.add(ty);
            }
        }
    }
    if changed {
        report_changes_to_input(result_collector);
    }
    if !any_survived {
        for ty in disjoint_fallback.collect_to_vec() {
            result_collector.collector.add(ty);
        }
    }
    Ok(())
}

fn concretize_and_run_predicate<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    l: &Type,
    variant: PredicateConcretetizerVariant,
    result_collector: &PredicateResultCollector,
    predicate_no_concretization: &dyn Fn(
        &mut Context<'cx>,
        DepthTrace,
        &PredicateResultCollector,
        &Type,
    ) -> Result<(), FlowJsException>,
) -> Result<(), FlowJsException> {
    let reason = reason_of_t(l);
    let ts = FlowJs::possible_concrete_types_for_predicate(variant, cx, env, reason, l)?;
    // Runs the predicate on a single concretized member `t`, reporting into `rc`.
    let mut process_one =
        |t: &Type, rc: &PredicateResultCollector| -> Result<(), FlowJsException> {
            match t.deref() {
                TypeInner::GenericT(box GenericTData {
                    bound,
                    name,
                    reason,
                    id,
                    no_infer,
                }) => {
                    let bound_type_collector = TypeCollector::create();
                    let changed = Rc::new(RefCell::new(false));
                    let bound_result_collector = PredicateResultCollector {
                        collector: bound_type_collector,
                        changed: changed.dupe(),
                        from_union: Cell::new(rc.from_union.get()),
                        disjoint_fallback: rc.disjoint_fallback.dupe(),
                        refined_generic: Some(RefinedGeneric {
                            generic: t.dupe(),
                            subsumed_by_guard: Rc::new(RefCell::new(Vec::new())),
                        }),
                    };
                    let repositioned_bound =
                        FlowJs::reposition_reason(cx, env, Some(trace), reason, None, bound)?;
                    concretize_and_run_predicate(
                        cx,
                        env,
                        trace,
                        &repositioned_bound,
                        variant.clone(),
                        &bound_result_collector,
                        predicate_no_concretization,
                    )?;
                    let changed_val = *changed.borrow();
                    if changed_val {
                        report_changes_to_input(rc);
                    }
                    let name = name.dupe();
                    let no_infer = *no_infer;
                    let id = id.clone();
                    bound_result_collector.collector.iter(|bound| {
                        let type_ = Type::new(TypeInner::GenericT(Box::new(GenericTData {
                            reason: reason_of_t(bound).dupe(),
                            name: name.dupe(),
                            bound: bound.dupe(),
                            no_infer,
                            id: id.clone(),
                        })));
                        report_filtering_result_to_predicate_result(
                            FilterResult {
                                type_,
                                changed: changed_val,
                            },
                            rc,
                        );
                        Ok(())
                    })?;
                    Ok(())
                }
                _ => predicate_no_concretization(cx, trace, rc, t),
            }
        };
    if ts.len() > 1 {
        run_predicate_over_union_members(&ts, result_collector, &mut process_one)?;
    } else {
        for t in ts.iter() {
            process_one(t, result_collector)?;
        }
    }
    Ok(())
}

fn concretize_binary_rhs_and_run_binary_predicate<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    l: &Type,
    r: &Type,
    sense: bool,
    b: &BinaryTest,
    result_collector: &PredicateResultCollector,
    rhs_concretizations: &RhsConcretizations,
) -> Result<(), FlowJsException> {
    let variant = match b {
        BinaryTest::InstanceofTest => {
            PredicateConcretetizerVariant::ConcretizeRHSForInstanceOfPredicateTest
        }
        BinaryTest::SentinelProp(_) => {
            PredicateConcretetizerVariant::ConcretizeRHSForLiteralPredicateTest
        }
        BinaryTest::EqTest => PredicateConcretetizerVariant::ConcretizeRHSForLiteralPredicateTest,
    };
    let ts = rhs_concretizations.get_or_concretize(cx, env, variant, r)?;
    for r in ts.iter() {
        binary_predicate(cx, env, trace, sense, b, l, r, result_collector)?;
    }
    Ok(())
}

// t - predicate output recipient (normally a tvar)
// l - incoming concrete LB (predicate input)
// result - guard result in case of success
// p - predicate
fn predicate_no_concretization<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    result_collector: &PredicateResultCollector,
    l: &Type,
    p: &Predicate,
    rhs_concretizations: &RhsConcretizations,
) -> Result<(), FlowJsException> {
    match p.deref() {
        // deconstruction of &&
        PredicateInner::AndP(p1, p2) => {
            let intermediate_type_collector = TypeCollector::create();
            let changed = Rc::new(RefCell::new(false));
            let intermediate_result_collector = PredicateResultCollector {
                collector: intermediate_type_collector,
                changed: changed.dupe(),
                from_union: Cell::new(result_collector.from_union.get()),
                disjoint_fallback: result_collector.disjoint_fallback.dupe(),
                refined_generic: result_collector.refined_generic.dupe(),
            };
            let p1_clone = p1.dupe();
            concretize_and_run_predicate(
                cx,
                env,
                trace,
                l,
                concretization_variant_of_predicate(p1),
                &intermediate_result_collector,
                &|cx, trace, rc, l| {
                    predicate_no_concretization(
                        cx,
                        env,
                        trace,
                        rc,
                        l,
                        &p1_clone,
                        rhs_concretizations,
                    )
                },
            )?;
            if *changed.borrow() {
                report_changes_to_input(result_collector);
            }
            let collected = intermediate_result_collector.collector.collect_to_vec();
            let p2_clone = p2.dupe();
            for t in collected.iter() {
                concretize_and_run_predicate(
                    cx,
                    env,
                    trace,
                    t,
                    concretization_variant_of_predicate(p2),
                    result_collector,
                    &|cx, trace, rc, l| {
                        predicate_no_concretization(
                            cx,
                            env,
                            trace,
                            rc,
                            l,
                            &p2_clone,
                            rhs_concretizations,
                        )
                    },
                )?;
            }
            Ok(())
        }
        // deconstruction of ||
        PredicateInner::OrP(p1, p2) => {
            let p1_clone = p1.dupe();
            concretize_and_run_predicate(
                cx,
                env,
                trace,
                l,
                concretization_variant_of_predicate(p1),
                result_collector,
                &|cx, trace, rc, l| {
                    predicate_no_concretization(
                        cx,
                        env,
                        trace,
                        rc,
                        l,
                        &p1_clone,
                        rhs_concretizations,
                    )
                },
            )?;
            let p2_clone = p2.dupe();
            concretize_and_run_predicate(
                cx,
                env,
                trace,
                l,
                concretization_variant_of_predicate(p2),
                result_collector,
                &|cx, trace, rc, l| {
                    predicate_no_concretization(
                        cx,
                        env,
                        trace,
                        rc,
                        l,
                        &p2_clone,
                        rhs_concretizations,
                    )
                },
            )?;
            Ok(())
        }
        // deconstruction of binary test
        PredicateInner::BinaryP(b, r) => concretize_binary_rhs_and_run_binary_predicate(
            cx,
            env,
            trace,
            l,
            r,
            true,
            b,
            result_collector,
            rhs_concretizations,
        ),
        PredicateInner::NotP(inner) => {
            match inner.deref() {
                PredicateInner::BinaryP(b, r) => concretize_binary_rhs_and_run_binary_predicate(
                    cx,
                    env,
                    trace,
                    l,
                    r,
                    false,
                    b,
                    result_collector,
                    rhs_concretizations,
                ),
                // typeof _ ~ "boolean"
                PredicateInner::BoolP(_) => {
                    report_filtering_result_to_predicate_result(
                        type_filter::not_boolean(l.dupe()),
                        result_collector,
                    );
                    Ok(())
                }
                // typeof _ ~ "string"
                PredicateInner::StrP(_) => {
                    report_filtering_result_to_predicate_result(
                        type_filter::not_string(l.dupe()),
                        result_collector,
                    );
                    Ok(())
                }
                // typeof _ ~ "symbol"
                PredicateInner::SymbolP(_) => {
                    report_filtering_result_to_predicate_result(
                        type_filter::not_symbol(l.dupe()),
                        result_collector,
                    );
                    Ok(())
                }
                // _ ~ "some string"
                PredicateInner::SingletonStrP(box (_, _, lit)) => {
                    let filtered_str =
                        type_filter::not_string_literal(FlowSmolStr::new(lit), l.dupe());
                    report_filtering_result_to_predicate_result(filtered_str, result_collector);
                    Ok(())
                }
                // _ ~ some number n
                PredicateInner::SingletonNumP(box (_, _, lit)) => {
                    let filtered_num = type_filter::not_number_literal(lit.clone(), l.dupe());
                    report_filtering_result_to_predicate_result(filtered_num, result_collector);
                    Ok(())
                }
                // typeof _ ~ "number"
                PredicateInner::NumP(_) => {
                    report_filtering_result_to_predicate_result(
                        type_filter::not_number(l.dupe()),
                        result_collector,
                    );
                    Ok(())
                }
                // _ ~ some bigint n
                PredicateInner::SingletonBigIntP(box (_, _, lit)) => {
                    let filtered_bigint = type_filter::not_bigint_literal(lit.clone(), l.dupe());
                    report_filtering_result_to_predicate_result(filtered_bigint, result_collector);
                    Ok(())
                }
                // typeof _ ~ "bigint"
                PredicateInner::BigIntP(_) => {
                    report_filtering_result_to_predicate_result(
                        type_filter::not_bigint(l.dupe()),
                        result_collector,
                    );
                    Ok(())
                }
                // typeof _ ~ "function"
                PredicateInner::FunP => {
                    report_filtering_result_to_predicate_result(
                        type_filter::not_function(l.dupe()),
                        result_collector,
                    );
                    Ok(())
                }
                // typeof _ ~ "object"
                PredicateInner::ObjP => {
                    report_filtering_result_to_predicate_result(
                        type_filter::not_object(l.dupe()),
                        result_collector,
                    );
                    Ok(())
                }
                // Array.isArray _
                PredicateInner::ArrP => {
                    report_filtering_result_to_predicate_result(
                        type_filter::not_array(l.dupe()),
                        result_collector,
                    );
                    Ok(())
                }
                // array length
                PredicateInner::ArrLenP { op, n } => {
                    report_filtering_result_to_predicate_result(
                        type_filter::array_length(false, op, *n, l.dupe()),
                        result_collector,
                    );
                    Ok(())
                }
                // typeof _ ~ "undefined"
                PredicateInner::VoidP => {
                    let filtered = type_filter::not_undefined(cx, l.dupe());
                    report_filtering_result_to_predicate_result(filtered, result_collector);
                    Ok(())
                }
                // null
                PredicateInner::NullP => {
                    let filtered = type_filter::not_null(cx, l.dupe());
                    report_filtering_result_to_predicate_result(filtered, result_collector);
                    Ok(())
                }
                // maybe
                PredicateInner::MaybeP => {
                    let filtered = type_filter::not_maybe(cx, l.dupe());
                    report_filtering_result_to_predicate_result(filtered, result_collector);
                    Ok(())
                }
                // true
                PredicateInner::SingletonBoolP(box (_, true)) => {
                    let filtered = type_filter::not_true(l.dupe());
                    report_filtering_result_to_predicate_result(filtered, result_collector);
                    Ok(())
                }
                // false
                PredicateInner::SingletonBoolP(box (_, false)) => {
                    let filtered = type_filter::not_false(l.dupe());
                    report_filtering_result_to_predicate_result(filtered, result_collector);
                    Ok(())
                }
                // truthyness
                PredicateInner::TruthyP => {
                    let filtered = type_filter::not_truthy(cx, l.dupe());
                    report_filtering_result_to_predicate_result(filtered, result_collector);
                    Ok(())
                }
                PredicateInner::PropExistsP {
                    propname,
                    reason: _,
                } => prop_exists_test(cx, env, propname, false, l, result_collector),
                PredicateInner::PropTruthyP(key, r) => {
                    prop_truthy_test(cx, env, key, r, false, l, result_collector)
                }
                PredicateInner::PropNonMaybeP(key, r) => {
                    prop_non_maybe_test(cx, env, key, r, false, l, result_collector)
                }
                PredicateInner::PropIsExactlyNullP(key, r) => {
                    prop_is_exactly_null_test(cx, env, key, r, false, l, result_collector)
                }
                PredicateInner::PropNonVoidP(key, r) => {
                    prop_non_void_test(cx, env, key, r, false, l, result_collector)
                }
                // classical logic i guess
                PredicateInner::NotP(inner_p) => predicate_no_concretization(
                    cx,
                    env,
                    trace,
                    result_collector,
                    l,
                    inner_p,
                    rhs_concretizations,
                ),
                PredicateInner::AndP(p1, p2) => {
                    let de_morgan = Predicate::new(PredicateInner::OrP(
                        Predicate::new(PredicateInner::NotP(p1.dupe())),
                        Predicate::new(PredicateInner::NotP(p2.dupe())),
                    ));
                    predicate_no_concretization(
                        cx,
                        env,
                        trace,
                        result_collector,
                        l,
                        &de_morgan,
                        rhs_concretizations,
                    )
                }
                PredicateInner::OrP(p1, p2) => {
                    let de_morgan = Predicate::new(PredicateInner::AndP(
                        Predicate::new(PredicateInner::NotP(p1.dupe())),
                        Predicate::new(PredicateInner::NotP(p2.dupe())),
                    ));
                    predicate_no_concretization(
                        cx,
                        env,
                        trace,
                        result_collector,
                        l,
                        &de_morgan,
                        rhs_concretizations,
                    )
                }
                // Latent predicate
                PredicateInner::LatentP(pred_funcall_info, idx) => {
                    let fun_t = &pred_funcall_info.2;
                    let loc = pred_funcall_info.1.dupe();
                    let use_op = pred_funcall_info.0.dupe();
                    let targs = &pred_funcall_info.3;
                    let argts = &pred_funcall_info.4;
                    let reason = mk_reason(None, loc);
                    call_latent_param_pred(
                        cx,
                        env,
                        trace,
                        fun_t,
                        &use_op,
                        &reason,
                        targs,
                        argts,
                        false,
                        idx,
                        l,
                        result_collector,
                    )
                }
                PredicateInner::LatentThisP(pred_funcall_info) => {
                    let fun_t = &pred_funcall_info.2;
                    let loc = pred_funcall_info.1.dupe();
                    let use_op = pred_funcall_info.0.dupe();
                    let targs = &pred_funcall_info.3;
                    let argts = &pred_funcall_info.4;
                    let reason = mk_reason(None, loc);
                    call_latent_this_pred(
                        cx,
                        env,
                        trace,
                        fun_t,
                        &use_op,
                        &reason,
                        targs,
                        argts,
                        false,
                        l,
                        result_collector,
                    )
                }
                // Impossible
                PredicateInner::ImpossibleP => {
                    report_filtering_result_to_predicate_result(
                        type_filter::unchanged_result(l.dupe()),
                        result_collector,
                    );
                    Ok(())
                }
            }
        }
        // typeof _ ~ "boolean"
        PredicateInner::BoolP(box loc) => {
            report_filtering_result_to_predicate_result(
                type_filter::boolean(loc.dupe(), l.dupe()),
                result_collector,
            );
            Ok(())
        }
        // typeof _ ~ "string"
        PredicateInner::StrP(box loc) => {
            report_filtering_result_to_predicate_result(
                type_filter::string(loc.dupe(), l.dupe()),
                result_collector,
            );
            Ok(())
        }
        // typeof _ ~ "symbol"
        PredicateInner::SymbolP(box loc) => {
            report_filtering_result_to_predicate_result(
                type_filter::symbol(loc.dupe(), l.dupe()),
                result_collector,
            );
            Ok(())
        }
        // _ ~ "some string"
        PredicateInner::SingletonStrP(box (expected_loc, _sense, lit)) => {
            let filtered_str =
                type_filter::string_literal(expected_loc.dupe(), FlowSmolStr::new(lit), l.dupe());
            report_filtering_result_to_predicate_result(filtered_str, result_collector);
            Ok(())
        }
        // _ ~ some number n
        PredicateInner::SingletonNumP(box (expected_loc, _sense, lit)) => {
            let filtered_num =
                type_filter::number_literal(expected_loc.dupe(), lit.clone(), l.dupe());
            report_filtering_result_to_predicate_result(filtered_num, result_collector);
            Ok(())
        }
        // typeof _ ~ "number"
        PredicateInner::NumP(box loc) => {
            report_filtering_result_to_predicate_result(
                type_filter::number(loc.dupe(), l.dupe()),
                result_collector,
            );
            Ok(())
        }
        // _ ~ some bigint n
        PredicateInner::SingletonBigIntP(box (expected_loc, _sense, lit)) => {
            let filtered_bigint =
                type_filter::bigint_literal(expected_loc.dupe(), lit.clone(), l.dupe());
            report_filtering_result_to_predicate_result(filtered_bigint, result_collector);
            Ok(())
        }
        // typeof _ ~ "bigint"
        PredicateInner::BigIntP(box loc) => {
            report_filtering_result_to_predicate_result(
                type_filter::bigint(loc.dupe(), l.dupe()),
                result_collector,
            );
            Ok(())
        }
        // typeof _ ~ "function"
        PredicateInner::FunP => {
            report_filtering_result_to_predicate_result(
                type_filter::function_(l.dupe()),
                result_collector,
            );
            Ok(())
        }
        // typeof _ ~ "object"
        PredicateInner::ObjP => {
            report_filtering_result_to_predicate_result(
                type_filter::object_(cx, l.dupe()),
                result_collector,
            );
            Ok(())
        }
        // Array.isArray _
        PredicateInner::ArrP => {
            report_filtering_result_to_predicate_result(
                type_filter::array(l.dupe()),
                result_collector,
            );
            Ok(())
        }
        // array length
        PredicateInner::ArrLenP { op, n } => {
            report_filtering_result_to_predicate_result(
                type_filter::array_length(true, op, *n, l.dupe()),
                result_collector,
            );
            Ok(())
        }
        // typeof _ ~ "undefined"
        PredicateInner::VoidP => {
            let filtered = type_filter::undefined(l.dupe());
            report_filtering_result_to_predicate_result(filtered, result_collector);
            Ok(())
        }
        // null
        PredicateInner::NullP => {
            let filtered = type_filter::null_filter(l.dupe());
            report_filtering_result_to_predicate_result(filtered, result_collector);
            Ok(())
        }
        // maybe
        PredicateInner::MaybeP => {
            let filtered = type_filter::maybe(cx, l.dupe());
            report_filtering_result_to_predicate_result(filtered, result_collector);
            Ok(())
        }
        // true
        PredicateInner::SingletonBoolP(box (_, true)) => {
            let filtered = type_filter::true_(l.dupe());
            report_filtering_result_to_predicate_result(filtered, result_collector);
            Ok(())
        }
        // false
        PredicateInner::SingletonBoolP(box (_, false)) => {
            let filtered = type_filter::false_(l.dupe());
            report_filtering_result_to_predicate_result(filtered, result_collector);
            Ok(())
        }
        // truthyness
        PredicateInner::TruthyP => {
            let filtered = type_filter::truthy(cx, l.dupe());
            report_filtering_result_to_predicate_result(filtered, result_collector);
            Ok(())
        }
        PredicateInner::PropExistsP {
            propname,
            reason: _,
        } => prop_exists_test(cx, env, propname, true, l, result_collector),
        PredicateInner::PropTruthyP(key, r) => {
            prop_truthy_test(cx, env, key, r, true, l, result_collector)
        }
        PredicateInner::PropNonMaybeP(key, r) => {
            prop_non_maybe_test(cx, env, key, r, true, l, result_collector)
        }
        PredicateInner::PropIsExactlyNullP(key, r) => {
            prop_is_exactly_null_test(cx, env, key, r, true, l, result_collector)
        }
        PredicateInner::PropNonVoidP(key, r) => {
            prop_non_void_test(cx, env, key, r, true, l, result_collector)
        }
        // Latent predicate
        PredicateInner::LatentP(pred_funcall_info, idx) => {
            let fun_t = &pred_funcall_info.2;
            let loc = pred_funcall_info.1.dupe();
            let use_op = pred_funcall_info.0.dupe();
            let targs = &pred_funcall_info.3;
            let argts = &pred_funcall_info.4;
            let reason = mk_reason(None, loc);
            call_latent_param_pred(
                cx,
                env,
                trace,
                fun_t,
                &use_op,
                &reason,
                targs,
                argts,
                true,
                idx,
                l,
                result_collector,
            )
        }
        PredicateInner::LatentThisP(pred_funcall_info) => {
            let fun_t = &pred_funcall_info.2;
            let loc = pred_funcall_info.1.dupe();
            let use_op = pred_funcall_info.0.dupe();
            let targs = &pred_funcall_info.3;
            let argts = &pred_funcall_info.4;
            let reason = mk_reason(None, loc);
            call_latent_this_pred(
                cx,
                env,
                trace,
                fun_t,
                &use_op,
                &reason,
                targs,
                argts,
                true,
                l,
                result_collector,
            )
        }
        // Impossible
        PredicateInner::ImpossibleP => {
            report_filtering_result_to_predicate_result(
                type_filter::empty(l.dupe()),
                result_collector,
            );
            Ok(())
        }
    }
}

/// call_latent_pred connects a predicate function with information available
/// at a call-site appearing in a conditional position (e.g. `if (pred(x))`).
/// [tin] is the incoming type of `x` and [tout] the refined result in the then-
/// branch. Since at the time of processing the call we do not know yet the
/// function's formal parameters, [idx] is the index of the argument that gets
/// refined.
fn call_latent_pred<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    fun_t: &Type,
    use_op: &UseOp,
    reason: &Reason,
    targs: &Option<Rc<[Targ]>>,
    argts: &[CallArg],
    sense: bool,
    is_target: &dyn Fn(&str, &[FunParam]) -> bool,
    tin: &Type,
    result_collector: &PredicateResultCollector,
) -> Result<(), FlowJsException> {
    let mut ts = FlowJs::possible_concrete_types_for_inspection_with_env(
        cx,
        env,
        reason_of_t(fun_t),
        fun_t,
    )?;
    // A nullish callee never yields a truthy result: calling it throws, and an
    // optional chain skips the call and evaluates to `undefined`. So it does not
    // dilute the positive refinement, but it keeps the negative one unrefined.
    let is_nullish = |t: &Type| {
        matches!(t.deref(), TypeInner::DefT(_, def_t)
            if matches!(def_t.deref(), DefTInner::NullT | DefTInner::VoidT))
    };
    if sense && ts.iter().any(|t| !is_nullish(t)) {
        ts.retain(|t| !is_nullish(t));
    }
    for t in ts.iter() {
        match t.deref() {
            TypeInner::IntersectionT(r, rep) => {
                let r = r.dupe();
                let members: Vec<Type> = rep.members_iter().duped().collect();
                let use_op_c = use_op.dupe();
                let reason_c = reason.dupe();
                let targs_c = targs.dupe();
                let argts_c = argts.to_vec();
                let tin_c = tin.dupe();
                let cases: Vec<
                    Box<
                        dyn FnOnce(&mut Context<'cx>, &FlowJsEnv) -> Result<(), FlowJsException>
                            + '_,
                    >,
                > = members
                    .into_iter()
                    .map(|member_t| {
                        let use_op_inner = use_op_c.dupe();
                        let reason_inner = reason_c.dupe();
                        let targs_inner = targs_c.dupe();
                        let argts_inner = argts_c.clone();
                        let tin_inner = tin_c.dupe();
                        Box::new(move |cx: &mut Context<'cx>, env: &FlowJsEnv| {
                            call_latent_pred(
                                cx,
                                env,
                                trace,
                                &member_t,
                                &use_op_inner,
                                &reason_inner,
                                &targs_inner,
                                &argts_inner,
                                sense,
                                is_target,
                                &tin_inner,
                                result_collector,
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
                    Some(use_op.dupe()),
                    None,
                    None,
                    r.loc().dupe(),
                    cases,
                )?;
            }
            TypeInner::DefT(r, def_t) => {
                match def_t.deref() {
                    // Calls to functions appearing in predicate refinement contexts dispatch
                    // to this case. Here, the callee function type holds the predicate
                    // that will refine the incoming `unrefined_t` and flow a filtered
                    // (refined) version of this type into `fresh_t`.
                    //
                    // Problematic cases (e.g. when the refining index is out of bounds w.r.t.
                    // `params`) raise errors, but also propagate the unrefined types (as if the
                    // refinement never took place).
                    DefTInner::FunT(_, fun_type)
                        if let Some(type_guard_value) = fun_type.type_guard.as_deref()
                            && let TypeGuardInner {
                                param_name: (_, param_name),
                                type_guard,
                                ..
                            } = type_guard_value =>
                    {
                        let one_sided = type_guard_value.one_sided();
                        // TODO: for the moment we only support simple keys (empty projection)
                        // that exactly correspond to the function's parameters
                        if is_target(param_name.as_str(), &fun_type.params) {
                            let filter_result = if sense {
                                match type_guard {
                                    None => type_filter::truthy(cx, tin.dupe()),
                                    Some(type_guard) => {
                                        let repositioned = FlowJs::reposition_reason(
                                            cx,
                                            env,
                                            Some(trace),
                                            reason,
                                            None,
                                            type_guard,
                                        )?;
                                        // An `any` in the bound trivially satisfies the guard, but
                                        // `intersect_type_guard` narrows it to the guard.
                                        if refined_generic_subsumed(
                                            cx,
                                            env,
                                            result_collector,
                                            type_guard,
                                            &repositioned,
                                        )? && !is_any(cx, env, tin)?
                                        {
                                            // The generic already satisfies the guard.
                                            FilterResult {
                                                type_: tin.dupe(),
                                                changed: false,
                                            }
                                        } else {
                                            match intersect_type_guard(
                                                cx,
                                                env,
                                                tin.dupe(),
                                                repositioned,
                                            )? {
                                                GuardIntersection::Refined(type_) => {
                                                    let changed = !Type::ptr_eq(&type_, tin);
                                                    FilterResult { type_, changed }
                                                }
                                                // Prune this union member, but keep the intersection
                                                // in case every member prunes.
                                                GuardIntersection::UnrelatedClasses(type_)
                                                    if result_collector.from_union.get() =>
                                                {
                                                    result_collector.disjoint_fallback.add(type_);
                                                    report_changes_to_input(result_collector);
                                                    continue;
                                                }
                                                GuardIntersection::UnrelatedClasses(type_) => {
                                                    FilterResult {
                                                        type_,
                                                        changed: true,
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else if let Some(type_guard) = type_guard
                                && !one_sided
                            {
                                let repositioned = FlowJs::reposition_reason(
                                    cx,
                                    env,
                                    Some(trace),
                                    reason,
                                    None,
                                    type_guard,
                                )?;
                                // As above, leave an `any` in the bound to `type_guard_diff`.
                                if refined_generic_subsumed(
                                    cx,
                                    env,
                                    result_collector,
                                    type_guard,
                                    &repositioned,
                                )? && !is_any(cx, env, tin)?
                                {
                                    // The generic always satisfies the guard.
                                    type_filter::empty(tin.dupe())
                                } else {
                                    type_guard_diff(cx, env, tin, &repositioned)?
                                }
                            } else {
                                // Do not refine else branch on one-sided type-guard
                                FilterResult {
                                    type_: tin.dupe(),
                                    changed: false,
                                }
                            };
                            report_filtering_result_to_predicate_result(
                                filter_result,
                                result_collector,
                            );
                        } else {
                            // This is not the refined parameter.
                            report_unchanged_filtering_result_to_predicate_result(
                                tin.dupe(),
                                result_collector,
                            );
                        }
                    }
                    DefTInner::PolyT(box PolyTData {
                        kind: _,
                        tparams_loc,
                        tparams: ids,
                        t_out: t_out_inner,
                        id: _,
                        strictness_kind: _,
                    }) => {
                        let reason_tapp = r;
                        let fun_t_c = t.dupe();
                        let calltype = flow_typing_implicit_instantiation_check::Call {
                            call_this_t: flow_typing_type::type_::global_this(reason.dupe()),
                            call_targs: targs.dupe(),
                            call_args_tlist: Rc::from(argts),
                            call_strict_arity: true,
                            call_speculation_hint_state: None,
                            call_specialized_callee: None,
                        };
                        let tparams_loc_c = tparams_loc.dupe();
                        let ids_c = ids.dupe();
                        let t_out_c = t_out_inner.dupe();
                        let reason_c = reason.dupe();
                        let calltype_c = calltype.clone();
                        let check = move || {
                            flow_typing_implicit_instantiation_check::ImplicitInstantiationCheck::of_call(
                                fun_t_c.dupe(),
                                (tparams_loc_c.dupe(), Vec1::try_from_vec(ids_c.to_vec()).unwrap(), t_out_c.dupe()),
                                unknown_use(),
                                reason_c.dupe(),
                                calltype_c.clone(),
                            )
                        };
                        let lparts = (
                            reason_tapp.dupe(),
                            tparams_loc.dupe(),
                            Vec1::try_from_vec(ids.to_vec()).unwrap(),
                            t_out_inner.dupe(),
                        );
                        let uparts = (
                            use_op.dupe(),
                            reason.dupe(),
                            calltype.call_targs.dupe(),
                            hint_unavailable(),
                        );
                        let t_ = FlowJs::instantiate_poly_call_or_new(
                            cx, env, trace, lparts, uparts, &check,
                        )?;
                        call_latent_pred(
                            cx,
                            env,
                            trace,
                            &t_,
                            use_op,
                            reason,
                            &None,
                            argts,
                            sense,
                            is_target,
                            tin,
                            result_collector,
                        )?;
                    }
                    // Fall through all the remaining cases
                    _ => {
                        report_unchanged_filtering_result_to_predicate_result(
                            tin.dupe(),
                            result_collector,
                        );
                    }
                }
            }
            // Fall through all the remaining cases
            _ => {
                report_unchanged_filtering_result_to_predicate_result(tin.dupe(), result_collector);
            }
        }
    }
    Ok(())
}

/// Whether the generic whose bound is being refined is a subtype of the type guard `type_guard`
/// (`repositioned` is the same guard with the reason of the call). The bound alone can miss this,
/// e.g. when the guard mentions the generic itself.
fn refined_generic_subsumed<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    result_collector: &PredicateResultCollector,
    type_guard: &Type,
    repositioned: &Type,
) -> Result<bool, JobError> {
    let Some(RefinedGeneric {
        generic,
        subsumed_by_guard,
    }) = &result_collector.refined_generic
    else {
        return Ok(false);
    };
    if let Some((_, subsumed)) = subsumed_by_guard
        .borrow()
        .iter()
        .find(|(guard, _)| Type::ptr_eq(guard, type_guard))
    {
        return Ok(*subsumed);
    }
    let subsumed = type_util::quick_subtype(None::<&mut fn(&Type)>, generic, repositioned)
        || FlowJs::speculative_subtyping_succeeds_with_env(cx, env, generic, repositioned)?;
    subsumed_by_guard
        .borrow_mut()
        .push((type_guard.dupe(), subsumed));
    Ok(subsumed)
}

fn call_latent_param_pred<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    fun_t: &Type,
    use_op: &UseOp,
    reason: &Reason,
    targs: &Option<Rc<[Targ]>>,
    argts: &[CallArg],
    sense: bool,
    idx: &[i32],
    tin: &Type,
    result_collector: &PredicateResultCollector,
) -> Result<(), FlowJsException> {
    let idx_vec: Vec<i32> = idx.to_vec();
    call_latent_pred(
        cx,
        env,
        trace,
        fun_t,
        use_op,
        reason,
        targs,
        argts,
        sense,
        &|type_guard_name: &str, params: &[FunParam]| {
            idx_vec.iter().any(|&i| match params.get(i as usize) {
                Some(FunParam(Some(name), _)) => name.as_str() == type_guard_name,
                _ => false,
            })
        },
        tin,
        result_collector,
    )
}

fn call_latent_this_pred<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    fun_t: &Type,
    use_op: &UseOp,
    reason: &Reason,
    targs: &Option<Rc<[Targ]>>,
    argts: &[CallArg],
    sense: bool,
    tin: &Type,
    result_collector: &PredicateResultCollector,
) -> Result<(), FlowJsException> {
    call_latent_pred(
        cx,
        env,
        trace,
        fun_t,
        use_op,
        reason,
        targs,
        argts,
        sense,
        &|type_guard_name: &str, _params: &[FunParam]| type_guard_name == "this",
        tin,
        result_collector,
    )
}

/// The concrete types of `t`, or none when concretizing it fails for a reason other than
/// cancellation, a timeout or a debug throw.
fn concrete_types_for_inspection<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t: &Type,
) -> Result<Vec<Type>, JobError> {
    match FlowJs::possible_concrete_types_for_inspection_with_env(cx, env, reason, t) {
        Ok(ts) => Ok(ts),
        Err(FlowJsException::WorkerCanceled(c)) => Err(JobError::Canceled(c)),
        Err(FlowJsException::TimedOut(t)) => Err(JobError::TimedOut(t)),
        Err(FlowJsException::DebugThrow { loc }) => Err(JobError::DebugThrow { loc }),
        Err(_) => Ok(Vec::new()),
    }
}

/// Whether `t` has `any` as an element type, a type argument or the type of a property.
fn has_shallow_any<'cx>(cx: &mut Context<'cx>, t: &Type) -> bool {
    fn resolves_to_any<'cx>(cx: &mut Context<'cx>, t: &Type) -> bool {
        match cx.find_resolved(t) {
            Some(t) => is_any_t(&t),
            None => is_any_t(t),
        }
    }
    match t.deref() {
        TypeInner::DefT(_, d) => match d.deref() {
            DefTInner::ArrT(arr) => match arr.deref() {
                ArrType::ArrayAT(box ArrayATData { elem_t, .. })
                | ArrType::ROArrayAT(box ROArrayATData { elem_t, .. }) => {
                    resolves_to_any(cx, elem_t)
                }
                ArrType::TupleAT(box TupleATData { elements, .. }) => {
                    elements.iter().any(|elem| resolves_to_any(cx, &elem.t))
                }
            },
            DefTInner::InstanceT(instance) => instance
                .inst
                .type_args
                .iter()
                .any(|(_, _, t, _)| resolves_to_any(cx, t)),
            DefTInner::ObjT(obj) => {
                let dict_any = match &obj.flags.obj_kind {
                    ObjKind::Indexed(dict) => resolves_to_any(cx, &dict.value),
                    _ => false,
                };
                dict_any
                    || cx
                        .find_props(obj.props_tmap.dupe())
                        .iter()
                        .any(|(_, prop)| match prop.deref() {
                            PropertyInner::Field(fd) => resolves_to_any(cx, &fd.type_),
                            _ => false,
                        })
            }
            _ => false,
        },
        _ => false,
    }
}

fn is_empty_t(t: &Type) -> bool {
    matches!(t.deref(), TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::EmptyT))
}

/// The refinement of a type by a type guard.
enum GuardIntersection {
    Refined(Type),
    /// The type is an instance of a class unrelated to the guard's (see `unrelated_classes`).
    /// Holds the intersection of the two, to use if nothing else survives the refinement.
    UnrelatedClasses(Type),
}

impl GuardIntersection {
    fn into_type(self) -> Type {
        match self {
            GuardIntersection::Refined(t) | GuardIntersection::UnrelatedClasses(t) => t,
        }
    }
}

/// An instance of a class that is checked nominally. Classes checked under TypeScript's rules are
/// structural, so another class may have the same instances.
fn class_instance(t: &Type) -> Option<&InstanceT> {
    match t.deref() {
        TypeInner::DefT(_, d) => match d.deref() {
            DefTInner::InstanceT(instance)
                if matches!(
                    instance.inst.inst_kind,
                    InstanceKind::ClassKind | InstanceKind::RecordKind { .. }
                ) && instance.inst.class_id != ALocId::none()
                    && !instance.inst.strictness_kind.is_typescript_loose() =>
            {
                Some(instance)
            }
            _ => None,
        },
        _ => None,
    }
}

/// Whether `i1` and `i2` are instances of the same class, including two declarations of one
/// logical module, which `InstanceT ~> InstanceT` also treats as one class.
fn same_class(cx: &Context<'_>, i1: &InstanceT, i2: &InstanceT) -> bool {
    i1.inst.class_id == i2.inst.class_id
        || type_util::nominal_id_have_same_logical_module(
            &cx.file_options(),
            cx.projects_options(),
            (&i1.inst.class_id, i1.inst.class_name.as_deref()),
            (&i2.inst.class_id, i2.inst.class_name.as_deref()),
        )
}

/// Whether the class of `instance` extends the class of `target`, or `None` when its superclass
/// chain cannot be followed to the root (e.g. it extends `any`, mixes in other classes, or loops
/// back on itself).
fn class_extends<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    instance: &InstanceT,
    target: &InstanceT,
) -> Result<Option<bool>, JobError> {
    let Some(mut seen) = visit_class(&SeenClasses::new(), instance) else {
        return Ok(None);
    };
    let mut super_ = instance.super_.dupe();
    loop {
        let supers = concrete_types_for_inspection(cx, env, reason_of_t(&super_), &super_)?;
        let [t] = supers.as_slice() else {
            return Ok(None);
        };
        match t.deref() {
            TypeInner::ObjProtoT(_) | TypeInner::NullProtoT(_) => return Ok(Some(false)),
            _ => match class_instance(t) {
                Some(instance) if same_class(cx, instance, target) => return Ok(Some(true)),
                Some(instance) => match visit_class(&seen, instance) {
                    Some(next) => {
                        seen = next;
                        super_ = instance.super_.dupe();
                    }
                    None => return Ok(None),
                },
                None => return Ok(None),
            },
        }
    }
}

/// Whether `t1` and `t2` are instances of two classes neither of which extends the other. With
/// single inheritance no value is an instance of both, unless a declared class mixes in both,
/// which `instanceof` refinements also ignore.
fn unrelated_classes<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    t1: &Type,
    t2: &Type,
) -> Result<bool, JobError> {
    let (Some(i1), Some(i2)) = (class_instance(t1), class_instance(t2)) else {
        return Ok(false);
    };
    if same_class(cx, i1, i2) {
        return Ok(false);
    }
    Ok(class_extends(cx, env, i1, i2)? == Some(false)
        && class_extends(cx, env, i2, i1)? == Some(false))
}

fn is_any<'cx>(cx: &mut Context<'cx>, env: &FlowJsEnv, t: &Type) -> Result<bool, JobError> {
    let ts = concrete_types_for_inspection(cx, env, reason_of_t(t), t)?;
    Ok(ts.iter().any(|t| {
        matches!(
            t.deref(),
            TypeInner::AnyT(..)
                | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
                    kind: ImplicitInstantiationTvarKind::Placeholder,
                    ..
                })
        )
    }))
}

/// Refines `t1` by the type guard `t2`. A member of `t1` that is an instance of a class unrelated
/// to the guard's is dropped when another member survives; otherwise the refinement is reported as
/// `UnrelatedClasses`, so that the caller can drop it in favor of other members of a union.
fn intersect_type_guard<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    t1: Type,
    t2: Type,
) -> Result<GuardIntersection, JobError> {
    fn is_null(t: &ConcretizedType) -> bool {
        match t.unwrap().deref() {
            TypeInner::DefT(_, def_t) => matches!(def_t.deref(), DefTInner::NullT),
            _ => false,
        }
    }

    fn is_void(t: &ConcretizedType) -> bool {
        match t.unwrap().deref() {
            TypeInner::DefT(_, def_t) => matches!(def_t.deref(), DefTInner::VoidT),
            _ => false,
        }
    }

    fn type_tags_differ<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        depth: i32,
        ts1: &[Type],
        ts2: &[Type],
    ) -> bool {
        match (ts1, ts2) {
            ([t1, rest1 @ ..], [t2, rest2 @ ..]) => {
                let t2_c = t2.dupe();
                let all_differ = ConcretizedType::for_all_concrete_ts(cx, env, t1, &|cx, ct1| {
                    types_differ(cx, env, depth, ct1, &t2_c)
                })
                .unwrap_or(false);
                all_differ || type_tags_differ(cx, env, depth, rest1, rest2)
            }
            _ => false,
        }
    }

    // C<T> has no overlap with C<S> iff T and S have no overlap
    fn instance_tags_differ<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        depth: i32,
        t1: &ConcretizedType,
        t2: &ConcretizedType,
    ) -> bool {
        match (t1.unwrap().deref(), t2.unwrap().deref()) {
            (TypeInner::DefT(_, d1), TypeInner::DefT(_, d2)) => match (d1.deref(), d2.deref()) {
                (DefTInner::InstanceT(inst_t1), DefTInner::InstanceT(inst_t2)) => {
                    let inst1 = &inst_t1.inst;
                    let inst2 = &inst_t2.inst;
                    let kind1_ok = matches!(
                        inst1.inst_kind,
                        InstanceKind::ClassKind | InstanceKind::RecordKind { .. }
                    );
                    let kind2_ok = matches!(
                        inst2.inst_kind,
                        InstanceKind::ClassKind | InstanceKind::RecordKind { .. }
                    );
                    if kind1_ok
                        && kind2_ok
                        && flow_js_utils::is_same_instance_type(inst_t1, inst_t2)
                    {
                        let ts1: Vec<Type> = inst1
                            .type_args
                            .iter()
                            .map(|(_, _, t, _)| t.dupe())
                            .collect();
                        let ts2: Vec<Type> = inst2
                            .type_args
                            .iter()
                            .map(|(_, _, t, _)| t.dupe())
                            .collect();
                        type_tags_differ(cx, env, depth, &ts1, &ts2)
                    } else {
                        false
                    }
                }
                _ => false,
            },
            _ => false,
        }
    }

    // Whether `t` and `u` have no overlap. Types without values, such as `empty` or an opaque type
    // whose supertype is `empty`, are disjoint from every type, even from themselves, so a
    // component of such a type does not make its container disjoint from another.
    fn inhabited_types_differ<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        depth: i32,
        t: &Type,
        u: &Type,
    ) -> bool {
        // To prevent infinite recursion, we use a simple depth mechanism.
        if depth > 2 || Type::ptr_eq(t, u) {
            return false;
        }
        let uninhabited = |cx: &mut Context<'cx>, ts: &[Type]| {
            ts.is_empty()
                || ts
                    .iter()
                    .any(|t| disjoint::tag_of_t(cx, t).is_some_and(|tags| tags.is_empty()))
        };
        let (Ok(ts), Ok(us)) = (
            concrete_types_for_inspection(cx, env, reason_of_t(t), t),
            concrete_types_for_inspection(cx, env, reason_of_t(u), u),
        ) else {
            return false;
        };
        if uninhabited(cx, &ts) || uninhabited(cx, &us) {
            return false;
        }
        ts.iter().all(|t| {
            us.iter().all(|u| {
                concrete_types_differ(
                    cx,
                    env,
                    depth + 1,
                    &ConcretizedType(t.dupe()),
                    &ConcretizedType(u.dupe()),
                )
            })
        })
    }

    // Tuples whose lengths cannot be equal have no overlap, and neither do tuples with elements at
    // the same index that have no overlap. As with `C<T>` above, `Array<T>` has no overlap with
    // `Array<S>` iff T and S have no overlap, and the same goes for `ReadonlyArray`.
    fn arr_tags_differ<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        depth: i32,
        t1: &ConcretizedType,
        t2: &ConcretizedType,
    ) -> bool {
        fn arr_of(t: &Type) -> Option<&ArrType> {
            match t.deref() {
                TypeInner::DefT(_, d) => match d.deref() {
                    DefTInner::ArrT(arr) => Some(arr),
                    _ => None,
                },
                _ => None,
            }
        }
        fn max_arity(tuple: &TupleATData) -> i32 {
            if tuple.inexact || tuple.rest.is_some() {
                i32::MAX
            } else {
                tuple.arity.1
            }
        }
        // The types of the elements that are present at the start of every value of `tuple`.
        fn required_elements(tuple: &TupleATData) -> impl Iterator<Item = Option<&Type>> {
            tuple
                .elements
                .iter()
                .take_while(|elem| !elem.optional)
                .map(|elem| match elem.polarity {
                    Polarity::Negative => None,
                    _ => Some(&elem.t),
                })
        }
        let (Some(a1), Some(a2)) = (arr_of(t1.unwrap()), arr_of(t2.unwrap())) else {
            return false;
        };
        match (a1, a2) {
            (ArrType::TupleAT(tup1), ArrType::TupleAT(tup2)) => {
                max_arity(tup1) < tup2.arity.0
                    || max_arity(tup2) < tup1.arity.0
                    || required_elements(tup1)
                        .zip(required_elements(tup2))
                        .filter_map(|(t1, t2)| Some((t1?, t2?)))
                        .any(|(t1, t2)| inhabited_types_differ(cx, env, depth, t1, t2))
            }
            (
                ArrType::ArrayAT(box ArrayATData { elem_t: e1, .. }),
                ArrType::ArrayAT(box ArrayATData { elem_t: e2, .. }),
            )
            | (
                ArrType::ROArrayAT(box ROArrayATData { elem_t: e1, .. }),
                ArrType::ROArrayAT(box ROArrayATData { elem_t: e2, .. }),
            ) => inhabited_types_differ(cx, env, depth, e1, e2),
            _ => false,
        }
    }

    // Objects, interfaces and instances with a property of disjoint types have no overlap. Only
    // properties that are present (not optional) and readable count.
    fn props_differ<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        depth: i32,
        t1: &ConcretizedType,
        t2: &ConcretizedType,
    ) -> bool {
        fn props_of(t: &Type) -> Option<Vec<properties::Id>> {
            match t.deref() {
                TypeInner::DefT(_, d) => match d.deref() {
                    DefTInner::ObjT(obj) => Some(vec![obj.props_tmap.dupe()]),
                    DefTInner::InstanceT(instance) => Some(vec![
                        instance.inst.own_props.dupe(),
                        instance.inst.proto_props.dupe(),
                    ]),
                    _ => None,
                },
                _ => None,
            }
        }
        // Own properties shadow those of the prototype.
        fn present_read_t(maps: &[PropertiesMap], name: &Name) -> Option<Type> {
            let t = maps
                .iter()
                .find_map(|map| property::read_t(map.get(name)?))?;
            (!is_prop_optional(&t)).then_some(t)
        }
        let (Some(ids1), Some(ids2)) = (props_of(t1.unwrap()), props_of(t2.unwrap())) else {
            return false;
        };
        if ids1 == ids2 {
            return false;
        }
        let maps1: Vec<PropertiesMap> = ids1.into_iter().map(|id| cx.find_props(id)).collect();
        let maps2: Vec<PropertiesMap> = ids2.into_iter().map(|id| cx.find_props(id)).collect();
        maps1
            .iter()
            .flat_map(|map| map.iter().map(|(name, _)| name))
            .filter_map(|name| Some((present_read_t(&maps1, name)?, present_read_t(&maps2, name)?)))
            .any(|(t1, t2)| inhabited_types_differ(cx, env, depth, &t1, &t2))
    }

    fn types_differ<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        depth: i32,
        t1: &ConcretizedType,
        t2: &Type,
    ) -> bool {
        // To prevent infinite recursion, we use a simple depth mechanism.
        if depth > 2 {
            return false;
        }
        ConcretizedType::for_all_concrete_ts(cx, env, t2, &|cx, ct2| {
            concrete_types_differ(cx, env, depth + 1, t1, ct2)
        })
        .unwrap_or(false)
    }

    fn concrete_types_differ<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        depth: i32,
        t1: &ConcretizedType,
        t2: &ConcretizedType,
    ) -> bool {
        disjoint::tags_differ(cx, t1.unwrap(), t2.unwrap())
            || disjoint::ground_types_differ(t1.unwrap(), t2.unwrap())
            || instance_tags_differ(cx, env, depth, t1, t2)
            || arr_tags_differ(cx, env, depth, t1, t2)
            || props_differ(cx, env, depth, t1, t2)
    }

    fn try_intersect<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason1: &Reason,
        t1_conc: &ConcretizedType,
        t2: &Type,
    ) -> Result<Option<Type>, JobError> {
        let t1 = t1_conc.unwrap();
        // A predicate over a union runs once per member, so avoid concretizing the
        // entire guard union below when its enum index can prove membership.
        let resolved_t2 = cx.find_resolved(t2);
        let quick_t2 = resolved_t2.as_ref().unwrap_or(t2);
        if let TypeInner::UnionT(_, rep) = quick_t2.deref() {
            if matches!(
                union_rep::quick_mem_enum(
                    cx,
                    |_, l, u| type_util::quick_subtype(None::<&mut fn(&Type)>, l, u),
                    t1,
                    rep,
                ),
                QuickMemResult::Yes
            ) {
                return Ok(Some(t1.dupe()));
            }
        }
        if types_differ(cx, env, 0, t1_conc, t2) {
            let r = reason1
                .dupe()
                .update_desc(|d| d.map(VirtualReasonDesc::invalidate_rtype_alias));
            Ok(Some(Type::new(TypeInner::DefT(
                r,
                DefT::new(DefTInner::EmptyT),
            ))))
        } else if is_any(cx, env, t1)? {
            Ok(Some(t2.dupe()))
        } else if is_any(cx, env, t2)? {
            // Filter out null and void types from the input if comparing with any
            let t1_wrapped = ConcretizedType::wrap_unsafe(t1.dupe());
            if is_null(&t1_wrapped) || is_void(&t1_wrapped) {
                let r = reason1
                    .dupe()
                    .update_desc(|d| d.map(VirtualReasonDesc::invalidate_rtype_alias));
                Ok(Some(Type::new(TypeInner::DefT(
                    r,
                    DefT::new(DefTInner::EmptyT),
                ))))
            } else {
                Ok(Some(t1.dupe()))
            }
        } else {
            try_intersect_related(cx, env, t1, t2)
        }
    }

    // `t1` and `t2` are concrete, neither is `any`, and they are not known to be disjoint.
    fn try_intersect_related<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        t1: &Type,
        t2: &Type,
    ) -> Result<Option<Type>, JobError> {
        // Only `t2` itself, `empty` and `any` are below a literal, `null` or `void`, so if `t2`
        // is below `t1` but not equal to it, the speculative check of `t1 <: t2` would fail.
        if is_literal_value(t2)
            && !type_util::quick_subtype(None::<&mut fn(&Type)>, t1, t2)
            && type_util::quick_subtype(None::<&mut fn(&Type)>, t2, t1)
        {
            Ok(Some(t2.dupe()))
        } else if type_util::quick_subtype(None::<&mut fn(&Type)>, t1, t2)
            || FlowJs::speculative_subtyping_succeeds_with_env(cx, env, t1, t2)?
        {
            // `any` inside `t1` makes it a subtype of the guard. When the guard is a subtype of
            // `t1` as well, it is the more precise of the two.
            if has_shallow_any(cx, t1)
                && (type_util::quick_subtype(None::<&mut fn(&Type)>, t2, t1)
                    || FlowJs::speculative_subtyping_succeeds_with_env(cx, env, t2, t1)?)
            {
                Ok(Some(t2.dupe()))
            } else {
                Ok(Some(t1.dupe()))
            }
        } else if type_util::quick_subtype(None::<&mut fn(&Type)>, t2, t1)
            || FlowJs::speculative_subtyping_succeeds_with_env(cx, env, t2, t1)?
        {
            Ok(Some(t2.dupe()))
        } else {
            match t1.deref() {
                TypeInner::NominalT {
                    reason: r,
                    nominal_type,
                } => {
                    // Apply the refinement on super and underlying type of opaque type.
                    // Preserve nominal_id to retain compatibility with original type.
                    let upper_t = Some(match &nominal_type.upper_t {
                        Some(upper) => {
                            intersect_type_guard(cx, env, upper.dupe(), t2.dupe())?.into_type()
                        }
                        None => t2.dupe(),
                    });
                    let underlying_t = match &nominal_type.underlying_t {
                        nominal::UnderlyingT::FullyOpaque => nominal::UnderlyingT::FullyOpaque,
                        nominal::UnderlyingT::CustomError(box nominal::CustomErrorData {
                            custom_error_loc,
                            t: inner_t,
                        }) => {
                            nominal::UnderlyingT::CustomError(Box::new(nominal::CustomErrorData {
                                custom_error_loc: custom_error_loc.dupe(),
                                t: intersect_type_guard(cx, env, inner_t.dupe(), t2.dupe())?
                                    .into_type(),
                            }))
                        }
                        nominal::UnderlyingT::OpaqueWithLocal { t: inner_t } => {
                            nominal::UnderlyingT::OpaqueWithLocal {
                                t: intersect_type_guard(cx, env, inner_t.dupe(), t2.dupe())?
                                    .into_type(),
                            }
                        }
                    };
                    Ok(Some(Type::new(TypeInner::NominalT {
                        reason: r.dupe(),
                        nominal_type: Rc::new(NominalType::new(NominalTypeInner {
                            nominal_id: nominal_type.nominal_id.clone(),
                            underlying_t,
                            lower_t: nominal_type.lower_t.as_ref().map(|t| t.dupe()),
                            upper_t,
                            nominal_type_args: nominal_type.nominal_type_args.dupe(),
                        })),
                    })))
                }
                _ => Ok(None),
            }
        }
    }

    // `t1` is concrete and neither a subtype nor a supertype of the guard `t2` as a whole. Since
    // `(A | B) & t1 = (A & t1) | (B & t1)`, a union guard is intersected with `t1` one member at a
    // time, so that members disjoint from `t1` drop out and members comparable with `t1` keep the
    // more specific type.
    fn intersect_guard_members<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason1: &Reason,
        t1: &Type,
        t2: &Type,
        t2s: &[Type],
    ) -> Result<GuardIntersection, JobError> {
        let r = reason1
            .dupe()
            .update_desc(|d| d.map(VirtualReasonDesc::invalidate_rtype_alias));
        let mk_intersection = |t2: &Type| {
            Type::new(TypeInner::IntersectionT(
                r.dupe(),
                inter_rep::make(t2.dupe(), t1.dupe(), Rc::from([])),
            ))
        };
        if t2s.len() < 2 {
            let intersection = mk_intersection(t2);
            return Ok(match t2s {
                [t2_member] if unrelated_classes(cx, env, t1, t2_member)? => {
                    GuardIntersection::UnrelatedClasses(intersection)
                }
                _ => GuardIntersection::Refined(intersection),
            });
        }
        let t1_conc = ConcretizedType::wrap_unsafe(t1.dupe());
        let mut simplified = false;
        let mut parts = Vec::new();
        let mut unrelated = Vec::new();
        for t2_member in t2s {
            // `try_intersect` has already ruled out `any` in `t1` and `t2`.
            let t2_conc = ConcretizedType::wrap_unsafe(t2_member.dupe());
            if concrete_types_differ(cx, env, 1, &t1_conc, &t2_conc) {
                simplified = true;
                continue;
            }
            match try_intersect_related(cx, env, t1, t2_member)? {
                Some(t) => {
                    simplified = true;
                    if !is_empty_t(&t) {
                        parts.push(t);
                    }
                }
                None if unrelated_classes(cx, env, t1, t2_member)? => {
                    simplified = true;
                    unrelated.push(mk_intersection(t2_member));
                }
                None => parts.push(mk_intersection(t2_member)),
            }
        }
        if !simplified {
            Ok(GuardIntersection::Refined(mk_intersection(t2)))
        } else if parts.is_empty() && !unrelated.is_empty() {
            Ok(GuardIntersection::UnrelatedClasses(type_util::union_of_ts(
                r, unrelated, None,
            )))
        } else {
            Ok(GuardIntersection::Refined(type_util::union_of_ts(
                r, parts, None,
            )))
        }
    }

    let reason1 = reason_of_t(&t1);
    match try_intersect(
        cx,
        env,
        reason1,
        &ConcretizedType::wrap_unsafe(t1.dupe()),
        &t2,
    )? {
        Some(t) => Ok(GuardIntersection::Refined(t)),
        None => {
            // No definitive refinement found. We fall back to more expensive
            //  concretization that breaks up all unions (including optimized ones)
            let ts = concrete_types_for_inspection(cx, env, reason1, &t1)?;
            let mut t2s: Option<Vec<Type>> = None;
            let mut mapped: Vec<Type> = Vec::new();
            let mut unrelated: Vec<Type> = Vec::new();
            for t1_inner in ts {
                // t1 was just concretized
                match try_intersect(
                    cx,
                    env,
                    reason1,
                    &ConcretizedType::wrap_unsafe(t1_inner.dupe()),
                    &t2,
                )? {
                    Some(t) => mapped.push(t),
                    None => {
                        let t2s: &[Type] = match &mut t2s {
                            Some(t2s) => t2s,
                            slot => slot.insert(concrete_types_for_inspection(
                                cx,
                                env,
                                reason_of_t(&t2),
                                &t2,
                            )?),
                        };
                        match intersect_guard_members(cx, env, reason1, &t1_inner, &t2, t2s)? {
                            GuardIntersection::Refined(t) => mapped.push(t),
                            GuardIntersection::UnrelatedClasses(t) => unrelated.push(t),
                        }
                    }
                }
            }
            let r = reason1
                .dupe()
                .update_desc(|d| d.map(VirtualReasonDesc::invalidate_rtype_alias));
            if unrelated.is_empty() || mapped.iter().any(|t| !is_empty_t(t)) {
                Ok(GuardIntersection::Refined(type_util::union_of_ts(
                    r, mapped, None,
                )))
            } else {
                Ok(GuardIntersection::UnrelatedClasses(type_util::union_of_ts(
                    r, unrelated, None,
                )))
            }
        }
    }
}

fn is_any_t(t: &Type) -> bool {
    matches!(
        t.deref(),
        TypeInner::AnyT(..)
            | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
                kind: ImplicitInstantiationTvarKind::Placeholder,
                ..
            })
    )
}

fn is_bool_general_t(t: &Type) -> bool {
    matches!(t.deref(), TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::BoolGeneralT))
}

/// This utility is expected to be used when negating the refinement of a type [t1]
/// with a type guard `x is t2`. It removes the members of t1 that are subtypes of t2,
/// since for those the positive branch is always taken; if t1 <: t2, we are left with
/// `empty` in the negated case. `any` may or may not satisfy the guard, so it is kept,
/// and `boolean` is split into `true | false`.
fn type_guard_diff<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    t1: &Type,
    t2: &Type,
) -> Result<FilterResult, JobError> {
    let reason1 = reason_of_t(t1);
    let satisfies_guard = |cx: &mut Context<'cx>, t: &Type| -> Result<bool, JobError> {
        Ok(type_util::quick_subtype(None::<&mut fn(&Type)>, t, t2)
            || FlowJs::speculative_subtyping_succeeds_with_env(cx, env, t, t2)?)
    };
    if is_any_t(t1) {
        Ok(FilterResult {
            type_: t1.dupe(),
            changed: false,
        })
    } else if satisfies_guard(cx, t1)? {
        let r = reason1
            .dupe()
            .update_desc(|d| d.map(VirtualReasonDesc::invalidate_rtype_alias));
        Ok(FilterResult {
            type_: Type::new(TypeInner::DefT(r, DefT::new(DefTInner::EmptyT))),
            changed: true,
        })
    } else {
        let t1s_conc = concrete_types_for_inspection(cx, env, reason1, t1)?;
        let mut ts: Vec<Type> = Vec::new();
        let mut changed = false;
        for t1_inner in t1s_conc {
            if !is_any_t(&t1_inner) && satisfies_guard(cx, &t1_inner)? {
                continue;
            }
            if is_bool_general_t(&t1_inner) {
                let true_t = type_filter::true_(t1_inner.dupe()).type_;
                let false_t = type_filter::false_(t1_inner.dupe()).type_;
                match (
                    satisfies_guard(cx, &true_t)?,
                    satisfies_guard(cx, &false_t)?,
                ) {
                    (true, true) => continue,
                    (true, false) => ts.push(false_t),
                    (false, true) => ts.push(true_t),
                    (false, false) => ts.push(t1_inner),
                }
                changed = true;
                continue;
            }
            ts.push(t1_inner);
            changed = true;
        }
        let r1 = reason1
            .dupe()
            .update_desc(|d| d.map(VirtualReasonDesc::invalidate_rtype_alias));
        Ok(FilterResult {
            type_: type_util::union_of_ts(r1, ts, None),
            changed,
        })
    }
}

fn prop_exists_test<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    key: &str,
    sense: bool,
    obj: &Type,
    result_collector: &PredicateResultCollector,
) -> Result<(), FlowJsException> {
    match has_prop(cx, env, &Name::new(key), obj)? {
        Some(has) => {
            if has == sense {
                report_unchanged_filtering_result_to_predicate_result(obj.dupe(), result_collector);
            } else {
                report_changes_to_input(result_collector);
            }
        }
        None => {
            report_unchanged_filtering_result_to_predicate_result(obj.dupe(), result_collector);
        }
    }
    Ok(())
}

/// If an object has an own or non-own prop, representing `'key' in obj`.
/// Returns `None` if it is unknown whether the object has the prop (for example
/// due to inexact objects)
fn has_prop<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    key: &Name,
    obj: &Type,
) -> Result<Option<bool>, FlowJsException> {
    fn some_has_prop(xs: impl IntoIterator<Item = Option<bool>>) -> Option<bool> {
        let mut acc = Some(false);
        for x in xs {
            acc = match (acc, x) {
                (Some(true), None) | (None, Some(true)) => Some(true),
                (_, None) | (None, _) => None,
                (Some(a), Some(b)) => Some(a || b),
            };
        }
        acc
    }

    fn try_all_have_prop<T, E>(
        xs: impl IntoIterator<Item = T>,
        mut f: impl FnMut(T) -> Result<Option<bool>, E>,
    ) -> Result<Option<bool>, E> {
        let mut acc = true;
        for x in xs {
            match f(x)? {
                Some(b) => acc = acc && b,
                None => return Ok(None),
            }
        }
        Ok(Some(acc))
    }

    fn try_some_has_prop<T, E>(
        xs: impl IntoIterator<Item = T>,
        mut f: impl FnMut(T) -> Result<Option<bool>, E>,
    ) -> Result<Option<bool>, E> {
        let mut acc = Some(false);
        for x in xs {
            acc = match (acc, f(x)?) {
                (Some(true), None) | (None, Some(true)) => Some(true),
                (_, None) | (None, _) => None,
                (Some(a), Some(b)) => Some(a || b),
            };
        }
        Ok(acc)
    }

    fn find_key<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        exact: bool,
        super_t: &Type,
        props_list: &[properties::Id],
        key: &Name,
    ) -> Result<Option<bool>, FlowJsException> {
        let current_has_prop =
            some_has_prop(
                props_list
                    .iter()
                    .map(|props| match cx.get_prop(props.dupe(), key) {
                        Some(prop) => match prop.deref() {
                            PropertyInner::Field(fd) if is_prop_optional(&fd.type_) => None,
                            _ => Some(true),
                        },
                        None => Some(false),
                    }),
            );

        match current_has_prop {
            Some(true) => Ok(Some(true)),
            _ => {
                let super_ts = FlowJs::possible_concrete_types_for_inspection_with_env(
                    cx,
                    env,
                    reason_of_t(super_t),
                    super_t,
                )?;
                let super_has_prop =
                    try_all_have_prop(super_ts.iter(), |t| has_prop(cx, env, key, t))?;
                match super_has_prop {
                    Some(true) => Ok(Some(true)),
                    Some(false) if !exact => Ok(None),
                    Some(false) => Ok(current_has_prop),
                    None => Ok(None),
                }
            }
        }
    }

    match obj.deref() {
        TypeInner::DefT(_, def_t) => match def_t.deref() {
            DefTInner::ObjT(obj_t) => find_key(
                cx,
                env,
                obj_type::is_exact(&obj_t.flags.obj_kind),
                &obj_t.proto_t,
                &[obj_t.props_tmap.dupe()],
                key,
            ),
            DefTInner::InstanceT(inst_t) => find_key(
                cx,
                env,
                false,
                &inst_t.super_,
                &[inst_t.inst.own_props.dupe(), inst_t.inst.proto_props.dupe()],
                key,
            ),
            _ => Ok(None),
        },
        TypeInner::NullProtoT(_) => Ok(Some(false)),
        TypeInner::ObjProtoT(_) => Ok(Some(flow_js_utils::is_object_prototype_method(key))),
        TypeInner::FunProtoT(_) => Ok(Some(flow_js_utils::is_function_prototype(key))),
        TypeInner::IntersectionT(reason, rep) => {
            let member_has_prop = try_some_has_prop(rep.members_iter(), |t| {
                let ts =
                    FlowJs::possible_concrete_types_for_inspection_with_env(cx, env, reason, t)?;
                try_all_have_prop(ts.iter(), |t| has_prop(cx, env, key, t))
            })?;
            Ok(member_has_prop)
        }
        _ => Ok(None),
    }
}

fn prop_truthy_test<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    key: &str,
    reason: &Reason,
    sense: bool,
    obj: &Type,
    result_collector: &PredicateResultCollector,
) -> Result<(), FlowJsException> {
    prop_exists_test_generic(
        key,
        reason,
        cx,
        env,
        result_collector,
        obj,
        sense,
        (&PropGuard::PropGuardTruthy, &PropGuard::PropGuardNotTruthy),
        obj,
    )
}

fn prop_non_maybe_test<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    key: &str,
    reason: &Reason,
    sense: bool,
    obj: &Type,
    result_collector: &PredicateResultCollector,
) -> Result<(), FlowJsException> {
    prop_exists_test_generic(
        key,
        reason,
        cx,
        env,
        result_collector,
        obj,
        sense,
        (&PropGuard::PropGuardNotMaybe, &PropGuard::PropGuardMaybe),
        obj,
    )
}

fn prop_is_exactly_null_test<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    key: &str,
    reason: &Reason,
    sense: bool,
    obj: &Type,
    result_collector: &PredicateResultCollector,
) -> Result<(), FlowJsException> {
    prop_exists_test_generic(
        key,
        reason,
        cx,
        env,
        result_collector,
        obj,
        sense,
        (&PropGuard::PropGuardNull, &PropGuard::PropGuardNotNull),
        obj,
    )
}

fn prop_non_void_test<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    key: &str,
    reason: &Reason,
    sense: bool,
    obj: &Type,
    result_collector: &PredicateResultCollector,
) -> Result<(), FlowJsException> {
    prop_exists_test_generic(
        key,
        reason,
        cx,
        env,
        result_collector,
        obj,
        sense,
        (&PropGuard::PropGuardNotVoid, &PropGuard::PropGuardVoid),
        obj,
    )
}

fn prop_exists_test_generic<'cx>(
    key: &str,
    reason: &Reason,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    result_collector: &PredicateResultCollector,
    orig_obj: &Type,
    sense: bool,
    pred_pair: (&PropGuard, &PropGuard),
    obj: &Type,
) -> Result<(), FlowJsException> {
    let (pred, not_pred) = pred_pair;
    match obj.deref() {
        TypeInner::DefT(_, def_t) => {
            match def_t.deref() {
                DefTInner::ObjT(obj_t) => {
                    match cx.get_prop(obj_t.props_tmap.dupe(), &Name::new(key)) {
                        Some(p) => {
                            match flow_typing_type::type_::property::read_t(&p) {
                                Some(t) => {
                                    // prop is present on object type
                                    let pred = if sense { pred } else { not_pred };
                                    concretize_and_guard_prop(
                                        cx,
                                        env,
                                        &t,
                                        pred,
                                        orig_obj,
                                        result_collector,
                                    )?;
                                }
                                None => {
                                    // prop cannot be read
                                    report_unchanged_filtering_result_to_predicate_result(
                                        orig_obj.dupe(),
                                        result_collector,
                                    );
                                    flow_js_utils::add_output_with_env(
                                        cx,
                                        env,
                                        ErrorMessage::EPropNotReadable(Box::new(
                                            EPropNotReadableData {
                                                prop_loc: reason.loc().dupe(),
                                                prop_name: Some(Name::new(key)),
                                                use_op: unknown_use(),
                                            },
                                        )),
                                    )?;
                                }
                            }
                        }
                        None if obj_type::is_exact(&obj_t.flags.obj_kind) => {
                            // prop is absent from exact object type
                            if sense {
                                report_changes_to_input(result_collector);
                            } else {
                                report_unchanged_filtering_result_to_predicate_result(
                                    orig_obj.dupe(),
                                    result_collector,
                                );
                            }
                        }
                        None => {
                            // prop is absent from inexact object type
                            // TODO: possibly unsound to filter out orig_obj here, but if we don't,
                            // case elimination based on prop existence checking doesn't work for
                            // (disjoint unions of) intersections of objects, where the prop appears
                            // in a different branch of the intersection. It is easy to avoid this
                            // unsoundness with slightly more work, but will wait until a
                            // refactoring of property lookup lands to revisit. Tracked by
                            // #11301092.
                            if orig_obj == obj {
                                report_unchanged_filtering_result_to_predicate_result(
                                    orig_obj.dupe(),
                                    result_collector,
                                );
                            }
                        }
                    }
                }
                // As in TypeScript, an instance is only ruled out as a member of a union, by a
                // field whose type consists of literal values, e.g. the `done` of
                // `IteratorResult`.
                DefTInner::InstanceT(inst_t) => {
                    let name = Name::new(key);
                    let prop = cx.get_prop(inst_t.inst.own_props.dupe(), &name);
                    match prop.as_ref().map(flow_typing_type::type_::property::read_t) {
                        Some(Some(t)) => {
                            let pred = if sense { pred } else { not_pred };
                            let ts =
                                FlowJs::possible_concrete_types_for_operators_checking_with_env(
                                    cx,
                                    env,
                                    reason_of_t(&t),
                                    &t,
                                )?;
                            if result_collector.from_union.get()
                                && ts.iter().all(is_literal_value)
                                && ts.iter().all(|t| guard_prop(cx, t, pred))
                            {
                                report_changes_to_input(result_collector);
                            } else {
                                report_unchanged_filtering_result_to_predicate_result(
                                    orig_obj.dupe(),
                                    result_collector,
                                );
                            }
                        }
                        Some(None) => {
                            report_unchanged_filtering_result_to_predicate_result(
                                orig_obj.dupe(),
                                result_collector,
                            );
                            flow_js_utils::add_output_with_env(
                                cx,
                                env,
                                ErrorMessage::EPropNotReadable(Box::new(EPropNotReadableData {
                                    prop_loc: reason.loc().dupe(),
                                    prop_name: Some(name),
                                    use_op: unknown_use(),
                                })),
                            )?;
                        }
                        // The prop may still be a method, an accessor or an inherited field.
                        None => {
                            report_unchanged_filtering_result_to_predicate_result(
                                orig_obj.dupe(),
                                result_collector,
                            );
                        }
                    }
                }
                DefTInner::ArrT(arr_t)
                    if let ArrType::TupleAT(box TupleATData { elements, rest, .. }) =
                        arr_t.deref()
                        && flow_js_utils::is_str_intlike(key) =>
                {
                    let i: usize = key.parse().unwrap();
                    match elements.get(i) {
                        // An element of the rest element may or may not exist.
                        None if rest.is_some() => {
                            report_unchanged_filtering_result_to_predicate_result(
                                orig_obj.dupe(),
                                result_collector,
                            );
                        }
                        Some(TupleElement {
                            t, polarity, name, ..
                        }) => {
                            if Polarity::compat(*polarity, Polarity::Positive) {
                                let pred = if sense { pred } else { not_pred };
                                concretize_and_guard_prop(
                                    cx,
                                    env,
                                    t,
                                    pred,
                                    orig_obj,
                                    result_collector,
                                )?;
                            } else {
                                report_unchanged_filtering_result_to_predicate_result(
                                    orig_obj.dupe(),
                                    result_collector,
                                );
                                flow_js_utils::add_output_with_env(
                                    cx,
                                    env,
                                    ErrorMessage::ETupleElementNotReadable(Box::new(
                                        ETupleElementNotReadableData {
                                            use_op: unknown_use(),
                                            loc: reason.loc().dupe(),
                                            index_def_loc: reason.def_loc().dupe(),
                                            index: i as i32,
                                            name: name.as_ref().map(|n| n.dupe()),
                                        },
                                    )),
                                )?;
                            }
                        }
                        None => {
                            // Element is absent from tuple type.
                            if sense {
                                report_changes_to_input(result_collector);
                            } else {
                                report_unchanged_filtering_result_to_predicate_result(
                                    orig_obj.dupe(),
                                    result_collector,
                                );
                            }
                        }
                    }
                }
                _ => {
                    report_unchanged_filtering_result_to_predicate_result(
                        orig_obj.dupe(),
                        result_collector,
                    );
                }
            }
        }
        TypeInner::IntersectionT(inter_reason, rep) => {
            // For an intersection of object types, try the test for each object type in
            // turn, while recording the original intersection so that we end up with
            // the right refinement. See the comment on the implementation of
            // IntersectionPreprocessKit for more details.
            for member_obj in rep.members_iter() {
                let ts = FlowJs::possible_concrete_types_for_inspection_with_env(
                    cx,
                    env,
                    inter_reason,
                    member_obj,
                )?;
                for t in ts.iter() {
                    prop_exists_test_generic(
                        key,
                        reason,
                        cx,
                        env,
                        result_collector,
                        orig_obj,
                        sense,
                        (pred, not_pred),
                        t,
                    )?;
                }
            }
        }
        _ => {
            report_unchanged_filtering_result_to_predicate_result(
                orig_obj.dupe(),
                result_collector,
            );
        }
    }
    Ok(())
}

fn binary_predicate<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    sense: bool,
    test: &BinaryTest,
    left: &Type,
    right: &Type,
    result_collector: &PredicateResultCollector,
) -> Result<(), FlowJsException> {
    match test {
        BinaryTest::InstanceofTest => instanceof_test(
            cx,
            env,
            trace,
            result_collector,
            sense,
            left,
            &InstanceofRhs::TypeOperand(right.dupe()),
        ),
        BinaryTest::SentinelProp(key) => {
            sentinel_prop_test(key, cx, env, result_collector, sense, left, right)
        }
        BinaryTest::EqTest => Ok(eq_test(cx, result_collector, sense, left, right)),
    }
}

// Failing the check means failing it against every instance the signatures can
// construct, so a value survives only if it is unrelated to all of them.
// Reporting each signature into the shared collector would union the results
// instead, handing back from one signature what the previous one pruned, so the
// survivors of each signature become the input of the next.
fn refine_away_construct_returns<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    result_collector: &PredicateResultCollector,
    left: &Type,
    classes: &[InstanceofRhs],
) -> Result<(), FlowJsException> {
    let mut survivors = vec![left.dupe()];
    let mut changed = false;
    for class_t in classes {
        if survivors.is_empty() {
            break;
        }
        let mut next = Vec::new();
        for survivor in &survivors {
            let sub = PredicateResultCollector {
                collector: TypeCollector::create(),
                changed: Rc::new(RefCell::new(false)),
                from_union: Cell::new(result_collector.from_union.get()),
                disjoint_fallback: result_collector.disjoint_fallback.dupe(),
                refined_generic: result_collector.refined_generic.dupe(),
            };
            instanceof_test(cx, env, trace, &sub, false, survivor, class_t)?;
            changed = changed || *sub.changed.borrow();
            next.extend(sub.collector.collect_to_vec());
        }
        survivors = next;
    }
    if changed {
        report_changes_to_input(result_collector);
    }
    for survivor in survivors {
        result_collector.collector.add(survivor);
    }
    Ok(())
}

fn instanceof_test<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    result_collector: &PredicateResultCollector,
    sense: bool,
    left: &Type,
    right: &InstanceofRhs,
) -> Result<(), FlowJsException> {
    if let InstanceofRhs::TypeOperand(right_t) = right
        && !matches!(
            right_t.deref(),
            TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::ClassT(_))
        )
    {
        let concretize = |cx: &mut Context<'_>, t: &Type| -> Result<Vec<Type>, FlowJsException> {
            FlowJs::possible_concrete_types_for_inspection(cx, reason_of_t(t), t)
        };
        let concretize_sig =
            |cx: &mut Context<'_>, t: &Type| -> Result<Vec<Type>, FlowJsException> {
                FlowJs::possible_concrete_types_for_predicate(
                    PredicateConcretetizerVariant::ConcretizeRHSForInstanceOfPredicateTest,
                    cx,
                    env,
                    reason_of_t(t),
                    t,
                )
            };
        let mut construct_returns =
            flow_js_utils::collect_construct_return_ts(&concretize, &concretize_sig, cx, right_t)?;
        // Like TypeScript, narrow to the type of `prototype` when there is no
        // construct signature to read the instance from.
        if construct_returns.is_empty()
            && flow_js_utils::collect_construct_ts(&concretize, cx, right_t)?.is_empty()
            && let Some(prototype) = flow_js_utils::collect_prototype_t(&concretize, cx, right_t)?
        {
            construct_returns.push(prototype);
        }
        if !construct_returns.is_empty() {
            let mut classes = Vec::new();
            for return_t in construct_returns {
                for instance_t in concretize(cx, &return_t)? {
                    classes.push(InstanceofRhs::TypeOperand(Type::new(TypeInner::DefT(
                        reason_of_t(right_t).dupe(),
                        DefT::new(DefTInner::ClassT(instance_t)),
                    ))));
                }
            }
            if sense {
                // A value passes the check when it matches any one signature, so the
                // union the collector already builds is the right combination.
                for class_t in &classes {
                    instanceof_test(cx, env, trace, result_collector, true, left, class_t)?;
                }
            } else {
                refine_away_construct_returns(cx, env, trace, result_collector, left, &classes)?;
            }
            return Ok(());
        }
    }
    match (sense, left.deref(), right) {
        // instanceof on an ArrT is a special case since we treat ArrT as its own
        // type, rather than an InstanceT of the Array builtin class. So, we resolve
        // the ArrT to an InstanceT of Array, and redo the instanceof check. We do
        // it at this stage instead of simply converting (ArrT, InstanceofP(c))
        // to (InstanceT(Array), InstanceofP(c)) because this allows c to be resolved
        // first.
        (true, TypeInner::DefT(reason, def_t), InstanceofRhs::TypeOperand(right_t))
            if let DefTInner::ArrT(arrtype) = def_t.deref()
                && let TypeInner::DefT(r, right_def) = right_t.deref()
                && let DefTInner::ClassT(a) = right_def.deref() =>
        {
            let arr = left.dupe();
            let elemt = elemt_of_arrtype(arrtype);
            let right = InstanceofRhs::InternalExtendsOperand {
                reason: r.dupe(),
                c: arr.dupe(),
                a: a.dupe(),
                seen: SeenClasses::new(),
            };
            let arrt =
                FlowJs::get_builtin_typeapp_with_env(cx, env, reason, None, "Array", vec![elemt]);
            concretize_and_run_predicate(
                cx,
                env,
                trace,
                &arrt,
                PredicateConcretetizerVariant::ConcretizeForGeneralPredicateTest,
                result_collector,
                &|cx, trace, rc, l| instanceof_test(cx, env, trace, rc, true, l, &right),
            )?;
        }
        (false, TypeInner::DefT(reason, def_t), InstanceofRhs::TypeOperand(right_t))
            if let DefTInner::ArrT(arrtype) = def_t.deref()
                && let TypeInner::DefT(r, right_def) = right_t.deref()
                && let DefTInner::ClassT(a) = right_def.deref() =>
        {
            let arr = left.dupe();
            let elemt = elemt_of_arrtype(arrtype);
            let right = InstanceofRhs::InternalExtendsOperand {
                reason: r.dupe(),
                c: arr.dupe(),
                a: a.dupe(),
                seen: SeenClasses::new(),
            };
            let arrt =
                FlowJs::get_builtin_typeapp_with_env(cx, env, reason, None, "Array", vec![elemt]);
            concretize_and_run_predicate(
                cx,
                env,
                trace,
                &arrt,
                PredicateConcretetizerVariant::ConcretizeForGeneralPredicateTest,
                result_collector,
                &|cx, trace, rc, l| instanceof_test(cx, env, trace, rc, false, l, &right),
            )?;
        }
        // Suppose that we have an instance x of class C, and we check whether x is
        // `instanceof` class A. To decide what the appropriate refinement for x
        // should be, we need to decide whether C extends A, choosing either C or A
        // based on the result. Thus, we generate a constraint to decide whether C
        // extends A (while remembering C), which may recursively generate further
        // constraints to decide super(C) extends A, and so on, until we hit the root
        // class. (As a technical tool, we use Extends(_, _) to perform this
        // recursion; it is also used elsewhere for running similar recursive
        // subclass decisions.)
        (true, TypeInner::DefT(_, def_t), InstanceofRhs::TypeOperand(right_t))
            if let DefTInner::InstanceT(..) = def_t.deref()
                && let TypeInner::DefT(r, right_def) = right_t.deref()
                && let DefTInner::ClassT(a) = right_def.deref() =>
        {
            let c = left;
            let right = InstanceofRhs::InternalExtendsOperand {
                reason: r.dupe(),
                c: c.dupe(),
                a: a.dupe(),
                seen: SeenClasses::new(),
            };
            instanceof_test(cx, env, trace, result_collector, true, c, &right)?;
        }
        // If C is a subclass of A, then don't refine the type of x. Otherwise,
        // refine the type of x to A. (In general, the type of x should be refined to
        // C & A, but that's hard to compute.)
        (
            true,
            TypeInner::DefT(reason, def_t),
            InstanceofRhs::InternalExtendsOperand {
                reason: extends_reason,
                c,
                a: a_t,
                seen,
            },
        ) if let DefTInner::InstanceT(instance_t) = def_t.deref()
            && let TypeInner::DefT(_, a_def) = a_t.deref()
            && let DefTInner::InstanceT(a_instance_t) = a_def.deref() =>
        {
            let _instance_c = &instance_t.inst;
            let super_c = &instance_t.super_;
            let _instance_a = &a_instance_t.inst;
            if flow_js_utils::is_same_instance_type(a_instance_t, instance_t) {
                report_unchanged_filtering_result_to_predicate_result(c.dupe(), result_collector);
            } else if let Some(seen) = visit_class(seen, instance_t) {
                // Recursively check whether super(C) extends A, with enough context.
                let repositioned =
                    FlowJs::reposition_reason(cx, env, Some(trace), reason, None, super_c)?;
                let right_clone = InstanceofRhs::InternalExtendsOperand {
                    reason: extends_reason.dupe(),
                    c: c.dupe(),
                    a: a_t.dupe(),
                    seen,
                };
                concretize_and_run_predicate(
                    cx,
                    env,
                    trace,
                    &repositioned,
                    PredicateConcretetizerVariant::ConcretizeForGeneralPredicateTest,
                    result_collector,
                    &|cx, trace, rc, l| instanceof_test(cx, env, trace, rc, true, l, &right_clone),
                )?;
            } else {
                // C's chain loops back on itself, so following it will never reach the root.
                // End the walk where a real prototype chain would have ended.
                let obj_proto = Type::new(TypeInner::ObjProtoT(reason.dupe()));
                instanceof_test(cx, env, trace, result_collector, true, &obj_proto, right)?;
            }
        }
        // An intersection is an instance of A when any of its members extends A. Like an
        // intersection lower bound in subtyping, each member is checked on its own.
        (sense, TypeInner::IntersectionT(_, rep), InstanceofRhs::TypeOperand(right_t))
            if let TypeInner::DefT(r, right_def) = right_t.deref()
                && let DefTInner::ClassT(a) = right_def.deref() =>
        {
            let mut member_extends_a = false;
            for member in rep.members_iter() {
                let extends_use: UseT<Cx> =
                    UseT::new(UseTInner::ExtendsUseT(Box::new(ExtendsUseTData {
                        use_op: unknown_use(),
                        reason: r.dupe(),
                        targs: Vec::new().into(),
                        true_t: member.dupe(),
                        false_t: a.dupe(),
                    })));
                match speculation_kit::try_singleton_throw_on_failure(
                    cx,
                    env,
                    trace,
                    member.dupe(),
                    extends_use,
                ) {
                    Ok(()) => {
                        member_extends_a = true;
                        break;
                    }
                    Err(FlowJsException::SpeculationSingletonError) => {}
                    Err(other) => return Err(other),
                }
            }
            if member_extends_a {
                if sense {
                    report_unchanged_filtering_result_to_predicate_result(
                        left.dupe(),
                        result_collector,
                    );
                } else {
                    report_changes_to_input(result_collector);
                }
            } else {
                let root = if sense {
                    Type::new(TypeInner::DefT(r.dupe(), DefT::new(DefTInner::NullT)))
                } else {
                    Type::new(TypeInner::ObjProtoT(r.dupe()))
                };
                let right = InstanceofRhs::InternalExtendsOperand {
                    reason: r.dupe(),
                    c: left.dupe(),
                    a: a.dupe(),
                    seen: SeenClasses::new(),
                };
                instanceof_test(cx, env, trace, result_collector, sense, &root, &right)?;
            }
        }
        (
            sense,
            TypeInner::IntersectionT(_, _),
            right @ InstanceofRhs::InternalExtendsOperand { reason, c, a, .. },
        ) => {
            let extends_use: UseT<Cx> =
                UseT::new(UseTInner::ExtendsUseT(Box::new(ExtendsUseTData {
                    use_op: unknown_use(),
                    reason: reason.dupe(),
                    targs: Vec::new().into(),
                    true_t: c.dupe(),
                    false_t: a.dupe(),
                })));
            let c_extends_a = match speculation_kit::try_singleton_throw_on_failure(
                cx,
                env,
                trace,
                left.dupe(),
                extends_use,
            ) {
                Ok(()) => true,
                Err(FlowJsException::SpeculationSingletonError) => false,
                Err(other) => return Err(other),
            };
            if c_extends_a {
                if sense {
                    report_unchanged_filtering_result_to_predicate_result(
                        c.dupe(),
                        result_collector,
                    );
                } else {
                    report_changes_to_input(result_collector);
                }
            } else {
                let root = if sense {
                    Type::new(TypeInner::DefT(reason.dupe(), DefT::new(DefTInner::NullT)))
                } else {
                    Type::new(TypeInner::ObjProtoT(reason.dupe()))
                };
                instanceof_test(cx, env, trace, result_collector, sense, &root, right)?;
            }
        }
        // If we are checking `instanceof Object` or `instanceof Function`, objects
        // with `ObjProtoT` or `FunProtoT` should pass.
        (
            true,
            TypeInner::ObjProtoT(reason),
            right @ InstanceofRhs::InternalExtendsOperand { .. },
        ) => {
            let obj_proto = FlowJs::get_builtin_type_with_env(
                cx,
                env,
                Some(trace),
                reason,
                Some(true),
                "Object",
            )?;
            let right_clone = right.clone();
            concretize_and_run_predicate(
                cx,
                env,
                trace,
                &obj_proto,
                PredicateConcretetizerVariant::ConcretizeForGeneralPredicateTest,
                result_collector,
                &|cx, trace, rc, l| instanceof_test(cx, env, trace, rc, true, l, &right_clone),
            )?;
        }
        (
            true,
            TypeInner::FunProtoT(reason),
            right @ InstanceofRhs::InternalExtendsOperand { .. },
        ) => {
            let fun_proto = FlowJs::get_builtin_type_with_env(
                cx,
                env,
                Some(trace),
                reason,
                Some(true),
                "CallableFunction",
            )?;
            let right_clone = right.clone();
            concretize_and_run_predicate(
                cx,
                env,
                trace,
                &fun_proto,
                PredicateConcretetizerVariant::ConcretizeForGeneralPredicateTest,
                result_collector,
                &|cx, trace, rc, l| instanceof_test(cx, env, trace, rc, true, l, &right_clone),
            )?;
        }
        // We hit the root class, so C is not a subclass of A
        (
            true,
            TypeInner::DefT(_, def_t),
            InstanceofRhs::InternalExtendsOperand {
                reason: r, c, a, ..
            },
        ) if let DefTInner::NullT = def_t.deref() => {
            // C is not a subclass of A. If A is a subclass of C it is a legitimate
            // downcast (keep A); otherwise A and C are unrelated, so from a union we
            // prune the member (else keep the guard to avoid a bare `empty`).
            let a_subclass_of_c =
                FlowJs::speculative_subtyping_succeeds_with_flow_errors(cx, env, a, c)?;
            let repositioned = flow_js::reposition_with_env(cx, env, r.loc().dupe(), a.dupe())?;
            if result_collector.from_union.get() && !a_subclass_of_c {
                // Prune this member, but stash the guard in case every member prunes.
                result_collector.disjoint_fallback.add(repositioned);
                report_changes_to_input(result_collector);
            } else {
                report_changed_filtering_result_to_predicate_result(repositioned, result_collector);
            }
        }
        // If we're refining `mixed` or `any` with instanceof A, then flow A to the result
        (true, TypeInner::DefT(_, def_t), InstanceofRhs::TypeOperand(right_t))
            if let DefTInner::MixedT(..) = def_t.deref()
                && let TypeInner::DefT(class_reason, right_def) = right_t.deref()
                && let DefTInner::ClassT(a) = right_def.deref() =>
        {
            let desc = reason_of_t(a).desc(true);
            let loc = class_reason.loc().dupe();
            let repositioned =
                FlowJs::reposition_with_env(cx, env, Some(trace), loc, Some(desc), None, a.dupe())?;
            report_changed_filtering_result_to_predicate_result(repositioned, result_collector);
        }
        (
            true,
            TypeInner::AnyT(..)
            | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
                kind: ImplicitInstantiationTvarKind::Placeholder,
                ..
            }),
            InstanceofRhs::TypeOperand(right_t),
        ) if let TypeInner::DefT(class_reason, right_def) = right_t.deref()
            && let DefTInner::ClassT(a) = right_def.deref() =>
        {
            let desc = reason_of_t(a).desc(true);
            let loc = class_reason.loc().dupe();
            let repositioned =
                FlowJs::reposition_with_env(cx, env, Some(trace), loc, Some(desc), None, a.dupe())?;
            report_changed_filtering_result_to_predicate_result(repositioned, result_collector);
        }
        // Prune the type when any other `instanceof` check succeeds (since this is
        // impossible).
        (true, _, _) => {
            report_changes_to_input(result_collector);
        }
        // Like above, now suppose that we have an instance x of class C, and we
        // check whether x is _not_ `instanceof` class A. To decide what the
        // appropriate refinement for x should be, we need to decide whether C
        // extends A, choosing either nothing or C based on the result.
        (false, TypeInner::DefT(_, def_t), InstanceofRhs::TypeOperand(right_t))
            if let DefTInner::InstanceT(..) = def_t.deref()
                && let TypeInner::DefT(r, right_def) = right_t.deref()
                && let DefTInner::ClassT(a) = right_def.deref()
                && let TypeInner::DefT(_, a_inner_def) = a.deref()
                && let DefTInner::InstanceT(..) = a_inner_def.deref() =>
        {
            let c = left;
            let right = InstanceofRhs::InternalExtendsOperand {
                reason: r.dupe(),
                c: c.dupe(),
                a: a.dupe(),
                seen: SeenClasses::new(),
            };
            instanceof_test(cx, env, trace, result_collector, false, c, &right)?;
        }
        // If C is a subclass of A, then do nothing, since this check cannot
        // succeed. Otherwise, don't refine the type of x.
        (
            false,
            TypeInner::DefT(reason, def_t),
            InstanceofRhs::InternalExtendsOperand {
                reason: extends_reason,
                c,
                a: a_t,
                seen,
            },
        ) if let DefTInner::InstanceT(instance_t) = def_t.deref()
            && let TypeInner::DefT(_, a_def) = a_t.deref()
            && let DefTInner::InstanceT(a_instance_t) = a_def.deref() =>
        {
            let _instance_c = &instance_t.inst;
            let super_c = &instance_t.super_;
            let _instance_a = &a_instance_t.inst;
            if flow_js_utils::is_same_instance_type(a_instance_t, instance_t) {
                report_changes_to_input(result_collector);
            } else if let Some(seen) = visit_class(seen, instance_t) {
                let repositioned =
                    FlowJs::reposition_reason(cx, env, Some(trace), reason, None, super_c)?;
                let right_clone = InstanceofRhs::InternalExtendsOperand {
                    reason: extends_reason.dupe(),
                    c: c.dupe(),
                    a: a_t.dupe(),
                    seen,
                };
                concretize_and_run_predicate(
                    cx,
                    env,
                    trace,
                    &repositioned,
                    PredicateConcretetizerVariant::ConcretizeForGeneralPredicateTest,
                    result_collector,
                    &|cx, trace, rc, l| instanceof_test(cx, env, trace, rc, false, l, &right_clone),
                )?;
            } else {
                // C's chain loops back on itself, so following it will never reach the root.
                // End the walk where a real prototype chain would have ended.
                let obj_proto = Type::new(TypeInner::ObjProtoT(reason.dupe()));
                instanceof_test(cx, env, trace, result_collector, false, &obj_proto, right)?;
            }
        }
        (
            false,
            TypeInner::ObjProtoT(_),
            InstanceofRhs::InternalExtendsOperand { reason: r, c, .. },
        ) => {
            // We hit the root class, so C is not a subclass of A.
            // In this case, we will refine the input to C
            let repositioned = flow_js::reposition_with_env(cx, env, r.loc().dupe(), c.dupe())?;
            report_changed_filtering_result_to_predicate_result(repositioned, result_collector);
        }
        (false, _, _) => {
            report_unchanged_filtering_result_to_predicate_result(left.dupe(), result_collector);
        }
    }
    Ok(())
}

fn sentinel_prop_test<'cx>(
    key: &str,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    result_collector: &PredicateResultCollector,
    sense: bool,
    obj: &Type,
    t: &Type,
) -> Result<(), FlowJsException> {
    sentinel_prop_test_generic(key, cx, env, result_collector, obj, sense, obj, t)
}

fn sentinel_prop_test_generic<'cx>(
    key: &str,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    result_collector: &PredicateResultCollector,
    orig_obj: &Type,
    sense: bool,
    obj: &Type,
    t: &Type,
) -> Result<(), FlowJsException> {
    // Evaluate a refinement predicate of the form
    //
    // obj.key eq value
    //
    // where eq is === or !==.
    //
    // * key is key
    // * (sense, obj, value) are the sense of the test, obj and value as above,
    // respectively.
    //
    // As with other predicate filters, the goal is to statically determine when
    // the predicate is definitely satisfied and when it is definitely
    // // unsatisfied, and narrow the possible types of obj under those conditions,
    // while not narrowing in all other cases.
    //
    // In this case, the predicate is definitely satisfied (respectively,
    // definitely unsatisfied) when the type of the key property in the type obj
    // can be statically verified as having (respectively, not having) value as
    // its only inhabitant.
    //
    // When satisfied, type obj flows to the recipient type result (in other
    // words, we allow all such types in the refined type for obj).
    //
    // Otherwise, nothing flows to type result (in other words, we don't allow
    // any such type in the refined type for obj).
    //
    // Overall the filtering process is somewhat tricky to understand. Refer to
    // the predicate function and its callers to understand how the context is
    // set up so that filtering ultimately only depends on what flows to
    // result.
    let flow_sentinel_obj = |cx: &mut Context<'cx>,
                             sense: bool,
                             props_tmap: properties::Id,
                             obj: &Type,
                             sentinel: &UnionEnumStar|
     -> Result<(), FlowJsException> {
        match cx.get_prop(props_tmap, &Name::new(key)) {
            Some(p) => match flow_typing_type::type_::property::read_t(&p) {
                Some(t_prop) => {
                    let reason = reason_of_t(orig_obj).dupe().replace_desc(None);
                    concretize_and_run_sentinel_prop_test(
                        cx,
                        env,
                        &reason,
                        orig_obj,
                        sense,
                        sentinel,
                        &t_prop,
                        result_collector,
                    )?;
                }
                None => {
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EPropNotReadable(Box::new(EPropNotReadableData {
                            prop_loc: reason_of_t(obj).loc().dupe(),
                            prop_name: Some(Name::new(key)),
                            use_op: unknown_use(),
                        })),
                    )?;
                }
            },
            None => {
                // TODO: possibly unsound to filter out orig_obj here, but if we
                // don't, case elimination based on sentinel prop checking doesn't
                // work for (disjoint unions of) intersections of objects, where the
                // sentinel prop and the payload appear in different branches of the
                // intersection. It is easy to avoid this unsoundness with slightly
                // more work, but will wait until a refactoring of property lookup
                // lands to revisit. Tracked by #11301092.
                if orig_obj == obj {
                    report_unchanged_filtering_result_to_predicate_result(
                        orig_obj.dupe(),
                        result_collector,
                    );
                }
            }
        }
        Ok(())
    };

    let flow_sentinel_tuple = |cx: &mut Context<'cx>,
                               sense: bool,
                               elements: &[TupleElement],
                               tuple: &Type,
                               sentinel: &UnionEnumStar|
     -> Result<(), FlowJsException> {
        let i: usize = key.parse().unwrap();
        match elements.get(i) {
            Some(TupleElement {
                t: elem_t,
                polarity,
                name,
                ..
            }) => {
                if Polarity::compat(*polarity, Polarity::Positive) {
                    let reason = reason_of_t(orig_obj).dupe().replace_desc(None);
                    concretize_and_run_sentinel_prop_test(
                        cx,
                        env,
                        &reason,
                        orig_obj,
                        sense,
                        sentinel,
                        elem_t,
                        result_collector,
                    )?;
                } else {
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::ETupleElementNotReadable(Box::new(
                            ETupleElementNotReadableData {
                                use_op: unknown_use(),
                                loc: reason_of_t(tuple).loc().dupe(),
                                index_def_loc: reason_of_t(tuple).def_loc().dupe(),
                                index: i as i32,
                                name: name.as_ref().map(|n| n.dupe()),
                            },
                        )),
                    )?;
                }
            }
            None => {
                if orig_obj == tuple {
                    report_unchanged_filtering_result_to_predicate_result(
                        orig_obj.dupe(),
                        result_collector,
                    );
                }
            }
        }
        Ok(())
    };

    fn sentinel_of_literal(t: &Type) -> Option<UnionEnumStar> {
        match t.deref() {
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                DefTInner::SingletonStrT { value, .. } => {
                    Some(UnionEnumStar::One(UnionEnum::Str(value.dupe())))
                }
                DefTInner::SingletonNumT { value, .. } => {
                    Some(UnionEnumStar::One(UnionEnum::Num(value.clone())))
                }
                DefTInner::SingletonBoolT { value, .. } => {
                    Some(UnionEnumStar::One(UnionEnum::Bool(*value)))
                }
                DefTInner::SingletonBigIntT { value, .. } => {
                    Some(UnionEnumStar::One(UnionEnum::BigInt(value.clone())))
                }
                DefTInner::VoidT => Some(UnionEnumStar::One(UnionEnum::Void)),
                DefTInner::NullT => Some(UnionEnumStar::One(UnionEnum::Null)),
                _ => None,
            },
            TypeInner::UnionT(_, rep) => rep.check_enum().map(UnionEnumStar::Many),
            _ => None,
        }
    }

    match sentinel_of_literal(t) {
        Some(s) => {
            match obj.deref() {
                TypeInner::DefT(_, def_t) => {
                    match def_t.deref() {
                        // obj.key ===/!== literal value
                        DefTInner::ObjT(obj_t) => {
                            flow_sentinel_obj(cx, sense, obj_t.props_tmap.dupe(), obj, &s)?;
                        }
                        // instance.key ===/!== literal value
                        DefTInner::InstanceT(instance_t) => {
                            flow_sentinel_obj(
                                cx,
                                sense,
                                instance_t.inst.own_props.dupe(),
                                obj,
                                &s,
                            )?;
                        }
                        // tuple.length ===/!== literal value
                        DefTInner::ArrT(arr_t)
                            if let ArrType::TupleAT(box TupleATData {
                                arity,
                                inexact,
                                rest,
                                ..
                            }) = arr_t.deref()
                                && key == "length" =>
                        {
                            let reason = reason_of_t(obj).dupe();
                            let input = type_util::tuple_length(
                                reason.dupe(),
                                *inexact || rest.is_some(),
                                arity.0,
                                arity.1,
                            );
                            concretize_and_run_sentinel_prop_test(
                                cx,
                                env,
                                &reason,
                                orig_obj,
                                sense,
                                &s,
                                &input,
                                result_collector,
                            )?;
                        }
                        DefTInner::ArrT(arr_t)
                            if let ArrType::TupleAT(box TupleATData { elements, .. }) =
                                arr_t.deref()
                                && flow_js_utils::is_str_intlike(key) =>
                        {
                            flow_sentinel_tuple(cx, sense, elements, obj, &s)?;
                        }
                        _ => {
                            report_unchanged_filtering_result_to_predicate_result(
                                orig_obj.dupe(),
                                result_collector,
                            );
                        }
                    }
                }
                TypeInner::IntersectionT(inter_reason, rep) => {
                    for member_obj in rep.members_iter() {
                        let ts = FlowJs::possible_concrete_types_for_inspection_with_env(
                            cx,
                            env,
                            inter_reason,
                            member_obj,
                        )?;
                        for l in ts.iter() {
                            sentinel_prop_test_generic(
                                key,
                                cx,
                                env,
                                result_collector,
                                orig_obj,
                                sense,
                                l,
                                t,
                            )?;
                        }
                    }
                }
                // not enough info to refine
                _ => {
                    report_unchanged_filtering_result_to_predicate_result(
                        orig_obj.dupe(),
                        result_collector,
                    );
                }
            }
        }
        // not enough info to refine
        None => {
            report_unchanged_filtering_result_to_predicate_result(
                orig_obj.dupe(),
                result_collector,
            );
        }
    }
    Ok(())
}

fn concretize_and_run_sentinel_prop_test<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    orig_obj: &Type,
    sense: bool,
    sentinel: &UnionEnumStar,
    input: &Type,
    result_collector: &PredicateResultCollector,
) -> Result<(), FlowJsException> {
    fn enum_to_def(e: &UnionEnum) -> DefTInner {
        match e {
            UnionEnum::Str(v) => DefTInner::SingletonStrT {
                from_annot: true,
                value: v.dupe(),
            },
            UnionEnum::Num(v) => DefTInner::SingletonNumT {
                from_annot: true,
                value: v.clone(),
            },
            UnionEnum::Bool(v) => DefTInner::SingletonBoolT {
                from_annot: true,
                value: *v,
            },
            UnionEnum::BigInt(v) => DefTInner::SingletonBigIntT {
                from_annot: true,
                value: v.clone(),
            },
            UnionEnum::Void => DefTInner::VoidT,
            UnionEnum::Null => DefTInner::NullT,
        }
    }

    let ts = FlowJs::possible_concrete_types_for_sentinel_prop_test(cx, env, reason, input)?;
    for t in ts.iter() {
        match t.deref() {
            TypeInner::UnionT(r, rep) => {
                let l = orig_obj;
                // we have the check l.key === sentinel where l.key is a union
                if sense {
                    match sentinel {
                        UnionEnumStar::One(one_enum) => {
                            let def = enum_to_def(one_enum);
                            match union_rep::quick_mem_enum(
                                cx,
                                |_, t1: &Type, t2: &Type| {
                                    type_util::quick_subtype(None::<&mut fn(&Type)>, t1, t2)
                                },
                                &Type::new(TypeInner::DefT(r.dupe(), DefT::new(def))),
                                rep,
                            ) {
                                // provably unreachable, so prune
                                QuickMemResult::No => {
                                    report_changes_to_input(result_collector);
                                }
                                QuickMemResult::Yes => {
                                    report_unchanged_filtering_result_to_predicate_result(
                                        l.dupe(),
                                        result_collector,
                                    );
                                }
                                // inconclusive: the union is not concretized
                                QuickMemResult::Conditional(_) | QuickMemResult::Unknown => {
                                    for member in rep.members_iter() {
                                        concretize_and_run_sentinel_prop_test(
                                            cx,
                                            env,
                                            reason,
                                            orig_obj,
                                            sense,
                                            sentinel,
                                            member,
                                            result_collector,
                                        )?;
                                    }
                                }
                            }
                        }
                        UnionEnumStar::Many(enums) => {
                            let acc = enums.iter().fold(QuickMemResult::No, |acc, one_enum| {
                                let def = enum_to_def(one_enum);
                                let check_t = Type::new(TypeInner::DefT(r.dupe(), DefT::new(def)));
                                let qm_result = union_rep::quick_mem_enum(
                                    cx,
                                    |_, t1: &Type, t2: &Type| {
                                        type_util::quick_subtype(None::<&mut fn(&Type)>, t1, t2)
                                    },
                                    &check_t,
                                    rep,
                                );
                                union_rep::join_quick_mem_results((acc, qm_result))
                            });
                            match acc {
                                // provably unreachable, so prune
                                QuickMemResult::No => {
                                    report_changes_to_input(result_collector);
                                }
                                QuickMemResult::Yes => {
                                    report_unchanged_filtering_result_to_predicate_result(
                                        l.dupe(),
                                        result_collector,
                                    );
                                }
                                // inconclusive: the union is not concretized
                                QuickMemResult::Conditional(_) | QuickMemResult::Unknown => {
                                    for member in rep.members_iter() {
                                        concretize_and_run_sentinel_prop_test(
                                            cx,
                                            env,
                                            reason,
                                            orig_obj,
                                            sense,
                                            sentinel,
                                            member,
                                            result_collector,
                                        )?;
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // for l.key !== sentinel where l.key is a union, we can't really prove
                    // that the check is guaranteed to fail (assuming the union doesn't
                    // degenerate to a singleton)
                    report_unchanged_filtering_result_to_predicate_result(
                        l.dupe(),
                        result_collector,
                    );
                }
            }
            _ => {
                let filter_result = type_filter::sentinel_refinement(
                    t,
                    reason.dupe(),
                    orig_obj.dupe(),
                    sense,
                    sentinel,
                );
                report_filtering_result_to_predicate_result(filter_result, result_collector);
            }
        }
    }
    Ok(())
}

fn eq_test<'cx>(
    cx: &mut Context<'cx>,
    result_collector: &PredicateResultCollector,
    sense: bool,
    left: &Type,
    right: &Type,
) {
    let expected_loc = loc_of_t(right);
    match right.deref() {
        TypeInner::DefT(_, def_t) => match def_t.deref() {
            DefTInner::SingletonStrT { value, .. } => {
                let filtered = if sense {
                    type_filter::string_literal(expected_loc.dupe(), value.dupe(), left.dupe())
                } else {
                    type_filter::not_string_literal(value.dupe(), left.dupe())
                };
                report_filtering_result_to_predicate_result(filtered, result_collector);
            }
            DefTInner::SingletonNumT { value, .. } => {
                let filtered = if sense {
                    type_filter::number_literal(expected_loc.dupe(), value.clone(), left.dupe())
                } else {
                    type_filter::not_number_literal(value.clone(), left.dupe())
                };
                report_filtering_result_to_predicate_result(filtered, result_collector);
            }
            DefTInner::SingletonBoolT { value: true, .. } => {
                let filtered = if sense {
                    type_filter::true_(left.dupe())
                } else {
                    type_filter::not_true(left.dupe())
                };
                report_filtering_result_to_predicate_result(filtered, result_collector);
            }
            DefTInner::SingletonBoolT { value: false, .. } => {
                let filtered = if sense {
                    type_filter::false_(left.dupe())
                } else {
                    type_filter::not_false(left.dupe())
                };
                report_filtering_result_to_predicate_result(filtered, result_collector);
            }
            DefTInner::SingletonBigIntT { value, .. } => {
                let filtered = if sense {
                    type_filter::bigint_literal(expected_loc.dupe(), value.clone(), left.dupe())
                } else {
                    type_filter::not_bigint_literal(value.clone(), left.dupe())
                };
                report_filtering_result_to_predicate_result(filtered, result_collector);
            }
            DefTInner::VoidT => {
                let filtered = if sense {
                    type_filter::undefined(left.dupe())
                } else {
                    type_filter::not_undefined(cx, left.dupe())
                };
                report_filtering_result_to_predicate_result(filtered, result_collector);
            }
            DefTInner::NullT => {
                let filtered = if sense {
                    type_filter::null_filter(left.dupe())
                } else {
                    type_filter::not_null(cx, left.dupe())
                };
                report_filtering_result_to_predicate_result(filtered, result_collector);
            }
            _ => {
                report_unchanged_filtering_result_to_predicate_result(
                    left.dupe(),
                    result_collector,
                );
            }
        },
        _ => {
            report_unchanged_filtering_result_to_predicate_result(left.dupe(), result_collector);
        }
    }
}

// guards
fn concretize_and_guard_prop<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    source: &Type,
    pred: &PropGuard,
    orig_obj: &Type,
    result_collector: &PredicateResultCollector,
) -> Result<(), FlowJsException> {
    let ts = FlowJs::possible_concrete_types_for_operators_checking_with_env(
        cx,
        env,
        reason_of_t(source),
        source,
    )?;
    let all_changed = ts.iter().all(|t| guard_prop(cx, t, pred));
    if all_changed {
        report_changes_to_input(result_collector);
    } else {
        report_unchanged_filtering_result_to_predicate_result(orig_obj.dupe(), result_collector);
    }
    Ok(())
}

fn guard_prop<'cx>(cx: &mut Context<'cx>, source: &Type, pred: &PropGuard) -> bool {
    fn is_empty_and_changed(result: FilterResult) -> bool {
        match result.type_.deref() {
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                DefTInner::EmptyT => result.changed,
                _ => false,
            },
            _ => false,
        }
    }

    match pred {
        PropGuard::PropGuardTruthy => is_empty_and_changed(type_filter::truthy(cx, source.dupe())),
        PropGuard::PropGuardNotTruthy => {
            is_empty_and_changed(type_filter::not_truthy(cx, source.dupe()))
        }
        PropGuard::PropGuardMaybe => is_empty_and_changed(type_filter::maybe(cx, source.dupe())),
        PropGuard::PropGuardNotMaybe => {
            is_empty_and_changed(type_filter::not_maybe(cx, source.dupe()))
        }
        PropGuard::PropGuardNull => is_empty_and_changed(type_filter::null_filter(source.dupe())),
        PropGuard::PropGuardNotNull => {
            is_empty_and_changed(type_filter::not_null(cx, source.dupe()))
        }
        PropGuard::PropGuardVoid => is_empty_and_changed(type_filter::undefined(source.dupe())),
        PropGuard::PropGuardNotVoid => {
            is_empty_and_changed(type_filter::not_undefined(cx, source.dupe()))
        }
    }
}

fn is_literal_value(t: &Type) -> bool {
    match t.deref() {
        TypeInner::DefT(_, def_t) => matches!(
            def_t.deref(),
            DefTInner::SingletonStrT { .. }
                | DefTInner::SingletonNumT { .. }
                | DefTInner::SingletonBoolT { .. }
                | DefTInner::SingletonBigIntT { .. }
                | DefTInner::VoidT
                | DefTInner::NullT
        ),
        _ => false,
    }
}

pub enum PredicateResult {
    TypeChanged(Type),
    TypeUnchanged(Type),
}

pub(crate) fn run_predicate_track_changes<'cx>(
    cx: &mut Context<'cx>,
    t: &Type,
    p: &Predicate,
    result_reason: Reason,
) -> Result<PredicateResult, JobError> {
    run_predicate_track_changes_with_env(cx, &FlowJsEnv::entry(), t, p, result_reason)
}

fn run_predicate_track_changes_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    t: &Type,
    p: &Predicate,
    result_reason: Reason,
) -> Result<PredicateResult, JobError> {
    let collector = TypeCollector::create();
    let changed = Rc::new(RefCell::new(false));
    let result_collector = PredicateResultCollector {
        collector,
        changed: changed.dupe(),
        from_union: Cell::new(false),
        disjoint_fallback: TypeCollector::create(),
        refined_generic: None,
    };
    let p_clone = p.dupe();
    let rhs_concretizations = RhsConcretizations::default();
    flow_js_utils::flow_js_result_to_job_error(concretize_and_run_predicate(
        cx,
        env,
        DepthTrace::unit_trace(),
        t,
        concretization_variant_of_predicate(p),
        &result_collector,
        &|cx, trace, rc, l| {
            predicate_no_concretization(cx, env, trace, rc, l, &p_clone, &rhs_concretizations)
        },
    ))?;
    let collected: Vec<Type> = result_collector
        .collector
        .collect()
        .into_iter()
        .map(|t| {
            flow_typing_flow_js::tvar_resolver::resolved_t(
                flow_typing_flow_js::tvar_resolver::default_no_lowers,
                true,
                cx,
                t,
            )
        })
        .collect();
    let result_t = type_util::union_of_ts(result_reason, collected, None);
    if *changed.borrow() {
        Ok(PredicateResult::TypeChanged(result_t))
    } else {
        Ok(PredicateResult::TypeUnchanged(result_t))
    }
}

/// Collects the types that survive predicate filtering.
pub(crate) fn collect_predicate_for_filtering<'cx>(
    cx: &mut Context<'cx>,
    t: &Type,
    p: &Predicate,
) -> Result<TypeCollector, JobError> {
    collect_predicate_for_filtering_with_env(cx, &FlowJsEnv::entry(), t, p)
}

pub(super) fn run_predicate_for_filtering_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    t: &Type,
    p: &Predicate,
    tout: &UseT<Cx>,
) -> Result<(), JobError> {
    let collector = collect_predicate_for_filtering_with_env(cx, env, t, p)?;
    for t in collector.collect_to_vec().iter() {
        flow_js_utils::flow_js_result_to_job_error(FlowJs::flow_with_env(cx, env, t, tout))?;
    }
    Ok(())
}

/// Collects the types that survive predicate filtering in the given environment.
pub(super) fn collect_predicate_for_filtering_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    t: &Type,
    p: &Predicate,
) -> Result<TypeCollector, JobError> {
    let collector = TypeCollector::create();
    let changed = Rc::new(RefCell::new(false));
    let result_collector = PredicateResultCollector {
        collector,
        changed: changed.dupe(),
        from_union: Cell::new(false),
        disjoint_fallback: TypeCollector::create(),
        refined_generic: None,
    };
    let p_clone = p.dupe();
    let rhs_concretizations = RhsConcretizations::default();
    flow_js_utils::flow_js_result_to_job_error(concretize_and_run_predicate(
        cx,
        env,
        DepthTrace::unit_trace(),
        t,
        concretization_variant_of_predicate(p),
        &result_collector,
        &|cx, trace, rc, l| {
            predicate_no_concretization(cx, env, trace, rc, l, &p_clone, &rhs_concretizations)
        },
    ))?;
    Ok(result_collector.collector)
}
