/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Normalization of intersection annotations, gated by
//! `experimental.normalize_intersections`.
//!
//! An intersection annotation without free type params is wrapped in an `EvalT`
//! with `Destructor::NormalizeIntersection`. Evaluating it replaces the
//! intersection with a single type when its concrete members allow it:
//!
//! - Scalars (`scalars`): `string & 'a'` is `'a'`, `number & number` is
//!   `number`, and `'a' & 'b'` or `string & number` is `empty`.
//! - Objects and interfaces (`objects`): `{a: A, ...} & {b: B, ...}` is
//!   `{a: A, b: B, ...}`. A prop in several members gets the intersection of
//!   their types, itself normalized by a nested `EvalT`.
//!
//! Any other intersection evaluates to itself.

use std::collections::VecDeque;
use std::ops::Deref;
use std::rc::Rc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_aloc::ALoc;
use flow_common::reason::Reason;
use flow_typing_context::Context;
use flow_typing_errors::error_message::EInvalidExactObjectIntersectionData;
use flow_typing_flow_common::flow_cache;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_common::flow_js_utils::tvar_visitors;
use flow_typing_flow_common::type_subst;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_type::type_::Destructor;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeDestructorT;
use flow_typing_type::type_::TypeDestructorTInner;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::eval;
use flow_typing_type::type_::inter_rep;
use flow_typing_type::type_::unknown_use;

use crate::flow_js::FlowJs;

mod objects;
mod scalars;

/// Unsupported intersections retain their original representation. Exact Flow
/// objects forbid the properties they do not declare, so an intersection of
/// Flow types is invalid when one member requires a property that an exact
/// member lacks. It still normalizes to the merge of its members.
pub enum Normalization {
    Normalized(Type),
    Unchanged,
    InvalidExact(Type, Box<EInvalidExactObjectIntersectionData<ALoc>>),
}

/// Normalization shares one reason across members and nested properties.
pub struct Normalizer<'a> {
    env: &'a FlowJsEnv,
    reason: &'a Reason,
}

impl<'a> Normalizer<'a> {
    /// The environment supplies the equality rules used when combining members.
    pub fn new(env: &'a FlowJsEnv, reason: &'a Reason) -> Self {
        Self { env, reason }
    }

    /// `concretize` resolves one member to its concrete types: the constraint
    /// system in `eval_destructor`, annotation inference after sig merge.
    /// Nested intersections are flattened, so `A & (B & C)` normalizes like
    /// `A & B & C`.
    ///
    /// Returns `Unchanged` when the intersection should stay as written: a
    /// member does not concretize to exactly one type (e.g. a union or maybe),
    /// or the members are not all scalars or all supported objects.
    pub fn normalize<'cx, E>(
        &self,
        cx: &mut Context<'cx>,
        t: &Type,
        mut concretize: impl FnMut(&mut Context<'cx>, &Reason, &Type) -> Result<Vec<Type>, E>,
    ) -> Result<Normalization, E> {
        let TypeInner::IntersectionT(_, rep) = t.deref() else {
            return Ok(Normalization::Unchanged);
        };
        let Some(members) = concretize_members(rep.members_iter().duped(), |member| {
            concretize(cx, self.reason, member)
        })?
        else {
            return Ok(Normalization::Unchanged);
        };
        if let Some(t) = scalars::normalize(cx, self, &members) {
            return Ok(Normalization::Normalized(t));
        }
        objects::normalize(cx, self, &members, &mut concretize)
    }
}

fn concretize_members<E>(
    members: impl IntoIterator<Item = Type>,
    mut concretize: impl FnMut(&Type) -> Result<Vec<Type>, E>,
) -> Result<Option<Vec<Type>>, E> {
    let mut todo: VecDeque<Type> = members.into_iter().collect();
    let mut result = Vec::new();
    while let Some(member) = todo.pop_front() {
        let concrete = concretize(&member)?;
        let [concrete] = concrete.as_slice() else {
            return Ok(None);
        };
        match concrete.deref() {
            TypeInner::IntersectionT(_, rep) => {
                let nested: Vec<Type> = rep.members_iter().duped().collect();
                for nested in nested.into_iter().rev() {
                    todo.push_front(nested);
                }
            }
            _ => result.push(concrete.dupe()),
        }
    }
    Ok(Some(result))
}

