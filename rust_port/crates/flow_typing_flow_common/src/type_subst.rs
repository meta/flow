/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeSet;
use std::ops::Deref;
use std::rc::Rc;

use dupe::Dupe;
use flow_common::alpha_rename;
use flow_common::polarity::Polarity;
use flow_common::reason::Reason;
use flow_common::subst_name::SubstName;
use flow_common::subst_name::SubstNameInner;
use flow_common_utils::list_utils;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_typing_context::Context;
use flow_typing_context::Cx;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::Destructor;
use flow_typing_type::type_::DestructorConditionalTypeData;
use flow_typing_type::type_::DestructorMappedTypeData;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::ImplicitInstantiationTvarData;
use flow_typing_type::type_::ImplicitInstantiationTvarKind;
use flow_typing_type::type_::InstanceT;
use flow_typing_type::type_::MappedTypeHomomorphicFlag;
use flow_typing_type::type_::NamedSymbol;
use flow_typing_type::type_::ObjType;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::Predicate;
use flow_typing_type::type_::PredicateInner;
use flow_typing_type::type_::ThisInstanceTData;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::TypeParam;
use flow_typing_type::type_::TypeParamInner;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::exports;
use flow_typing_type::type_::mixed_t;
use flow_typing_type::type_::poly;
use flow_typing_type::type_::properties;
use flow_typing_type::type_::property;
use flow_typing_type::type_util::mod_reason_of_t;
use flow_typing_type::type_util::reason_of_t;
use flow_typing_type::type_util::union_of_ts;
use flow_typing_visitors::type_mapper;
use flow_typing_visitors::type_mapper::TypeMapper;
use flow_typing_visitors::type_visitor::TypeVisitor;

use crate::flow_cache;

// substitutions

#[derive(Debug, Clone)]
pub enum Replacement {
    TypeSubst(
        Type,
        // Free vars in type (lazy for memoization). The lazy receives cx at
        // force time (no context capture needed).
        Rc<flow_lazy::Lazy<Cx, FlowOrdSet<SubstName>>>,
    ),
    AlphaRename(SubstName),
}

#[derive(Debug, Clone)]
struct FvAcc {
    bound: FlowOrdSet<SubstName>,
    free: FlowOrdSet<SubstName>,
}

struct FreeVarVisitor;

impl TypeVisitor<FvAcc> for FreeVarVisitor {
    fn type_<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        mut acc: FvAcc,
        t: &Type,
    ) -> FvAcc {
        match t.deref() {
            TypeInner::ImplicitInstantiationTvar(_) => acc,
            TypeInner::GenericT(box GenericTData { name, .. }) if !acc.bound.contains(name) => {
                acc.free.insert(name.dupe());
                // A synthetic generic also refers to the generics it was built from.
                if let SubstNameInner::Synthetic { ts, .. } = name.deref() {
                    acc.free.extend(
                        ts.iter()
                            .filter(|n| !acc.bound.contains(*n))
                            .map(Dupe::dupe),
                    );
                }
                flow_typing_visitors::type_visitor::type_default(self, cx, pole, acc, t)
            }
            TypeInner::DefT(_, def_t) => {
                if let DefTInner::PolyT(box PolyTData {
                    tparams: xs,
                    t_out: inner,
                    ..
                }) = def_t.deref()
                {
                    let orig_bound = acc.bound.dupe();
                    for tp in xs.iter() {
                        acc.bound.insert(tp.name.dupe());
                        acc = self.type_param(cx, pole, acc, tp);
                    }
                    acc = self.type_(cx, pole, acc, inner);
                    acc.bound = orig_bound;
                    acc
                } else {
                    flow_typing_visitors::type_visitor::type_default(self, cx, pole, acc, t)
                }
            }
            TypeInner::ThisInstanceT(box ThisInstanceTData {
                instance,
                subst_name: this_name,
                ..
            }) => {
                let orig_bound = acc.bound.dupe();
                acc.bound.insert(this_name.dupe());
                acc = self.instance_type(cx, pole, acc, instance);
                acc.bound = orig_bound;
                acc
            }
            _ => flow_typing_visitors::type_visitor::type_default(self, cx, pole, acc, t),
        }
    }

    fn destructor<'cx>(&mut self, cx: &mut Context<'cx>, acc: FvAcc, t: &Destructor) -> FvAcc {
        match t {
            Destructor::ConditionalType(box DestructorConditionalTypeData {
                distributive_tparam_name,
                infer_tparams,
                extends_t,
                true_t,
                false_t,
            }) => self.with_distributive_tparam_name(
                acc,
                distributive_tparam_name,
                |this, mut acc| {
                    let pole = Polarity::Neutral;
                    acc = this.type_(cx, pole, acc, false_t);
                    for tp in infer_tparams.iter() {
                        acc.bound.insert(tp.name.dupe());
                        acc = this.type_param(cx, pole, acc, tp);
                    }
                    acc = this.type_(cx, pole, acc, extends_t);
                    this.type_(cx, pole, acc, true_t)
                },
            ),
            Destructor::MappedType(box DestructorMappedTypeData {
                distributive_tparam_name,
                property_type,
                name_type,
                homomorphic,
                ..
            }) => self.with_distributive_tparam_name(
                acc,
                distributive_tparam_name,
                |this, mut acc| {
                    let pole_todo = Polarity::Neutral;
                    acc = this.type_(cx, pole_todo, acc, property_type);
                    acc = match name_type {
                        Some(t) => this.type_(cx, pole_todo, acc, t),
                        None => acc,
                    };
                    match homomorphic {
                        MappedTypeHomomorphicFlag::SemiHomomorphic(t) => {
                            this.type_(cx, pole_todo, acc, t)
                        }
                        MappedTypeHomomorphicFlag::Homomorphic
                        | MappedTypeHomomorphicFlag::Unspecialized => acc,
                    }
                },
            ),
            _ => flow_typing_visitors::type_visitor::destructor_default(self, cx, acc, t),
        }
    }
}

