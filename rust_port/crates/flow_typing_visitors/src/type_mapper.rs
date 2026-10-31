/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;
use std::rc::Rc;

use dupe::Dupe;
use flow_common::reason::Reason;
use flow_common_utils::list_utils;
use flow_common_utils::option_utils;
use flow_typing_context::Context;
use flow_typing_type::type_::ArrType;
use flow_typing_type::type_::ArrayATData;
use flow_typing_type::type_::CallArg;
use flow_typing_type::type_::CallArgInner;
use flow_typing_type::type_::CanonicalRendersForm;
use flow_typing_type::type_::ComponentKind;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::Destructor;
use flow_typing_type::type_::DestructorConditionalTypeData;
use flow_typing_type::type_::DestructorMappedTypeData;
use flow_typing_type::type_::DestructorSpreadTupleTypeData;
use flow_typing_type::type_::DestructorSpreadTypeData;
use flow_typing_type::type_::DictType;
use flow_typing_type::type_::EnumConcreteInfo;
use flow_typing_type::type_::EnumConcreteInfoInner;
use flow_typing_type::type_::EnumInfo;
use flow_typing_type::type_::EnumInfoInner;
use flow_typing_type::type_::ExportTypes;
use flow_typing_type::type_::Flags;
use flow_typing_type::type_::FunParam;
use flow_typing_type::type_::FunRestParam;
use flow_typing_type::type_::FunType;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::InstType;
use flow_typing_type::type_::InstTypeInner;
use flow_typing_type::type_::InstanceT;
use flow_typing_type::type_::InstanceTInner;
use flow_typing_type::type_::MappedTypeHomomorphicFlag;
use flow_typing_type::type_::NamespaceType;
use flow_typing_type::type_::NominalType;
use flow_typing_type::type_::NominalTypeInner;
use flow_typing_type::type_::ObjKind;
use flow_typing_type::type_::ObjType;
use flow_typing_type::type_::OptionalIndexedAccessIndex;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::Predicate;
use flow_typing_type::type_::PredicateInner;
use flow_typing_type::type_::Property;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::ReactAbstractComponentTData;
use flow_typing_type::type_::ResolvedArgData;
use flow_typing_type::type_::ResolvedParam;
use flow_typing_type::type_::ResolvedSpreadArgData;
use flow_typing_type::type_::Selector;
use flow_typing_type::type_::Targ;
use flow_typing_type::type_::ThisInstanceTData;
use flow_typing_type::type_::ThisTypeAppTData;
use flow_typing_type::type_::TupleATData;
use flow_typing_type::type_::TupleElement;
use flow_typing_type::type_::TupleView;
use flow_typing_type::type_::Tvar;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeDestructorT;
use flow_typing_type::type_::TypeDestructorTInner;
use flow_typing_type::type_::TypeGuard;
use flow_typing_type::type_::TypeGuardInner;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::TypeMap;
use flow_typing_type::type_::TypeParam;
use flow_typing_type::type_::TypeParamInner;
use flow_typing_type::type_::UnresolvedArgData;
use flow_typing_type::type_::UnresolvedParam;
use flow_typing_type::type_::constraint;
use flow_typing_type::type_::eval;
use flow_typing_type::type_::exports;
use flow_typing_type::type_::inter_rep;
use flow_typing_type::type_::nominal;
use flow_typing_type::type_::object;
use flow_typing_type::type_::poly;
use flow_typing_type::type_::properties;
use flow_typing_type::type_::union_rep;
use flow_typing_type::type_::void;
use flow_typing_type::type_util;

/// NOTE: While union flattening could be performed at any time, it is most effective when we know
/// that all tvars have been resolved.
pub fn union_flatten<'cx>(cx: &mut Context<'cx>, ts: impl IntoIterator<Item = Type>) -> Vec<Type> {
    // Push results into a single accumulator instead of returning a fresh
    // `Vec<Type>` per call. The OCaml original returns lists (cons-cell
    // allocation is cheap); the Rust port was returning `Vec` per call,
    // which meant ~N small `Vec` allocations per N-member union — costly
    // under jemalloc + Rc traffic. Collecting into one shared `Vec` keeps
    // the shape identical but avoids the per-member alloc.
    //
    // Use a `HashSet` for the seen-tvar set instead of `BTreeSet`: each
    // `OpenT` lookup becomes O(1) instead of O(log N), and for unions
    // dominated by simple `DefT` members (the common shape) the set is
    // never touched at all so the constant factor matters more than
    // memory layout.
    #[inline]
    fn flatten_into<'cx>(
        cx: &mut Context<'cx>,
        seen: &mut std::collections::HashSet<i32>,
        t: Type,
        out: &mut Vec<Type>,
    ) {
        match &*t {
            _ if let Some(node) = type_util::constraint_node_id(&t) => {
                let id = node.id();
                if seen.contains(&id) {
                    return;
                }
                seen.insert(id);
                match cx.find_constraints(id).1 {
                    constraint::Constraints::Resolved(inner) => flatten_into(cx, seen, inner, out),
                    constraint::Constraints::FullyResolved(s) => {
                        let forced = cx.force_fully_resolved_tvar(&s);
                        flatten_into(cx, seen, forced, out)
                    }
                    constraint::Constraints::Unresolved(_) => out.push(t),
                }
            }
            TypeInner::AnnotT(_, inner_t, _) => flatten_into(cx, seen, inner_t.dupe(), out),
            TypeInner::UnionT(_, rep) => {
                for inner in rep.members_iter() {
                    flatten_into(cx, seen, inner.dupe(), out);
                }
            }
            TypeInner::MaybeT(r, inner_t) => {
                out.push(Type::new(TypeInner::DefT(
                    r.dupe(),
                    DefT::new(DefTInner::NullT),
                )));
                out.push(Type::new(TypeInner::DefT(
                    r.dupe(),
                    DefT::new(DefTInner::VoidT),
                )));
                flatten_into(cx, seen, inner_t.dupe(), out);
            }
            TypeInner::OptionalT {
                reason,
                type_,
                use_desc,
            } => {
                out.push(void::why_with_use_desc(*use_desc, reason.dupe()));
                flatten_into(cx, seen, type_.dupe(), out);
            }
            TypeInner::DefT(_, def_t) if matches!(&**def_t, DefTInner::EmptyT) => {}
            TypeInner::NominalT { nominal_type, .. } => match &nominal_type.underlying_t {
                nominal::UnderlyingT::CustomError(box nominal::CustomErrorData { t, .. }) => {
                    flatten_into(cx, seen, t.dupe(), out)
                }
                _ => out.push(t),
            },
            TypeInner::EvalT {
                defer_use_t, id, ..
            } => {
                let destructor = &defer_use_t.2;
                if matches!(&**destructor, Destructor::ValuesType) {
                    match cx.evaluated().get(id) {
                        Some(cached_t) => flatten_into(cx, seen, cached_t.dupe(), out),
                        None => out.push(t),
                    }
                } else {
                    out.push(t);
                }
            }
            _ => out.push(t),
        }
    }

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for t in ts {
        flatten_into(cx, &mut seen, t, &mut out);
    }
    out
}

