/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;

use flow_typing_context::Context;
use flow_typing_context::Cx;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeDestructorT;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::UseT;
use flow_typing_type::type_::UseTInner;
use flow_typing_type::type_::VirtualRootUseOp;
use flow_typing_type::type_::VirtualUseOp;
use flow_typing_type::type_::eval as type_eval;
use flow_typing_type::type_::string_of_ctor;
use flow_typing_type::type_::string_of_use_ctor;
use flow_typing_type::type_util::mod_use_op_of_use_t;
use flow_typing_type::type_util::reason_of_t;

// Cache that remembers pairs of types that are passed to __flow.
pub mod flow_constraint {
    use std::ops::Deref;

    use dupe::Dupe;

    use super::*;

    fn toplevel_use_op(use_op: &UseOp) -> UseOp {
        match &use_op {
            VirtualUseOp::Frame(_, inner_use_op) => toplevel_use_op(inner_use_op),
            VirtualUseOp::Op(root) => {
                if let VirtualRootUseOp::Speculation(inner) = &**root {
                    toplevel_use_op(inner)
                } else {
                    use_op.dupe()
                }
            }
        }
    }

    /// Returns `true` iff `toplevel_use_op` would return the same UseOp
    /// unchanged (i.e. it is already an `Op(non-Speculation root)`). The
    /// common case for non-speculation callers — checking this lets the
    /// outer cache lookup skip the `mod_use_op_of_use_t` allocation that
    /// would otherwise produce a structurally-identical `UseT`.
    fn is_toplevel_use_op(use_op: &UseOp) -> bool {
        match use_op {
            VirtualUseOp::Frame(_, _) => false,
            VirtualUseOp::Op(root) => !matches!(&**root, VirtualRootUseOp::Speculation(_)),
        }
    }

    // attempt to read LB/UB pair from cache, add if absent
    pub fn get<'cx>(cx: &mut Context<'cx>, l: &Type, u: &UseT<Cx>) -> bool {
        match (l.deref(), u.deref()) {
            // Don't cache constraints involving type variables, since the
            // corresponding typing rules are already sufficiently robust.
            (_, _) if flow_typing_type::type_util::constraint_node_id(l).is_some() => false,
            (_, UseTInner::UseT(_, inner_t))
                if flow_typing_type::type_util::constraint_node_id(inner_t).is_some() =>
            {
                false
            }
            (_, UseTInner::ReposUseT(..)) => false,
            // Don't cache concretization constraints, since the concretization operation should already
            // have defense against bad cyclic types.
            (_, UseTInner::ConcretizeT(..)) => false,
            (
                _,
                UseTInner::CallT(..)
                | UseTInner::MethodT(..)
                | UseTInner::PrivateMethodT(..)
                | UseTInner::ConstructorT(..)
                | UseTInner::LookupT(..)
                | UseTInner::GetPropT(..)
                | UseTInner::ObjRestT { .. }
                | UseTInner::ArrRestT(..)
                | UseTInner::ValueToTypeReferenceT(..)
                | UseTInner::SpecializeT(..)
                | UseTInner::EvalTypeDestructorT(..)
                | UseTInner::ConcretizeTypeAppsT { .. }
                | UseTInner::ExtendsUseT(..)
                | UseTInner::ResolveUnionT(..)
                | UseTInner::ConditionalT(..)
                | UseTInner::DeepReadOnlyT { .. }
                | UseTInner::ElemT(..)
                | UseTInner::ReposLowerT { .. }
                | UseTInner::ObjKitT { .. }
                | UseTInner::HasOwnPropT(..)
                | UseTInner::ResolveSpreadT(..),
            ) => false,
            _ => {
                // Use ops are purely for better error messages: they should have no
                // effect on type checking. However, recursively nested use ops can pose
                // non-termination problems. To ensure proper caching, we hash use ops
                // to just their toplevel structure.
                //
                // Fast path: if every use_op the `UseT` carries is already
                // top-level, `mod_use_op_of_use_t` would allocate a fresh
                // `Rc<UseTInner>` whose contents are bit-for-bit identical
                // to the input. Skip the allocation in that case and reuse
                // `u.dupe()` (cheap Rc clone) for both the cache key and
                // the verbose-log path.
                let already_toplevel = match u.deref() {
                    UseTInner::UseT(use_op, _) => is_toplevel_use_op(use_op),
                    _ => false,
                };
                let u_owned = if already_toplevel {
                    u.dupe()
                } else {
                    mod_use_op_of_use_t(toplevel_use_op, u)
                };
                let found = !cx.constraint_cache_mut().add(l.dupe(), u_owned.dupe());
                if !found {
                    // New entry
                } else if cx.is_verbose() {
                    eprintln!(
                        "{}FlowConstraint cache hit on ({}, {})",
                        cx.pid_prefix(),
                        string_of_ctor(l),
                        string_of_use_ctor(&u_owned)
                    );
                }
                found
            }
        }
    }
}

