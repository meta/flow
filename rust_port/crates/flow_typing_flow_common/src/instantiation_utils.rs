/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;

use dupe::Dupe;
use flow_common::polarity::Polarity;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::reason::locationless_reason;
use flow_common::reason::mk_reason;
use flow_typing_context::Context;
use flow_typing_type::type_::ArrType;
use flow_typing_type::type_::ArrayATData;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::TupleATData;
use flow_typing_type::type_::Tvar;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::TypeParam;
use flow_typing_type::type_util;
use flow_typing_visitors::type_visitor::TypeVisitor;

// instantiation utils

pub mod implicit_type_argument {
    use super::*;

    pub fn mk_targ_reason(
        typeparam: &TypeParam,
        reason_op: &Reason,
        reason_tapp: &Reason,
    ) -> Reason {
        let loc_op = reason_op.loc().dupe();
        let desc = VirtualReasonDesc::RTypeParam(Box::new((
            typeparam.name.dupe(),
            loc_op.dupe(),
            reason_tapp.def_loc().dupe(),
        )));
        mk_reason(desc, typeparam.reason.def_loc().dupe()).reposition(loc_op)
    }

    /// Make a type argument for a given type parameter, given a reason. Note that
    /// not all type arguments are tvars; the following function is used only when
    /// polymorphic types need to be implicitly instantiated, because there was no
    /// explicit instantiation (via a type application), or when we want to cache a
    /// unique instantiation and unify it with other explicit instantiations.
    pub fn mk_targ<'cx>(
        cx: &mut Context<'cx>,
        typeparam: &TypeParam,
        reason_op: &Reason,
        reason_tapp: &Reason,
    ) -> Type {
        // Create a reason that is positioned at reason_op, but has a def_loc at typeparam.reason.
        let reason = mk_targ_reason(typeparam, reason_op, reason_tapp);
        flow_typing_tvar::mk(cx, reason)
    }

    /// Abstract a type argument that is created by implicit instantiation
    /// above. Sometimes, these type arguments are involved in type expansion
    /// loops, so we abstract them to detect such loops.
    pub fn abstract_targ(tvar: &Type) -> Option<Type> {
        type_util::constraint_node_id(tvar)?;
        let reason = type_util::reason_of_t(tvar);
        let desc = reason.desc(true).cloned();
        match &desc {
            Some(VirtualReasonDesc::RTypeParam(box (_, _, _))) => {
                let reason = locationless_reason(desc);
                Some(Type::new(TypeInner::OpenT(Tvar::new(reason, 0))))
            }
            _ => None,
        }
    }
}

// We maintain a stack of entries representing type applications processed
// during calls to flow, for the purpose of terminating unbounded expansion of
// type applications. Intuitively, we may have a potential infinite loop when
// processing a type application leads to another type application with the same
// root, but expanding type arguments. The entries in a stack contain
// approximate measurements that allow us to detect such expansion.
//
// An entry representing a type application with root C and type args T1,...,Tn
// is of the form (C, [A1,...,An]), where each Ai is a list of the roots of type
// applications nested in Ti. We consider a stack to indicate a potential
// infinite loop when the top of the stack is (C, [A1,...,An]) and there is
// another entry (C, [B1,...,Bn]) in the stack, such that each Bi is non-empty
// and is contained in Ai.

pub mod type_app_expansion {
    use std::collections::BTreeSet;

    use flow_typing_flow_js_env::FlowJsEnv;
    use flow_typing_flow_js_env::type_app_expansion::Bound;
    use flow_typing_flow_js_env::type_app_expansion::Root;
    use flow_typing_flow_js_env::type_app_expansion::RootSet;
    use flow_typing_type::type_::BoundedId;
    use flow_typing_type::type_::properties;

    use super::*;

    // visitor to collect roots of type applications nested in a type
    #[derive(Default)]
    struct RootsCollector {
        /// The property maps already visited. Many properties can reach the same map, such as
        /// the source object of a mapped type, which each property's type refers to.
        seen_props: BTreeSet<properties::Id>,
    }