/// This trait should be used when trying to perform some mapping function on
/// a type. It will recurse through the structure of the type, applying it to
/// each sub-part.
///
/// Generic parameter `A` is the "map context" passed through all methods.
pub trait TypeMapper<'cx, A> {
    fn tvar(&mut self, cx: &mut Context<'cx>, map_cx: &A, r: &Reason, id: u32) -> u32;
    fn exports(&mut self, cx: &mut Context<'cx>, map_cx: &A, id: exports::Id) -> exports::Id;
    fn call_prop(&mut self, cx: &mut Context<'cx>, map_cx: &A, id: i32) -> i32;
    fn props(&mut self, cx: &mut Context<'cx>, map_cx: &A, id: properties::Id) -> properties::Id;
    fn eval_id(&mut self, cx: &mut Context<'cx>, map_cx: &A, id: eval::Id) -> eval::Id;

    fn type_(&mut self, cx: &mut Context<'cx>, map_cx: &A, t: Type) -> Type {
        type_default(self, cx, map_cx, t)
    }

    fn tout(
        &mut self,
        cx: &mut Context<'cx>,
        map_cx: &A,
        r: Reason,
        tvar_id: u32,
    ) -> (Reason, u32) {
        tout_default(self, cx, map_cx, r, tvar_id)
    }

    fn targ(&mut self, cx: &mut Context<'cx>, map_cx: &A, t: Targ) -> Targ {
        targ_default(self, cx, map_cx, t)
    }

    fn call_arg(&mut self, cx: &mut Context<'cx>, map_cx: &A, a: CallArg) -> CallArg {
        call_arg_default(self, cx, map_cx, a)
    }

    fn enum_concrete_info(
        &mut self,
        cx: &mut Context<'cx>,
        map_cx: &A,
        e: EnumConcreteInfo,
    ) -> EnumConcreteInfo {
        enum_concrete_info_default(self, cx, map_cx, e)
    }

    fn enum_info(&mut self, cx: &mut Context<'cx>, map_cx: &A, e: &EnumInfo) -> EnumInfo {
        enum_info_default(self, cx, map_cx, e)
    }

    fn def_type(&mut self, cx: &mut Context<'cx>, map_cx: &A, r: &Reason, t: &DefT) -> DefT {
        def_type_default(self, cx, map_cx, r, t)
    }

    fn canonical_renders_form(
        &mut self,
        cx: &mut Context<'cx>,
        map_cx: &A,
        form: Rc<CanonicalRendersForm>,
    ) -> Rc<CanonicalRendersForm> {
        canonical_renders_form_default(self, cx, map_cx, form)
    }

    fn defer_use_type(
        &mut self,
        cx: &mut Context<'cx>,
        map_cx: &A,
        t: TypeDestructorT,
    ) -> TypeDestructorT {
        defer_use_type_default(self, cx, map_cx, t)
    }

    fn export_types(&mut self, cx: &mut Context<'cx>, map_cx: &A, t: ExportTypes) -> ExportTypes {
        export_types_default(self, cx, map_cx, t)
    }

    fn fun_type(&mut self, cx: &mut Context<'cx>, map_cx: &A, t: Rc<FunType>) -> Rc<FunType> {
        fun_type_default(self, cx, map_cx, t)
    }

    fn inst_type(&mut self, cx: &mut Context<'cx>, map_cx: &A, i: InstType) -> InstType {
        inst_type_default(self, cx, map_cx, i)
    }

    fn instance_type(&mut self, cx: &mut Context<'cx>, map_cx: &A, t: &InstanceT) -> InstanceT {
        instance_type_default(self, cx, map_cx, t)
    }

    fn type_param(&mut self, cx: &mut Context<'cx>, map_cx: &A, t: &TypeParam) -> TypeParam {
        type_param_default(self, cx, map_cx, t)
    }

    fn selector(&mut self, cx: &mut Context<'cx>, map_cx: &A, t: Selector) -> Selector {
        selector_default(self, cx, map_cx, t)
    }

    fn destructor(
        &mut self,
        cx: &mut Context<'cx>,
        map_cx: &A,
        t: Rc<Destructor>,
    ) -> Rc<Destructor> {
        destructor_default(self, cx, map_cx, t)
    }

    fn object_kit_spread_operand_slice(
        &mut self,
        cx: &mut Context<'cx>,
        map_cx: &A,
        slice: object::spread::OperandSlice,
    ) -> object::spread::OperandSlice {
        object_kit_spread_operand_slice_default(self, cx, map_cx, slice)
    }

    fn object_kit_spread_operand(
        &mut self,
        cx: &mut Context<'cx>,
        map_cx: &A,
        operand: object::spread::Operand,
    ) -> object::spread::Operand {
        object_kit_spread_operand_default(self, cx, map_cx, operand)
    }

    fn func_type_guard(
        &mut self,
        cx: &mut Context<'cx>,
        map_cx: &A,
        type_guard: &TypeGuard,
    ) -> TypeGuard {
        func_type_guard_default(self, cx, map_cx, type_guard)
    }

    fn obj_flags(&mut self, cx: &mut Context<'cx>, map_cx: &A, flags: Flags) -> Flags {
        obj_flags_default(self, cx, map_cx, flags)
    }

    fn obj_type(&mut self, cx: &mut Context<'cx>, map_cx: &A, t: Rc<ObjType>) -> Rc<ObjType> {
        obj_type_default(self, cx, map_cx, t)
    }

    fn namespace_type(
        &mut self,
        cx: &mut Context<'cx>,
        map_cx: &A,
        t: Rc<NamespaceType>,
    ) -> Rc<NamespaceType> {
        namespace_type_default(self, cx, map_cx, t)
    }

    fn dict_type(&mut self, cx: &mut Context<'cx>, map_cx: &A, t: DictType) -> DictType {
        dict_type_default(self, cx, map_cx, t)
    }

    fn arr_type(&mut self, cx: &mut Context<'cx>, map_cx: &A, t: Rc<ArrType>) -> Rc<ArrType> {
        arr_type_default(self, cx, map_cx, t)
    }

    fn tuple_element(
        &mut self,
        cx: &mut Context<'cx>,
        map_cx: &A,
        element: TupleElement,
    ) -> TupleElement {
        tuple_element_default(self, cx, map_cx, element)
    }

    fn predicate(&mut self, cx: &mut Context<'cx>, map_cx: &A, p: &Predicate) -> Predicate {
        predicate_default(self, cx, map_cx, p)
    }

    fn prop(&mut self, cx: &mut Context<'cx>, map_cx: &A, prop: Property) -> Property {
        prop_default(self, cx, map_cx, prop)
    }
}

// =============================================================================
// Default implementations
// =============================================================================

pub fn type_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    t: Type,
) -> Type {
    match &*t {
        TypeInner::ImplicitInstantiationTvar(_) => t,
        TypeInner::OpenT(tvar) => {
            let id_prime = mapper.tvar(cx, map_cx, tvar.reason(), tvar.id());
            if id_prime == tvar.id() {
                t
            } else {
                Type::new(TypeInner::OpenT(Tvar::new(tvar.reason().dupe(), id_prime)))
            }
        }
        TypeInner::DefT(r, def_t) => {
            let def_t_prime = mapper.def_type(cx, map_cx, r, def_t);
            if def_t.ptr_eq(&def_t_prime) {
                t
            } else {
                Type::new(TypeInner::DefT(r.dupe(), def_t_prime))
            }
        }
        TypeInner::EvalT {
            type_,
            defer_use_t,
            id,
        } => {
            let type_prime = mapper.type_(cx, map_cx, type_.dupe());
            let dt_prime = mapper.defer_use_type(cx, map_cx, defer_use_t.dupe());
            let id_prime = mapper.eval_id(cx, map_cx, id.dupe());
            if type_.ptr_eq(&type_prime) && defer_use_t.ptr_eq(&dt_prime) && id_prime == *id {
                t
            } else {
                Type::new(TypeInner::EvalT {
                    type_: type_prime,
                    defer_use_t: dt_prime,
                    id: id_prime,
                })
            }
        }
        TypeInner::ThisInstanceT(box ThisInstanceTData {
            reason,
            instance,
            is_this,
            subst_name,
        }) => {
            let instance_prime = mapper.instance_type(cx, map_cx, instance);
            if instance.ptr_eq(&instance_prime) {
                t
            } else {
                Type::new(TypeInner::ThisInstanceT(Box::new(ThisInstanceTData {
                    reason: reason.dupe(),
                    instance: instance_prime,
                    is_this: *is_this,
                    subst_name: subst_name.clone(),
                })))
            }
        }
        TypeInner::ThisTypeAppT(box ThisTypeAppTData {
            reason,
            this_t,
            type_,
            targs,
        }) => {
            let this_t_prime = mapper.type_(cx, map_cx, this_t.dupe());
            let type_prime = mapper.type_(cx, map_cx, type_.dupe());
            let targs_prime = option_utils::ident_map(
                |ts: &Rc<[Type]>| {
                    list_utils::ident_map(
                        |t_arg| mapper.type_(cx, map_cx, t_arg.dupe()),
                        Type::ptr_eq,
                        ts.dupe(),
                    )
                },
                Rc::ptr_eq,
                targs.clone(),
            );
            let targs_unchanged = match (targs, &targs_prime) {
                (None, None) => true,
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                _ => false,
            };
            if this_t.ptr_eq(&this_t_prime) && type_.ptr_eq(&type_prime) && targs_unchanged {
                t
            } else {
                Type::new(TypeInner::ThisTypeAppT(Box::new(ThisTypeAppTData {
                    reason: reason.dupe(),
                    this_t: this_t_prime,
                    type_: type_prime,
                    targs: targs_prime,
                })))
            }
        }
        TypeInner::TypeAppT(box TypeAppTData {
            reason,
            use_op,
            type_,
            targs,
            from_value,
            use_desc,
        }) => {
            let type_prime = mapper.type_(cx, map_cx, type_.dupe());
            let targs_prime = list_utils::ident_map(
                |t_arg| mapper.type_(cx, map_cx, t_arg.dupe()),
                Type::ptr_eq,
                targs.dupe(),
            );
            if type_.ptr_eq(&type_prime) && Rc::ptr_eq(targs, &targs_prime) {
                t
            } else {
                Type::new(TypeInner::TypeAppT(Box::new(TypeAppTData {
                    reason: reason.dupe(),
                    use_op: use_op.clone(),
                    type_: type_prime,
                    targs: targs_prime,
                    from_value: *from_value,
                    use_desc: *use_desc,
                })))
            }
        }
        TypeInner::FunProtoT(_) | TypeInner::ObjProtoT(_) | TypeInner::NullProtoT(_) => t,
        TypeInner::GenericT(box GenericTData {
            reason,
            name,
            bound,
            no_infer,
            id,
        }) => {
            let bound_prime = mapper.type_(cx, map_cx, bound.dupe());
            if bound_prime.ptr_eq(bound) {
                t
            } else {
                Type::new(TypeInner::GenericT(Box::new(GenericTData {
                    reason: reason.dupe(),
                    name: name.clone(),
                    bound: bound_prime,
                    no_infer: *no_infer,
                    id: id.clone(),
                })))
            }
        }
        TypeInner::KeysT(r, inner_t) => {
            let inner_t_prime = mapper.type_(cx, map_cx, inner_t.dupe());
            if inner_t_prime.ptr_eq(inner_t) {
                t
            } else {
                Type::new(TypeInner::KeysT(r.dupe(), inner_t_prime))
            }
        }
        TypeInner::TemplateLiteralT {
            reason,
            from_annot,
            quasis,
            types,
        } => {
            let types_prime = list_utils::ident_map(
                |t| mapper.type_(cx, map_cx, t.dupe()),
                Type::ptr_eq,
                types.clone().into(),
            );
            let types_unchanged = types
                .iter()
                .zip(types_prime.iter())
                .all(|(a, b)| a.ptr_eq(b));
            if types_unchanged {
                t
            } else {
                Type::new(TypeInner::TemplateLiteralT {
                    reason: reason.dupe(),
                    from_annot: *from_annot,
                    quasis: quasis.to_vec(),
                    types: types_prime.to_vec(),
                })
            }
        }
        TypeInner::StringMappingT { reason, kind, arg } => {
            let arg_prime = mapper.type_(cx, map_cx, arg.dupe());
            if arg_prime.ptr_eq(arg) {
                t
            } else {
                Type::new(TypeInner::StringMappingT {
                    reason: reason.dupe(),
                    kind: *kind,
                    arg: arg_prime,
                })
            }
        }
        TypeInner::AnnotT(r, inner_t, use_desc) => {
            let inner_t_prime = mapper.type_(cx, map_cx, inner_t.dupe());
            if inner_t_prime.ptr_eq(inner_t) {
                t
            } else {
                Type::new(TypeInner::AnnotT(r.dupe(), inner_t_prime, *use_desc))
            }
        }
        TypeInner::NominalT {
            reason,
            nominal_type,
        } => {
            let mut underlying_t_changed = false;
            let underlying_t = match &nominal_type.underlying_t {
                nominal::UnderlyingT::OpaqueWithLocal { t: inner } => {
                    let inner_prime = mapper.type_(cx, map_cx, inner.dupe());
                    if inner.ptr_eq(&inner_prime) {
                        nominal_type.underlying_t.clone()
                    } else {
                        underlying_t_changed = true;
                        nominal::UnderlyingT::OpaqueWithLocal { t: inner_prime }
                    }
                }
                nominal::UnderlyingT::CustomError(box nominal::CustomErrorData {
                    custom_error_loc,
                    t: inner,
                }) => {
                    let inner_prime = mapper.type_(cx, map_cx, inner.dupe());
                    if inner.ptr_eq(&inner_prime) {
                        nominal_type.underlying_t.clone()
                    } else {
                        underlying_t_changed = true;
                        nominal::UnderlyingT::CustomError(Box::new(nominal::CustomErrorData {
                            custom_error_loc: custom_error_loc.clone(),
                            t: inner_prime,
                        }))
                    }
                }
                nominal::UnderlyingT::FullyOpaque => nominal_type.underlying_t.clone(),
            };
            let lower_t = option_utils::ident_map(
                |lt| mapper.type_(cx, map_cx, lt.dupe()),
                Type::ptr_eq,
                nominal_type.lower_t.clone(),
            );
            let lower_t_unchanged = match (&nominal_type.lower_t, &lower_t) {
                (None, None) => true,
                (Some(a), Some(b)) => a.ptr_eq(b),
                _ => false,
            };
            let upper_t = option_utils::ident_map(
                |ut| mapper.type_(cx, map_cx, ut.dupe()),
                Type::ptr_eq,
                nominal_type.upper_t.clone(),
            );
            let upper_t_unchanged = match (&nominal_type.upper_t, &upper_t) {
                (None, None) => true,
                (Some(a), Some(b)) => a.ptr_eq(b),
                _ => false,
            };
            let nominal_type_args = list_utils::ident_map(
                |x| {
                    let (s, r, t, p) = x;
                    let t_prime = mapper.type_(cx, map_cx, t.dupe());
                    if t.ptr_eq(&t_prime) {
                        x.dupe()
                    } else {
                        (s.dupe(), r.dupe(), t_prime, *p)
                    }
                },
                |a, b| a.2.ptr_eq(&b.2),
                nominal_type.nominal_type_args.dupe(),
            );
            if !underlying_t_changed
                && lower_t_unchanged
                && upper_t_unchanged
                && Rc::ptr_eq(&nominal_type.nominal_type_args, &nominal_type_args)
            {
                t
            } else {
                Type::new(TypeInner::NominalT {
                    reason: reason.dupe(),
                    nominal_type: Rc::new(NominalType::new(NominalTypeInner {
                        nominal_id: nominal_type.nominal_id.clone(),
                        underlying_t,
                        lower_t,
                        upper_t,
                        nominal_type_args,
                    })),
                })
            }
        }
        TypeInner::NamespaceT(namespace_t) => {
            let namespace_t_prime = mapper.namespace_type(cx, map_cx, namespace_t.dupe());
            if Rc::ptr_eq(&namespace_t_prime, namespace_t) {
                t
            } else {
                Type::new(TypeInner::NamespaceT(namespace_t_prime))
            }
        }
        TypeInner::AnyT(_, _) => t,
        TypeInner::OptionalT {
            reason,
            type_,
            use_desc,
        } => {
            let type_prime = mapper.type_(cx, map_cx, type_.dupe());
            if type_prime.ptr_eq(type_) {
                t
            } else {
                Type::new(TypeInner::OptionalT {
                    reason: reason.dupe(),
                    type_: type_prime,
                    use_desc: *use_desc,
                })
            }
        }
        TypeInner::MaybeT(r, inner_t) => {
            let inner_t_prime = mapper.type_(cx, map_cx, inner_t.dupe());
            if inner_t_prime.ptr_eq(inner_t) {
                t
            } else {
                Type::new(TypeInner::MaybeT(r.dupe(), inner_t_prime))
            }
        }
        TypeInner::IntersectionT(r, irep) => {
            let mut members_prime = Vec::new();
            let mut changed = false;
            for member in irep.members_iter() {
                let member_prime = mapper.type_(cx, map_cx, member.dupe());
                changed = changed || !member_prime.ptr_eq(member);
                members_prime.push(member_prime);
            }
            if changed {
                let mut iter = members_prime.into_iter();
                let t0 = iter.next().unwrap();
                let t1 = iter.next().unwrap();
                let ts = iter.collect();
                let irep_prime = inter_rep::make(t0, t1, ts);
                Type::new(TypeInner::IntersectionT(r.dupe(), irep_prime))
            } else {
                t
            }
        }
        TypeInner::UnionT(r, urep) => {
            let mut members_prime = Vec::new();
            let mut changed = false;
            for member in urep.members_iter() {
                let member_prime = mapper.type_(cx, map_cx, member.dupe());
                changed = changed || !member_prime.ptr_eq(member);
                members_prime.push(member_prime);
            }
            if changed {
                let mut iter = members_prime.into_iter();
                let t0 = iter.next().unwrap();
                let t1 = iter.next().unwrap();
                let ts = iter.collect();
                let urep_prime = union_rep::make(None, urep.union_kind(), t0, t1, ts);
                Type::new(TypeInner::UnionT(r.dupe(), urep_prime))
            } else {
                t
            }
        }
    }
}

pub fn tout_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    r: Reason,
    tvar_id: u32,
) -> (Reason, u32) {
    let tvar_prime = mapper.tvar(cx, map_cx, &r, tvar_id);
    if tvar_prime == tvar_id {
        (r, tvar_id)
    } else {
        (r, tvar_prime)
    }
}

pub fn targ_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    t: Targ,
) -> Targ {
    match t {
        Targ::ImplicitArg(_) => t,
        Targ::ExplicitArg(ref inner_t) => {
            let inner_t_prime = mapper.type_(cx, map_cx, inner_t.dupe());
            if inner_t.ptr_eq(&inner_t_prime) {
                t
            } else {
                Targ::ExplicitArg(inner_t_prime)
            }
        }
    }
}

pub fn call_arg_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    a: CallArg,
) -> CallArg {
    match &*a {
        CallArgInner::Arg(inner_t) => {
            let inner_t_prime = mapper.type_(cx, map_cx, inner_t.dupe());
            if inner_t.ptr_eq(&inner_t_prime) {
                a
            } else {
                CallArg::arg(inner_t_prime)
            }
        }
        CallArgInner::SpreadArg(inner_t) => {
            let inner_t_prime = mapper.type_(cx, map_cx, inner_t.dupe());
            if inner_t.ptr_eq(&inner_t_prime) {
                a
            } else {
                CallArg::spread_arg(inner_t_prime)
            }
        }
    }
}