pub mod eval {
    use dupe::Dupe;

    use super::*;

    pub fn id(cx: &mut Context, t: Type, defer_use: TypeDestructorT) -> Type {
        if let TypeInner::EvalT {
            defer_use_t: d,
            id: i,
            ..
        } = &*t
        {
            if d == &defer_use {
                if let Some(cached_t) = cx.eval_id_cache().get(i) {
                    return cached_t.dupe();
                }
                let i = type_eval::Id::generate_id();
                cx.eval_id_cache_mut().insert(i.dupe(), t.dupe());
                return Type::new(TypeInner::EvalT {
                    type_: t,
                    defer_use_t: defer_use,
                    id: i,
                });
            }
        }

        let cache_key = (t.dupe(), defer_use.dupe());
        let id = match cx.id_cache().get(&cache_key).cloned() {
            Some(i) => i,
            None => {
                let i = type_eval::Id::generate_id();
                cx.id_cache_mut().insert(cache_key, i.dupe());
                i
            }
        };
        Type::new(TypeInner::EvalT {
            type_: t,
            defer_use_t: defer_use,
            id,
        })
    }

    pub fn find_repos(
        cx: &Context,
        t: &Type,
        defer_use: &TypeDestructorT,
        id: &type_eval::Id,
    ) -> Option<Type> {
        let repos_cache = cx.eval_repos_cache();
        let cache_key = (t.dupe(), defer_use.dupe(), id.dupe());
        repos_cache.get(&cache_key).map(|t| t.dupe())
    }

    pub fn add_repos(
        cx: &mut Context,
        t: Type,
        defer_use: TypeDestructorT,
        id: type_eval::Id,
        tvar: Type,
    ) {
        let repos_cache = cx.eval_repos_cache_mut();
        let cache_key = (t, defer_use, id);
        repos_cache.insert(cache_key, tvar);
    }
}

pub mod fix {
    use dupe::Dupe;

    use super::*;

    pub fn find(cx: &Context, is_this: bool, i: &Type) -> Option<Type> {
        let cache = cx.fix_cache();
        let cache_key = (is_this, i.dupe());
        cache.get(&cache_key).map(|t| t.dupe())
    }

    pub fn add(cx: &mut Context, is_this: bool, i: Type, tvar: Type) {
        let cache = cx.fix_cache_mut();
        let cache_key = (is_this, i);
        cache.insert(cache_key, tvar);
    }
}

// Keyed on the this-abstracted class or interface and the `this` it is specialized with. A
// property lookup that walks up a `ThisTypeAppT` super specializes the same super once per
// lookup, so without this every key of `keyof Map` re-substitutes the whole super interface.
pub mod this_specialize {
    use dupe::Dupe;

    use super::*;

    pub fn find(cx: &Context, c: &Type, this: &Type) -> Option<Type> {
        let cache = cx.this_specialize_cache();
        cache.get(&(c.dupe(), this.dupe())).map(|t| t.dupe())
    }

    pub fn add(cx: &mut Context, c: Type, this: Type, t: Type) {
        cx.this_specialize_cache_mut().insert((c, this), t);
    }
}

// Keyed on a bounded type, its upper bound and the use that narrows it, with the use's output
// replaced by a placeholder: the type variable holding the narrowed upper bound.
pub mod narrowing {
    use dupe::Dupe;
    use flow_typing_type::type_::BoundedId;

    use super::*;

    pub fn find(cx: &Context, bounded_id: &BoundedId, upper: &Type, u: &UseT<Cx>) -> Option<Type> {
        (cx.narrowing_cache())
            .get(&(bounded_id.clone(), upper.dupe(), u.dupe()))
            .map(|t| t.dupe())
    }

    pub fn add(cx: &mut Context, bounded_id: BoundedId, upper: Type, u: UseT<Cx>, narrowed: Type) {
        cx.narrowing_cache_mut()
            .insert((bounded_id, upper, u), narrowed);
    }
}

// debug util: please don't dead-code-eliminate
// Summarize flow constraints in cache as ctor/reason pairs, and return counts
//    for each group.
pub fn summarize_flow_constraint(cx: &Context) -> Vec<(String, i32)> {
    let cache = cx.constraint_cache();
    let mut group_counts: BTreeMap<String, i32> = BTreeMap::new();

    cache.fold(
        |(), (l, u)| {
            let key = format!(
                "[{}] {:?} => [{}]",
                string_of_ctor(l),
                reason_of_t(l),
                string_of_use_ctor(u),
            );
            *group_counts.entry(key).or_insert(0) += 1;
        },
        (),
    );

    let mut result: Vec<(String, i32)> = group_counts.into_iter().collect();
    result.sort_by_key(|(_, i1)| *i1);
    result
}
