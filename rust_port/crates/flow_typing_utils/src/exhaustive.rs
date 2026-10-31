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
use std::sync::Arc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_aloc::ALoc;
use flow_aloc::ALocId;
use flow_aloc::ALocSet;
use flow_common::reason::Reason;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_data_structure_wrapper::vector::FlowVector;
use flow_parser::ast;
use flow_parser::loc::Loc;
use flow_typing_context::Context;
use flow_typing_context::Cx;
use flow_typing_errors::error_message::EnumErrorKind;
use flow_typing_errors::error_message::EnumInvalidCheckData;
use flow_typing_errors::error_message::EnumReferenceData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::ErrorTypeReferenceWithLocData;
use flow_typing_errors::error_message::MatchErrorKind;
use flow_typing_errors::error_message::MatchExampleReference;
use flow_typing_errors::error_message::MatchInvalidIdentOrMemberPatternData;
use flow_typing_errors::error_message::MatchNonExhaustiveObjectPatternData;
use flow_typing_errors::error_message::MatchNonExplicitEnumCheckData;
use flow_typing_errors::error_message::MatchNotExhaustiveData;
use flow_typing_errors::error_message::MatchUnusedPatternData;
use flow_typing_errors::intermediate_error_types::MatchPatternReferenceData;
use flow_typing_errors::intermediate_error_types::MatchPatternReferenceKind;
use flow_typing_flow_common::concrete_type_eq;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_js::flow_js;
use flow_typing_flow_js::flow_js::FlowJs;
use flow_typing_loc_env::match_pattern_ir;
use flow_typing_loc_env::match_pattern_ir::LeafSet;
use flow_typing_loc_env::match_pattern_ir::ObjKind;
use flow_typing_loc_env::match_pattern_ir::leaf;
use flow_typing_loc_env::match_pattern_ir::pattern_object;
use flow_typing_loc_env::match_pattern_ir::pattern_union;
use flow_typing_loc_env::match_pattern_ir::value_object;
use flow_typing_loc_env::match_pattern_ir::value_union;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_util;
use flow_typing_type::type_util::reason_of_t;
use flow_typing_visitors::type_mapper;
use flow_utils_concurrency::job_error::JobError;

type SMap<V> = FlowOrdMap<FlowSmolStr, V>;
type IMap<V> = FlowOrdMap<usize, V>;

// ****************************************
// * Helper functions for analyzing types *
// ****************************************

fn singleton_concrete_type<'cx>(cx: &mut Context<'cx>, t: &Type) -> Result<Type, JobError> {
    flow_js_utils::flow_js_result_to_job_error(FlowJs::singleton_concrete_type_for_match_arg(
        cx,
        false,
        reason_of_t(t),
        t,
    ))
}

fn possible_concrete_types<'cx>(
    cx: &mut Context<'cx>,
    keep_unions: bool,
    t: &Type,
) -> Result<Vec<Type>, JobError> {
    flow_js_utils::flow_js_result_to_job_error(FlowJs::possible_concrete_types_for_match_arg(
        cx,
        keep_unions,
        reason_of_t(t),
        t,
    ))
}

fn attempt_union_rep_optimization<'cx>(
    cx: &mut Context<'cx>,
    rep: &flow_typing_type::type_::union_rep::UnionRep,
) {
    if !rep.is_optimized_finally() {
        rep.optimize(
            cx,
            |cx, t1, t2| concrete_type_eq::eq(cx, t1, t2),
            |cx, ts: &mut dyn Iterator<Item = &Type>| type_mapper::union_flatten(cx, ts.duped()),
            |cx, t| cx.find_resolved(t),
            |cx, id| cx.find_props(id),
        );
    }
}

// Walk the `super` chain of an instance, collecting the ids of all super
// classes. Does not include `class_id` itself. Stops on recursive extends.
fn get_super_class_ids<'cx>(
    cx: &mut Context<'cx>,
    class_id: &ALocId,
    super_: &Type,
) -> Result<FlowOrdSet<ALocId>, JobError> {
    fn loop_<'cx>(
        cx: &mut Context<'cx>,
        class_id: &ALocId,
        mut acc: FlowOrdSet<ALocId>,
        t: &Type,
    ) -> Result<FlowOrdSet<ALocId>, JobError> {
        Ok(match singleton_concrete_type(cx, t)?.deref() {
            TypeInner::DefT(_, d) => match d.deref() {
                DefTInner::InstanceT(inst_rc) => {
                    let inst = inst_rc.deref();
                    let inst_data = &inst.inst;
                    match &inst_data.inst_kind {
                        flow_typing_type::type_::InstanceKind::ClassKind
                        | flow_typing_type::type_::InstanceKind::RecordKind { .. } => {
                            let super_class_id = &inst_data.class_id;
                            let super_ = &inst.super_;
                            if super_class_id == class_id || acc.contains(super_class_id) {
                                // Recursive extends: stop looping
                                acc
                            } else {
                                acc.insert(super_class_id.dupe());
                                loop_(cx, class_id, acc, super_)?
                            }
                        }
                        _ => acc,
                    }
                }
                _ => acc,
            },
            _ => acc,
        })
    }
    loop_(cx, class_id, FlowOrdSet::default(), super_)
}

// Like `get_class_info`, but also returns the set of super class ids and the
// instance type of the constructor. Used when building instance patterns, so
// we can later recognize patterns whose class is a strict subclass of the
// value's class.
fn get_class_info_with_supers<'cx>(
    cx: &mut Context<'cx>,
    t: &Type,
) -> Result<Option<(ALocId, Option<FlowSmolStr>, FlowOrdSet<ALocId>, Type)>, JobError> {
    let instance_t = match singleton_concrete_type(cx, t)?.deref() {
        TypeInner::DefT(_, d) => match d.deref() {
            DefTInner::ClassT(class_t) => singleton_concrete_type(cx, class_t)?,
            _ => flow_typing_type::type_::empty_t::why(reason_of_t(t).dupe()),
        },
        _ => flow_typing_type::type_::empty_t::why(reason_of_t(t).dupe()),
    };
    Ok(match instance_t.deref() {
        TypeInner::DefT(_, d) => match d.deref() {
            DefTInner::InstanceT(inst_rc) => {
                let inst = inst_rc.deref();
                let inst_data = &inst.inst;
                let super_ = &inst.super_;
                match &inst_data.inst_kind {
                    flow_typing_type::type_::InstanceKind::ClassKind
                    | flow_typing_type::type_::InstanceKind::RecordKind { .. } => Some((
                        inst_data.class_id.dupe(),
                        inst_data.class_name.dupe(),
                        get_super_class_ids(cx, &inst_data.class_id, super_)?,
                        instance_t.dupe(),
                    )),
                    _ => None,
                }
            }
            _ => None,
        },
        _ => None,
    })
}

pub fn get_class_info<'cx>(
    cx: &mut Context<'cx>,
    t: &Type,
) -> Result<Option<(ALocId, Option<FlowSmolStr>)>, JobError> {
    Ok(get_class_info_with_supers(cx, t)?
        .map(|(class_id, class_name, _, _)| (class_id, class_name)))
}

// Datatype builders

pub mod pattern_union_builder {
    use flow_parser::ast;
    use flow_typing_type::type_;

    use super::*;

    // Builder helpers