pub fn enum_concrete_info_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    e: EnumConcreteInfo,
) -> EnumConcreteInfo {
    let representation_t_prime = mapper.type_(cx, map_cx, e.representation_t.dupe());
    if e.representation_t.ptr_eq(&representation_t_prime) {
        e
    } else {
        EnumConcreteInfo::new(EnumConcreteInfoInner {
            enum_name: e.enum_name.dupe(),
            enum_id: e.enum_id.dupe(),
            members: e.members.dupe(),
            representation_t: representation_t_prime,
            has_unknown_members: e.has_unknown_members,
        })
    }
}

pub fn enum_info_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    e: &EnumInfo,
) -> EnumInfo {
    match e.deref() {
        EnumInfoInner::ConcreteEnum(concrete_info) => {
            let concrete_info_prime = mapper.enum_concrete_info(cx, map_cx, concrete_info.dupe());
            if concrete_info.ptr_eq(&concrete_info_prime) {
                e.dupe()
            } else {
                EnumInfo::new(EnumInfoInner::ConcreteEnum(concrete_info_prime))
            }
        }
        EnumInfoInner::AbstractEnum { representation_t } => {
            let representation_t_prime = mapper.type_(cx, map_cx, representation_t.dupe());
            if representation_t.ptr_eq(&representation_t_prime) {
                e.dupe()
            } else {
                EnumInfo::new(EnumInfoInner::AbstractEnum {
                    representation_t: representation_t_prime,
                })
            }
        }
    }
}