impl FreeVarVisitor {
    fn with_distributive_tparam_name<F>(
        &mut self,
        mut acc: FvAcc,
        name: &Option<SubstName>,
        f: F,
    ) -> FvAcc
    where
        F: FnOnce(&mut Self, FvAcc) -> FvAcc,
    {
        let orig_bound = acc.bound.dupe();
        if let Some(n) = name {
            acc.bound.insert(n.dupe());
        }
        acc = f(self, acc);
        acc.bound = orig_bound;
        acc
    }
}

pub fn free_var_finder<'cx>(
    cx: &mut Context<'cx>,
    bound: Option<FlowOrdSet<SubstName>>,
    t: &Type,
) -> FlowOrdSet<SubstName> {
    let bound = bound.unwrap_or_default();
    let mut visitor = FreeVarVisitor;
    let result = visitor.type_(
        cx,
        Polarity::Positive,
        FvAcc {
            free: FlowOrdSet::new(),
            bound,
        },
        t,
    );
    result.free
}

pub fn free_var_finder_in_destructor<'cx>(
    cx: &mut Context<'cx>,
    bound: Option<FlowOrdSet<SubstName>>,
    d: &Destructor,
) -> FlowOrdSet<SubstName> {
    let bound = bound.unwrap_or_default();
    let mut visitor = FreeVarVisitor;
    let result = visitor.destructor(
        cx,
        FvAcc {
            free: FlowOrdSet::new(),
            bound,
        },
        d,
    );
    result.free
}

// Substitute bound type variables with associated types in a type.

fn fvs_of_map<'cx>(
    cx: &mut Context<'cx>,
    map: &FlowOrdMap<SubstName, Replacement>,
) -> FlowOrdSet<SubstName> {
    let mut acc = FlowOrdSet::new();
    for replacement in map.values() {
        match replacement {
            Replacement::TypeSubst(_, fvs) => {
                acc = acc.union(fvs.get_forced(cx).dupe());
            }
            Replacement::AlphaRename(_) => {}
        }
    }
    acc
}

fn union_ident_map_and_dedup<F>(
    mut f: F,
    t: Type,
    r: &Reason,
    rep: &flow_typing_type::type_::union_rep::UnionRep,
) -> Type
where
    F: FnMut(Type) -> Type,
{
    fn flatten_into(t: Type, seen: &mut BTreeSet<Type>, out: &mut Vec<Type>) {
        match t.deref() {
            TypeInner::UnionT(_, rep) => {
                for t in rep.members_iter() {
                    flatten_into(t.dupe(), seen, out);
                }
            }
            TypeInner::MaybeT(r, inner_t) => {
                let null_t = Type::new(TypeInner::DefT(r.dupe(), DefT::new(DefTInner::NullT)));
                let void_t = Type::new(TypeInner::DefT(r.dupe(), DefT::new(DefTInner::VoidT)));
                if seen.insert(null_t.dupe()) {
                    out.push(null_t);
                }
                if seen.insert(void_t.dupe()) {
                    out.push(void_t);
                }
                flatten_into(inner_t.dupe(), seen, out);
            }
            TypeInner::OptionalT {
                reason,
                type_,
                use_desc,
            } => {
                let void_t =
                    flow_typing_type::type_::void::why_with_use_desc(*use_desc, reason.dupe());
                if seen.insert(void_t.dupe()) {
                    out.push(void_t);
                }
                flatten_into(type_.dupe(), seen, out);
            }
            _ => {
                if seen.insert(t.dupe()) {
                    out.push(t);
                }
            }
        }
    }

    let mut mapped_members: Option<Vec<Type>> = None;
    for (i, member) in rep.members_iter().enumerate() {
        let mapped = f(member.dupe());
        if let Some(mapped_members) = mapped_members.as_mut() {
            mapped_members.push(mapped);
        } else if !member.ptr_eq(&mapped) {
            let mut members = Vec::with_capacity(rep.members_iter().size_hint().0);
            members.extend(rep.members_iter().take(i).map(Dupe::dupe));
            members.push(mapped);
            mapped_members = Some(members);
        }
    }

    let Some(mapped_members) = mapped_members else {
        return t;
    };

    let mut acc: Vec<Type> = Vec::new();
    let mut seen = BTreeSet::new();
    for t_item in mapped_members {
        flatten_into(t_item, &mut seen, &mut acc);
    }
    union_of_ts(r.dupe(), acc, None)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Purpose {
    #[default]
    Normal,
    // In generic conditional type, we have a step to replace all GenericT with any, to see whether
    // the check will still fail (so that we can always take the false branch).
    // During the substitution, we might encounter `Subst_name.Synthetic`. We shouldn't crash
    // over there. Instead, we should keep them as is.
    ConditionalTypeAnySubst,
}

type MapCx = (
    FlowOrdMap<SubstName, Replacement>,
    bool,
    bool,
    Purpose,
    Option<UseOp>,
);

pub fn call_prop<'cx, MapCx, M: TypeMapper<'cx, MapCx>>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &MapCx,
    id: i32,
) -> i32 {
    let t = cx.find_call(id);
    let t_prime = mapper.type_(cx, map_cx, t.dupe());
    if t.ptr_eq(&t_prime) {
        id
    } else {
        cx.make_call_prop(t_prime)
    }
}