/// An intersection over type params gets no `EvalT`, and substitution maps over
/// the plain `IntersectionT`, so instantiations of a generic alias such as
/// `type G<T> = T & {...}` are never normalized either.
fn should_normalize<'cx>(cx: &mut Context<'cx>, intersection: &Type) -> bool {
    cx.normalize_intersections() && type_subst::free_var_finder(cx, None, intersection).is_empty()
}

/// Signature merging defers evaluation until the imported members resolve.
/// `annotation_inference::elab_t` evaluates the `EvalT` on demand.
pub fn defer_annotation<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    intersection: Type,
    id: impl FnOnce(&mut Context<'cx>) -> eval::Id,
) -> Type {
    if should_normalize(cx, &intersection) {
        make_eval(reason, intersection, id(cx))
    } else {
        intersection
    }
}

/// Local annotations evaluate immediately when all their members are resolved.
/// Otherwise the `EvalT` is left for `eval_destructor` once they are.
pub fn evaluate_annotation<'cx>(
    cx: &mut Context<'cx>,
    reason: &Reason,
    intersection: Type,
    id: impl FnOnce(&mut Context<'cx>) -> eval::Id,
) -> Result<Type, FlowJsException> {
    if !should_normalize(cx, &intersection) {
        return Ok(intersection);
    }
    let id = id(cx);
    if tvar_visitors::has_unresolved_tvars(cx, &intersection)
        || tvar_visitors::has_placeholders(cx, &intersection)
    {
        return Ok(make_eval(reason.dupe(), intersection, id));
    }
    FlowJs::mk_possibly_evaluated_destructor_for_annotations(
        cx,
        unknown_use(),
        reason,
        &intersection,
        &Destructor::NormalizeIntersection { check_exact: true },
        id,
    )
}

/// The type of a prop shared by several members. It is evaluated lazily, and
/// `flow_cache::eval::id` keys it on its operands, so normalizing `A & B` for
/// `type A = {next: A, ...}` and `type B = {next: B, ...}` gives an object
/// whose `next` is this same `EvalT` again rather than recursing forever.
///
/// `?L & ?R` becomes `?(L & R)`: as a member, a maybe concretizes to several
/// types, which would leave the whole intersection unnormalized.
fn make_recursive_eval<'cx>(
    cx: &mut Context<'cx>,
    reason: &Reason,
    left: Type,
    right: Type,
) -> Type {
    if let (TypeInner::MaybeT(_, left), TypeInner::MaybeT(_, right)) = (left.deref(), right.deref())
    {
        return Type::new(TypeInner::MaybeT(
            reason.dupe(),
            make_recursive_eval(cx, reason, left.dupe(), right.dupe()),
        ));
    }
    let intersection = Type::new(TypeInner::IntersectionT(
        reason.dupe(),
        inter_rep::make(left, right, Rc::from([])),
    ));
    let destructor = TypeDestructorT::new(TypeDestructorTInner(
        unknown_use(),
        reason.dupe(),
        Rc::new(Destructor::NormalizeIntersection { check_exact: false }),
    ));
    flow_cache::eval::id(cx, intersection, destructor)
}

fn make_eval(reason: Reason, intersection: Type, id: eval::Id) -> Type {
    Type::new(TypeInner::EvalT {
        type_: intersection,
        defer_use_t: TypeDestructorT::new(TypeDestructorTInner(
            unknown_use(),
            reason,
            Rc::new(Destructor::NormalizeIntersection { check_exact: true }),
        )),
        id,
    })
}