pub fn def_type_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    _r: &Reason,
    t: &DefT,
) -> DefT {
    match t.deref() {
        DefTInner::NumGeneralT(_)
        | DefTInner::StrGeneralT(_)
        | DefTInner::BoolGeneralT
        | DefTInner::BigIntGeneralT(_)
        | DefTInner::EmptyT
        | DefTInner::MixedT(_)
        | DefTInner::SymbolT
        | DefTInner::UniqueSymbolT(_)
        | DefTInner::NullT
        | DefTInner::VoidT => t.dupe(),
        DefTInner::FunT(s, f) => {
            let s_prime = mapper.type_(cx, map_cx, s.dupe());
            let f_prime = mapper.fun_type(cx, map_cx, f.dupe());
            if s.ptr_eq(&s_prime) && Rc::ptr_eq(f, &f_prime) {
                t.dupe()
            } else {
                DefT::new(DefTInner::FunT(s_prime, f_prime))
            }
        }
        DefTInner::ObjT(objtype) => {
            let objtype_prime = mapper.obj_type(cx, map_cx, objtype.dupe());
            if Rc::ptr_eq(objtype, &objtype_prime) {
                t.dupe()
            } else {
                DefT::new(DefTInner::ObjT(objtype_prime))
            }
        }
        DefTInner::ArrT(arrtype) => {
            let arrtype_prime = mapper.arr_type(cx, map_cx, arrtype.dupe());
            if Rc::ptr_eq(arrtype, &arrtype_prime) {
                t.dupe()
            } else {
                DefT::new(DefTInner::ArrT(arrtype_prime))
            }
        }
        DefTInner::ClassT(inner_t) => {
            let inner_t_prime = mapper.type_(cx, map_cx, inner_t.dupe());
            if inner_t.ptr_eq(&inner_t_prime) {
                t.dupe()
            } else {
                DefT::new(DefTInner::ClassT(inner_t_prime))
            }
        }
        DefTInner::EnumValueT(enum_info) => {
            let enum_info_prime = mapper.enum_info(cx, map_cx, enum_info);
            if enum_info.ptr_eq(&enum_info_prime) {
                t.dupe()
            } else {
                DefT::new(DefTInner::EnumValueT(Rc::new(enum_info_prime)))
            }
        }
        DefTInner::EnumObjectT {
            enum_value_t,
            enum_info,
        } => {
            let enum_value_t_prime = mapper.type_(cx, map_cx, enum_value_t.dupe());
            let enum_info_prime = mapper.enum_info(cx, map_cx, enum_info);
            if enum_value_t.ptr_eq(&enum_value_t_prime) && enum_info.ptr_eq(&enum_info_prime) {
                t.dupe()
            } else {
                DefT::new(DefTInner::EnumObjectT {
                    enum_value_t: enum_value_t_prime,
                    enum_info: Rc::new(enum_info_prime),
                })
            }
        }
        DefTInner::InstanceT(instance_t) => {
            let instance_t_prime = mapper.instance_type(cx, map_cx, instance_t);
            if instance_t.ptr_eq(&instance_t_prime) {
                t.dupe()
            } else {
                DefT::new(DefTInner::InstanceT(Rc::new(instance_t_prime)))
            }
        }
        DefTInner::NumericStrKeyT(_)
        | DefTInner::SingletonStrT { .. }
        | DefTInner::SingletonNumT { .. }
        | DefTInner::SingletonBoolT { .. }
        | DefTInner::SingletonBigIntT { .. } => t.dupe(),
        DefTInner::TypeT(kind, inner_t) => {
            let inner_t_prime = mapper.type_(cx, map_cx, inner_t.dupe());
            if inner_t.ptr_eq(&inner_t_prime) {
                t.dupe()
            } else {
                DefT::new(DefTInner::TypeT(*kind, inner_t_prime))
            }
        }
        DefTInner::PolyT(box PolyTData {
            kind: poly_kind,
            tparams_loc,
            tparams,
            t_out,
            id: _,
            strictness_kind,
        }) => {
            let mut tparams_changed = false;
            let tparams_prime: Rc<[TypeParam]> = tparams
                .iter()
                .map(|tp| {
                    let tp_prime = mapper.type_param(cx, map_cx, tp);
                    if !tp.ptr_eq(&tp_prime) {
                        tparams_changed = true;
                    }
                    tp_prime
                })
                .collect();
            let t_out_prime = mapper.type_(cx, map_cx, t_out.dupe());
            if !tparams_changed && t_out.ptr_eq(&t_out_prime) {
                t.dupe()
            } else {
                DefT::new(DefTInner::PolyT(Box::new(PolyTData {
                    kind: *poly_kind,
                    tparams_loc: tparams_loc.clone(),
                    tparams: tparams_prime,
                    t_out: t_out_prime,
                    id: poly::Id::generate_id(),
                    strictness_kind: *strictness_kind,
                })))
            }
        }
        DefTInner::ReactAbstractComponentT(box ReactAbstractComponentTData {
            config,
            renders,
            component_kind,
        }) => {
            let config_prime = mapper.type_(cx, map_cx, config.dupe());
            let renders_prime = mapper.type_(cx, map_cx, renders.dupe());
            let mut component_kind_changed = false;
            let component_kind_prime = match component_kind {
                ComponentKind::Structural => ComponentKind::Structural,
                ComponentKind::Nominal(id, s, ts) => {
                    let ts_prime = option_utils::ident_map(
                        |types: &Rc<[Type]>| {
                            list_utils::ident_map(
                                |t_arg| mapper.type_(cx, map_cx, t_arg.dupe()),
                                Type::ptr_eq,
                                types.dupe(),
                            )
                        },
                        Rc::ptr_eq,
                        ts.clone(),
                    );
                    let ts_unchanged = match (ts, &ts_prime) {
                        (None, None) => true,
                        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                        _ => false,
                    };
                    if ts_unchanged {
                        component_kind.clone()
                    } else {
                        component_kind_changed = true;
                        ComponentKind::Nominal(id.clone(), s.dupe(), ts_prime)
                    }
                }
            };
            if config.ptr_eq(&config_prime)
                && renders.ptr_eq(&renders_prime)
                && !component_kind_changed
            {
                t.dupe()
            } else {
                DefT::new(DefTInner::ReactAbstractComponentT(Box::new(
                    ReactAbstractComponentTData {
                        config: config_prime,
                        renders: renders_prime,
                        component_kind: component_kind_prime,
                    },
                )))
            }
        }
        DefTInner::RendersT(canonical_form) => {
            let canonical_form_prime =
                mapper.canonical_renders_form(cx, map_cx, canonical_form.dupe());
            if Rc::ptr_eq(canonical_form, &canonical_form_prime) {
                t.dupe()
            } else {
                DefT::new(DefTInner::RendersT(canonical_form_prime))
            }
        }
    }
}

pub fn canonical_renders_form_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    form: Rc<CanonicalRendersForm>,
) -> Rc<CanonicalRendersForm> {
    match &*form {
        CanonicalRendersForm::IntrinsicRenders(_) => form,
        CanonicalRendersForm::NominalRenders {
            renders_id,
            renders_name,
            renders_super,
        } => {
            let renders_super_prime = mapper.type_(cx, map_cx, renders_super.dupe());
            if renders_super_prime.ptr_eq(renders_super) {
                form
            } else {
                Rc::new(CanonicalRendersForm::NominalRenders {
                    renders_id: renders_id.clone(),
                    renders_name: renders_name.dupe(),
                    renders_super: renders_super_prime,
                })
            }
        }
        CanonicalRendersForm::StructuralRenders {
            renders_variant,
            renders_structural_type,
        } => {
            let renders_structural_type_prime =
                mapper.type_(cx, map_cx, renders_structural_type.dupe());
            if renders_structural_type_prime.ptr_eq(renders_structural_type) {
                form
            } else {
                Rc::new(CanonicalRendersForm::StructuralRenders {
                    renders_variant: renders_variant.clone(),
                    renders_structural_type: renders_structural_type_prime,
                })
            }
        }
        CanonicalRendersForm::DefaultRenders => form,
    }
}

pub fn defer_use_type_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    t: TypeDestructorT,
) -> TypeDestructorT {
    let d_prime = mapper.destructor(cx, map_cx, t.2.dupe());
    if Rc::ptr_eq(&t.2, &d_prime) {
        t
    } else {
        TypeDestructorT::new(TypeDestructorTInner(t.0.clone(), t.1.dupe(), d_prime))
    }
}