    fn not_seen_wildcard<'cx>(
        cx: &mut Context<'cx>,
        raise_errors: bool,
        pattern_union: &pattern_union::PatternUnion,
        reason: &Reason,
        kind: MatchPatternReferenceKind,
    ) -> bool {
        match &pattern_union.wildcard {
            Some(already_seen) => {
                if raise_errors {
                    flow_js::add_output_non_speculating(
                        cx,
                        ErrorMessage::EMatchError(MatchErrorKind::MatchUnusedPattern(Box::new(
                            MatchUnusedPatternData {
                                pattern: MatchPatternReferenceData {
                                    loc: reason.loc().dupe(),
                                    kind,
                                },
                                already_seen: Some(MatchPatternReferenceData {
                                    loc: already_seen.loc().dupe(),
                                    kind: MatchPatternReferenceKind::Wildcard,
                                }),
                            },
                        ))),
                    );
                }
                false
            }
            None => true,
        }
    }

    fn add_leaf<'cx>(
        cx: &mut Context<'cx>,
        raise_errors: bool,
        guarded: bool,
        mut pattern_union: pattern_union::PatternUnion,
        leaf_val: leaf::Leaf,
    ) -> pattern_union::PatternUnion {
        let reason = &leaf_val.0;
        if not_seen_wildcard(
            cx,
            raise_errors,
            &pattern_union,
            reason,
            MatchPatternReferenceKind::Pattern,
        ) {
            if let Some(existing) = pattern_union
                .leafs
                .get_prev(&leaf_val)
                .filter(|e| *e == &leaf_val)
            {
                let already_seen = existing.0.loc().dupe();
                if raise_errors {
                    flow_js::add_output_non_speculating(
                        cx,
                        ErrorMessage::EMatchError(MatchErrorKind::MatchUnusedPattern(Box::new(
                            MatchUnusedPatternData {
                                pattern: MatchPatternReferenceData {
                                    loc: reason.loc().dupe(),
                                    kind: MatchPatternReferenceKind::Pattern,
                                },
                                already_seen: Some(MatchPatternReferenceData {
                                    loc: already_seen,
                                    kind: MatchPatternReferenceKind::Pattern,
                                }),
                            },
                        ))),
                    );
                }
                pattern_union
            } else if guarded {
                pattern_union.guarded_leafs.push(leaf_val);
                pattern_union
            } else {
                pattern_union.leafs.insert(leaf_val);
                pattern_union
            }
        } else {
            pattern_union
        }
    }

    fn add_wildcard<'cx>(
        cx: &mut Context<'cx>,
        raise_errors: bool,
        guarded: bool,
        last: bool,
        mut pattern_union: pattern_union::PatternUnion,
        reason: Reason,
    ) -> pattern_union::PatternUnion {
        if not_seen_wildcard(
            cx,
            raise_errors,
            &pattern_union,
            &reason,
            MatchPatternReferenceKind::Wildcard,
        ) {
            if guarded {
                if last {
                    let loc = reason.loc();
                    // We avoid more complex analysis by simply erroring when there is a
                    // guarded wilcard which is in the last case.
                    if raise_errors {
                        flow_js::add_output_non_speculating(
                            cx,
                            ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidGuardedWildcard(
                                loc.dupe(),
                            )),
                        );
                    }
                }
                pattern_union
            } else {
                pattern_union.wildcard = Some(reason);
                pattern_union
            }
        } else {
            pattern_union
        }
    }

    fn add_tuple<'cx>(
        cx: &mut Context<'cx>,
        raise_errors: bool,
        mut pattern_union: pattern_union::PatternUnion,
        length: usize,
        tuple: pattern_object::WithIndex,
    ) -> pattern_union::PatternUnion {
        let (_, ref po) = tuple;
        let reason = &po.0;
        let inner = po.1.as_ref();
        let kind = &inner.kind;
        let rest = &inner.rest;
        if not_seen_wildcard(
            cx,
            raise_errors,
            &pattern_union,
            reason,
            MatchPatternReferenceKind::Pattern,
        ) {
            if rest.is_some() || *kind == ObjKind::Obj {
                pattern_union
                    .tuples_inexact
                    .entry(length)
                    .or_insert_with(FlowVector::default)
                    .push(tuple);
            } else {
                pattern_union
                    .tuples_exact
                    .entry(length)
                    .or_insert_with(FlowVector::default)
                    .push(tuple);
            }
            pattern_union
        } else {
            pattern_union
        }
    }

    fn add_object<'cx>(
        cx: &mut Context<'cx>,
        raise_errors: bool,
        mut pattern_union: pattern_union::PatternUnion,
        obj: pattern_object::WithIndex,
    ) -> pattern_union::PatternUnion {
        let (_, ref po) = obj;
        let reason = &po.0;
        if not_seen_wildcard(
            cx,
            raise_errors,
            &pattern_union,
            reason,
            MatchPatternReferenceKind::Pattern,
        ) {
            // Accumulate backwards, reverse at end of pattern_union creation.
            pattern_union.objects.push(obj);
            pattern_union
        } else {
            pattern_union
        }
    }

    // Construction from AST

    fn leaf_of_type<'cx>(
        cx: &mut Context<'cx>,
        raise_errors: bool,
        t: &Type,
        pattern_ast: &ast::match_pattern::MatchPattern<ALoc, (ALoc, Type)>,
    ) -> Option<leaf::Leaf> {
        use flow_parser::ast::match_pattern::MatchPattern;
        use flow_parser::ast::match_pattern::member_pattern::Property as MemberProperty;

        let loc = pattern_ast.loc();
        let reason = flow_common::reason::mk_reason(None, loc.dupe());
        match t.deref() {
            TypeInner::DefT(_, d) => match d.deref() {
                DefTInner::EnumValueT(enum_info_rc) => match &***enum_info_rc {
                    type_::EnumInfoInner::ConcreteEnum(enum_info) => {
                        match pattern_ast {
                            MatchPattern::MemberPattern { inner, .. } => match &inner.property {
                                MemberProperty::PropertyIdentifier(id) => {
                                    let member_name = id.name.dupe();
                                    return Some(leaf::Leaf(
                                        reason,
                                        leaf::LeafCtor::EnumMemberC(leaf::EnumMember {
                                            member_name,
                                            enum_info: std::rc::Rc::new(enum_info.dupe()),
                                        }),
                                    ));
                                }
                                _ => {}
                            },
                            _ => {}
                        }
                        let example_member = enum_info.members.iter().next().map(|(k, _)| k.dupe());
                        if raise_errors {
                            flow_js::add_output_non_speculating(
                                cx,
                                ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidCheck(
                                    Box::new(EnumInvalidCheckData {
                                        loc: loc.dupe(),
                                        enum_: EnumReferenceData {
                                            loc: type_util::ref_loc_of_t(t).dupe(),
                                            name: Some(enum_info.enum_name.dupe()),
                                        },
                                        example_member,
                                        from_match: true,
                                    }),
                                )),
                            );
                        }
                        return None;
                    }
                    _ => {}
                },
                DefTInner::SingletonBoolT { value, .. } => {
                    return Some(leaf::Leaf(reason, leaf::LeafCtor::BoolC(*value)));
                }
                DefTInner::SingletonNumT { value, .. } => {
                    return Some(leaf::Leaf(reason, leaf::LeafCtor::NumC(value.dupe())));
                }
                DefTInner::SingletonBigIntT { value, .. } => {
                    return Some(leaf::Leaf(reason, leaf::LeafCtor::BigIntC(value.dupe())));
                }
                DefTInner::SingletonStrT { value, .. } => {
                    return Some(leaf::Leaf(reason, leaf::LeafCtor::StrC(value.dupe())));
                }
                DefTInner::NullT => return Some(leaf::Leaf(reason, leaf::LeafCtor::NullC)),
                DefTInner::VoidT => return Some(leaf::Leaf(reason, leaf::LeafCtor::VoidC)),
                _ => {}
            },
            // Ignore any-typed patterns.
            TypeInner::AnyT(_, _) => return None,
            _ => {}
        }
        if raise_errors {
            flow_js::add_output_non_speculating(
                cx,
                ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidIdentOrMemberPattern(
                    Box::new(MatchInvalidIdentOrMemberPatternData {
                        loc: loc.dupe(),
                        type_loc: type_util::ref_loc_of_t(t).dupe(),
                        type_desc: flow_js_utils::type_or_type_desc_for_error(t),
                    }),
                )),
            );
        }
        None
    }

    fn of_pattern_ast<'cx>(
        cx: &mut Context<'cx>,
        is_global_var: &dyn Fn(&Context<'cx>, ALoc) -> bool,
        raise_errors: bool,
        pattern_ast: &ast::match_pattern::MatchPattern<ALoc, (ALoc, Type)>,
    ) -> Result<pattern_union::PatternUnion, JobError> {
        let (pattern_union, _) = of_pattern_ast_inner(
            cx,
            is_global_var,
            raise_errors,
            (pattern_union::empty(), 0),
            false,
            false,
            pattern_ast,
        )?;
        Ok(pattern_union)
    }

    fn of_pattern_ast_inner<'cx>(
        cx: &mut Context<'cx>,
        is_global_var: &dyn Fn(&Context<'cx>, ALoc) -> bool,
        raise_errors: bool,
        acc: (pattern_union::PatternUnion, usize),
        guarded: bool,
        last: bool,
        pattern_ast: &ast::match_pattern::MatchPattern<ALoc, (ALoc, Type)>,
    ) -> Result<(pattern_union::PatternUnion, usize), JobError> {
        use flow_parser::ast::match_pattern::MatchPattern;
        use flow_parser::ast::match_pattern::unary_pattern;

        let (mut pattern_union, i) = acc;
        let loc = pattern_ast.loc();
        let reason = flow_common::reason::mk_reason(None, loc.dupe());
        let next_i = i + 1;
        Ok(match pattern_ast {
            MatchPattern::BindingPattern { .. } => {
                let reason = flow_common::reason::mk_reason(None, loc.dupe());
                pattern_union =
                    add_wildcard(cx, raise_errors, guarded, last, pattern_union, reason);
                (pattern_union, next_i)
            }
            MatchPattern::WildcardPattern { .. } => {
                let reason = flow_common::reason::mk_reason(None, loc.dupe());
                pattern_union =
                    add_wildcard(cx, raise_errors, guarded, last, pattern_union, reason);
                (pattern_union, next_i)
            }
            MatchPattern::BooleanPattern { inner, .. } => {
                let leaf_val = leaf::Leaf(reason, leaf::LeafCtor::BoolC(inner.value));
                pattern_union = add_leaf(cx, raise_errors, guarded, pattern_union, leaf_val);
                (pattern_union, next_i)
            }
            MatchPattern::NumberPattern { inner, .. } => {
                let leaf_val = leaf::Leaf(
                    reason,
                    leaf::LeafCtor::NumC(type_::NumberLiteral(inner.value, inner.raw.dupe())),
                );
                pattern_union = add_leaf(cx, raise_errors, guarded, pattern_union, leaf_val);
                (pattern_union, next_i)
            }
            MatchPattern::BigIntPattern { inner, .. } => {
                let leaf_val = leaf::Leaf(
                    reason,
                    leaf::LeafCtor::BigIntC(type_::BigIntLiteral(inner.value, inner.raw.dupe())),
                );
                pattern_union = add_leaf(cx, raise_errors, guarded, pattern_union, leaf_val);
                (pattern_union, next_i)
            }
            MatchPattern::StringPattern { inner, .. } => {
                let leaf_val = leaf::Leaf(reason, leaf::LeafCtor::StrC(inner.value.dupe()));
                pattern_union = add_leaf(cx, raise_errors, guarded, pattern_union, leaf_val);
                (pattern_union, next_i)
            }
            MatchPattern::NullPattern { .. } => {
                let leaf_val = leaf::Leaf(reason, leaf::LeafCtor::NullC);
                pattern_union = add_leaf(cx, raise_errors, guarded, pattern_union, leaf_val);
                (pattern_union, next_i)
            }
            MatchPattern::UnaryPattern { inner, .. } => {
                let operator = &inner.operator;
                let (_, ref argument) = inner.argument;
                match argument {
                    unary_pattern::Argument::NumberLiteral(num_lit) => {
                        let value = num_lit.value;
                        let raw = num_lit.raw.dupe();
                        if value == 0.0 {
                            pattern_union.contains_invalid_pattern = true;
                            (pattern_union, i)
                        } else {
                            let literal = match operator {
                                unary_pattern::Operator::Plus => (value, raw),
                                unary_pattern::Operator::Minus => {
                                    let (v, r) =
                                        flow_parser::ast_utils::negate_number_literal((value, raw));
                                    (v, r)
                                }
                            };
                            let leaf_val = leaf::Leaf(
                                reason,
                                leaf::LeafCtor::NumC(type_::NumberLiteral(literal.0, literal.1)),
                            );
                            pattern_union =
                                add_leaf(cx, raise_errors, guarded, pattern_union, leaf_val);
                            (pattern_union, next_i)
                        }
                    }
                    unary_pattern::Argument::BigIntLiteral(bigint_lit) => {
                        let value = bigint_lit.value;
                        let raw = bigint_lit.raw.dupe();
                        match operator {
                            // We've already errored on `+` on bigint literal patterns.
                            unary_pattern::Operator::Plus => {
                                pattern_union.contains_invalid_pattern = true;
                                (pattern_union, i)
                            }
                            unary_pattern::Operator::Minus => {
                                let literal =
                                    flow_parser::ast_utils::negate_bigint_literal((value, raw));
                                let leaf_val = leaf::Leaf(
                                    reason,
                                    leaf::LeafCtor::BigIntC(type_::BigIntLiteral(
                                        literal.0, literal.1,
                                    )),
                                );
                                pattern_union =
                                    add_leaf(cx, raise_errors, guarded, pattern_union, leaf_val);
                                (pattern_union, next_i)
                            }
                        }
                    }
                }
            }
            MatchPattern::IdentifierPattern { inner, .. }
                if inner.name.as_str() == "NaN" && is_global_var(cx, inner.loc.0.dupe()) =>
            {
                // The global `NaN`. It has type `number`, so it does not contribute to
                // exhaustiveness, but it is a valid pattern and is marked as used whenever
                // `number` is part of the input type.
                let leaf_val = leaf::Leaf(reason, leaf::LeafCtor::NaNC);
                pattern_union = add_leaf(cx, raise_errors, guarded, pattern_union, leaf_val);
                (pattern_union, next_i)
            }
            MatchPattern::IdentifierPattern { inner, .. } => {
                let (_, ref id_t) = inner.loc;
                let concrete_t = singleton_concrete_type(cx, id_t)?;
                match leaf_of_type(cx, raise_errors, &concrete_t, pattern_ast) {
                    Some(leaf_val) => {
                        pattern_union =
                            add_leaf(cx, raise_errors, guarded, pattern_union, leaf_val);
                        (pattern_union, next_i)
                    }
                    None => {
                        pattern_union.contains_invalid_pattern = true;
                        (pattern_union, i)
                    }
                }
            }
            MatchPattern::MemberPattern { inner, .. } => {
                let (_, ref member_t) = inner.loc;
                let concrete_t = singleton_concrete_type(cx, member_t)?;
                match leaf_of_type(cx, raise_errors, &concrete_t, pattern_ast) {
                    Some(leaf_val) => {
                        pattern_union =
                            add_leaf(cx, raise_errors, guarded, pattern_union, leaf_val);
                        (pattern_union, next_i)
                    }
                    None => {
                        pattern_union.contains_invalid_pattern = true;
                        (pattern_union, i)
                    }
                }
            }
            MatchPattern::AsPattern { inner, .. } => of_pattern_ast_inner(
                cx,
                is_global_var,
                raise_errors,
                (pattern_union, i),
                guarded,
                last,
                &inner.pattern,
            )?,
            MatchPattern::OrPattern { inner, .. } => {
                let mut acc = (pattern_union, i);
                for pat in inner.patterns.iter() {
                    acc = of_pattern_ast_inner(
                        cx,
                        is_global_var,
                        raise_errors,
                        acc,
                        guarded,
                        last,
                        pat,
                    )?;
                }
                acc
            }
            MatchPattern::ArrayPattern { inner, .. } => {
                let elements = &inner.elements;
                let rest = &inner.rest;
                let length = elements.len();
                let mut props: SMap<pattern_object::Property> = FlowOrdMap::default();
                let mut keys_order: Vec<FlowSmolStr> = Vec::new();
                let mut contains_invalid_pattern_arr = false;
                for (idx, elem) in elements.iter().enumerate() {
                    let key: FlowSmolStr = idx.to_string().into();
                    let value = of_pattern_ast(cx, is_global_var, raise_errors, &elem.pattern)?;
                    contains_invalid_pattern_arr =
                        contains_invalid_pattern_arr || value.contains_invalid_pattern;
                    props.insert(
                        key.dupe(),
                        pattern_object::Property {
                            loc: loc.dupe(),
                            value,
                        },
                    );
                    keys_order.push(key);
                }
                let rest_reason = rest.as_ref().map(|r| {
                    flow_common::reason::mk_reason(
                        flow_common::reason::VirtualReasonDesc::RArrayPatternRestProp,
                        r.loc.dupe(),
                    )
                });
                keys_order.sort_by(|key_a, key_b| {
                    let prop_a = props.get(key_a).unwrap();
                    let prop_b = props.get(key_b).unwrap();
                    prop_a.cmp(prop_b)
                });
                let tuple: pattern_object::WithIndex = (
                    next_i,
                    pattern_object::PatternObject(
                        reason,
                        std::rc::Rc::new(pattern_object::PatternObjectInner {
                            props,
                            class_info: None,
                            keys_order,
                            rest: rest_reason,
                            kind: ObjKind::Tuple { length },
                            contains_invalid_pattern: contains_invalid_pattern_arr,
                            guarded,
                        }),
                    ),
                );
                pattern_union = add_tuple(cx, raise_errors, pattern_union, length, tuple);
                (pattern_union, next_i)
            }
            MatchPattern::ObjectPattern { inner, .. } => object_pattern(
                cx,
                is_global_var,
                raise_errors,
                &reason,
                next_i,
                pattern_union,
                guarded,
                None,
                inner,
            )?,
            MatchPattern::InstancePattern { inner, .. } => {
                use flow_parser::ast::match_pattern::InstancePatternConstructor;
                use flow_parser::ast::match_pattern::member_pattern::Property as MemberProperty;
                let (constructor_t, constructor_name): (&Type, Option<FlowSmolStr>) =
                    match &inner.constructor {
                        InstancePatternConstructor::IdentifierConstructor(id) => {
                            let (_, ref t) = id.loc;
                            (t, Some(id.name.dupe()))
                        }
                        InstancePatternConstructor::MemberConstructor(member) => {
                            let (_, ref t) = member.loc;
                            match &member.property {
                                MemberProperty::PropertyIdentifier(id) => (t, Some(id.name.dupe())),
                                _ => (t, None),
                            }
                        }
                    };
                match get_class_info_with_supers(cx, constructor_t)? {
                    Some((class_id, class_name, super_ids, instance_t)) => {
                        let class_name = constructor_name.or(class_name);
                        let class_info = Some((class_id, class_name, super_ids, instance_t));
                        let (_, ref properties) = inner.properties;
                        object_pattern(
                            cx,
                            is_global_var,
                            raise_errors,
                            &reason,
                            next_i,
                            pattern_union,
                            guarded,
                            class_info,
                            properties,
                        )?
                    }
                    None => (pattern_union, next_i),
                }
            }
        })
    }

    fn object_pattern<'cx>(
        cx: &mut Context<'cx>,
        is_global_var: &dyn Fn(&Context<'cx>, ALoc) -> bool,
        raise_errors: bool,
        reason: &Reason,
        next_i: usize,
        mut pattern_union: pattern_union::PatternUnion,
        guarded: bool,
        class_info: Option<(ALocId, Option<FlowSmolStr>, FlowOrdSet<ALocId>, Type)>,
        pattern: &ast::match_pattern::ObjectPattern<ALoc, (ALoc, Type)>,
    ) -> Result<(pattern_union::PatternUnion, usize), JobError> {
        use flow_parser::ast::match_pattern::object_pattern as ast_obj_pattern;
        use flow_parser::ast::match_pattern::object_pattern::Key;

        let properties = &pattern.properties;
        let rest = &pattern.rest;
        let mut props: SMap<pattern_object::Property> = FlowOrdMap::default();
        let mut keys_order: Vec<FlowSmolStr> = Vec::new();
        let mut tuple_like: Option<f64> = Some(0.0);
        let mut contains_invalid_pattern_obj = false;
        for prop in properties.iter() {
            match prop {
                ast_obj_pattern::Property::Valid { property, .. } => {
                    let (key_loc, propname): (ALoc, FlowSmolStr) = match &property.key {
                        Key::Identifier(id) => {
                            let (id_loc, _) = &id.loc;
                            (id_loc.dupe(), id.name.dupe())
                        }
                        Key::StringLiteral((key_loc, str_lit)) => {
                            (key_loc.dupe(), str_lit.value.dupe())
                        }
                        Key::NumberLiteral((key_loc, num_lit)) => (
                            key_loc.dupe(),
                            flow_common::js_number::ecma_string_of_float(num_lit.value).into(),
                        ),
                        Key::BigIntLiteral((key_loc, bigint_lit)) => {
                            (key_loc.dupe(), bigint_lit.raw.dupe())
                        }
                    };
                    let value = of_pattern_ast(cx, is_global_var, raise_errors, &property.pattern)?;
                    // If the object patterns seems like it could also match tuples, record that.
                    tuple_like = match tuple_like {
                        None => None,
                        Some(prev_i) => {
                            let propname_str: &str = &propname;
                            let float_opt: Option<f64> = propname_str.parse().ok();
                            match (propname_str, float_opt) {
                                (_, Some(fval))
                                    if flow_common::js_number::is_float_safe_integer(fval) =>
                                {
                                    Some(f64::max(prev_i, fval + 1.0))
                                }
                                ("length", _) if value.only_wildcard().is_some() => tuple_like,
                                _ => None,
                            }
                        }
                    };
                    let property_val = pattern_object::Property {
                        loc: key_loc,
                        value,
                    };
                    contains_invalid_pattern_obj =
                        contains_invalid_pattern_obj || property_val.value.contains_invalid_pattern;
                    props.insert(propname.dupe(), property_val);
                    keys_order.push(propname);
                }
                _ => {
                    contains_invalid_pattern_obj = true;
                }
            }
        }
        keys_order.sort_by(|key_a, key_b| {
            let prop_a = props.get(key_a).unwrap();
            let prop_b = props.get(key_b).unwrap();
            prop_a.cmp(prop_b)
        });
        let rest_reason = rest.as_ref().map(|r| {
            flow_common::reason::mk_reason(
                flow_common::reason::VirtualReasonDesc::RObjectPatternRestProp,
                r.loc.dupe(),
            )
        });
        let obj: pattern_object::WithIndex = (
            next_i,
            pattern_object::PatternObject(
                reason.dupe(),
                std::rc::Rc::new(pattern_object::PatternObjectInner {
                    props: props.dupe(),
                    class_info: class_info.clone(),
                    keys_order: keys_order.clone(),
                    rest: rest_reason,
                    kind: ObjKind::Obj,
                    contains_invalid_pattern: contains_invalid_pattern_obj,
                    guarded,
                }),
            ),
        );
        pattern_union = add_object(cx, raise_errors, pattern_union, obj.dupe());
        pattern_union = match (&class_info, &tuple_like) {
            (None, Some(float_i)) => {
                add_tuple(cx, raise_errors, pattern_union, *float_i as usize, obj)
            }
            _ => pattern_union,
        };
        Ok((pattern_union, next_i))
    }

    pub fn finalize(pattern_union: pattern_union::PatternUnion) -> pattern_union::PatternUnion {
        pattern_union
    }

    pub fn add_pattern<'cx>(
        cx: &mut Context<'cx>,
        is_global_var: &dyn Fn(&Context<'cx>, ALoc) -> bool,
        raise_errors: bool,
        acc: (pattern_union::PatternUnion, usize),
        (pattern_ast, guarded): (&ast::match_pattern::MatchPattern<ALoc, (ALoc, Type)>, bool),
        last: bool,
    ) -> Result<(pattern_union::PatternUnion, usize), JobError> {
        of_pattern_ast_inner(
            cx,
            is_global_var,
            raise_errors,
            acc,
            guarded,
            last,
            pattern_ast,
        )
    }

    pub(super) fn of_patterns_ast<'cx>(
        cx: &mut Context<'cx>,
        is_global_var: &dyn Fn(&Context<'cx>, ALoc) -> bool,
        raise_errors: bool,
        patterns_ast: &match_pattern_ir::PatternAstList,
    ) -> Result<pattern_union::PatternUnion, JobError> {
        let last_i = if patterns_ast.is_empty() {
            0
        } else {
            patterns_ast.len() - 1
        };
        let mut acc = (pattern_union::empty(), 0);
        for (i, (pat, guarded)) in patterns_ast.iter().enumerate() {
            acc = add_pattern(
                cx,
                is_global_var,
                raise_errors,
                acc,
                (pat, *guarded),
                i == last_i,
            )?;
        }
        let (pattern_union, _) = acc;
        Ok(finalize(pattern_union))
    }
}

