/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::ops::Deref;

use dupe::Dupe;
use flow_common::polarity::Polarity;
use flow_common::reason::Reason;
use flow_typing_context::Context;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_type::type_::ImplicitInstantiationTvarData;
use flow_typing_type::type_::ImplicitInstantiationTvarKind;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::constraint::Constraints;
use flow_typing_type::type_::constraint::forcing_state::ForcingState;
use flow_typing_type::type_::empty_t;
use flow_typing_type::type_::union_rep::UnionKind;
use flow_typing_type::type_util;
use flow_typing_visitors::type_visitor;
use flow_typing_visitors::type_visitor::TypeVisitor;

pub fn default_no_lowers(r: &Reason) -> Type {
    empty_t::why(r.dupe())
}

struct Resolver<F>
where
    F: Fn(&Reason) -> Type,
{
    no_lowers: F,
    filter_empty: bool,
}

type Acc = (BTreeMap<i32, bool>, bool);

impl<F> Resolver<F>
where
    F: Fn(&Reason) -> Type,
{
    fn constraint_node<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        seen: Acc,
        r: &Reason,
        id: i32,
    ) -> Acc {
        let (root_id, constraints) = cx.find_constraints(id);
        match constraints {
            Constraints::FullyResolved(_) => seen,
            Constraints::Unresolved(_) | Constraints::Resolved(_) => {
                let (mut seen_tvars, seen_placeholders) = seen;
                if let Some(&seen_placeholders_in_map) = seen_tvars.get(&root_id) {
                    return (seen_tvars, seen_placeholders_in_map);
                }
                seen_tvars.insert(root_id, false);
                let t = match flow_js_utils::merge_tvar_opt(
                    cx,
                    self.filter_empty,
                    UnionKind::ResolvedKind,
                    r,
                    root_id,
                ) {
                    Some(t) => t,
                    None => (self.no_lowers)(r),
                };
                let (mut seen_tvars, seen_placeholders_in_t) =
                    self.type_(cx, pole, (seen_tvars, false), &t);
                seen_tvars.insert(root_id, seen_placeholders_in_t);
                let constraints = if seen_placeholders_in_t {
                    Constraints::Resolved(t)
                } else {
                    Constraints::FullyResolved(ForcingState::of_non_lazy_t(t))
                };
                cx.set_constraints(root_id, constraints);
                (seen_tvars, seen_placeholders || seen_placeholders_in_t)
            }
        }
    }
}

impl<F> TypeVisitor<Acc> for Resolver<F>
where
    F: Fn(&Reason) -> Type,
{
    fn type_<'cx>(&mut self, cx: &mut Context<'cx>, pole: Polarity, seen: Acc, t: &Type) -> Acc {
        match t.deref() {
            TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
                kind: ImplicitInstantiationTvarKind::Placeholder,
                ..
            }) => {
                let (seen_tvars, _) = seen;
                (seen_tvars, true)
            }
            _ if let Some(node) = type_util::constraint_node_id(t) => {
                self.constraint_node(cx, pole, seen, type_util::reason_of_t(t), node.id())
            }
            _ => type_visitor::type_default(self, cx, pole, seen, t),
        }
    }

    fn tvar<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        seen: Acc,
        r: &Reason,
        id: u32,
    ) -> Acc {
        self.constraint_node(cx, pole, seen, r, id as i32)
    }
}

pub fn resolve<'cx, F>(cx: &mut Context<'cx>, no_lowers: F, filter_empty: bool, t: &Type)
where
    F: Fn(&Reason) -> Type,
{
    let mut resolver = Resolver {
        no_lowers,
        filter_empty,
    };
    resolver.type_(cx, Polarity::Positive, (BTreeMap::new(), false), t);
}

pub fn resolved_t<'cx, F>(no_lowers: F, filter_empty: bool, cx: &mut Context<'cx>, t: Type) -> Type
where
    F: Fn(&Reason) -> Type,
{
    resolve(cx, no_lowers, filter_empty, &t);
    t
}