    impl RootsCollector {
        fn arrtype(&self, r: &Reason, arr: &ArrType) -> Root {
            match arr {
                ArrType::ArrayAT(box ArrayATData { .. }) => Root::Array(r.dupe()),
                ArrType::ROArrayAT(box ROArrayATData { .. }) => Root::ROArray(r.dupe()),
                ArrType::TupleAT(box TupleATData { elements, .. }) => {
                    Root::Tuple(r.dupe(), elements.len())
                }
            }
        }
    }

    impl TypeVisitor<RootSet> for RootsCollector {
        fn type_<'cx>(
            &mut self,
            cx: &mut Context<'cx>,
            pole: Polarity,
            mut acc: RootSet,
            t: &Type,
        ) -> RootSet {
            match t.deref() {
                TypeInner::TypeAppT(box TypeAppTData {
                    type_: inner_type, ..
                }) => {
                    acc.insert(Root::Type(inner_type.dupe()));
                    flow_typing_visitors::type_visitor::type_default(self, cx, pole, acc, t)
                }
                TypeInner::DefT(r, def_t) => match def_t.deref() {
                    DefTInner::ArrT(arr) => {
                        acc.insert(self.arrtype(r, arr));
                        flow_typing_visitors::type_visitor::type_default(self, cx, pole, acc, t)
                    }
                    _ => flow_typing_visitors::type_visitor::type_default(self, cx, pole, acc, t),
                },
                _ if type_util::constraint_node_id(t).is_some() => {
                    match implicit_type_argument::abstract_targ(t) {
                        None => acc,
                        Some(abstract_t) => {
                            acc.insert(Root::Type(abstract_t));
                            acc
                        }
                    }
                }
                _ => flow_typing_visitors::type_visitor::type_default(self, cx, pole, acc, t),
            }
        }

        fn props<'cx>(
            &mut self,
            cx: &mut Context<'cx>,
            pole: Polarity,
            acc: RootSet,
            props_id: properties::Id,
        ) -> RootSet {
            if self.seen_props.insert(props_id.dupe()) {
                flow_typing_visitors::type_visitor::props_default(self, cx, pole, acc, props_id)
            } else {
                acc
            }
        }
    }

    fn collect_roots<'cx>(cx: &mut Context<'cx>, t: &Type) -> RootSet {
        let mut collector = RootsCollector::default();
        collector.type_(cx, Polarity::Neutral, RootSet::new(), t)
    }

    /// Detect whether expanding `c<ts>` would loop. Returns the env to expand
    /// under, or [`None`] if the expansion limit has been reached.
    pub fn push_unless_loop<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        side: Bound,
        c: &Type,
        ts: &[Type],
    ) -> Option<FlowJsEnv> {
        let tss: Vec<RootSet> = ts.iter().map(|t| collect_roots(cx, t)).collect();
        let limit = cx.type_expansion_recursion_limit();
        match env.push_typeapp_unless_loop(limit, side, c, tss) {
            None => {
                if cx.is_verbose() {
                    eprintln!("encountered the same TypeAppT again for {} times", limit);
                }
                None
            }
            Some(env) => {
                if cx.is_verbose() {
                    eprintln!("typeapp stack entry pushed");
                }
                Some(env)
            }
        }
    }

    /// Detect whether flowing the bound of the type with identity `bounded_id` in its place would
    /// loop. Returns the env to flow the bound under, or [`None`] if the expansion limit has been
    /// reached.
    pub fn push_bound_unless_loop<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        bounded_id: &BoundedId,
    ) -> Option<FlowJsEnv> {
        let tss: Vec<RootSet> = (bounded_id.type_args())
            .map(|t| collect_roots(cx, t))
            .collect();
        env.push_bound_unless_loop(cx.type_expansion_recursion_limit(), bounded_id, tss)
    }
}