mod value_object_property_builder {
    use flow_typing_type::type_;

    use super::*;

    pub(super) fn of_type_prop(
        key: &FlowSmolStr,
        p: &type_::Property,
    ) -> Option<value_object::Property<Cx>> {
        use flow_typing_type::type_::property;

        match property::read_loc(p) {
            Some(loc) => {
                let (t, optional) = match property::read_t(p) {
                    Some(t) => {
                        let optional = matches!(t.deref(), TypeInner::OptionalT { .. });
                        (t, optional)
                    }
                    None => {
                        let t = type_::mixed_t::make(flow_common::reason::mk_reason(
                            flow_common::reason::VirtualReasonDesc::RProperty(Some(
                                flow_common::reason::Name::new(key.dupe()),
                            )),
                            loc.dupe(),
                        ));
                        let optional = match property::write_t(p) {
                            Some(wt) => matches!(wt.deref(), TypeInner::OptionalT { .. }),
                            None => true,
                        };
                        (t, optional)
                    }
                };
                let t = t.dupe();
                let lazy_value: value_object::LazyValueUnion<Cx> =
                    std::rc::Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                        value_union_builder::of_type(cx, &t)
                    }));
                Some(value_object::Property {
                    loc,
                    value: lazy_value,
                    optional,
                })
            }
            None => None,
        }
    }
}

mod value_union_builder {
    use super::*;