pub fn export_types_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    t: ExportTypes,
) -> ExportTypes {
    let value_exports_tmap_prime = mapper.exports(cx, map_cx, t.value_exports_tmap);
    let type_exports_tmap_prime = mapper.exports(cx, map_cx, t.type_exports_tmap);
    let cjs_export_prime = option_utils::ident_map(
        |export| {
            let (loc_opt, inner_t) = export;
            let inner_t_prime = mapper.type_(cx, map_cx, inner_t.dupe());
            if inner_t.ptr_eq(&inner_t_prime) {
                export.clone()
            } else {
                (loc_opt.clone(), inner_t_prime)
            }
        },
        |a, b| a.1.ptr_eq(&b.1),
        t.cjs_export.clone(),
    );
    let cjs_export_unchanged = match (&t.cjs_export, &cjs_export_prime) {
        (None, None) => true,
        (Some((_, a)), Some((_, b))) => a.ptr_eq(b),
        _ => false,
    };
    if value_exports_tmap_prime == t.value_exports_tmap
        && type_exports_tmap_prime == t.type_exports_tmap
        && cjs_export_unchanged
    {
        t
    } else {
        ExportTypes {
            value_exports_tmap: value_exports_tmap_prime,
            type_exports_tmap: type_exports_tmap_prime,
            cjs_export: cjs_export_prime,
            has_every_named_export: t.has_every_named_export,
        }
    }
}

pub fn fun_type_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    t: Rc<FunType>,
) -> Rc<FunType> {
    let this_prime = mapper.type_(cx, map_cx, t.this_t.0.dupe());
    let params_prime = list_utils::ident_map(
        |param| {
            let FunParam(name, inner_t) = param;
            let inner_t_prime = mapper.type_(cx, map_cx, inner_t.dupe());
            if inner_t.ptr_eq(&inner_t_prime) {
                param.dupe()
            } else {
                FunParam(name.dupe(), inner_t_prime)
            }
        },
        |a, b| a.1.ptr_eq(&b.1),
        t.params.dupe(),
    );
    let mut rest_param_changed = false;
    let rest_param_prime = t
        .rest_param
        .as_ref()
        .map(|FunRestParam(name, loc, inner_t)| {
            let inner_t_prime = mapper.type_(cx, map_cx, inner_t.dupe());
            if !inner_t.ptr_eq(&inner_t_prime) {
                rest_param_changed = true;
            }
            FunRestParam(name.dupe(), loc.clone(), inner_t_prime)
        });
    let type_guard_prime = option_utils::ident_map(
        |tg| mapper.func_type_guard(cx, map_cx, tg),
        TypeGuard::ptr_eq,
        t.type_guard.clone(),
    );
    let type_guard_unchanged = match (&t.type_guard, &type_guard_prime) {
        (None, None) => true,
        (Some(a), Some(b)) => a.ptr_eq(b),
        _ => false,
    };
    let return_t_prime = mapper.type_(cx, map_cx, t.return_t.dupe());
    if t.this_t.0.ptr_eq(&this_prime)
        && t.return_t.ptr_eq(&return_t_prime)
        && Rc::ptr_eq(&t.params, &params_prime)
        && !rest_param_changed
        && type_guard_unchanged
    {
        t
    } else {
        Rc::new(FunType {
            this_t: (this_prime, t.this_t.1.clone()),
            params: params_prime,
            rest_param: rest_param_prime,
            return_t: return_t_prime,
            type_guard: type_guard_prime,
            def_reason: t.def_reason.dupe(),
            function_reference_kind: t.function_reference_kind,
            effect_: t.effect_.clone(),
            strictness_kind: t.strictness_kind,
            literal: t.literal,
        })
    }
}

pub fn inst_type_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    i: InstType,
) -> InstType {
    let type_args_prime = list_utils::ident_map(
        |x| {
            let (s, r, t, p) = x;
            let t_prime = mapper.type_(cx, map_cx, t.dupe());
            if t.ptr_eq(&t_prime) {
                x.dupe()
            } else {
                (s.dupe(), r.dupe(), t_prime, *p)
            }
        },
        |a, b| a.2.ptr_eq(&b.2),
        i.type_args.dupe(),
    );
    let own_props_prime = mapper.props(cx, map_cx, i.own_props.dupe());
    let proto_props_prime = mapper.props(cx, map_cx, i.proto_props.dupe());
    let inst_call_t_prime = option_utils::ident_map(
        |id| mapper.call_prop(cx, map_cx, *id),
        |a, b| a == b,
        i.inst_call_t,
    );
    let inst_construct_t_prime = option_utils::ident_map(
        |id| mapper.call_prop(cx, map_cx, *id),
        |a, b| a == b,
        i.inst_construct_t,
    );
    let inst_dict_prime = option_utils::ident_map(
        |d| mapper.dict_type(cx, map_cx, d.clone()),
        |a, b| a == b,
        i.inst_dict.clone(),
    );
    let class_private_fields_prime = mapper.props(cx, map_cx, i.class_private_fields.dupe());
    let class_private_static_fields_prime =
        mapper.props(cx, map_cx, i.class_private_static_fields.dupe());
    let class_private_methods_prime = mapper.props(cx, map_cx, i.class_private_methods.dupe());
    let class_private_static_methods_prime =
        mapper.props(cx, map_cx, i.class_private_static_methods.dupe());

    if Rc::ptr_eq(&i.type_args, &type_args_prime)
        && own_props_prime == i.own_props
        && proto_props_prime == i.proto_props
        && i.inst_call_t == inst_call_t_prime
        && i.inst_construct_t == inst_construct_t_prime
        && i.inst_dict == inst_dict_prime
        && class_private_fields_prime == i.class_private_fields
        && class_private_static_fields_prime == i.class_private_static_fields
        && class_private_methods_prime == i.class_private_methods
        && class_private_static_methods_prime == i.class_private_static_methods
    {
        i
    } else {
        InstType::new(InstTypeInner {
            class_id: i.class_id.dupe(),
            class_name: i.class_name.dupe(),
            type_args: type_args_prime,
            own_props: own_props_prime,
            proto_props: proto_props_prime,
            inst_call_t: inst_call_t_prime,
            inst_construct_t: inst_construct_t_prime,
            initialized_fields: i.initialized_fields.clone(),
            initialized_static_fields: i.initialized_static_fields.clone(),
            inst_kind: i.inst_kind.clone(),
            inst_dict: inst_dict_prime,
            class_private_fields: class_private_fields_prime,
            class_private_static_fields: class_private_static_fields_prime,
            class_private_methods: class_private_methods_prime,
            class_private_static_methods: class_private_static_methods_prime,
            inst_react_dro: i.inst_react_dro.clone(),
            inst_abstract: i.inst_abstract,
            inst_abstract_props: i.inst_abstract_props.clone(),
            strictness_kind: i.strictness_kind,
        })
    }
}

pub fn instance_type_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    t: &InstanceT,
) -> InstanceT {
    let static_prime = mapper.type_(cx, map_cx, t.static_.dupe());
    let super_prime = mapper.type_(cx, map_cx, t.super_.dupe());
    let implements_prime = list_utils::ident_map(
        |impl_t| mapper.type_(cx, map_cx, impl_t.dupe()),
        Type::ptr_eq,
        t.implements.dupe(),
    );
    let inst_prime = mapper.inst_type(cx, map_cx, t.inst.clone());
    if t.static_.ptr_eq(&static_prime)
        && t.super_.ptr_eq(&super_prime)
        && Rc::ptr_eq(&t.implements, &implements_prime)
        && t.inst.ptr_eq(&inst_prime)
    {
        t.dupe()
    } else {
        InstanceT::new(InstanceTInner {
            static_: static_prime,
            super_: super_prime,
            implements: implements_prime,
            inst: inst_prime,
        })
    }
}

pub fn type_param_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    t: &TypeParam,
) -> TypeParam {
    let bound_prime = mapper.type_(cx, map_cx, t.bound.dupe());
    let default_prime = option_utils::ident_map(
        |d| mapper.type_(cx, map_cx, d.dupe()),
        Type::ptr_eq,
        t.default.clone(),
    );
    let default_unchanged = match (&t.default, &default_prime) {
        (None, None) => true,
        (Some(a), Some(b)) => a.ptr_eq(b),
        _ => false,
    };
    if t.bound.ptr_eq(&bound_prime) && default_unchanged {
        t.dupe()
    } else {
        TypeParam::new(TypeParamInner {
            reason: t.reason.dupe(),
            name: t.name.clone(),
            bound: bound_prime,
            polarity: t.polarity,
            default: default_prime,
            is_this: t.is_this,
            is_const: t.is_const,
        })
    }
}

pub fn selector_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    t: Selector,
) -> Selector {
    match t {
        Selector::Prop(_, _) => t,
        Selector::Elem(ref inner_t) => {
            let inner_t_prime = mapper.type_(cx, map_cx, inner_t.dupe());
            if inner_t.ptr_eq(&inner_t_prime) {
                t
            } else {
                Selector::Elem(inner_t_prime)
            }
        }
        Selector::ObjRest(_) | Selector::ArrRest(_) | Selector::Default => t,
    }
}