pub fn props<'cx, MapCx, M: TypeMapper<'cx, MapCx>>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &MapCx,
    id: properties::Id,
) -> properties::Id {
    let props_map = cx.find_props(id.dupe());
    let props_map_prime = props_map.ident_map(|prop| {
        property::ident_map_t(|t| mapper.type_(cx, map_cx, t.dupe()), prop).into_owned()
    });
    if props_map.ptr_eq(&props_map_prime) {
        id
    } else {
        // When substitution results in a new property map, we have to use a
        // generated id, rather than a location from source. The substituted
        // object will have the same location as the generic version, meaning
        // that this location will not serve as a unique identifier.
        cx.generate_property_map(props_map_prime)
    }
}

pub fn exports<'cx, MapCx, M: TypeMapper<'cx, MapCx>>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &MapCx,
    id: exports::Id,
) -> exports::Id {
    let exps = cx.find_exports(id);
    let exps_prime = exps.ident_map(|ns| {
        let type_prime = mapper.type_(cx, map_cx, ns.type_.dupe());
        let type_for_extends_prime = ns
            .type_for_extends
            .as_ref()
            .map(|t| mapper.type_(cx, map_cx, t.dupe()));
        let type_for_extends_unchanged = match (&ns.type_for_extends, &type_for_extends_prime) {
            (None, None) => true,
            (Some(a), Some(b)) => a.ptr_eq(b),
            _ => false,
        };
        if ns.type_.ptr_eq(&type_prime) && type_for_extends_unchanged {
            ns.dupe()
        } else {
            NamedSymbol::new(
                ns.name_loc.dupe(),
                ns.preferred_def_locs.clone(),
                type_prime,
                type_for_extends_prime,
            )
        }
    });
    if exps.ptr_eq(&exps_prime) {
        id
    } else {
        cx.make_export_map(exps_prime)
    }
}

struct Substituter {
    change_id: bool,
    obj_reachable_targs: Option<Vec<(Type, Polarity)>>,
    cached_fvs: Option<(FlowOrdMap<SubstName, Replacement>, FlowOrdSet<SubstName>)>,
}

impl Substituter {
    fn new() -> Self {
        Self {
            change_id: false,
            obj_reachable_targs: None,
            cached_fvs: None,
        }
    }

    fn get_fvs_of_map(
        &mut self,
        cx: &mut Context<'_>,
        map: &FlowOrdMap<SubstName, Replacement>,
    ) -> FlowOrdSet<SubstName> {
        match &self.cached_fvs {
            Some((cached_map, fvs)) if cached_map.as_inner().ptr_eq(map.as_inner()) => fvs.dupe(),
            _ => {
                let fvs = fvs_of_map(cx, map);
                self.cached_fvs = Some((map.dupe(), fvs.dupe()));
                fvs
            }
        }
    }

    fn name_in_fvs_of_map(
        &self,
        cx: &mut Context<'_>,
        map: &FlowOrdMap<SubstName, Replacement>,
        name: &SubstName,
    ) -> bool {
        match &self.cached_fvs {
            Some((cached_map, fvs)) if cached_map.as_inner().ptr_eq(map.as_inner()) => {
                fvs.contains(name)
            }
            _ => map.values().any(|replacement| match replacement {
                Replacement::TypeSubst(_, fvs) => fvs.get_forced(cx).contains(name),
                Replacement::AlphaRename(_) => false,
            }),
        }
    }