    fn of_type_inner<'cx>(
        cx: &mut Context<'cx>,
        value_union: value_union::ValueUnion<Cx>,
        t: &Type,
    ) -> Result<value_union::ValueUnion<Cx>, JobError> {
        use flow_typing_type::type_::ArrType;
        use flow_typing_type::type_::ArrayATData;
        use flow_typing_type::type_::EnumInfoInner;
        use flow_typing_type::type_::InstanceKind;
        use flow_typing_type::type_::ObjKind as TypeObjKind;
        use flow_typing_type::type_::TupleATData;

        let possible = possible_concrete_types(cx, true, t)?;
        let mut ts: Vec<Type> = Vec::new();
        let mut all_sentinel_props: BTreeSet<flow_common::reason::Name> = BTreeSet::new();
        for pt in possible {
            match pt.deref() {
                TypeInner::UnionT(_, rep) => {
                    attempt_union_rep_optimization(cx, rep);
                    if let Some(props) = rep.disjoint_object_union_props() {
                        all_sentinel_props.extend(props);
                    }
                    let union_ts = possible_concrete_types(cx, false, &pt)?;
                    ts.extend(union_ts);
                }
                _ => {
                    ts.push(pt);
                }
            }
        }
        let mut value_union = value_union;
        for t in ts.iter() {
            match t.deref() {
                TypeInner::DefT(enum_reason, d) => match d.deref() {
                    DefTInner::EnumValueT(enum_info_rc) => match enum_info_rc.deref().deref() {
                        EnumInfoInner::ConcreteEnum(enum_info) => {
                            let members = &enum_info.members;
                            let has_unknown_members = enum_info.has_unknown_members;
                            let mut enum_leafs: LeafSet = FlowOrdSet::default();
                            for (member_name, member_loc) in members.iter() {
                                let reason = flow_common::reason::mk_reason(
                                    enum_reason.desc(true).cloned().map(|enum_desc| {
                                        flow_common::reason::VirtualReasonDesc::REnumMember {
                                            enum_desc: Arc::new(enum_desc),
                                            member_name: member_name.dupe(),
                                        }
                                    }),
                                    member_loc.dupe(),
                                );
                                enum_leafs.insert(leaf::Leaf(
                                    reason,
                                    leaf::LeafCtor::EnumMemberC(leaf::EnumMember {
                                        member_name: member_name.dupe(),
                                        enum_info: std::rc::Rc::new(enum_info.dupe()),
                                    }),
                                ));
                            }
                            if has_unknown_members {
                                let reason = enum_reason.dupe().replace_desc(None);
                                value_union.enum_unknown_members.push((
                                    reason,
                                    enum_info.enum_name.dupe(),
                                    enum_leafs.dupe(),
                                ));
                            }
                            value_union.leafs.extend(enum_leafs);
                        }
                        _ => {
                            value_union.inexhaustible.push(t.dupe());
                        }
                    },
                    DefTInner::SingletonBoolT { value, .. } => {
                        value_union.leafs.insert(leaf::Leaf(
                            enum_reason.dupe(),
                            leaf::LeafCtor::BoolC(*value),
                        ));
                    }
                    DefTInner::SingletonNumT { value, .. } => {
                        value_union.leafs.insert(leaf::Leaf(
                            enum_reason.dupe(),
                            leaf::LeafCtor::NumC(value.dupe()),
                        ));
                    }
                    DefTInner::SingletonBigIntT { value, .. } => {
                        value_union.leafs.insert(leaf::Leaf(
                            enum_reason.dupe(),
                            leaf::LeafCtor::BigIntC(value.dupe()),
                        ));
                    }
                    DefTInner::SingletonStrT { value, .. } => {
                        value_union.leafs.insert(leaf::Leaf(
                            enum_reason.dupe(),
                            leaf::LeafCtor::StrC(value.dupe()),
                        ));
                    }
                    DefTInner::NullT => {
                        value_union
                            .leafs
                            .insert(leaf::Leaf(enum_reason.dupe(), leaf::LeafCtor::NullC));
                    }
                    DefTInner::VoidT => {
                        value_union
                            .leafs
                            .insert(leaf::Leaf(enum_reason.dupe(), leaf::LeafCtor::VoidC));
                    }
                    DefTInner::BoolGeneralT => {
                        value_union
                            .leafs
                            .insert(leaf::Leaf(enum_reason.dupe(), leaf::LeafCtor::BoolC(true)));
                        value_union
                            .leafs
                            .insert(leaf::Leaf(enum_reason.dupe(), leaf::LeafCtor::BoolC(false)));
                    }
                    DefTInner::ArrT(arr_rc) => match arr_rc.deref() {
                        ArrType::TupleAT(box TupleATData {
                            elements,
                            arity: (num_req, num_total),
                            inexact,
                            rest: rest_elem,
                            ..
                        }) => {
                            let reason = enum_reason;
                            // A tuple with a rest element has no maximum length, like an inexact
                            // tuple.
                            let rest = if *inexact || rest_elem.is_some() {
                                Some(t.dupe())
                            } else {
                                None
                            };
                            let props_list: Vec<_> = elements
                                .iter()
                                .map(|elem| {
                                    let elem_t = if elem.optional {
                                        match elem.t.deref() {
                                            TypeInner::OptionalT { type_, .. } => type_.dupe(),
                                            _ => elem.t.dupe(),
                                        }
                                    } else {
                                        elem.t.dupe()
                                    };
                                    let elem_t = if flow_common::polarity::Polarity::compat(
                                        elem.polarity,
                                        flow_common::polarity::Polarity::Positive,
                                    ) {
                                        elem_t
                                    } else {
                                        flow_typing_type::type_::mixed_t::make(elem.reason.dupe())
                                    };
                                    let elem_t = elem_t.dupe();
                                    let lazy_value: value_object::LazyValueUnion<Cx> =
                                        std::rc::Rc::new(flow_lazy::Lazy::new(
                                            move |cx: &mut Context<'_>| of_type(cx, &elem_t),
                                        ));
                                    let elem_loc = elem.reason.loc();
                                    value_object::Property {
                                        loc: elem_loc.dupe(),
                                        value: lazy_value,
                                        optional: false,
                                    }
                                })
                                .collect();
                            let num_req = *num_req as usize;
                            let num_total = *num_total as usize;
                            for idx in 0..=(num_total - num_req) {
                                let length = num_req + idx;
                                let mut props: FlowOrdMap<
                                    FlowSmolStr,
                                    Option<value_object::Property<Cx>>,
                                > = FlowOrdMap::default();
                                for (prop_i, prop) in props_list.iter().take(length).enumerate() {
                                    let key: FlowSmolStr = prop_i.to_string().into();
                                    props.insert(key, Some(prop.dupe()));
                                }
                                let tuple = value_object::ValueObject(
                                    reason.dupe(),
                                    std::rc::Rc::new(value_object::ValueObjectInner {
                                        kind: ObjKind::Tuple { length },
                                        t: t.dupe(),
                                        props,
                                        class_info: None,
                                        rest: rest.dupe(),
                                        sentinel_props: FlowOrdSet::default(),
                                    }),
                                );
                                value_union.tuples.push(tuple);
                            }
                        }
                        ArrType::ArrayAT(box ArrayATData { .. })
                        | ArrType::ROArrayAT(box ROArrayATData { .. }) => {
                            let reason = enum_reason;
                            let arr = value_object::ValueObject(
                                reason.dupe(),
                                std::rc::Rc::new(value_object::ValueObjectInner {
                                    kind: ObjKind::Obj,
                                    t: t.dupe(),
                                    props: FlowOrdMap::default(),
                                    class_info: None,
                                    rest: Some(t.dupe()),
                                    sentinel_props: FlowOrdSet::default(),
                                }),
                            );
                            value_union.arrays.push(arr);
                        }
                    },
                    DefTInner::ObjT(obj_rc) => {
                        let obj = obj_rc.deref();
                        if obj.call_t.is_none() {
                            let reason = enum_reason;
                            let obj_kind = &obj.flags.obj_kind;
                            let props_tmap = obj.props_tmap.dupe();
                            let rest = match obj_kind {
                                TypeObjKind::Exact => None,
                                TypeObjKind::Inexact => Some(t.dupe()),
                                TypeObjKind::Indexed(dict) => Some(dict.key.dupe()),
                            };
                            let props_map = cx.find_props(props_tmap);
                            let mut obj_props: FlowOrdMap<
                                FlowSmolStr,
                                Option<value_object::Property<Cx>>,
                            > = FlowOrdMap::default();
                            let mut sentinel_props_set: FlowOrdSet<FlowSmolStr> =
                                FlowOrdSet::default();
                            for (name, p) in props_map.iter() {
                                // Symbol-typed keys have no string spelling and cannot be
                                // discriminated by (string-keyed) match patterns, so skip them
                                // rather than collapse distinct symbols to a placeholder.
                                let flow_common::reason::Name::Str(key) = name else {
                                    continue;
                                };
                                let key: FlowSmolStr = key.dupe();
                                let prop = value_object_property_builder::of_type_prop(&key, p);
                                obj_props.insert(key.dupe(), prop);
                                if all_sentinel_props.contains(name) {
                                    sentinel_props_set.insert(key);
                                }
                            }
                            let obj_value = value_object::ValueObject(
                                reason.dupe(),
                                std::rc::Rc::new(value_object::ValueObjectInner {
                                    kind: ObjKind::Obj,
                                    t: t.dupe(),
                                    props: obj_props,
                                    class_info: None,
                                    rest,
                                    sentinel_props: sentinel_props_set,
                                }),
                            );
                            value_union.objects.push(obj_value);
                        } else {
                            value_union.inexhaustible.push(t.dupe());
                        }
                    }
                    DefTInner::InstanceT(inst_rc) => {
                        let reason = enum_reason;
                        let inst = inst_rc.deref();
                        let inst_data = &inst.inst;
                        let class_id = &inst_data.class_id;
                        let class_name = &inst_data.class_name;
                        let inst_kind = &inst_data.inst_kind;
                        let super_ = &inst.super_;
                        let rest = match inst_kind {
                            InstanceKind::RecordKind { .. } => None,
                            InstanceKind::ClassKind | InstanceKind::InterfaceKind { .. } => {
                                Some(t.dupe())
                            }
                        };
                        let class_info = Some((
                            class_id.dupe(),
                            class_name.as_ref().map(|n| n.dupe()),
                            get_super_class_ids(cx, class_id, super_)?,
                        ));
                        let obj_value = value_object::ValueObject(
                            reason.dupe(),
                            std::rc::Rc::new(value_object::ValueObjectInner {
                                kind: ObjKind::Obj,
                                t: t.dupe(),
                                props: FlowOrdMap::default(),
                                class_info,
                                rest,
                                sentinel_props: FlowOrdSet::default(),
                            }),
                        );
                        value_union.objects.push(obj_value);
                    }
                    DefTInner::EmptyT => {}
                    _ => {
                        value_union.inexhaustible.push(t.dupe());
                    }
                },
                _ => {
                    value_union.inexhaustible.push(t.dupe());
                }
            }
        }
        Ok(value_union)
    }

    // We implement this ourselves for now because we want to know if we are getting a
    // property from an indexer or not, while preserving type-level method access.
    // TODO: Update existing machinery to return the info we want and use that instead.
    pub fn get_prop<'cx>(
        cx: &mut Context<'cx>,
        key: &(ALoc, FlowSmolStr),
        obj: &Type,
    ) -> Result<Option<value_object::Property<Cx>>, JobError> {
        fn get_prop_from_dict<'cx>(
            cx: &mut Context<'cx>,
            key_loc: &ALoc,
            key_name: &FlowSmolStr,
            dict: Option<&flow_typing_type::type_::DictType>,
        ) -> Result<Option<value_object::Property<Cx>>, JobError> {
            match dict {
                Some(dict) => {
                    let reason_key = flow_common::reason::mk_reason(
                        flow_common::reason::VirtualReasonDesc::RProperty(Some(
                            flow_common::reason::Name::new(key_name.dupe()),
                        )),
                        key_loc.dupe(),
                    );
                    let key_t = flow_typing_flow_common::flow_js_utils::type_of_key_name(
                        flow_common::reason::Name::new(key_name.dupe()),
                        &reason_key,
                    );
                    if flow_common::polarity::Polarity::compat(
                        dict.dict_polarity,
                        flow_common::polarity::Polarity::Positive,
                    ) && flow_js::FlowJs::speculative_subtyping_succeeds(cx, &key_t, &dict.key)?
                    {
                        let loc = reason_of_t(&dict.key).loc();
                        let dict_value = dict.value.dupe();
                        let lazy_value: value_object::LazyValueUnion<Cx> =
                            std::rc::Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                                of_type(cx, &dict_value)
                            }));
                        Ok(Some(value_object::Property {
                            loc: loc.dupe(),
                            value: lazy_value,
                            optional: true,
                        }))
                    } else {
                        Ok(None)
                    }
                }
                None => Ok(None),
            }
        }

        fn find_key<'cx>(
            cx: &mut Context<'cx>,
            super_t: &Type,
            props_list: &[flow_typing_type::type_::properties::Id],
            dict: Option<&flow_typing_type::type_::DictType>,
            key: &(ALoc, FlowSmolStr),
        ) -> Result<Option<value_object::Property<Cx>>, JobError> {
            let (_, key_name) = key;
            let current_prop = props_list.iter().find_map(|id| {
                let name = flow_common::reason::Name::new(key_name.dupe());
                cx.get_prop(id.dupe(), &name)
                    .and_then(|p| value_object_property_builder::of_type_prop(key_name, &p))
            });
            if current_prop.is_some() {
                return Ok(current_prop);
            }
            let super_prop = get_prop(cx, key, super_t)?;
            if super_prop.is_some() {
                return Ok(super_prop);
            }
            let (key_loc, key_name) = key;
            get_prop_from_dict(cx, key_loc, key_name, dict)
        }

        let (_, key_name) = key;
        let concrete = singleton_concrete_type(cx, obj)?;
        match concrete.deref() {
            TypeInner::DefT(reason, d) => match d.deref() {
                DefTInner::ObjT(obj_rc) => {
                    let obj_t = obj_rc.deref();
                    let dict =
                        flow_typing_flow_common::obj_type::get_dict_opt(&obj_t.flags.obj_kind);
                    find_key(cx, &obj_t.proto_t, &[obj_t.props_tmap.dupe()], dict, key)
                }
                DefTInner::InstanceT(inst_rc) => {
                    let inst = inst_rc.deref();
                    let inst_data = &inst.inst;
                    find_key(
                        cx,
                        &inst.super_,
                        &[inst_data.own_props.dupe(), inst_data.proto_props.dupe()],
                        inst_data.inst_dict.as_ref(),
                        key,
                    )
                }
                DefTInner::ArrT(arr_rc) => {
                    let elem_t = flow_typing_type::type_::elemt_of_arrtype(arr_rc);
                    let arr_t =
                        FlowJs::get_builtin_typeapp(cx, reason, None, "Array", vec![elem_t]);
                    get_prop(cx, key, &arr_t)
                }
                _ => Ok(None),
            },
            TypeInner::NullProtoT(_) => Ok(None),
            TypeInner::ObjProtoT(reason) => {
                let name = flow_common::reason::Name::new(key_name.dupe());
                if flow_typing_flow_common::flow_js_utils::is_object_prototype_method(&name) {
                    let builtin_t = flow_js::get_builtin_type(cx, reason, None, "Object");
                    // Propagate WorkerCanceled, TimedOut, and DebugThrow; other
                    // FlowJsException variants are dropped here matching the
                    // original `.ok()?` behavior. See plan.md §"JobError".
                    let builtin_t = match builtin_t {
                        Ok(t) => t,
                        Err(FlowJsException::WorkerCanceled(c)) => {
                            return Err(JobError::Canceled(c));
                        }
                        Err(FlowJsException::TimedOut(t)) => return Err(JobError::TimedOut(t)),
                        Err(FlowJsException::DebugThrow { loc }) => {
                            return Err(JobError::DebugThrow { loc });
                        }
                        Err(_) => return Ok(None),
                    };
                    get_prop(cx, key, &builtin_t)
                } else {
                    Ok(None)
                }
            }
            TypeInner::IntersectionT(_, rep) => {
                for member_t in rep.members_iter() {
                    let p = get_prop(cx, key, member_t)?;
                    if p.is_some() {
                        return Ok(p);
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    pub(super) fn of_type<'cx>(
        cx: &mut Context<'cx>,
        t: &Type,
    ) -> Result<value_union::ValueUnion<Cx>, JobError> {
        // The list members of the `ValueUnion` are accumulated in reverse order,
        // put them back in their original order.
        let mut vu = of_type_inner(cx, value_union::ValueUnion::empty(), t)?;
        vu.tuples = vu.tuples.into_iter().rev().collect();
        vu.arrays = vu.arrays.into_iter().rev().collect();
        vu.objects = vu.objects.into_iter().rev().collect();
        vu.enum_unknown_members = vu.enum_unknown_members.into_iter().rev().collect();
        vu.inexhaustible = vu.inexhaustible.into_iter().rev().collect();
        Ok(vu)
    }
}

// *******************
// * Filtering logic *
// *******************

fn is_leaf_subtype_of_inexhaustible<'cx>(
    cx: &mut Context<'cx>,
    leaf_val: &leaf::Leaf,
    inexhaustible: &FlowVector<Type>,
) -> Result<bool, JobError> {
    let leaf_type = leaf_val.to_type();
    for t in inexhaustible.iter() {
        if FlowJs::speculative_subtyping_succeeds(cx, &leaf_type, t)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn is_object_subtype_of_inexhaustible(inexhaustible: &FlowVector<Type>) -> Option<Type> {
    inexhaustible.iter().find_map(|t| match t.deref() {
        TypeInner::DefT(_, d) => match d.deref() {
            DefTInner::MixedT(flow_typing_type::type_::MixedFlavor::MixedFunction) => None,
            DefTInner::MixedT(_) => Some(t.dupe()),
            _ => None,
        },
        TypeInner::AnyT(_, _) => Some(t.dupe()),
        _ => None,
    })
}

enum FilterObjectResult {
    Match {
        used_pattern_locs: ALocSet,
        // If an object value was only partially matched by a pattern,
        // we store the remainders so we can add them to the queue to check.
        queue_additions: Vec<value_object::ValueObject<Cx>>,
        matched: value_object::ValueObject<Cx>,
    },
    NoMatch {
        used_pattern_locs: ALocSet,
        left: value_object::ValueObject<Cx>,
    },
}

struct FilterUnionResult {
    value_left: value_union::ValueUnion<Cx>,
    value_matched: value_union::ValueUnion<Cx>,
    used_pattern_locs: ALocSet,
}

/// Filter some values by some patterns. This results in values which were matched by the patterns,
/// and those which were not matched and left over. We also computed the set of locs of patterns
/// which were useful - that is used to computed the unnecessary patterns.
fn filter_values_by_patterns<'cx>(
    cx: &mut Context<'cx>,
    raise_errors: bool,
    value_union: &value_union::ValueUnion<Cx>,
    pattern_union: &pattern_union::PatternUnion,
) -> Result<FilterUnionResult, JobError> {
    let value_union::ValueUnion {
        leafs: value_leafs,
        tuples: value_tuples,
        arrays,
        objects: value_objects,
        enum_unknown_members,
        inexhaustible,
    } = value_union;
    let pattern_union::PatternUnion {
        leafs: pattern_leafs,
        guarded_leafs,
        tuples_exact: pattern_tuples_exact,
        tuples_inexact: pattern_tuples_inexact,
        objects: pattern_objects,
        wildcard,
        contains_invalid_pattern: _,
    } = pattern_union;

    // Leafs
    let mut leafs_matched: LeafSet = FlowOrdSet::default();
    let mut leafs_left: LeafSet = FlowOrdSet::default();
    for leaf_val in value_leafs.iter() {
        if pattern_leafs.contains(leaf_val) {
            leafs_matched.insert(leaf_val.clone());
        } else {
            leafs_left.insert(leaf_val.clone());
        }
    }
    let mut used_pattern_locs: ALocSet = ALocSet::new();
    for leaf_pattern in pattern_leafs.iter() {
        mark_leaf_pattern_used(
            cx,
            &leafs_matched,
            inexhaustible,
            leaf_pattern,
            &mut used_pattern_locs,
        )?;
    }
    for leaf_val in guarded_leafs.iter() {
        mark_leaf_pattern_used(
            cx,
            value_leafs,
            inexhaustible,
            leaf_val,
            &mut used_pattern_locs,
        )?;
    }

    // The `undefined` pattern is always marked as used, so that we do not tell users
    // to remove it in unsafe situations that could arise due to Flow's unsoundness.
    if let Some(found) = pattern_leafs
        .get_prev(&leaf::Leaf(
            flow_common::reason::locationless_reason(flow_common::reason::VirtualReasonDesc::RVoid),
            leaf::LeafCtor::VoidC,
        ))
        .filter(|l| l.1 == leaf::LeafCtor::VoidC)
    {
        used_pattern_locs.insert(found.0.loc().dupe());
    }

    // Tuples
    let tuples_lte_length = |tuples: &IMap<FlowVector<pattern_object::WithIndex>>,
                             length: usize|
     -> Vec<pattern_object::WithIndex> {
        tuples
            .range(..=length)
            .flat_map(|(_, v)| v.iter().duped())
            .collect()
    };
    let tuples_gte_length = |tuples: &IMap<FlowVector<pattern_object::WithIndex>>,
                             length: usize|
     -> Vec<pattern_object::WithIndex> {
        tuples
            .range(length..)
            .flat_map(|(_, v)| v.iter().duped())
            .collect()
    };

    let mut tuples_left: Vec<value_object::ValueObject<Cx>> = Vec::new();
    let mut tuples_matched: Vec<value_object::ValueObject<Cx>> = Vec::new();
    for tuple_value in value_tuples.iter() {
        let inner = tuple_value.1.as_ref();
        let value_rest = &inner.rest;
        let value_kind = &inner.kind;
        let value_is_inexact = value_rest.is_some();
        let mut pattern_tuples: Vec<pattern_object::WithIndex> = match value_kind {
            ObjKind::Tuple { length } => {
                if value_is_inexact {
                    let mut pt: Vec<pattern_object::WithIndex> = pattern_tuples_inexact
                        .values()
                        .flat_map(|v| v.iter())
                        .duped()
                        .collect();
                    let gte = tuples_gte_length(pattern_tuples_exact, *length);
                    pt.extend(gte);
                    pt
                } else {
                    let exact_length: Vec<pattern_object::WithIndex> = pattern_tuples_exact
                        .get(length)
                        .map(|v| v.iter().duped().collect())
                        .unwrap_or_default();
                    let inexact_pt = tuples_lte_length(pattern_tuples_inexact, *length);
                    let mut combined = exact_length;
                    combined.extend(inexact_pt);
                    combined
                }
            }
            ObjKind::Obj => Vec::new(),
        };
        match_pattern_ir::sort_object_patterns_by_index(&mut pattern_tuples);

        if pattern_tuples.is_empty() {
            tuples_left.push(tuple_value.dupe());
        } else {
            let (tl, tm, upl) = filter_objects_by_patterns(
                cx,
                raise_errors,
                &[tuple_value.dupe()],
                &pattern_tuples,
            )?;
            used_pattern_locs.extend(upl);
            tuples_left.extend(tl);
            tuples_matched.extend(tm);
        }
    }

    // Arrays
    let arrays_vec: Vec<_> = arrays.iter().duped().collect();
    let (mut arrays_left, mut arrays_matched, array_used_pattern_locs) = if arrays.is_empty() {
        (Vec::new(), Vec::new(), ALocSet::new())
    } else {
        let all_tuples_and_objects = pattern_union.all_tuples_and_objects();
        filter_objects_by_patterns(cx, raise_errors, &arrays_vec, &all_tuples_and_objects)?
    };
    arrays_left.reverse();
    arrays_matched.reverse();
    used_pattern_locs.extend(array_used_pattern_locs);
    // Objects
    let objects_vec: Vec<_> = value_objects.iter().duped().collect();
    let (mut objects_left, mut objects_matched, obj_used_pattern_locs) =
        filter_objects_by_patterns(cx, raise_errors, &objects_vec, pattern_objects)?;
    objects_left.reverse();
    objects_matched.reverse();
    used_pattern_locs.extend(obj_used_pattern_locs);
    let value_left = value_union::ValueUnion {
        leafs: leafs_left,
        tuples: tuples_left.into_iter().collect(),
        arrays: arrays_left.into_iter().collect(),
        objects: objects_left.into_iter().collect(),
        enum_unknown_members: enum_unknown_members.dupe(),
        inexhaustible: inexhaustible.dupe(),
    };
    // Mixed/any
    let mixed_used_pattern_locs = match is_object_subtype_of_inexhaustible(inexhaustible) {
        None => ALocSet::new(),
        Some(t) => visit_mixed(cx, raise_errors, &t, pattern_union)?,
    };
    used_pattern_locs.extend(mixed_used_pattern_locs);
    // Wildcard
    match wildcard {
        Some(wildcard_reason) => {
            if !value_left.is_empty() {
                let loc = wildcard_reason.loc();
                used_pattern_locs.insert(loc.dupe());
            }
            Ok(FilterUnionResult {
                value_left: value_union::ValueUnion::empty(),
                value_matched: value_union.dupe(),
                used_pattern_locs,
            })
        }
        None => {
            let value_matched = value_union::ValueUnion {
                leafs: leafs_matched,
                tuples: tuples_matched.into_iter().collect(),
                arrays: arrays_matched.into_iter().collect(),
                objects: objects_matched.into_iter().collect(),
                enum_unknown_members: FlowVector::default(),
                inexhaustible: FlowVector::default(),
            };
            Ok(FilterUnionResult {
                value_left,
                value_matched,
                used_pattern_locs,
            })
        }
    }
}

/// Given a list of object values, and a list of object patterns, match the
/// values against the patterns, and compute which objects were matched, and
/// which were not matched.
fn filter_objects_by_patterns<'cx>(
    cx: &mut Context<'cx>,
    raise_errors: bool,
    value_objects: &[value_object::ValueObject<Cx>],
    pattern_objects: &[pattern_object::WithIndex],
) -> Result<
    (
        Vec<value_object::ValueObject<Cx>>,
        Vec<value_object::ValueObject<Cx>>,
        ALocSet,
    ),
    JobError,
> {
    let mut acc_left: Vec<value_object::ValueObject<Cx>> = Vec::new();
    let mut acc_matched: Vec<value_object::ValueObject<Cx>> = Vec::new();
    let mut acc_used_pattern_locs: ALocSet = ALocSet::new();
    let mut queue: VecDeque<value_object::ValueObject<Cx>> = value_objects.iter().duped().collect();

    while let Some(value_object) = queue.pop_front() {
        // Fold over the object patterns over the current object value. We thread
        // `subclass_covered`: the set of class ids of earlier subclass patterns
        // that fully cover their class. A later subclass pattern for the same
        // class or a subclass of it is then redundant (see
        // `filter_object_by_pattern`).
        let mut result = FilterObjectResult::NoMatch {
            used_pattern_locs: ALocSet::new(),
            left: value_object.dupe(),
        };
        let mut additional_used_pattern_locs: ALocSet = ALocSet::new();
        let mut subclass_covered: FlowOrdSet<ALocId> = FlowOrdSet::default();
        for (_, pattern_object) in pattern_objects.iter() {
            match &result {
                FilterObjectResult::Match { .. } => {}
                FilterObjectResult::NoMatch {
                    used_pattern_locs, ..
                } => {
                    additional_used_pattern_locs.extend(used_pattern_locs.iter().map(|l| l.dupe()));
                    let (new_result, new_subclass_covered) = filter_object_by_pattern(
                        cx,
                        raise_errors,
                        &subclass_covered,
                        &value_object,
                        pattern_object,
                    )?;
                    result = new_result;
                    subclass_covered = new_subclass_covered;
                }
            }
        }

        match result {
            FilterObjectResult::NoMatch {
                used_pattern_locs,
                left,
            } => {
                acc_left.push(left);
                acc_used_pattern_locs.extend(used_pattern_locs);
            }
            FilterObjectResult::Match {
                used_pattern_locs,
                queue_additions,
                matched,
            } => {
                if !queue_additions.is_empty() {
                    for addition in queue_additions.into_iter().rev() {
                        queue.push_front(addition);
                    }
                }
                acc_matched.push(matched);
                acc_used_pattern_locs.extend(used_pattern_locs);
            }
        }
        acc_used_pattern_locs.extend(additional_used_pattern_locs);
    }
    Ok((acc_left, acc_matched, acc_used_pattern_locs))
}

// Filter an object value by an object pattern. Also threads `subclass_covered`:
// the set of class ids of earlier subclass patterns that fully cover their
// class, returning the (possibly extended) set.
fn filter_object_by_pattern<'cx>(
    cx: &mut Context<'cx>,
    raise_errors: bool,
    subclass_covered: &FlowOrdSet<ALocId>,
    value_object: &value_object::ValueObject<Cx>,
    pattern_object: &pattern_object::PatternObject,
) -> Result<(FilterObjectResult, FlowOrdSet<ALocId>), JobError> {
    let value_object::ValueObject(reason_value, value_inner) = value_object;
    let value_object::ValueObjectInner {
        props: value_props_orig,
        class_info: value_class_info,
        rest: value_rest,
        t,
        kind: value_kind,
        sentinel_props,
    } = value_inner.as_ref();
    let pattern_object::PatternObject(reason_pattern, pattern_inner) = pattern_object;
    let pattern_object::PatternObjectInner {
        props: pattern_props,
        class_info: pattern_class_info,
        keys_order,
        rest: pattern_rest,
        kind: pattern_kind,
        guarded,
        contains_invalid_pattern: _,
    } = pattern_inner.as_ref();

    enum MatchKind {
        Match,
        NoMatch,
        SubclassPattern,
    }
    let match_kind = match (value_class_info, pattern_class_info) {
        // Structural pattern: can match
        (_, None) => MatchKind::Match,
        // Structural value, nominal pattern: cannot match
        (None, Some(_)) => MatchKind::NoMatch,
        // Nominal value, nominal pattern
        (
            Some((value_class_id, _, value_super_ids)),
            Some((pattern_class_id, _, pattern_super_ids, _)),
        ) => {
            if value_class_id == pattern_class_id || value_super_ids.contains(pattern_class_id) {
                // Pattern's class is the value's class or a super class: matches.
                MatchKind::Match
            } else if pattern_super_ids.contains(value_class_id) {
                // Pattern's class is a strict subclass of the value's class.
                MatchKind::SubclassPattern
            } else {
                MatchKind::NoMatch
            }
        }
    };
    match match_kind {
        MatchKind::NoMatch => {
            return Ok((
                FilterObjectResult::NoMatch {
                    used_pattern_locs: ALocSet::new(),
                    left: value_object.dupe(),
                },
                subclass_covered.dupe(),
            ));
        }
        MatchKind::SubclassPattern => {
            // Flow does not track all the subclasses of a class, so a subclass pattern
            // cannot reduce the value for exhaustiveness checking - we leave the value
            // unconsumed, just like `NoMatch`. But the runtime value could be an
            // instance of the subclass, so the pattern is reachable: we mark it (and
            // its sub-patterns) as used so it is not reported as unused.
            return match pattern_class_info {
                None => {
                    // Unreachable: `SubclassPattern` implies a nominal pattern.
                    Ok((
                        FilterObjectResult::NoMatch {
                            used_pattern_locs: ALocSet::new(),
                            left: value_object.dupe(),
                        },
                        subclass_covered.dupe(),
                    ))
                }
                Some((pattern_class_id, _, pattern_super_ids, pattern_instance_t)) => {
                    let covered_by_earlier = subclass_covered.contains(pattern_class_id)
                        || !pattern_super_ids
                            .dupe()
                            .into_inner()
                            .intersection(subclass_covered.dupe().into_inner())
                            .is_empty();
                    if covered_by_earlier {
                        // An earlier subclass pattern already fully covers this class (or a
                        // super class of it), so this pattern is unused.
                        Ok((
                            FilterObjectResult::NoMatch {
                                used_pattern_locs: ALocSet::new(),
                                left: value_object.dupe(),
                            },
                            subclass_covered.dupe(),
                        ))
                    } else {
                        // Validate the object part of the pattern by matching it against a value
                        // of the pattern's own class, reusing the normal filtering path.
                        let value_union = value_union_builder::of_type(cx, pattern_instance_t)?;
                        let pattern_union = pattern_union::PatternUnion {
                            objects: vec![(0, pattern_object.dupe())],
                            ..pattern_union::empty()
                        };
                        let FilterUnionResult {
                            value_left,
                            mut used_pattern_locs,
                            ..
                        } = filter_values_by_patterns(
                            cx,
                            raise_errors,
                            &value_union,
                            &pattern_union,
                        )?;
                        used_pattern_locs.insert(reason_pattern.loc().dupe());
                        // If the pattern fully consumes a value of its own class, it covers that
                        // class, so later equal-or-narrower subclass patterns become unused. A
                        // non-total pattern (missing `...`, an unsatisfiable property, or a
                        // guard) leaves the class uncovered.
                        let subclass_covered = if value_left.is_empty() {
                            let mut subclass_covered = subclass_covered.dupe();
                            subclass_covered.insert(pattern_class_id.dupe());
                            subclass_covered
                        } else {
                            subclass_covered.dupe()
                        };
                        Ok((
                            FilterObjectResult::NoMatch {
                                used_pattern_locs,
                                left: value_object.dupe(),
                            },
                            subclass_covered,
                        ))
                    }
                }
            };
        }
        MatchKind::Match => {}
    }
    // A `Match`/`NoMatch` outcome here never changes `subclass_covered`.

    // Sort the keys that are sentinel props for the value first.
    let pattern_keys = {
        let mut sentinel_keys: Vec<FlowSmolStr> = Vec::new();
        let mut other_keys_vec: Vec<FlowSmolStr> = Vec::new();
        for key in keys_order.iter() {
            if sentinel_props.contains(key) {
                sentinel_keys.push(key.dupe());
            } else {
                other_keys_vec.push(key.dupe());
            }
        }
        sentinel_keys.extend(other_keys_vec);
        sentinel_keys
    };

    // If every key in the pattern also exists in the value, then return the props of the
    // value, otherwise return `None`, so we can skip below directly to `NoMatch`.
    let mut value_props = value_props_orig.dupe();
    let mut value_props_early_break = false;
    for key in pattern_keys.iter() {
        match value_props.get(key) {
            Some(Some(_)) => {}
            Some(None) => {
                value_props_early_break = true;
                break;
            }
            None => {
                let pattern_prop = pattern_props.get(key).unwrap();
                let key_loc = &pattern_prop.loc;
                let prop = value_union_builder::get_prop(cx, &(key_loc.dupe(), key.dupe()), t)?;
                if prop.is_none() {
                    value_props_early_break = true;
                    break;
                }
                value_props.insert(key.dupe(), prop);
            }
        }
    }
    if value_props_early_break {
        return Ok((
            FilterObjectResult::NoMatch {
                used_pattern_locs: ALocSet::new(),
                left: value_object.dupe(),
            },
            subclass_covered.dupe(),
        ));
    }

    // If this pattern is gaurded, then it can't filter out values
    let mut no_match = *guarded;
    // We fold over the `pattern_keys` of the pattern and match the value at that key to the pattern
    // at that key. We build up `head` which is the properties matched so far. The `remainder_value`
    // is the properties of the value left to check. If we don't have a match, but need to continue
    // checking for the purposes of marking patterns as used, we set `no_match` to true.
    let mut used_pattern_locs: ALocSet = ALocSet::new();
    let mut queue_additions = Vec::new();
    let mut head: FlowOrdMap<FlowSmolStr, Option<value_object::Property<Cx>>> =
        FlowOrdMap::default();
    let mut remainder_value = value_props.dupe();

    let mut early_stop: Option<FilterObjectResult> = None;
    for key in pattern_keys.iter() {
        let pattern_prop = pattern_props.get(key).unwrap();
        let pattern = &pattern_prop.value;
        let prop_value = value_props.get(key).unwrap().as_ref().unwrap();
        let loc_value = &prop_value.loc;
        let value = &prop_value.value;
        let optional = prop_value.optional;
        remainder_value.remove(key);

        let (value_left, value_matched, value_matched_is_empty, new_used_pattern_locs) =
            match pattern.only_wildcard() {
                Some(wildcard_reason) => {
                    let mut locs = ALocSet::new();
                    locs.insert(wildcard_reason.loc().dupe());
                    (value_union::ValueUnion::empty(), value.dupe(), false, locs)
                }
                None => {
                    let forced_value = value.get_forced(cx).as_ref().map_err(Dupe::dupe)?;
                    let FilterUnionResult {
                        value_left,
                        value_matched,
                        used_pattern_locs: new_used_pattern_locs,
                    } = filter_values_by_patterns(cx, raise_errors, forced_value, pattern)?;
                    let is_empty = value_matched.is_empty();
                    let matched_lazy: value_object::LazyValueUnion<Cx> =
                        // We need to create an already forced value to match ocaml behavior
                        // of creating already forced value for `lazy expr`, where expr is
                        // an already computed expression.
                        std::rc::Rc::new(flow_lazy::Lazy::new_forced(Ok(value_matched)));
                    (value_left, matched_lazy, is_empty, new_used_pattern_locs)
                }
            };

        used_pattern_locs.extend(new_used_pattern_locs.iter().map(|l| l.dupe()));
        let property_matched = value_object::Property {
            loc: loc_value.dupe(),
            value: value_matched,
            optional,
        };
        head.insert(key.dupe(), Some(property_matched));

        if no_match || value_matched_is_empty {
            if new_used_pattern_locs.is_empty() {
                early_stop = Some(FilterObjectResult::NoMatch {
                    used_pattern_locs: ALocSet::new(),
                    left: value_object.dupe(),
                });
                break;
            } else {
                no_match = true;
            }
        } else if !optional && value_left.is_empty() {
            // Full match
        } else {
            // Partial match .
            let (props_left_entry, rest) = if value_left.is_empty() {
                // Optional value is ignored from left pattern, but then there are additional,
                // unknown properties.
                let rest = if value_rest.is_some() {
                    value_rest.dupe()
                } else {
                    Some(t.dupe())
                };
                (None, rest)
            } else {
                let left_lazy: value_object::LazyValueUnion<Cx> =
                    // We need to create an already forced value to match ocaml behavior
                    // of creating already forced value for `lazy expr`, where expr is
                    // an already computed expression.
                    std::rc::Rc::new(flow_lazy::Lazy::new_forced(Ok(value_left)));
                let property_left = value_object::Property {
                    loc: loc_value.dupe(),
                    value: left_lazy,
                    optional,
                };
                (Some(property_left), value_rest.dupe())
            };

            let mut props_left = head.dupe();
            props_left.insert(key.dupe(), props_left_entry);
            for (k, v) in remainder_value.iter() {
                props_left.entry(k.dupe()).or_insert_with(|| v.dupe());
            }

            let object_left = value_object::ValueObject(
                reason_value.dupe(),
                std::rc::Rc::new(value_object::ValueObjectInner {
                    props: props_left,
                    class_info: value_class_info.dupe(),
                    rest,
                    t: t.dupe(),
                    kind: *value_kind,
                    sentinel_props: sentinel_props.dupe(),
                }),
            );
            queue_additions.push(object_left);
        }
    }

    if let Some(result) = early_stop {
        return Ok((result, subclass_covered.dupe()));
    }

    let pattern_loc = reason_pattern.loc();
    used_pattern_locs.insert(pattern_loc.dupe());

    if value_rest.is_some() || !value_object::properties::is_empty(&remainder_value) {
        match pattern_rest {
            None => {
                if *pattern_kind == ObjKind::Obj {
                    let missing_props: Vec<FlowSmolStr> = remainder_value
                        .iter()
                        .filter(|(_, prop)| prop.is_some())
                        .map(|(key, _)| key.dupe())
                        .collect();
                    if raise_errors {
                        flow_js::add_output_non_speculating(
                            cx,
                            ErrorMessage::EMatchError(MatchErrorKind::MatchNonExhaustiveObjectPattern(Box::new(MatchNonExhaustiveObjectPatternData {
                                loc: reason_pattern.loc().dupe(),
                                rest: value_rest.as_ref().map(flow_js_utils::type_reference_for_error),
                                missing_props,
                                pattern_kind: match pattern_class_info {
                                    Some(_) => flow_typing_errors::intermediate_error_types::MatchObjPatternKind::Instance,
                                    None => flow_typing_errors::intermediate_error_types::MatchObjPatternKind::Object,
                                },
                            }))),
                        );
                    }
                }
            }
            Some(pattern_rest_reason) => {
                used_pattern_locs.insert(pattern_rest_reason.loc().dupe());
            }
        }
    }

    let non_matching_rest =
        *pattern_kind != ObjKind::Obj && value_rest.is_some() && pattern_rest.is_none();

    if no_match || non_matching_rest {
        Ok((
            FilterObjectResult::NoMatch {
                used_pattern_locs,
                left: value_object.dupe(),
            },
            subclass_covered.dupe(),
        ))
    } else {
        let matched = value_object::ValueObject(
            reason_value.dupe(),
            std::rc::Rc::new(value_object::ValueObjectInner {
                props: head,
                class_info: value_class_info.dupe(),
                rest: value_rest.dupe(),
                t: t.dupe(),
                kind: *value_kind,
                sentinel_props: sentinel_props.dupe(),
            }),
        );
        Ok((
            FilterObjectResult::Match {
                used_pattern_locs,
                queue_additions,
                matched,
            },
            subclass_covered.dupe(),
        ))
    }
}