pub fn destructor_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    t: Rc<Destructor>,
) -> Rc<Destructor> {
    match &*t {
        Destructor::ReactCheckComponentConfig { props } => {
            let props_prime = props.ident_map(|prop| mapper.prop(cx, map_cx, prop.dupe()));
            if props.ptr_eq(&props_prime) {
                t.dupe()
            } else {
                Rc::new(Destructor::ReactCheckComponentConfig { props: props_prime })
            }
        }
        Destructor::NormalizeIntersection { .. }
        | Destructor::ReactDRO(_)
        | Destructor::NonMaybeType
        | Destructor::PropertyType { .. }
        | Destructor::OptionalIndexedAccessResultType { .. } => t.dupe(),
        Destructor::OptionalIndexedAccessNonMaybeType {
            index: OptionalIndexedAccessIndex::OptionalIndexedAccessStrLitIndex(_),
        } => t.dupe(),
        Destructor::ElementType { index_type } => {
            let index_type_prime = mapper.type_(cx, map_cx, index_type.dupe());
            if index_type.ptr_eq(&index_type_prime) {
                t.dupe()
            } else {
                Rc::new(Destructor::ElementType {
                    index_type: index_type_prime,
                })
            }
        }
        Destructor::OptionalIndexedAccessNonMaybeType {
            index: OptionalIndexedAccessIndex::OptionalIndexedAccessTypeIndex(index_type),
        } => {
            let index_type_prime = mapper.type_(cx, map_cx, index_type.dupe());
            if index_type.ptr_eq(&index_type_prime) {
                t.dupe()
            } else {
                Rc::new(Destructor::OptionalIndexedAccessNonMaybeType {
                    index: OptionalIndexedAccessIndex::OptionalIndexedAccessTypeIndex(
                        index_type_prime,
                    ),
                })
            }
        }
        Destructor::ExactType
        | Destructor::ReadOnlyType
        | Destructor::RequiredType
        | Destructor::PartialType
        | Destructor::EnumType => t.dupe(),
        Destructor::SpreadType(box DestructorSpreadTypeData(options, tlist, acc)) => {
            let (tlist_prime, tlist_changed) = list_utils::ident_map_ocaml_list(
                |op| mapper.object_kit_spread_operand(cx, map_cx, op.clone()),
                |a, b| match (a, b) {
                    (object::spread::Operand::Slice(sa), object::spread::Operand::Slice(sb)) => {
                        sa.ptr_eq(sb)
                    }
                    (object::spread::Operand::Type(ta), object::spread::Operand::Type(tb)) => {
                        ta.ptr_eq(tb)
                    }
                    _ => false,
                },
                tlist.dupe(),
            );
            let acc_prime = option_utils::ident_map(
                |a| mapper.object_kit_spread_operand_slice(cx, map_cx, a.clone()),
                |a, b| a.ptr_eq(b),
                acc.clone(),
            );
            let acc_unchanged = match (acc, &acc_prime) {
                (None, None) => true,
                (Some(a), Some(b)) => a.ptr_eq(b),
                _ => false,
            };
            if !tlist_changed && acc_unchanged {
                t.dupe()
            } else {
                Rc::new(Destructor::SpreadType(Box::new(DestructorSpreadTypeData(
                    options.clone(),
                    tlist_prime,
                    acc_prime,
                ))))
            }
        }
        Destructor::SpreadTupleType(box DestructorSpreadTupleTypeData {
            reason_tuple,
            reason_spread,
            inexact,
            resolved,
            unresolved,
        }) => {
            let mut unresolved_changed = false;
            let unresolved_prime: flow_data_structure_wrapper::list::FlowOcamlList<
                UnresolvedParam,
            > = unresolved
                .iter()
                .map(|el| match el {
                    UnresolvedParam::UnresolvedArg(box UnresolvedArgData(element, generic)) => {
                        let element_prime = mapper.tuple_element(cx, map_cx, element.clone());
                        if !element.t.ptr_eq(&element_prime.t) {
                            unresolved_changed = true;
                        }
                        UnresolvedParam::UnresolvedArg(Box::new(UnresolvedArgData(
                            element_prime,
                            generic.clone(),
                        )))
                    }
                    UnresolvedParam::UnresolvedSpreadArg(inner_t) => {
                        let inner_t_prime = mapper.type_(cx, map_cx, inner_t.dupe());
                        if !inner_t.ptr_eq(&inner_t_prime) {
                            unresolved_changed = true;
                        }
                        UnresolvedParam::UnresolvedSpreadArg(inner_t_prime)
                    }
                })
                .collect();
            let mut resolved_changed = false;
            let resolved_prime: flow_data_structure_wrapper::list::FlowOcamlList<ResolvedParam> =
                resolved
                    .iter()
                    .map(|el| match el {
                        ResolvedParam::ResolvedArg(box ResolvedArgData(element, generic)) => {
                            let element_prime = mapper.tuple_element(cx, map_cx, element.clone());
                            if !element.t.ptr_eq(&element_prime.t) {
                                resolved_changed = true;
                            }
                            ResolvedParam::ResolvedArg(Box::new(ResolvedArgData(
                                element_prime,
                                generic.clone(),
                            )))
                        }
                        ResolvedParam::ResolvedSpreadArg(box ResolvedSpreadArgData(
                            reason,
                            arr,
                            generic,
                        )) => {
                            let arr_rc = Rc::new(arr.clone());
                            let arr_prime = mapper.arr_type(cx, map_cx, arr_rc.dupe());
                            if !Rc::ptr_eq(&arr_rc, &arr_prime) {
                                resolved_changed = true;
                            }
                            ResolvedParam::ResolvedSpreadArg(Box::new(ResolvedSpreadArgData(
                                reason.dupe(),
                                (*arr_prime).clone(),
                                generic.clone(),
                            )))
                        }
                        ResolvedParam::ResolvedAnySpreadArg(_, _) => el.clone(),
                    })
                    .collect();
            if !unresolved_changed && !resolved_changed {
                t.dupe()
            } else {
                Rc::new(Destructor::SpreadTupleType(Box::new(
                    DestructorSpreadTupleTypeData {
                        reason_tuple: reason_tuple.dupe(),
                        reason_spread: reason_spread.dupe(),
                        inexact: *inexact,
                        resolved: resolved_prime,
                        unresolved: unresolved_prime,
                    },
                )))
            }
        }
        Destructor::RestType(options, x) => {
            let x_prime = mapper.type_(cx, map_cx, x.dupe());
            if x.ptr_eq(&x_prime) {
                t.dupe()
            } else {
                Rc::new(Destructor::RestType(options.clone(), x_prime))
            }
        }
        Destructor::ValuesType => t.dupe(),
        Destructor::ConditionalType(box DestructorConditionalTypeData {
            distributive_tparam_name,
            infer_tparams,
            extends_t,
            true_t,
            false_t,
        }) => {
            let infer_tparams_prime = list_utils::ident_map(
                |tp| mapper.type_param(cx, map_cx, tp),
                TypeParam::ptr_eq,
                infer_tparams.dupe(),
            );
            let extends_t_prime = mapper.type_(cx, map_cx, extends_t.dupe());
            let true_t_prime = mapper.type_(cx, map_cx, true_t.dupe());
            let false_t_prime = mapper.type_(cx, map_cx, false_t.dupe());
            if Rc::ptr_eq(infer_tparams, &infer_tparams_prime)
                && extends_t.ptr_eq(&extends_t_prime)
                && true_t.ptr_eq(&true_t_prime)
                && false_t.ptr_eq(&false_t_prime)
            {
                t.dupe()
            } else {
                Rc::new(Destructor::ConditionalType(Box::new(
                    DestructorConditionalTypeData {
                        distributive_tparam_name: distributive_tparam_name.clone(),
                        infer_tparams: infer_tparams_prime,
                        extends_t: extends_t_prime,
                        true_t: true_t_prime,
                        false_t: false_t_prime,
                    },
                )))
            }
        }
        Destructor::TypeMap(TypeMap::ObjectKeyMirror) => t.dupe(),
        Destructor::MappedType(box DestructorMappedTypeData {
            property_type,
            name_type,
            mapped_type_flags,
            homomorphic,
            distributive_tparam_name,
        }) => {
            let property_type_prime = mapper.type_(cx, map_cx, property_type.dupe());
            let name_type_prime = match name_type {
                None => None,
                Some(nt) => {
                    let nt_prime = mapper.type_(cx, map_cx, nt.dupe());
                    if nt.ptr_eq(&nt_prime) {
                        name_type.clone()
                    } else {
                        Some(nt_prime)
                    }
                }
            };
            let mut homomorphic_changed = false;
            let homomorphic_prime = match homomorphic {
                MappedTypeHomomorphicFlag::SemiHomomorphic(inner_t) => {
                    let inner_t_prime = mapper.type_(cx, map_cx, inner_t.dupe());
                    if !inner_t.ptr_eq(&inner_t_prime) {
                        homomorphic_changed = true;
                    }
                    MappedTypeHomomorphicFlag::SemiHomomorphic(inner_t_prime)
                }
                MappedTypeHomomorphicFlag::Homomorphic => MappedTypeHomomorphicFlag::Homomorphic,
                MappedTypeHomomorphicFlag::Unspecialized => {
                    MappedTypeHomomorphicFlag::Unspecialized
                }
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
                t.dupe()
            } else {
                Rc::new(Destructor::MappedType(Box::new(DestructorMappedTypeData {
                    property_type: property_type_prime,
                    name_type: name_type_prime,
                    mapped_type_flags: mapped_type_flags.clone(),
                    homomorphic: homomorphic_prime,
                    distributive_tparam_name: distributive_tparam_name.clone(),
                })))
            }
        }
        Destructor::ReactElementConfigType => t.dupe(),
    }
}