    fn avoid_capture(
        &mut self,
        cx: &mut Context<'_>,
        map: FlowOrdMap<SubstName, Replacement>,
        name: SubstName,
    ) -> (SubstName, FlowOrdMap<SubstName, Replacement>) {
        if self.name_in_fvs_of_map(cx, &map, &name) {
            let fvs = self.get_fvs_of_map(cx, &map);
            let mut used_names = fvs.dupe();
            used_names.extend(map.keys().map(Dupe::dupe));
            let new_name_val = alpha_rename::subst_name(&name, &used_names);
            let mut new_map = map;
            new_map.insert(name, Replacement::AlphaRename(new_name_val.dupe()));
            self.cached_fvs = Some((new_map.dupe(), fvs));
            (new_name_val, new_map)
        } else if map.contains_key(&name) {
            let cached_fvs = match &self.cached_fvs {
                Some((cached_map, fvs)) if cached_map.as_inner().ptr_eq(map.as_inner()) => {
                    Some(fvs.dupe())
                }
                _ => None,
            };
            let mut new_map = map;
            new_map.remove(&name);
            if let Some(fvs) = cached_fvs {
                self.cached_fvs = Some((new_map.dupe(), fvs));
            }
            (name, new_map)
        } else {
            (name, map)
        }
    }

    fn subst_distributive_tparam_name(
        &mut self,
        cx: &mut Context<'_>,
        name: &Option<SubstName>,
        map: FlowOrdMap<SubstName, Replacement>,
    ) -> (Option<SubstName>, FlowOrdMap<SubstName, Replacement>) {
        match name {
            None => (None, map),
            Some(name) => {
                let (new_name, new_map) = self.avoid_capture(cx, map, name.dupe());
                (Some(new_name), new_map)
            }
        }
    }
}

impl<'cx> TypeMapper<'cx, MapCx> for Substituter {
    fn tvar(&mut self, _cx: &mut Context<'cx>, _map_cx: &MapCx, _r: &Reason, id: u32) -> u32 {
        id
    }

    fn call_prop(&mut self, cx: &mut Context<'cx>, map_cx: &MapCx, id: i32) -> i32 {
        let (map, _, _, _, _) = map_cx;
        if map.is_empty() {
            id
        } else {
            call_prop(self, cx, map_cx, id)
        }
    }

    fn props(
        &mut self,
        cx: &mut Context<'cx>,
        map_cx: &MapCx,
        id: properties::Id,
    ) -> properties::Id {
        let (map, _, _, _, _) = map_cx;
        if map.is_empty() {
            id
        } else {
            props(self, cx, map_cx, id)
        }
    }

    fn exports(&mut self, cx: &mut Context<'cx>, map_cx: &MapCx, id: exports::Id) -> exports::Id {
        let (map, _, _, _, _) = map_cx;
        if map.is_empty() {
            id
        } else {
            exports(self, cx, map_cx, id)
        }
    }