/// mixed/any values mark object and tuple patterns as used.
fn visit_mixed<'cx>(
    cx: &mut Context<'cx>,
    raise_errors: bool,
    t: &Type,
    pattern_union: &pattern_union::PatternUnion,
) -> Result<ALocSet, JobError> {
    let reason = reason_of_t(t);
    let pattern_tuples_exact = &pattern_union.tuples_exact;
    let pattern_tuples_inexact = &pattern_union.tuples_inexact;
    let pattern_objects = &pattern_union.objects;

    let no_object_patterns = pattern_objects.is_empty();

    let array_used_pattern_locs = if no_object_patterns
        && pattern_tuples_exact.is_empty()
        && pattern_tuples_inexact.is_empty()
    {
        ALocSet::new()
    } else {
        // As a shorthand to mark tuple patterns as used, if our value is mixed/any, we
        // can match against `ReadonlyArray<mixed>`
        let mixed_t = flow_typing_type::type_::mixed_t::make(reason.dupe());
        let arr_t = Type::new(TypeInner::DefT(
            reason.dupe(),
            flow_typing_type::type_::DefT::new(DefTInner::ArrT(std::rc::Rc::new(
                flow_typing_type::type_::ArrType::ROArrayAT(Box::new(ROArrayATData {
                    elem_t: mixed_t,
                    react_dro: None,
                    literal: false,
                })),
            ))),
        ));
        let mixed_array = value_object::ValueObject(
            reason.dupe(),
            std::rc::Rc::new(value_object::ValueObjectInner {
                kind: ObjKind::Obj,
                t: arr_t,
                class_info: None,
                props: FlowOrdMap::default(),
                rest: Some(t.dupe()),
                sentinel_props: FlowOrdSet::default(),
            }),
        );
        let all_pattern_objects = pattern_union.all_tuples_and_objects();
        let (_, _, used_pattern_locs) =
            filter_objects_by_patterns(cx, raise_errors, &[mixed_array], &all_pattern_objects)?;
        used_pattern_locs
    };

    let object_used_pattern_locs = if no_object_patterns {
        ALocSet::new()
    } else {
        // As a shorthand to mark object patterns as used, if our value is mixed/any, we
        // can match against `{+[string]: mixed}`
        let id =
            cx.generate_property_map(flow_typing_type::type_::properties::PropertiesMap::new());
        let flags = flow_typing_type::type_::Flags {
            obj_kind: flow_typing_type::type_::ObjKind::Indexed(
                flow_typing_type::type_::DictType {
                    dict_name: None,
                    key: flow_typing_type::type_::str_module_t::make(reason.dupe()),
                    value: flow_typing_type::type_::mixed_t::make(reason.dupe()),
                    dict_polarity: flow_common::polarity::Polarity::Positive,
                },
            ),
            react_dro: None,
        };
        let obj_type = flow_typing_type::type_::mk_objecttype(
            Some(flags),
            None,
            None,
            id,
            flow_typing_type::type_::dummy_prototype(),
            false,
        );
        let obj_t = Type::new(TypeInner::DefT(
            reason.dupe(),
            flow_typing_type::type_::DefT::new(DefTInner::ObjT(std::rc::Rc::new(obj_type))),
        ));
        let mixed_object = value_object::ValueObject(
            reason.dupe(),
            std::rc::Rc::new(value_object::ValueObjectInner {
                kind: ObjKind::Obj,
                t: obj_t,
                props: FlowOrdMap::default(),
                class_info: None,
                rest: Some(t.dupe()),
                sentinel_props: FlowOrdSet::default(),
            }),
        );
        let (_, _, used_pattern_locs) =
            filter_objects_by_patterns(cx, raise_errors, &[mixed_object], pattern_objects)?;
        used_pattern_locs
    };

    let mut result = array_used_pattern_locs;
    result.extend(object_used_pattern_locs);
    Ok(result)
}