struct AbortableResolver {
    filter_empty: bool,
    aborted: bool,
}

impl AbortableResolver {
    fn constraint_node<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        seen: Acc,
        r: &Reason,
        id: i32,
    ) -> Acc {
        if self.aborted {
            return seen;
        }
        let (root_id, constraints) = cx.find_constraints(id);
        match constraints {
            Constraints::FullyResolved(_) => seen,
            Constraints::Unresolved(_) | Constraints::Resolved(_) => {
                let (mut seen_tvars, seen_placeholders) = seen;
                if let Some(&seen_placeholders_in_map) = seen_tvars.get(&root_id) {
                    return (seen_tvars, seen_placeholders_in_map);
                }
                seen_tvars.insert(root_id, false);
                let t = match flow_js_utils::merge_tvar_opt(
                    cx,
                    self.filter_empty,
                    UnionKind::ResolvedKind,
                    r,
                    root_id,
                ) {
                    Some(t) => t,
                    None => {
                        self.aborted = true;
                        return (seen_tvars, seen_placeholders);
                    }
                };
                let (mut seen_tvars, seen_placeholders_in_t) =
                    self.type_(cx, pole, (seen_tvars, false), &t);
                if self.aborted {
                    return (seen_tvars, seen_placeholders);
                }
                seen_tvars.insert(root_id, seen_placeholders_in_t);
                let constraints = if seen_placeholders_in_t {
                    Constraints::Resolved(t)
                } else {
                    Constraints::FullyResolved(ForcingState::of_non_lazy_t(t))
                };
                cx.set_constraints(root_id, constraints);
                (seen_tvars, seen_placeholders || seen_placeholders_in_t)
            }
        }
    }
}

impl TypeVisitor<Acc> for AbortableResolver {
    fn type_<'cx>(&mut self, cx: &mut Context<'cx>, pole: Polarity, seen: Acc, t: &Type) -> Acc {
        if self.aborted {
            return seen;
        }
        match t.deref() {
            TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
                kind: ImplicitInstantiationTvarKind::Placeholder,
                ..
            }) => {
                let (seen_tvars, _) = seen;
                (seen_tvars, true)
            }
            _ if let Some(node) = type_util::constraint_node_id(t) => {
                self.constraint_node(cx, pole, seen, type_util::reason_of_t(t), node.id())
            }
            _ => type_visitor::type_default(self, cx, pole, seen, t),
        }
    }

    fn tvar<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        seen: Acc,
        r: &Reason,
        id: u32,
    ) -> Acc {
        self.constraint_node(cx, pole, seen, r, id as i32)
    }
}

pub fn resolved_t_abortable<'cx>(
    filter_empty: bool,
    cx: &mut Context<'cx>,
    t: Type,
) -> Option<Type> {
    let mut resolver = AbortableResolver {
        filter_empty,
        aborted: false,
    };
    resolver.type_(cx, Polarity::Positive, (BTreeMap::new(), false), &t);
    if resolver.aborted { None } else { Some(t) }
}

pub fn mk_tvar_and_fully_resolve_where<'cx, E>(
    cx: &mut Context<'cx>,
    reason: Reason,
    f: impl FnOnce(&mut Context<'cx>, &Type) -> Result<(), E>,
) -> Result<Type, E> {
    let tvar = flow_typing_tvar::mk_where(cx, reason, f)?;
    resolve(cx, default_no_lowers, true, &tvar);
    Ok(tvar)
}

pub fn mk_tvar_and_fully_resolve_no_wrap_where<'cx, E>(
    cx: &mut Context<'cx>,
    reason: Reason,
    f: impl FnOnce(&mut Context<'cx>, &Reason, i32) -> Result<(), E>,
) -> Result<Type, E> {
    let tvar = flow_typing_tvar::mk_no_wrap_where(cx, reason, f)?;
    resolve(cx, default_no_lowers, true, &tvar);
    Ok(tvar)
}