    fn type_(&mut self, cx: &mut Context<'cx>, map_cx: &MapCx, t: Type) -> Type {
        let (map, force, placeholder_no_infer, purpose, use_op) = map_cx;
        if map.is_empty() {
            return t;
        }
        let t_out = match t.deref() {
            TypeInner::GenericT(box GenericTData {
                name: subst_name,
                reason,
                ..
            }) if matches!(subst_name.deref(), SubstNameInner::Synthetic { .. }) => {
                let (name, op_kind, ids) = match &**subst_name {
                    SubstNameInner::Synthetic { name, op_kind, ts } => (name, op_kind, ts),
                    _ => unreachable!(),
                };
                if *purpose == Purpose::Normal && ids.iter().any(|name| map.contains_key(name)) {
                    let op_kind_str = match op_kind {
                        None => "None".to_string(),
                        Some(kind) => format!("{:?}", kind),
                    };
                    panic!(
                        "Cannot substitute through synthetic name {}(kind={}) at {:?}.",
                        name, op_kind_str, reason
                    );
                } else {
                    type_mapper::type_default(self, cx, map_cx, t.dupe())
                }
            }
            TypeInner::GenericT(box GenericTData {
                reason: tp_reason,
                name,
                no_infer,
                bound,
                id: gen_id,
            }) => {
                let annot_loc = tp_reason.loc();
                match map.get(name) {
                    None => type_mapper::type_default(self, cx, map_cx, t.dupe()),
                    Some(Replacement::TypeSubst(param_t, _)) => {
                        self.change_id = true;
                        match (*placeholder_no_infer, *no_infer, &**param_t) {
                            (true, true, TypeInner::ImplicitInstantiationTvar(data)) => {
                                cx.mk_placeholder(reason_of_t(param_t).dupe(), data.bound.dupe())
                            }
                            (true, true, _) => {
                                let reason = reason_of_t(param_t);
                                cx.mk_placeholder(reason.dupe(), mixed_t::why(reason.dupe()))
                            }
                            _ => match (&mut self.obj_reachable_targs, &**param_t) {
                                (
                                    obj_reachable_targs,
                                    TypeInner::ImplicitInstantiationTvar(
                                        box ImplicitInstantiationTvarData {
                                            kind: ImplicitInstantiationTvarKind::Inferred,
                                            ..
                                        },
                                    ),
                                ) => {
                                    if let Some(targs) = obj_reachable_targs {
                                        targs.push((param_t.dupe(), Polarity::Neutral));
                                    }
                                    param_t.dupe()
                                }
                                (_, TypeInner::GenericT(..)) => mod_reason_of_t(
                                    &|param_reason| {
                                        param_reason
                                            .reposition(annot_loc.dupe())
                                            .annotate(annot_loc.dupe())
                                    },
                                    param_t,
                                ),
                                (Some(targs), TypeInner::OpenT(_)) => {
                                    targs.push((param_t.dupe(), Polarity::Neutral));
                                    param_t.dupe()
                                }
                                _ => param_t.dupe(),
                            },
                        }
                    }
                    Some(Replacement::AlphaRename(name_prime)) => {
                        let t = Type::new(TypeInner::GenericT(Box::new(GenericTData {
                            reason: tp_reason.dupe(),
                            name: name_prime.dupe(),
                            bound: bound.dupe(),
                            no_infer: *no_infer,
                            id: gen_id.clone(),
                        })));
                        type_mapper::type_default(self, cx, map_cx, t)
                    }
                }
            }
            TypeInner::DefT(reason, def_t) => {
                if let DefTInner::PolyT(box PolyTData {
                    kind: poly_kind,
                    tparams_loc,
                    tparams: xs,
                    t_out: inner,
                    id: poly_id,
                    strictness_kind,
                }) = def_t.deref()
                {
                    let prev_change_id = self.change_id;
                    self.change_id = false;
                    let mut xs_new: Option<Vec<TypeParam>> = None;
                    let mut current_map = map.dupe();
                    let mut changed = false;

                    for (i, typeparam) in xs.iter().enumerate() {
                        let bound = self.type_(
                            cx,
                            &(
                                current_map.dupe(),
                                *force,
                                *placeholder_no_infer,
                                *purpose,
                                use_op.dupe(),
                            ),
                            typeparam.bound.dupe(),
                        );
                        let default = match &typeparam.default {
                            None => None,
                            Some(default) => {
                                let default_prime = self.type_(
                                    cx,
                                    &(
                                        current_map.dupe(),
                                        *force,
                                        *placeholder_no_infer,
                                        *purpose,
                                        use_op.dupe(),
                                    ),
                                    default.dupe(),
                                );
                                if default.ptr_eq(&default_prime) {
                                    typeparam.default.dupe()
                                } else {
                                    Some(default_prime)
                                }
                            }
                        };
                        let (name, new_map) =
                            self.avoid_capture(cx, current_map, typeparam.name.dupe());
                        current_map = new_map;
                        let bound_changed = !typeparam.bound.ptr_eq(&bound);
                        let name_changed = name != typeparam.name;
                        let default_changed = match (&typeparam.default, &default) {
                            (None, None) => false,
                            (Some(a), Some(b)) => !a.ptr_eq(b),
                            _ => true,
                        };
                        changed = changed || bound_changed || default_changed;
                        let tparam_changed = name_changed || bound_changed || default_changed;
                        if let Some(xs_new) = xs_new.as_mut() {
                            if tparam_changed {
                                xs_new.push(TypeParam::new(TypeParamInner {
                                    reason: typeparam.reason.dupe(),
                                    name,
                                    bound,
                                    polarity: typeparam.polarity,
                                    default,
                                    is_this: typeparam.is_this,
                                    is_const: typeparam.is_const,
                                }));
                            } else {
                                xs_new.push(typeparam.dupe());
                            }
                        } else if tparam_changed {
                            let mut new_xs = Vec::with_capacity(xs.len());
                            new_xs.extend(xs[..i].iter().map(Dupe::dupe));
                            new_xs.push(TypeParam::new(TypeParamInner {
                                reason: typeparam.reason.dupe(),
                                name,
                                bound,
                                polarity: typeparam.polarity,
                                default,
                                is_this: typeparam.is_this,
                                is_const: typeparam.is_const,
                            }));
                            xs_new = Some(new_xs);
                        }
                    }

                    let inner_prime = self.type_(
                        cx,
                        &(current_map, false, *placeholder_no_infer, *purpose, None),
                        inner.dupe(),
                    );
                    let changed = changed || !inner.ptr_eq(&inner_prime);
                    let new_id = if self.change_id {
                        poly::Id::generate_id()
                    } else {
                        poly_id.dupe()
                    };
                    self.change_id = prev_change_id || self.change_id;
                    if changed {
                        Type::new(TypeInner::DefT(
                            reason.dupe(),
                            DefT::new(DefTInner::PolyT(Box::new(PolyTData {
                                kind: *poly_kind,
                                tparams_loc: tparams_loc.dupe(),
                                tparams: xs_new.map_or_else(|| xs.dupe(), Into::into),
                                t_out: inner_prime,
                                id: new_id,
                                strictness_kind: *strictness_kind,
                            }))),
                        ))
                    } else {
                        t.dupe()
                    }
                } else {
                    type_mapper::type_default(self, cx, map_cx, t.dupe())
                }
            }
            TypeInner::ThisInstanceT(box ThisInstanceTData {
                reason: r,
                instance: this,
                is_this: i,
                subst_name: this_name,
            }) => {
                let (name, new_map) = self.avoid_capture(cx, map.dupe(), this_name.dupe());
                let this_prime = self.instance_type(
                    cx,
                    &(
                        new_map,
                        *force,
                        *placeholder_no_infer,
                        *purpose,
                        use_op.dupe(),
                    ),
                    this,
                );
                if this.ptr_eq(&this_prime) && &name == this_name {
                    t.dupe()
                } else {
                    Type::new(TypeInner::ThisInstanceT(Box::new(ThisInstanceTData {
                        reason: r.dupe(),
                        instance: this_prime,
                        is_this: *i,
                        subst_name: name,
                    })))
                }
            }
            TypeInner::TypeAppT(box TypeAppTData {
                reason,
                use_op: op,
                type_: type_inner,
                targs,
                from_value,
                use_desc,
            }) => {
                let type_prime = self.type_(cx, map_cx, type_inner.dupe());
                let targs_prime = list_utils::ident_map(
                    |targ| self.type_(cx, map_cx, targ.dupe()),
                    Type::ptr_eq,
                    targs.dupe(),
                );
                if type_inner.ptr_eq(&type_prime) && Rc::ptr_eq(targs, &targs_prime) {
                    t.dupe()
                } else {
                    // If the TypeAppT changed then one of the type arguments had a
                    // GenericT that was substituted. In this case, also change the use_op
                    // so we can point at the op which instantiated the types that
                    // were substituted.
                    let new_use_op = use_op.as_ref().unwrap_or(op).dupe();
                    Type::new(TypeInner::TypeAppT(Box::new(TypeAppTData {
                        reason: reason.dupe(),
                        use_op: new_use_op,
                        type_: type_prime,
                        targs: targs_prime,
                        from_value: *from_value,
                        use_desc: *use_desc,
                    })))
                }
            }
            TypeInner::EvalT {
                type_: x,
                defer_use_t,
                ..
            } => {
                let op = &defer_use_t.0;
                let r = &defer_use_t.1;
                let d = &defer_use_t.2;
                let x_prime = self.type_(cx, map_cx, x.dupe());
                let d_prime = self.destructor(cx, map_cx, d.dupe());
                if x.ptr_eq(&x_prime) && Rc::ptr_eq(d, &d_prime) {
                    t.dupe()
                } else {
                    // If the EvalT changed then either the target or destructor had a
                    // GenericT that was substituted. In this case, also change the use_op
                    // so we can point at the op which instantiated the types that
                    // were substituted.
                    let new_use_op = use_op.as_ref().unwrap_or(op).dupe();
                    let new_defer_use = flow_typing_type::type_::TypeDestructorT::new(
                        flow_typing_type::type_::TypeDestructorTInner(
                            new_use_op,
                            r.dupe(),
                            d_prime,
                        ),
                    );
                    flow_cache::eval::id(cx, x_prime, new_defer_use)
                }
            }
            TypeInner::UnionT(r, urep) => union_ident_map_and_dedup(
                |t_elem| self.type_(cx, map_cx, t_elem),
                t.dupe(),
                r,
                urep,
            ),
            TypeInner::StringMappingT { reason, kind, arg } => {
                let arg_prime = self.type_(cx, map_cx, arg.dupe());
                if arg.ptr_eq(&arg_prime) {
                    t.dupe()
                } else {
                    // After substitution, `arg` may have become a shape we can
                    // eagerly reduce (e.g. a singleton string, a union of
                    // singletons, a template literal type). Re-resolve so that
                    // `Uppercase<T>` with `T = 'foo'` collapses to `'FOO'`
                    // instead of remaining a deferred
                    // `StringMappingT { arg: 'foo', .. }`, which subtyping would
                    // otherwise compare against the un-cased literal.
                    crate::string_case_transform::resolve(
                        None,
                        cx,
                        *kind,
                        reason.loc().dupe(),
                        arg_prime,
                    )
                }
            }
            _ => type_mapper::type_default(self, cx, map_cx, t.dupe()),
        };
        if t.ptr_eq(&t_out) {
            t
        } else {
            use flow_common::reason::VirtualReasonDesc;
            let t_out_reason = reason_of_t(&t_out);
            match t_out_reason.desc(false) {
                Some(VirtualReasonDesc::RTypeAlias(box (name, Some(_), desc))) => {
                    let new_desc = flow_common::reason::VirtualReasonDesc::RTypeAlias(Box::new((
                        name.dupe(),
                        None,
                        desc.dupe(),
                    )));
                    mod_reason_of_t(&|r| r.replace_desc(new_desc.clone()), &t_out)
                }
                _ => t_out,
            }
        }
    }