fn mark_leaf_pattern_used<'cx>(
    cx: &mut Context<'cx>,
    value_leafs_matched: &LeafSet,
    inexhaustible: &FlowVector<Type>,
    leaf_pattern: &leaf::Leaf,
    used_pattern_locs: &mut ALocSet,
) -> Result<(), JobError> {
    let reason = &leaf_pattern.0;
    let loc = reason.loc();
    if value_leafs_matched.contains(leaf_pattern)
        || is_leaf_subtype_of_inexhaustible(cx, leaf_pattern, inexhaustible)?
    {
        used_pattern_locs.insert(loc.dupe());
    }
    Ok(())
}

// ***************************
// * Unused pattern analysis *
// ***************************

/// We've accumulated a set of locs of every pattern that was used in some way.
/// Now we iterate through the patterns, and we error on the outermost unused patterns.
/// If there are patterns which contain invalid patterns, for which we have already emitted
/// other errors, do not also emit an unused pattern error.
fn check_for_unused_patterns<'cx>(
    cx: &mut Context<'cx>,
    pattern_union: &pattern_union::PatternUnion,
    used_pattern_locs: &ALocSet,
) {
    let check = |reason: &Reason| -> bool {
        let loc = reason.loc();
        used_pattern_locs.contains(loc)
    };
    let error = |cx: &mut Context<'cx>, reason: &Reason, kind: MatchPatternReferenceKind| {
        flow_js::add_output_non_speculating(
            cx,
            ErrorMessage::EMatchError(MatchErrorKind::MatchUnusedPattern(Box::new(
                MatchUnusedPatternData {
                    pattern: MatchPatternReferenceData {
                        loc: reason.loc().dupe(),
                        kind,
                    },
                    already_seen: None,
                },
            ))),
        )
    };
    for leaf_val in &pattern_union.leafs {
        let reason = &leaf_val.0;
        if !check(reason) {
            error(cx, reason, MatchPatternReferenceKind::Pattern);
        }
    }
    for leaf_val in &pattern_union.guarded_leafs {
        let reason = &leaf_val.0;
        if !check(reason) {
            error(cx, reason, MatchPatternReferenceKind::Pattern);
        }
    }
    for (_, po) in pattern_union.all_tuples_and_objects() {
        let reason = &po.0;
        let inner = po.1.as_ref();
        let props = &inner.props;
        let rest = &inner.rest;
        let contains_invalid_pattern = inner.contains_invalid_pattern;
        if contains_invalid_pattern {
            continue;
        }
        if !check(reason) {
            error(cx, reason, MatchPatternReferenceKind::Pattern);
        } else {
            for (_, prop) in props.iter() {
                check_for_unused_patterns(cx, &prop.value, used_pattern_locs);
                if let Some(rest_reason) = rest {
                    if !check(rest_reason) {
                        let kind = match inner.kind {
                            ObjKind::Tuple { .. } => MatchPatternReferenceKind::ArrayRest,
                            ObjKind::Obj => MatchPatternReferenceKind::ObjectRest,
                        };
                        error(cx, rest_reason, kind);
                    }
                }
            }
        }
    }
    if let Some(ref wildcard) = pattern_union.wildcard {
        if !check(wildcard) {
            error(cx, wildcard, MatchPatternReferenceKind::Wildcard);
        }
    }
}