pub fn object_kit_spread_operand_slice_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    slice: object::spread::OperandSlice,
) -> object::spread::OperandSlice {
    let prop_map_prime = slice
        .prop_map
        .ident_map(|prop| mapper.prop(cx, map_cx, prop.dupe()));
    let dict_prime = option_utils::ident_map(
        |d| mapper.dict_type(cx, map_cx, d.clone()),
        |a, b| a == b,
        slice.dict.clone(),
    );
    let reachable_targs_prime = list_utils::ident_map(
        |tup| {
            let (inner_t, p) = tup;
            let inner_t_prime = mapper.type_(cx, map_cx, inner_t.dupe());
            if inner_t.ptr_eq(&inner_t_prime) {
                tup.dupe()
            } else {
                (inner_t_prime, *p)
            }
        },
        |a, b| a.0.ptr_eq(&b.0),
        slice.reachable_targs.dupe(),
    );
    if slice.prop_map.ptr_eq(&prop_map_prime)
        && slice.dict == dict_prime
        && Rc::ptr_eq(&slice.reachable_targs, &reachable_targs_prime)
    {
        slice
    } else {
        object::spread::OperandSlice::new(object::spread::OperandSliceInner {
            reason: slice.reason.dupe(),
            prop_map: prop_map_prime,
            dict: dict_prime,
            generics: slice.generics.clone(),
            reachable_targs: reachable_targs_prime,
        })
    }
}

pub fn object_kit_spread_operand_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    operand: object::spread::Operand,
) -> object::spread::Operand {
    match operand {
        object::spread::Operand::Slice(ref slice) => {
            let slice_prime = mapper.object_kit_spread_operand_slice(cx, map_cx, slice.clone());
            if slice.ptr_eq(&slice_prime) {
                operand
            } else {
                object::spread::Operand::Slice(slice_prime)
            }
        }
        object::spread::Operand::Type(ref inner_t) => {
            let inner_t_prime = mapper.type_(cx, map_cx, inner_t.dupe());
            if inner_t.ptr_eq(&inner_t_prime) {
                operand
            } else {
                object::spread::Operand::Type(inner_t_prime)
            }
        }
    }
}

pub fn func_type_guard_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    type_guard: &TypeGuard,
) -> TypeGuard {
    let tg_prime = type_guard
        .type_guard
        .as_ref()
        .map(|type_guard| mapper.type_(cx, map_cx, type_guard.dupe()));
    let unchanged = match (&type_guard.type_guard, &tg_prime) {
        (None, None) => true,
        (Some(before), Some(after)) => before.ptr_eq(after),
        _ => false,
    };
    if unchanged {
        type_guard.dupe()
    } else {
        TypeGuard::new(TypeGuardInner {
            reason: type_guard.reason.dupe(),
            kind: type_guard.kind,
            inferred: type_guard.inferred,
            param_name: type_guard.param_name.clone(),
            type_guard: tg_prime,
        })
    }
}

pub fn obj_flags_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    flags: Flags,
) -> Flags {
    match flags.obj_kind {
        ObjKind::Indexed(ref dict) => {
            let dict_prime = mapper.dict_type(cx, map_cx, dict.clone());
            if dict_prime == *dict {
                flags
            } else {
                Flags {
                    react_dro: flags.react_dro.clone(),
                    obj_kind: ObjKind::Indexed(dict_prime),
                }
            }
        }
        ObjKind::Exact | ObjKind::Inexact => flags,
    }
}

pub fn obj_type_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    t: Rc<ObjType>,
) -> Rc<ObjType> {
    let flags_prime = mapper.obj_flags(cx, map_cx, t.flags.clone());
    let props_tmap_prime = mapper.props(cx, map_cx, t.props_tmap.dupe());
    let proto_t_prime = mapper.type_(cx, map_cx, t.proto_t.dupe());
    let call_t_prime = option_utils::ident_map(
        |id| mapper.call_prop(cx, map_cx, *id),
        |a, b| a == b,
        t.call_t,
    );
    let reachable_targs_prime = list_utils::ident_map(
        |tup| {
            let (inner_t, p) = tup;
            let inner_t_prime = mapper.type_(cx, map_cx, inner_t.dupe());
            if inner_t.ptr_eq(&inner_t_prime) {
                tup.dupe()
            } else {
                (inner_t_prime, *p)
            }
        },
        |a, b| a.0.ptr_eq(&b.0),
        t.reachable_targs.dupe(),
    );
    if flags_prime == t.flags
        && props_tmap_prime == t.props_tmap
        && t.proto_t.ptr_eq(&proto_t_prime)
        && t.call_t == call_t_prime
        && Rc::ptr_eq(&t.reachable_targs, &reachable_targs_prime)
    {
        t
    } else {
        Rc::new(ObjType {
            flags: flags_prime,
            props_tmap: props_tmap_prime,
            proto_t: proto_t_prime,
            call_t: call_t_prime,
            reachable_targs: reachable_targs_prime,
            strictness_kind: t.strictness_kind,
            literal: t.literal,
        })
    }
}

pub fn namespace_type_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    t: Rc<NamespaceType>,
) -> Rc<NamespaceType> {
    let values_type_prime = mapper.type_(cx, map_cx, t.values_type.dupe());
    let types_tmap_prime = mapper.props(cx, map_cx, t.types_tmap.dupe());
    if t.values_type.ptr_eq(&values_type_prime) && types_tmap_prime == t.types_tmap {
        t
    } else {
        Rc::new(NamespaceType {
            namespace_symbol: t.namespace_symbol.clone(),
            values_type: values_type_prime,
            types_tmap: types_tmap_prime,
        })
    }
}

pub fn dict_type_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    t: DictType,
) -> DictType {
    let key_prime = mapper.type_(cx, map_cx, t.key.dupe());
    let value_prime = mapper.type_(cx, map_cx, t.value.dupe());
    if t.key.ptr_eq(&key_prime) && t.value.ptr_eq(&value_prime) {
        t
    } else {
        DictType {
            dict_name: t.dict_name.dupe(),
            key: key_prime,
            value: value_prime,
            dict_polarity: t.dict_polarity,
        }
    }
}

pub fn arr_type_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    t: Rc<ArrType>,
) -> Rc<ArrType> {
    match &*t {
        ArrType::ArrayAT(box ArrayATData {
            react_dro,
            elem_t,
            tuple_view,
            strictness_kind,
            ..
        }) => {
            let elem_t_prime = mapper.type_(cx, map_cx, elem_t.dupe());
            let same_tuple_view = |a: &TupleView, b: &TupleView| {
                Rc::ptr_eq(&a.elements, &b.elements)
                    && match (&a.rest, &b.rest) {
                        (None, None) => true,
                        (Some(a), Some(b)) => a.t.ptr_eq(&b.t),
                        _ => false,
                    }
            };
            let tuple_view_prime = option_utils::ident_map(
                |tv: &TupleView| {
                    let TupleView {
                        kind,
                        elements,
                        arity,
                        inexact,
                        rest,
                    } = tv;
                    let elements_prime = list_utils::ident_map(
                        |el| mapper.tuple_element(cx, map_cx, el.clone()),
                        |a, b| a.t.ptr_eq(&b.t),
                        elements.dupe(),
                    );
                    let rest_prime = option_utils::ident_map(
                        |el: &TupleElement| mapper.tuple_element(cx, map_cx, el.clone()),
                        |a, b| a.t.ptr_eq(&b.t),
                        rest.clone(),
                    );
                    TupleView {
                        kind: *kind,
                        elements: elements_prime,
                        arity: *arity,
                        inexact: *inexact,
                        rest: rest_prime,
                    }
                },
                same_tuple_view,
                tuple_view.clone(),
            );
            let tuple_view_unchanged = match (tuple_view, &tuple_view_prime) {
                (None, None) => true,
                (Some(a), Some(b)) => same_tuple_view(a, b),
                _ => false,
            };
            if elem_t.ptr_eq(&elem_t_prime) && tuple_view_unchanged {
                t
            } else {
                Rc::new(ArrType::ArrayAT(Box::new(ArrayATData {
                    react_dro: react_dro.clone(),
                    elem_t: elem_t_prime,
                    tuple_view: tuple_view_prime,
                    strictness_kind: *strictness_kind,
                    literal: t.literal(),
                    fresh: t.fresh(),
                })))
            }
        }
        ArrType::TupleAT(box TupleATData {
            react_dro,
            elem_t,
            elements,
            arity,
            inexact,
            rest,
            strictness_kind,
            ..
        }) => {
            let elem_t_prime = mapper.type_(cx, map_cx, elem_t.dupe());
            let elements_prime = list_utils::ident_map(
                |el| mapper.tuple_element(cx, map_cx, el.clone()),
                |a, b| a.t.ptr_eq(&b.t),
                elements.dupe(),
            );
            let rest_prime = option_utils::ident_map(
                |el: &TupleElement| mapper.tuple_element(cx, map_cx, el.clone()),
                |a, b| a.t.ptr_eq(&b.t),
                rest.clone(),
            );
            let rest_unchanged = match (rest, &rest_prime) {
                (None, None) => true,
                (Some(a), Some(b)) => a.t.ptr_eq(&b.t),
                _ => false,
            };
            if elem_t.ptr_eq(&elem_t_prime)
                && Rc::ptr_eq(elements, &elements_prime)
                && rest_unchanged
            {
                t
            } else {
                Rc::new(ArrType::TupleAT(Box::new(TupleATData {
                    react_dro: react_dro.clone(),
                    elem_t: elem_t_prime,
                    elements: elements_prime,
                    arity: *arity,
                    inexact: *inexact,
                    rest: rest_prime,
                    strictness_kind: *strictness_kind,
                    literal: t.literal(),
                    fresh: t.fresh(),
                })))
            }
        }
        ArrType::ROArrayAT(box ROArrayATData {
            elem_t: inner_t,
            react_dro: dro,
            ..
        }) => {
            let inner_t_prime = mapper.type_(cx, map_cx, inner_t.dupe());
            if inner_t.ptr_eq(&inner_t_prime) {
                t
            } else {
                Rc::new(ArrType::ROArrayAT(Box::new(ROArrayATData {
                    elem_t: inner_t_prime,
                    react_dro: dro.clone(),
                    literal: t.literal(),
                })))
            }
        }
    }
}