    fn predicate(&mut self, cx: &mut Context<'cx>, map_cx: &MapCx, p: &Predicate) -> Predicate {
        match p.deref() {
            PredicateInner::LatentP(..) => type_mapper::predicate_default(self, cx, map_cx, p),
            PredicateInner::LatentThisP(..) => type_mapper::predicate_default(self, cx, map_cx, p),
            _ => p.dupe(),
        }
    }

    fn obj_type(&mut self, cx: &mut Context<'cx>, map_cx: &MapCx, t: Rc<ObjType>) -> Rc<ObjType> {
        let old_obj_reachable_targs = self.obj_reachable_targs.replace(Vec::new());
        let t_prime = type_mapper::obj_type_default(self, cx, map_cx, t.dupe());
        let reachable_targs = self
            .obj_reachable_targs
            .take()
            .expect("Invariant violation: reachable targs are None after visiting an object");
        self.obj_reachable_targs = match old_obj_reachable_targs {
            Some(mut targs) => {
                targs.extend(reachable_targs.iter().map(|(t, p)| (t.dupe(), *p)));
                Some(targs)
            }
            None => None,
        };
        let targs_same = reachable_targs.len() == t_prime.reachable_targs.len()
            && reachable_targs
                .iter()
                .zip(t_prime.reachable_targs.iter())
                .all(|((t1, p1), (t2, p2))| t1.ptr_eq(t2) && p1 == p2);
        if targs_same {
            t_prime
        } else {
            Rc::new(ObjType {
                flags: t_prime.flags.clone(),
                props_tmap: t_prime.props_tmap.dupe(),
                proto_t: t_prime.proto_t.dupe(),
                call_t: t_prime.call_t,
                reachable_targs: reachable_targs.into(),
                strictness_kind: t_prime.strictness_kind,
                literal: t_prime.literal,
            })
        }
    }