// ***************
// * Entry point *
// ***************

/// If there is no values left over after filtering by the patterns, the check is exhaustive.
/// Otherwise, build up examples of patterns that could be added to make the `match` exhaustive.
/// Then, check the patterns to see if any are unused.
pub fn analyze<'cx>(
    cx: &mut Context<'cx>,
    is_global_var: &dyn Fn(&Context<'cx>, ALoc) -> bool,
    match_loc: ALoc,
    patterns: &match_pattern_ir::PatternAstList,
    arg_t: &Type,
) -> Result<(), JobError> {
    let pattern_union = pattern_union_builder::of_patterns_ast(cx, is_global_var, true, patterns)?;
    let value_union = value_union_builder::of_type(cx, arg_t)?;
    let FilterUnionResult {
        value_left,
        value_matched,
        used_pattern_locs,
    } = filter_values_by_patterns(cx, true, &value_union, &pattern_union)?;
    if value_left.is_empty() {
        let enum_unknown_members = &value_matched.enum_unknown_members;
        let pattern_leafs = &pattern_union.leafs;
        let wildcard = &pattern_union.wildcard;
        if let Some(wildcard_reason) = wildcard {
            for (_, _, enum_leafs) in enum_unknown_members.iter() {
                if !enum_leafs.is_subset(&**pattern_leafs) {
                    let unchecked_members: Vec<FlowSmolStr> = enum_leafs
                        .iter()
                        .filter(|leaf| !pattern_leafs.contains(leaf))
                        .map(|leaf| leaf.1.to_string().into())
                        .collect();
                    flow_js::add_output_non_speculating(
                        cx,
                        ErrorMessage::EMatchError(MatchErrorKind::MatchNonExplicitEnumCheck(
                            Box::new(MatchNonExplicitEnumCheckData {
                                loc: match_loc.dupe(),
                                wildcard_loc: wildcard_reason.loc().dupe(),
                                unchecked_members,
                            }),
                        )),
                    );
                }
            }
        }
    } else {
        let value_union::ValueUnion {
            leafs,
            tuples,
            arrays,
            objects,
            enum_unknown_members,
            inexhaustible,
        } = &value_left;
        let mut examples: Vec<(
            FlowSmolStr,
            Vec<MatchExampleReference<ALoc, ErrorTypeReferenceWithLocData<ALoc>>>,
        )> = Vec::new();
        let mut asts: Vec<(Loc, ast::match_pattern::MatchPattern<Loc, Loc>)> = Vec::new();
        for leaf in leafs.iter() {
            let leaf::Leaf(reason, leaf_ctor) = leaf;
            let example: FlowSmolStr = leaf_ctor.to_string().into();
            let reference = match leaf_ctor {
                leaf::LeafCtor::EnumMemberC(leaf::EnumMember {
                    enum_info,
                    member_name,
                }) => MatchExampleReference::EnumMember {
                    loc: reason
                        .annot_loc()
                        .unwrap_or_else(|| reason.def_loc())
                        .dupe(),
                    member_name: member_name.dupe(),
                    enum_name: enum_info.enum_name.dupe(),
                },
                _ => MatchExampleReference::Type(
                    flow_js_utils::type_reference_with_reason_for_error(
                        &leaf.to_type(),
                        reason.dupe(),
                    ),
                ),
            };
            examples.push((example, vec![reference]));
            asts.push(leaf_ctor.to_ast());
        }

        // Sort examples based on their original order
        let mut tuple_patterns: Vec<(pattern_object::PatternObject, Type)> = tuples
            .iter()
            .map(|vo| Ok((vo.to_pattern(cx)?, vo.to_original_type())))
            .collect::<Result<_, JobError>>()?;
        tuple_patterns.sort_by(|(p1, _), (p2, _)| p1.cmp(p2));
        let mut tuple_examples_map: BTreeMap<
            FlowSmolStr,
            (usize, pattern_object::PatternObject, BTreeMap<Reason, Type>),
        > = BTreeMap::new();
        for (i, (tuple_pattern, tuple_t)) in tuple_patterns.into_iter().enumerate() {
            let pattern_object::PatternObject(ref reason, _) = tuple_pattern;
            let example: FlowSmolStr = tuple_pattern.to_string().into();
            tuple_examples_map
                .entry(example)
                .and_modify(|(_, _, types_by_reason)| {
                    types_by_reason.insert(reason.dupe(), tuple_t.dupe());
                })
                .or_insert_with(|| {
                    let mut types_by_reason = BTreeMap::new();
                    types_by_reason.insert(reason.dupe(), tuple_t);
                    (i, tuple_pattern.dupe(), types_by_reason)
                });
        }
        // Add the the pattern that matches all arrays to the tuple examples map
        if let Some(value_object::ValueObject(reason, _)) = arrays.front() {
            let types_by_reason: BTreeMap<Reason, Type> = arrays
                .iter()
                .map(|value_object| (value_object.0.dupe(), value_object.to_original_type()))
                .collect();
            let i = tuples.len();
            let pattern = match_pattern_ir::empty_inexact_tuple_pattern(reason.dupe());
            let example: FlowSmolStr = pattern.to_string().into();
            tuple_examples_map
                .entry(example)
                .and_modify(|(_, _, existing_types_by_reason)| {
                    existing_types_by_reason
                        .extend(types_by_reason.iter().map(|(r, t)| (r.dupe(), t.dupe())));
                })
                .or_insert_with(|| (i, pattern, types_by_reason));
        }
        // Turn the map into a list of examples
        let mut tuple_entries: Vec<(
            FlowSmolStr,
            (usize, pattern_object::PatternObject, BTreeMap<Reason, Type>),
        )> = tuple_examples_map.into_iter().collect();
        tuple_entries.sort_by_key(|(_, (a, _, _))| *a);
        for (example, (_, pattern, types_by_reason)) in tuple_entries {
            let type_refs = types_by_reason
                .into_iter()
                .map(|(reason, t)| {
                    MatchExampleReference::Type(
                        flow_js_utils::type_reference_with_reason_for_error(&t, reason),
                    )
                })
                .collect();
            examples.push((example, type_refs));
            asts.push(pattern.to_ast());
        }
        // Compute the list of object examples
        let mut object_patterns: Vec<(pattern_object::PatternObject, Type)> = objects
            .iter()
            .map(|vo| Ok((vo.to_pattern(cx)?, vo.to_original_type())))
            .collect::<Result<_, JobError>>()?;
        object_patterns.sort_by(|(p1, _), (p2, _)| p1.cmp(p2));
        let mut object_examples_map: BTreeMap<
            FlowSmolStr,
            (usize, pattern_object::PatternObject, BTreeMap<Reason, Type>),
        > = BTreeMap::new();
        for (i, (object_pattern, object_t)) in object_patterns.into_iter().enumerate() {
            let pattern_object::PatternObject(ref reason, _) = object_pattern;
            let example: FlowSmolStr = object_pattern.to_string().into();
            object_examples_map
                .entry(example)
                .and_modify(|(_, _, types_by_reason)| {
                    types_by_reason.insert(reason.dupe(), object_t.dupe());
                })
                .or_insert_with(|| {
                    let mut types_by_reason = BTreeMap::new();
                    types_by_reason.insert(reason.dupe(), object_t);
                    (i, object_pattern.dupe(), types_by_reason)
                });
        }
        let mut object_entries: Vec<(
            FlowSmolStr,
            (usize, pattern_object::PatternObject, BTreeMap<Reason, Type>),
        )> = object_examples_map.into_iter().collect();
        object_entries.sort_by_key(|(_, (a, _, _))| *a);
        for (example, (_, pattern, types_by_reason)) in object_entries {
            let type_refs = types_by_reason
                .into_iter()
                .map(|(reason, t)| {
                    MatchExampleReference::Type(
                        flow_js_utils::type_reference_with_reason_for_error(&t, reason),
                    )
                })
                .collect();
            examples.push((example, type_refs));
            asts.push(pattern.to_ast());
        }
        let wildcard_example =
            |reason: Reason,
             mut examples: Vec<(
                FlowSmolStr,
                Vec<MatchExampleReference<ALoc, ErrorTypeReferenceWithLocData<ALoc>>>,
            )>,
             mut asts: Vec<(Loc, ast::match_pattern::MatchPattern<Loc, Loc>)>|
             -> (
                Vec<(
                    FlowSmolStr,
                    Vec<MatchExampleReference<ALoc, ErrorTypeReferenceWithLocData<ALoc>>>,
                )>,
                Vec<(Loc, ast::match_pattern::MatchPattern<Loc, Loc>)>,
            ) {
                let pattern = match_pattern_ir::wildcard_pattern(reason);
                let example: FlowSmolStr = pattern.to_string().into();
                let mut refs_by_reason: BTreeMap<
                    Reason,
                    MatchExampleReference<ALoc, ErrorTypeReferenceWithLocData<ALoc>>,
                > = inexhaustible
                    .iter()
                    .map(|t| {
                        let reason = reason_of_t(t).dupe();
                        let reference = MatchExampleReference::Type(
                            flow_js_utils::type_reference_with_reason_for_error(t, reason.dupe()),
                        );
                        (reason, reference)
                    })
                    .collect();
                for (r, enum_name, _) in enum_unknown_members.iter() {
                    refs_by_reason.insert(
                        r.dupe(),
                        MatchExampleReference::EnumUnknownMembers {
                            loc: r.annot_loc().unwrap_or_else(|| r.def_loc()).dupe(),
                            enum_name: enum_name.dupe(),
                        },
                    );
                }
                examples.push((example, refs_by_reason.into_values().collect()));
                asts.push(pattern.to_ast());
                (examples, asts)
            };

        let (examples, asts) = match (inexhaustible.front(), enum_unknown_members.front()) {
            (None, None) => (examples, asts),
            (Some(first_t), _) => wildcard_example(reason_of_t(first_t).dupe(), examples, asts),
            (None, Some((reason, _, _))) => wildcard_example(reason.dupe(), examples, asts),
        };

        let missing_pattern_asts: Vec<ast::match_pattern::MatchPattern<Loc, Loc>> =
            asts.into_iter().take(25).map(|(_, pat)| pat).collect();
        flow_js::add_output_non_speculating(
            cx,
            ErrorMessage::EMatchError(MatchErrorKind::MatchNotExhaustive(Box::new(
                MatchNotExhaustiveData {
                    loc: match_loc.dupe(),
                    examples,
                    missing_pattern_asts,
                },
            ))),
        );
    }
    check_for_unused_patterns(cx, &pattern_union, &used_pattern_locs);
    Ok(())
}

/// Filter a type by a finalized PatternUnion
pub fn filter_by_pattern_union<'cx>(
    cx: &mut Context<'cx>,
    root_t: &Type,
    pattern_union: &pattern_union::PatternUnion,
) -> Result<value_union::ValueUnion<Cx>, JobError> {
    let value_union = value_union_builder::of_type(cx, root_t)?;
    let FilterUnionResult {
        value_left,
        value_matched: _,
        used_pattern_locs: _,
    } = filter_values_by_patterns(cx, false, &value_union, pattern_union)?;
    Ok(value_left)
}