pub fn tuple_element_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    element: TupleElement,
) -> TupleElement {
    let t_prime = mapper.type_(cx, map_cx, element.t.dupe());
    if element.t.ptr_eq(&t_prime) {
        element
    } else {
        TupleElement {
            reason: element.reason.dupe(),
            name: element.name.dupe(),
            t: t_prime,
            polarity: element.polarity,
            optional: element.optional,
        }
    }
}

pub fn predicate_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    p: &Predicate,
) -> Predicate {
    match p.deref() {
        PredicateInner::AndP(p1, p2) => {
            let p1_prime = mapper.predicate(cx, map_cx, p1);
            let p2_prime = mapper.predicate(cx, map_cx, p2);
            if p1.ptr_eq(&p1_prime) && p2.ptr_eq(&p2_prime) {
                p.dupe()
            } else {
                Predicate::new(PredicateInner::AndP(p1_prime, p2_prime))
            }
        }
        PredicateInner::OrP(p1, p2) => {
            let p1_prime = mapper.predicate(cx, map_cx, p1);
            let p2_prime = mapper.predicate(cx, map_cx, p2);
            if p1.ptr_eq(&p1_prime) && p2.ptr_eq(&p2_prime) {
                p.dupe()
            } else {
                Predicate::new(PredicateInner::OrP(p1_prime, p2_prime))
            }
        }
        PredicateInner::NotP(inner_p) => {
            let inner_p_prime = mapper.predicate(cx, map_cx, inner_p);
            if inner_p.ptr_eq(&inner_p_prime) {
                p.dupe()
            } else {
                Predicate::new(PredicateInner::NotP(inner_p_prime))
            }
        }
        PredicateInner::BinaryP(test, inner_t) => {
            let inner_t_prime = mapper.type_(cx, map_cx, inner_t.dupe());
            if inner_t.ptr_eq(&inner_t_prime) {
                p.dupe()
            } else {
                Predicate::new(PredicateInner::BinaryP(test.clone(), inner_t_prime))
            }
        }
        PredicateInner::TruthyP
        | PredicateInner::NullP
        | PredicateInner::MaybeP
        | PredicateInner::SingletonBoolP(_)
        | PredicateInner::SingletonStrP(_)
        | PredicateInner::SingletonNumP(_)
        | PredicateInner::SingletonBigIntP(_)
        | PredicateInner::BoolP(_)
        | PredicateInner::FunP
        | PredicateInner::NumP(_)
        | PredicateInner::BigIntP(_)
        | PredicateInner::ObjP
        | PredicateInner::StrP(_)
        | PredicateInner::SymbolP(_)
        | PredicateInner::VoidP
        | PredicateInner::ArrP
        | PredicateInner::ArrLenP { .. }
        | PredicateInner::PropNonMaybeP(_, _)
        | PredicateInner::PropNonVoidP(_, _)
        | PredicateInner::PropIsExactlyNullP(_, _)
        | PredicateInner::PropTruthyP(_, _)
        | PredicateInner::PropExistsP { .. }
        | PredicateInner::ImpossibleP => p.dupe(),
        PredicateInner::LatentP(info, indices) => {
            let inner_t_prime = mapper.type_(cx, map_cx, info.2.dupe());
            let targs_prime = option_utils::ident_map(
                |ts: &Rc<[Targ]>| {
                    list_utils::ident_map(
                        |t| mapper.targ(cx, map_cx, t.clone()),
                        |a, b| a == b,
                        ts.dupe(),
                    )
                },
                Rc::ptr_eq,
                info.3.clone(),
            );
            let targs_unchanged = match (&info.3, &targs_prime) {
                (None, None) => true,
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                _ => false,
            };
            let argts_prime = list_utils::ident_map(
                |a| mapper.call_arg(cx, map_cx, a.clone()),
                CallArg::ptr_eq,
                info.4.dupe(),
            );
            if info.2.ptr_eq(&inner_t_prime) && targs_unchanged && Rc::ptr_eq(&info.4, &argts_prime)
            {
                p.dupe()
            } else {
                Predicate::new(PredicateInner::LatentP(
                    Box::new(flow_typing_type::type_::PredFuncallInfo(
                        info.0.clone(),
                        info.1.clone(),
                        inner_t_prime,
                        targs_prime,
                        argts_prime,
                    )),
                    indices.clone(),
                ))
            }
        }
        PredicateInner::LatentThisP(info) => {
            let inner_t_prime = mapper.type_(cx, map_cx, info.2.dupe());
            let targs_prime = option_utils::ident_map(
                |ts: &Rc<[Targ]>| {
                    list_utils::ident_map(
                        |t| mapper.targ(cx, map_cx, t.clone()),
                        |a, b| a == b,
                        ts.dupe(),
                    )
                },
                Rc::ptr_eq,
                info.3.clone(),
            );
            let targs_unchanged = match (&info.3, &targs_prime) {
                (None, None) => true,
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                _ => false,
            };
            let argts_prime = list_utils::ident_map(
                |a| mapper.call_arg(cx, map_cx, a.clone()),
                CallArg::ptr_eq,
                info.4.dupe(),
            );
            if info.2.ptr_eq(&inner_t_prime) && targs_unchanged && Rc::ptr_eq(&info.4, &argts_prime)
            {
                p.dupe()
            } else {
                Predicate::new(PredicateInner::LatentThisP(Box::new(
                    flow_typing_type::type_::PredFuncallInfo(
                        info.0.clone(),
                        info.1.clone(),
                        inner_t_prime,
                        targs_prime,
                        argts_prime,
                    ),
                )))
            }
        }
    }
}

pub fn prop_default<'cx, A, M: TypeMapper<'cx, A> + ?Sized>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &A,
    prop: Property,
) -> Property {
    use std::ops::Deref;

    use flow_typing_type::type_::FieldData;
    use flow_typing_type::type_::GetSetData;
    use flow_typing_type::type_::PropertyInner;
    match prop.deref() {
        PropertyInner::Field(fd) => {
            let type_prime = mapper.type_(cx, map_cx, fd.type_.dupe());
            if fd.type_.ptr_eq(&type_prime) {
                prop
            } else {
                Property::new(PropertyInner::Field(Box::new(FieldData {
                    preferred_def_locs: fd.preferred_def_locs.clone(),
                    key_loc: fd.key_loc.clone(),
                    type_: type_prime,
                    polarity: fd.polarity,
                })))
            }
        }
        PropertyInner::Get { key_loc, type_ } => {
            let type_prime = mapper.type_(cx, map_cx, type_.dupe());
            if type_.ptr_eq(&type_prime) {
                prop
            } else {
                Property::new(PropertyInner::Get {
                    key_loc: key_loc.clone(),
                    type_: type_prime,
                })
            }
        }
        PropertyInner::Set { key_loc, type_ } => {
            let type_prime = mapper.type_(cx, map_cx, type_.dupe());
            if type_.ptr_eq(&type_prime) {
                prop
            } else {
                Property::new(PropertyInner::Set {
                    key_loc: key_loc.clone(),
                    type_: type_prime,
                })
            }
        }
        PropertyInner::GetSet(gs) => {
            let get_type_prime = mapper.type_(cx, map_cx, gs.get_type.dupe());
            let set_type_prime = mapper.type_(cx, map_cx, gs.set_type.dupe());
            if gs.get_type.ptr_eq(&get_type_prime) && gs.set_type.ptr_eq(&set_type_prime) {
                prop
            } else {
                Property::new(PropertyInner::GetSet(Box::new(GetSetData {
                    get_key_loc: gs.get_key_loc.clone(),
                    get_type: get_type_prime,
                    set_key_loc: gs.set_key_loc.clone(),
                    set_type: set_type_prime,
                })))
            }
        }
        PropertyInner::Method { key_loc, type_ } => {
            let type_prime = mapper.type_(cx, map_cx, type_.dupe());
            if type_.ptr_eq(&type_prime) {
                prop
            } else {
                Property::new(PropertyInner::Method {
                    key_loc: key_loc.clone(),
                    type_: type_prime,
                })
            }
        }
    }
}