    fn destructor(
        &mut self,
        cx: &mut Context<'cx>,
        map_cx: &MapCx,
        t: Rc<Destructor>,
    ) -> Rc<Destructor> {
        let (map, _, placeholder_no_infer, purpose, use_op) = map_cx;
        match t.deref() {
            Destructor::ConditionalType(box DestructorConditionalTypeData {
                distributive_tparam_name,
                infer_tparams,
                extends_t,
                true_t,
                false_t,
            }) => {
                let (distributive_tparam_name_new, mut current_map) =
                    self.subst_distributive_tparam_name(cx, distributive_tparam_name, map.dupe());
                let false_t_prime = self.type_(
                    cx,
                    &(
                        current_map.dupe(),
                        false,
                        *placeholder_no_infer,
                        *purpose,
                        use_op.dupe(),
                    ),
                    false_t.dupe(),
                );
                let mut tparams: Vec<TypeParam> = Vec::new();
                let mut changed = false;
                for typeparam in infer_tparams.iter() {
                    let bound = self.type_(
                        cx,
                        &(
                            current_map.dupe(),
                            false,
                            *placeholder_no_infer,
                            *purpose,
                            use_op.dupe(),
                        ),
                        typeparam.bound.dupe(),
                    );
                    let (name, new_map) =
                        self.avoid_capture(cx, current_map, typeparam.name.dupe());
                    current_map = new_map;
                    let bound_changed = !typeparam.bound.ptr_eq(&bound);
                    changed = changed || bound_changed;
                    tparams.push(TypeParam::new(TypeParamInner {
                        reason: typeparam.reason.dupe(),
                        name,
                        bound,
                        polarity: typeparam.polarity,
                        default: typeparam.default.dupe(),
                        is_this: typeparam.is_this,
                        is_const: typeparam.is_const,
                    }));
                }
                let extends_t_prime = self.type_(
                    cx,
                    &(
                        current_map.dupe(),
                        false,
                        *placeholder_no_infer,
                        *purpose,
                        use_op.dupe(),
                    ),
                    extends_t.dupe(),
                );
                let true_t_prime = self.type_(
                    cx,
                    &(
                        current_map,
                        false,
                        *placeholder_no_infer,
                        *purpose,
                        use_op.dupe(),
                    ),
                    true_t.dupe(),
                );
                if changed
                    || !extends_t.ptr_eq(&extends_t_prime)
                    || !true_t.ptr_eq(&true_t_prime)
                    || !false_t.ptr_eq(&false_t_prime)
                {
                    Rc::new(Destructor::ConditionalType(Box::new(
                        DestructorConditionalTypeData {
                            distributive_tparam_name: distributive_tparam_name_new,
                            infer_tparams: tparams.into(),
                            extends_t: extends_t_prime,
                            true_t: true_t_prime,
                            false_t: false_t_prime,
                        },
                    )))
                } else {
                    t
                }
            }
            Destructor::MappedType(box DestructorMappedTypeData {
                distributive_tparam_name,
                property_type,
                name_type,
                mapped_type_flags,
                homomorphic,
                strictness_kind,
            }) => {
                let (distributive_tparam_name_new, new_map) =
                    self.subst_distributive_tparam_name(cx, distributive_tparam_name, map.dupe());
                let property_type_prime = self.type_(
                    cx,
                    &(new_map.dupe(), false, *placeholder_no_infer, *purpose, None),
                    property_type.dupe(),
                );
                let name_type_prime = match name_type {
                    None => None,
                    Some(nt) => {
                        let nt_prime = self.type_(
                            cx,
                            &(new_map.dupe(), false, *placeholder_no_infer, *purpose, None),
                            nt.dupe(),
                        );
                        if nt.ptr_eq(&nt_prime) {
                            name_type.clone()
                        } else {
                            Some(nt_prime)
                        }
                    }
                };
                let (homomorphic_prime, homomorphic_changed) = match homomorphic {
                    MappedTypeHomomorphicFlag::SemiHomomorphic(hom_t) => {
                        let hom_t_prime = self.type_(
                            cx,
                            &(new_map, false, *placeholder_no_infer, *purpose, None),
                            hom_t.dupe(),
                        );
                        if hom_t.ptr_eq(&hom_t_prime) {
                            (homomorphic.dupe(), false)
                        } else {
                            (
                                MappedTypeHomomorphicFlag::SemiHomomorphic(hom_t_prime),
                                true,
                            )
                        }
                    }
                    MappedTypeHomomorphicFlag::Homomorphic
                    | MappedTypeHomomorphicFlag::Unspecialized => (homomorphic.dupe(), false),
                };
                let name_type_changed = match (name_type, &name_type_prime) {
                    (None, None) => false,
                    (Some(a), Some(b)) => !a.ptr_eq(b),
                    _ => true,
                };
                if property_type.ptr_eq(&property_type_prime)
                    && !name_type_changed
                    && !homomorphic_changed
                {
                    t
                } else {
                    Rc::new(Destructor::MappedType(Box::new(DestructorMappedTypeData {
                        distributive_tparam_name: distributive_tparam_name_new,
                        property_type: property_type_prime,
                        name_type: name_type_prime,
                        mapped_type_flags: *mapped_type_flags,
                        homomorphic: homomorphic_prime,
                        strictness_kind: *strictness_kind,
                    })))
                }
            }
            _ => type_mapper::destructor_default(self, cx, map_cx, t),
        }
    }

    fn eval_id(
        &mut self,
        _cx: &mut Context<'cx>,
        _map_cx: &MapCx,
        _id: flow_typing_type::type_::eval::Id,
    ) -> flow_typing_type::type_::eval::Id {
        unreachable!("eval_id should never be called on substituter")
    }
}

pub fn subst<'cx>(
    cx: &mut Context<'cx>,
    use_op: Option<UseOp>,
    force: bool,
    placeholder_no_infer: bool,
    purpose: Purpose,
    map: &FlowOrdMap<SubstName, Type>,
    t: Type,
) -> Type {
    // Optimization: if map is empty, return type unchanged
    if map.is_empty() {
        return t;
    }

    let replacement_map: FlowOrdMap<SubstName, Replacement> = map
        .iter()
        .map(|(k, v)| {
            let v = v.dupe();
            let v_c = v.dupe();
            (
                k.dupe(),
                Replacement::TypeSubst(
                    v,
                    Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                        free_var_finder(cx, None, &v_c)
                    })),
                ),
            )
        })
        .collect();

    let mut substituter = Substituter::new();
    substituter.type_(
        cx,
        &(
            replacement_map,
            force,
            placeholder_no_infer,
            purpose,
            use_op,
        ),
        t,
    )
}

pub fn subst_destructor<'cx>(
    cx: &mut Context<'cx>,
    use_op: Option<UseOp>,
    force: bool,
    placeholder_no_infer: bool,
    purpose: Purpose,
    map: &FlowOrdMap<SubstName, Type>,
    destructor: Rc<Destructor>,
) -> Rc<Destructor> {
    // Optimization: if map is empty, return destructor unchanged
    if map.is_empty() {
        return destructor;
    }

    let replacement_map: FlowOrdMap<SubstName, Replacement> = map
        .iter()
        .map(|(k, v)| {
            let v = v.dupe();
            let v_c = v.dupe();
            (
                k.dupe(),
                Replacement::TypeSubst(
                    v,
                    Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                        free_var_finder(cx, None, &v_c)
                    })),
                ),
            )
        })
        .collect();

    let mut substituter = Substituter::new();
    substituter.destructor(
        cx,
        &(
            replacement_map,
            force,
            placeholder_no_infer,
            purpose,
            use_op,
        ),
        destructor,
    )
}

pub fn subst_instance_type<'cx>(
    cx: &mut Context<'cx>,
    use_op: Option<UseOp>,
    force: bool,
    placeholder_no_infer: bool,
    purpose: Purpose,
    map: &FlowOrdMap<SubstName, Type>,
    instance: &InstanceT,
) -> InstanceT {
    // Optimization: if map is empty, return instance unchanged
    if map.is_empty() {
        return instance.dupe();
    }

    let replacement_map: FlowOrdMap<SubstName, Replacement> = map
        .iter()
        .map(|(k, v)| {
            let v = v.dupe();
            let v_c = v.dupe();
            (
                k.dupe(),
                Replacement::TypeSubst(
                    v,
                    Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                        free_var_finder(cx, None, &v_c)
                    })),
                ),
            )
        })
        .collect();

    let mut substituter = Substituter::new();
    substituter.instance_type(
        cx,
        &(
            replacement_map,
            force,
            placeholder_no_infer,
            purpose,
            use_op,
        ),
        instance,
    )
}
